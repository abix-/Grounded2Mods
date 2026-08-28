//! Unreal connection for Modforge player input and player observation.

use std::sync::OnceLock;
use std::time::Duration;

use modforge::input::{Axis, Button, InputSurface, Key, PlayerCommand};
use modforge::route::{PlayerObservation, Position};
use serde_json::Value as Json;

use crate::ue::UObject;
use crate::ue::actor::LiveActor;
use crate::ue::uobject::NativeProperty;

const INPUT_TIMEOUT: Duration = Duration::from_secs(3);
static PAWN_CONTROLLER_OFFSET: OnceLock<Result<usize, String>> = OnceLock::new();
static CONTROL_ROTATION_OFFSET: OnceLock<Result<usize, String>> = OnceLock::new();

type PlayerInput = dyn Fn(&[PlayerCommand]) -> Result<(), String> + Send + Sync;

pub struct UnrealInputSurface {
    name: &'static str,
    player: &'static LiveActor,
    player_input: Box<PlayerInput>,
}

impl UnrealInputSurface {
    pub fn new(
        name: &'static str,
        player: &'static LiveActor,
        player_input: impl Fn(&[PlayerCommand]) -> Result<(), String> + Send + Sync + 'static,
    ) -> Self {
        Self {
            name,
            player,
            player_input: Box::new(player_input),
        }
    }

    fn on_game_thread(
        &'static self,
        action: impl FnOnce(&Self) -> Result<(), String> + Send + 'static,
    ) -> Result<(), String> {
        crate::game_thread::run(
            move || {
                action(self)?;
                Ok(Json::Null)
            },
            INPUT_TIMEOUT,
        )?;
        Ok(())
    }

    fn player(&self) -> Result<&'static UObject, String> {
        self.player
            .retained()
            .ok_or_else(|| "Unreal input has no retained local player".into())
    }
}

impl InputSurface for &'static UnrealInputSurface {
    fn name(&self) -> &'static str {
        self.name
    }

    fn click(&self, _button: Button, _x: i32, _y: i32) -> Result<(), String> {
        Err("absolute UI clicks are not implemented by Unreal input".into())
    }

    fn move_abs(&self, _x: i32, _y: i32) -> Result<(), String> {
        Err("absolute cursor movement is not implemented by Unreal input".into())
    }

    fn move_rel(&self, dx: i32, dy: i32) -> Result<(), String> {
        self.commands(&[PlayerCommand::mouse_delta(dx, dy)])
    }

    fn key(&self, key: Key, down: bool) -> Result<(), String> {
        self.commands(&[PlayerCommand::key(key, down)])
    }

    fn axis(&self, axis: Axis, value: f32, _delta_time: f32) -> Result<(), String> {
        match axis {
            Axis::MouseX => self.move_rel(value.round() as i32, 0),
            Axis::MouseY => self.move_rel(0, value.round() as i32),
            Axis::MoveForward | Axis::MoveRight => Err(format!(
                "Unreal bot movement requires virtual keys, not axis {axis:?}"
            )),
        }
    }

    fn observe_player(&self) -> Result<PlayerObservation, String> {
        let surface = *self;
        let value = crate::game_thread::run(
            move || {
                let player = surface.player()?;
                let position =
                    // SAFETY: the retained player is live for this game-thread job.
                    unsafe { crate::ue::transform::world_location(player.as_ptr() as *const u8) }
                        .ok_or("could not read the retained player's world location")?;
                // SAFETY: the retained player and reflected fields are read on the game thread.
                let (pitch_deg, yaw_deg) = unsafe { read_control_rotation(player)? };
                serde_json::to_value(PlayerObservation {
                    position: Position::new(position.0, position.1, position.2),
                    yaw_deg,
                    pitch_deg,
                })
                .map_err(|error| format!("serialize player observation: {error}"))
            },
            INPUT_TIMEOUT,
        )?;
        serde_json::from_value(value).map_err(|error| format!("decode player observation: {error}"))
    }

    fn commands(&self, commands: &[PlayerCommand]) -> Result<(), String> {
        let surface = *self;
        let commands = commands.to_vec();
        surface.on_game_thread(move |surface| {
            surface.player()?;
            forward_player_commands(surface.player_input.as_ref(), &commands)
        })
    }
}

pub fn register(
    name: &'static str,
    player: &'static LiveActor,
    player_input: impl Fn(&[PlayerCommand]) -> Result<(), String> + Send + Sync + 'static,
) {
    let surface: &'static UnrealInputSurface = Box::leak(Box::new(UnrealInputSurface::new(
        name,
        player,
        player_input,
    )));
    modforge::input::set_input_surface(surface);
}

