//! Unreal implementation of Modforge's player input surface.
//!
//! Unreal calculates navigation paths, but it does not execute bot travel.
//! This adapter sends movement and look axes through standard player and
//! controller UFunctions on the game thread. A consumer supplies only its
//! retained player and game-specific key action handler.

use std::ffi::c_void;
use std::time::Duration;

use modforge::input::{Axis, Button, InputSurface, Key, PlayerCommand, PlayerPose};
use serde_json::Value as Json;

use crate::ue::actor::LiveActor;
use crate::ue::uobject::NativeProperty;
use crate::ue::{UFunction, UObject};

const INPUT_TIMEOUT: Duration = Duration::from_secs(3);

type KeyHandler = dyn Fn(&UObject, Key, bool) -> Result<(), String> + Send + Sync;

/// Player input routed through Unreal's standard movement and look functions.
pub struct UnrealInputSurface {
    name: &'static str,
    player: &'static LiveActor,
    key_handler: Box<KeyHandler>,
}

impl UnrealInputSurface {
    pub fn new(
        name: &'static str,
        player: &'static LiveActor,
        key_handler: impl Fn(&UObject, Key, bool) -> Result<(), String> + Send + Sync + 'static,
    ) -> Self {
        Self {
            name,
            player,
            key_handler: Box::new(key_handler),
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
        Err("absolute UI clicks are not implemented by UnrealInputSurface".into())
    }

    fn move_abs(&self, _x: i32, _y: i32) -> Result<(), String> {
        Err("absolute cursor movement is not implemented by UnrealInputSurface".into())
    }

    fn key(&self, key: Key, down: bool) -> Result<(), String> {
        self.commands(&[PlayerCommand::key(key, down)])
    }

    fn axis(&self, axis: Axis, value: f32, delta_time: f32) -> Result<(), String> {
        self.commands(&[PlayerCommand::axis(axis, value, delta_time)])
    }

    fn pose(&self) -> Result<PlayerPose, String> {
        let surface = *self;
        let value = crate::game_thread::run(
            move || {
                let player = surface.player()?;
                let position =
                    unsafe { crate::ue::transform::world_location(player.as_ptr() as *const u8) }
                        .ok_or("could not read the retained player's world location")?;
                let controller = unsafe { pawn_controller(player)? };
                let yaw_deg = unsafe { read_control_yaw(controller)? };
                serde_json::to_value(PlayerPose {
                    position: [position.0, position.1, position.2],
                    yaw_deg,
                })
                .map_err(|error| format!("serialize player pose: {error}"))
            },
            INPUT_TIMEOUT,
        )?;
        serde_json::from_value(value).map_err(|error| format!("decode player pose: {error}"))
    }

    fn commands(&self, commands: &[PlayerCommand]) -> Result<(), String> {
        let surface = *self;
        let commands = commands.to_vec();
        surface.on_game_thread(move |surface| {
            let player = surface.player()?;
            let mut controller = None;
            let mut control_yaw = None;
            for command in commands {
                match command {
                    PlayerCommand::Key { key, down } => {
                        (surface.key_handler)(player, key, down)?;
                    }
                    PlayerCommand::Axis {
                        axis,
                        value,
                        delta_time: _,
                    } => {
                        let controller = match controller {
                            Some(controller) => controller,
                            None => {
                                let found = unsafe { pawn_controller(player)? };
                                controller = Some(found);
                                found
                            }
                        };
                        match axis {
                            Axis::MouseX => unsafe {
                                call_f32(
                                    controller,
                                    "PlayerController",
                                    "AddYawInput",
                                    "Val",
                                    value,
                                )?
                            },
                            Axis::MouseY => unsafe {
                                call_f32(
                                    controller,
                                    "PlayerController",
                                    "AddPitchInput",
                                    "Val",
                                    value,
                                )?
                            },
                            Axis::MoveForward | Axis::MoveRight => {
                                let yaw = match control_yaw {
                                    Some(yaw) => yaw,
                                    None => {
                                        let found = unsafe { read_control_yaw(controller)? };
                                        control_yaw = Some(found);
                                        found
                                    }
                                };
                                let (forward, right) = match axis {
                                    Axis::MoveForward => (value as f64, 0.0),
                                    Axis::MoveRight => (0.0, value as f64),
                                    _ => unreachable!(),
                                };
                                let direction = movement_direction(yaw, forward, right);
                                unsafe { add_movement_input(player, direction, value.abs())? };
                            }
                        }
                    }
                }
            }
            Ok(())
        })
    }
}

/// Leak and register an Unreal input surface in Modforge's process-wide slot.
pub fn register(
    name: &'static str,
    player: &'static LiveActor,
    key_handler: impl Fn(&UObject, Key, bool) -> Result<(), String> + Send + Sync + 'static,
) {
    let surface: &'static UnrealInputSurface =
        Box::leak(Box::new(UnrealInputSurface::new(name, player, key_handler)));
    modforge::input::set_input_surface(surface);
}

fn movement_direction(yaw_deg: f64, forward: f64, right: f64) -> (f64, f64, f64) {
    let (sin, cos) = yaw_deg.to_radians().sin_cos();
    (
        cos * forward - sin * right,
        sin * forward + cos * right,
        0.0,
    )
}

unsafe fn pawn_controller(player: &UObject) -> Result<&'static UObject, String> {
    let function = function(player, "Pawn", "GetController")?;
    let properties = function.iter_parameters();
    let return_value = property(&properties, "ReturnValue", 8)?;
    let mut parms = vec![0u8; function.parms_size().max(1) as usize];
    unsafe { player.process_event(function, parms.as_mut_ptr() as *mut c_void) };
    let address = read_u64(&parms, return_value.offset as usize)?;
    if address == 0 {
        return Err("local player has no controller".into());
    }
    Ok(unsafe { &*(address as *const UObject) })
}

unsafe fn read_control_yaw(controller: &UObject) -> Result<f64, String> {
    let function = function(controller, "Controller", "GetControlRotation")?;
    let properties = function.iter_parameters();
    let return_value = property(&properties, "ReturnValue", 24)?;
    let mut parms = vec![0u8; function.parms_size().max(1) as usize];
    unsafe { controller.process_event(function, parms.as_mut_ptr() as *mut c_void) };
    read_f64(&parms, return_value.offset as usize + 8)
}

unsafe fn call_f32(
    target: &UObject,
    owner: &str,
    function_name: &str,
    parameter_name: &str,
    value: f32,
) -> Result<(), String> {
    let function = function(target, owner, function_name)?;
    let properties = function.iter_parameters();
    let parameter = property(&properties, parameter_name, 4)?;
    let mut parms = vec![0u8; function.parms_size().max(1) as usize];
    write_bytes(&mut parms, parameter.offset as usize, &value.to_le_bytes())?;
    unsafe { target.process_event(function, parms.as_mut_ptr() as *mut c_void) };
    Ok(())
}

unsafe fn add_movement_input(
    player: &UObject,
    direction: (f64, f64, f64),
    scale: f32,
) -> Result<(), String> {
    let function = function(player, "Pawn", "AddMovementInput")?;
    let properties = function.iter_parameters();
    let world_direction = property(&properties, "WorldDirection", 24)?;
    let scale_value = property(&properties, "ScaleValue", 4)?;
    let mut parms = vec![0u8; function.parms_size().max(1) as usize];
    for (index, value) in [direction.0, direction.1, direction.2]
        .into_iter()
        .enumerate()
    {
        write_bytes(
            &mut parms,
            world_direction.offset as usize + index * 8,
            &value.to_le_bytes(),
        )?;
    }
    write_bytes(
        &mut parms,
        scale_value.offset as usize,
        &scale.to_le_bytes(),
    )?;
    unsafe { player.process_event(function, parms.as_mut_ptr() as *mut c_void) };
    Ok(())
}

fn function<'a>(target: &'a UObject, owner: &str, name: &str) -> Result<&'a UFunction, String> {
    target
        .class()
        .and_then(|class| class.get_function(owner, name))
        .ok_or_else(|| format!("{owner} has no {name}"))
}

fn property<'a>(
    properties: &'a [NativeProperty],
    name: &str,
    minimum_size: u32,
) -> Result<&'a NativeProperty, String> {
    let property = properties
        .iter()
        .find(|property| property.name == name)
        .ok_or_else(|| format!("input UFunction has no {name} parameter"))?;
    if property.element_size < minimum_size {
        return Err(format!(
            "input UFunction parameter {name} is {} bytes, expected at least {minimum_size}",
            property.element_size
        ));
    }
    Ok(property)
}

fn write_bytes(buffer: &mut [u8], offset: usize, bytes: &[u8]) -> Result<(), String> {
    let destination = buffer
        .get_mut(offset..offset + bytes.len())
        .ok_or_else(|| "input UFunction parameter is outside ParmsSize".to_string())?;
    destination.copy_from_slice(bytes);
    Ok(())
}

fn read_u64(buffer: &[u8], offset: usize) -> Result<u64, String> {
    let bytes: [u8; 8] = buffer
        .get(offset..offset + 8)
        .ok_or_else(|| "input UFunction return value is outside ParmsSize".to_string())?
        .try_into()
        .expect("checked eight-byte return slice");
    Ok(u64::from_le_bytes(bytes))
}

fn read_f64(buffer: &[u8], offset: usize) -> Result<f64, String> {
    Ok(f64::from_bits(read_u64(buffer, offset)?))
}

#[cfg(test)]
mod tests {
    use super::movement_direction;

    #[test]
    fn movement_axes_follow_control_yaw() {
        let forward = movement_direction(90.0, 1.0, 0.0);
        assert!(forward.0.abs() < 1.0e-12);
        assert!((forward.1 - 1.0).abs() < 1.0e-12);

        let left = movement_direction(0.0, 0.0, -1.0);
        assert!(left.0.abs() < 1.0e-12);
        assert!((left.1 + 1.0).abs() < 1.0e-12);
    }
}