fn forward_player_commands(
    player_input: &PlayerInput,
    commands: &[PlayerCommand],
) -> Result<(), String> {
    player_input(commands)
}

unsafe fn pawn_controller(player: &UObject) -> Result<&'static UObject, String> {
    let offset = match PAWN_CONTROLLER_OFFSET
        .get_or_init(|| class_property_offset(player, "Controller", 8))
    {
        Ok(offset) => *offset,
        Err(error) => return Err(error.clone()),
    };
    // SAFETY: the reflected property is at least pointer-sized and belongs to the live player.
    let address = unsafe { (player.field_ptr(offset) as *const usize).read_unaligned() };
    if address == 0 {
        return Err("local player has no controller".into());
    }
    // SAFETY: Unreal owns the non-null controller pointer for the retained player.
    Ok(unsafe { &*(address as *const UObject) })
}

unsafe fn read_control_rotation(player: &UObject) -> Result<(f64, f64), String> {
    // SAFETY: caller provides the retained player on the game thread.
    let controller = unsafe { pawn_controller(player)? };
    let offset = match CONTROL_ROTATION_OFFSET
        .get_or_init(|| class_property_offset(controller, "ControlRotation", 24))
    {
        Ok(offset) => *offset,
        Err(error) => return Err(error.clone()),
    };
    // SAFETY: the reflected rotation field belongs to the live controller.
    let rotation = unsafe { controller.field_ptr(offset) };
    // SAFETY: the reflected rotation field is at least 24 bytes and contains pitch then yaw.
    let pitch = unsafe { (rotation as *const f64).read_unaligned() };
    // SAFETY: the reflected rotation field is at least 24 bytes.
    let yaw = unsafe { (rotation.add(8) as *const f64).read_unaligned() };
    Ok((pitch, yaw))
}

fn class_property_offset(object: &UObject, name: &str, minimum_size: u32) -> Result<usize, String> {
    let mut class = object.class();
    let mut depth = 0;
    while let Some(current) = class {
        if depth >= 64 {
            return Err(format!(
                "class chain exceeded 64 entries while resolving {name}"
            ));
        }
        let properties = current.cached_native_properties();
        if let Some(offset) = reflected_property_offset(&properties, name, minimum_size)? {
            return Ok(offset);
        }
        class = current.super_class();
        depth += 1;
    }
    Err(format!("player class chain has no reflected {name} field"))
}

fn reflected_property_offset(
    properties: &[NativeProperty],
    name: &str,
    minimum_size: u32,
) -> Result<Option<usize>, String> {
    let Some(property) = properties.iter().find(|property| property.name == name) else {
        return Ok(None);
    };
    if property.element_size < minimum_size {
        return Err(format!(
            "reflected field {name} is {} bytes, expected at least {minimum_size}",
            property.element_size
        ));
    }
    Ok(Some(property.offset as usize))
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use modforge::input::{Key, PlayerCommand};

    use super::{forward_player_commands, reflected_property_offset};
    use crate::ue::uobject::NativeProperty;

    #[test]
    fn player_observation_uses_reflected_controller_and_rotation_fields() {
        let source = include_str!("input.rs");
        assert!(source.contains("\"Controller\""));
        assert!(source.contains("\"ControlRotation\""));
        let unavailable = ["GetControl", "Rotation"].concat();
        assert!(!source.contains(&unavailable));
    }

    #[test]
    fn reflected_field_requires_the_expected_size() {
        let properties = [NativeProperty {
            name: "ControlRotation".into(),
            offset: 0x320,
            element_size: 24,
        }];
        assert_eq!(
            reflected_property_offset(&properties, "ControlRotation", 24).unwrap(),
            Some(0x320)
        );
        assert!(reflected_property_offset(&properties, "ControlRotation", 32).is_err());
    }

    #[test]
    fn unreal_input_forwards_the_shared_command_batch_unchanged() {
        let received = Arc::new(Mutex::new(Vec::new()));
        let recorded = received.clone();
        let commands = [
            PlayerCommand::key(Key(0x57), true),
            PlayerCommand::mouse_delta(5, -2),
            PlayerCommand::key(Key(0x45), true),
            PlayerCommand::key(Key(0x45), false),
        ];

        forward_player_commands(
            &move |batch| {
                recorded.lock().unwrap().extend_from_slice(batch);
                Ok(())
            },
            &commands,
        )
        .unwrap();

        assert_eq!(*received.lock().unwrap(), commands);
    }
}
