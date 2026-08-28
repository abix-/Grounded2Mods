//! Unreal connection for Modforge player input and player observation.

use std::sync::OnceLock;
use std::time::Duration;

use crate::ue::UObject;
use crate::ue::actor::LiveActor;
use crate::ue::uobject::NativeProperty;
use modforge::input::{Axis, Button, InputSurface, Key, PlayerCommand};
use modforge::route::{PlayerObservation, Position};

const INPUT_TIMEOUT: Duration = Duration::from_secs(3);
static PAWN_CONTROLLER_OFFSET: OnceLock<Result<usize, String>> = OnceLock::new();
static CONTROL_ROTATION_OFFSET: OnceLock<Result<usize, String>> = OnceLock::new();

/// Maps a virtual-key code to the name of the Enhanced Input action
/// it drives, plus the two look actions. Game-specific (the action
/// names come from the game's InputMappingContext), so the consumer
/// supplies it; the injection mechanism below is UE-generic.
pub struct ActionBindings {
    /// (VK code, Enhanced Input action name) for held movement /
    /// interaction keys.
    pub keys: &'static [(u16, &'static str)],
    /// Action driven by relative mouse X (yaw / turn).
    pub yaw: &'static str,
    /// Action driven by relative mouse Y (pitch / look up-down).
    pub pitch: &'static str,
}

pub struct UnrealInputSurface {
    name: &'static str,
    player: &'static LiveActor,
    bindings: ActionBindings,
}

impl UnrealInputSurface {
    pub fn new(name: &'static str, player: &'static LiveActor, bindings: ActionBindings) -> Self {
        Self {
            name,
            player,
            bindings,
        }
    }

    fn player(&self) -> Result<&'static UObject, String> {
        self.player
            .retained()
            .ok_or_else(|| "Unreal input has no retained local player".into())
    }

    fn action_for_key(&self, key: Key) -> Result<&'static str, String> {
        self.bindings
            .keys
            .iter()
            .find(|(vk, _)| *vk == key.0)
            .map(|(_, name)| *name)
            .ok_or_else(|| format!("no Enhanced Input action bound to key 0x{:X}", key.0))
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
        let (yaw, pitch) = (self.bindings.yaw, self.bindings.pitch);
        crate::game_thread::run(
            move || {
                let subsystem = enhanced::subsystem()?;
                if dx != 0 {
                    enhanced::inject_once(subsystem, enhanced::action(yaw)?, dx as f64)?;
                }
                if dy != 0 {
                    enhanced::inject_once(subsystem, enhanced::action(pitch)?, dy as f64)?;
                }
                Ok(serde_json::Value::Null)
            },
            INPUT_TIMEOUT,
        )
        .map(|_| ())
    }

    fn key(&self, key: Key, down: bool) -> Result<(), String> {
        let action = self.action_for_key(key)?;
        crate::game_thread::run(
            move || {
                let subsystem = enhanced::subsystem()?;
                let action_ptr = enhanced::action(action)?;
                if down {
                    enhanced::start_continuous(subsystem, action_ptr, 1.0)?;
                } else {
                    enhanced::stop_continuous(subsystem, action_ptr)?;
                }
                Ok(serde_json::Value::Null)
            },
            INPUT_TIMEOUT,
        )
        .map(|_| ())
    }

    fn axis(&self, axis: Axis, value: f32, _delta_time: f32) -> Result<(), String> {
        match axis {
            Axis::MouseX => self.move_rel(value.round() as i32, 0),
            Axis::MouseY => self.move_rel(0, value.round() as i32),
            Axis::MoveForward | Axis::MoveRight => Err(format!(
                "Unreal bot movement uses held keys (key op), not axis {axis:?}"
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
        for command in commands {
            match *command {
                PlayerCommand::Key { key, down } => self.key(key, down)?,
                PlayerCommand::MouseDelta { dx, dy } => self.move_rel(dx, dy)?,
            }
        }
        Ok(())
    }
}

pub fn register(name: &'static str, player: &'static LiveActor, bindings: ActionBindings) {
    let surface: &'static UnrealInputSurface =
        Box::leak(Box::new(UnrealInputSurface::new(name, player, bindings)));
    modforge::input::set_input_surface(surface);
}

/// UE-generic Enhanced Input action injection. The action objects and
/// the subsystem are resolved by reflection (class chain + object
/// name), so nothing here is game-specific; the caller supplies action
/// names. The two BlueprintCallable UFunctions
/// (`Start`/`StopContinuousInputInjectionForAction`) hold and release
/// an action across frames; `InjectInputForAction` fires it for one
/// tick (used for mouse deltas). Verified live: injecting ForwardInput
/// walked the character. See misery research.md section 31.
mod enhanced {
    use super::UObject;
    use parking_lot::Mutex;
    use std::collections::HashMap;

    const IFACE: &str = "EnhancedInputSubsystemInterface";

    static SUBSYSTEM: Mutex<Option<u64>> = Mutex::new(None);
    static ACTIONS: Mutex<Option<HashMap<String, u64>>> = Mutex::new(None);

    fn readable(addr: u64) -> bool {
        addr > 0x1_0000 && modforge::winproc::is_addr_readable(addr as usize)
    }

    /// The live `EnhancedInputLocalPlayerSubsystem`, cached and
    /// re-resolved if the cached pointer goes stale (level reload).
    pub fn subsystem() -> Result<&'static UObject, String> {
        let mut cache = SUBSYSTEM.lock();
        if let Some(addr) = *cache {
            if readable(addr) {
                // SAFETY: the address is a live UObject we found before
                // and just confirmed readable.
                return Ok(unsafe { &*(addr as *const UObject) });
            }
        }
        let addr = crate::ue::actor::find_objects_by_chain("EnhancedInputLocalPlayerSubsystem")
            .first()
            .map(|p| *p as u64)
            .ok_or("EnhancedInputLocalPlayerSubsystem not found")?;
        *cache = Some(addr);
        // SAFETY: address came from the GObjects walk.
        Ok(unsafe { &*(addr as *const UObject) })
    }

    /// The `UInputAction` object pointer for `name`, cached per name.
    pub fn action(name: &str) -> Result<u64, String> {
        let mut cache = ACTIONS.lock();
        let map = cache.get_or_insert_with(HashMap::new);
        if let Some(&addr) = map.get(name) {
            if readable(addr) {
                return Ok(addr);
            }
        }
        for p in crate::ue::actor::find_objects_by_chain("InputAction") {
            // SAFETY: p came from the GObjects walk.
            let obj = unsafe { &*(p as *const UObject) };
            if obj.name() == name {
                let addr = p as u64;
                map.insert(name.to_string(), addr);
                return Ok(addr);
            }
        }
        Err(format!("UInputAction '{name}' not found"))
    }

    // Parm block (72 bytes) shared by Start / Inject: Action ptr at
    // +0x00; FInputActionValue at +0x08 (FVector Value; Value.X carries
    // the magnitude; ValueType at +0x18 = Axis1D(1)); empty Modifiers
    // TArray at +0x28 and Triggers TArray at +0x38.
    fn value_parms(action_ptr: u64, value: f64) -> Vec<u8> {
        let mut p = vec![0u8; 72];
        p[0..8].copy_from_slice(&action_ptr.to_le_bytes());
        p[8..16].copy_from_slice(&value.to_le_bytes());
        p[32] = 1;
        p
    }

    fn call(subsystem: &UObject, function: &str, parms: Vec<u8>) -> Result<(), String> {
        crate::ops::exec_call(subsystem, IFACE, function, parms).map(|_| ())
    }

    pub fn start_continuous(subsystem: &UObject, action_ptr: u64, value: f64) -> Result<(), String> {
        call(
            subsystem,
            "StartContinuousInputInjectionForAction",
            value_parms(action_ptr, value),
        )
    }

    pub fn stop_continuous(subsystem: &UObject, action_ptr: u64) -> Result<(), String> {
        call(
            subsystem,
            "StopContinuousInputInjectionForAction",
            action_ptr.to_le_bytes().to_vec(),
        )
    }

    pub fn inject_once(subsystem: &UObject, action_ptr: u64, value: f64) -> Result<(), String> {
        call(
            subsystem,
            "InjectInputForAction",
            value_parms(action_ptr, value),
        )
    }
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
    use modforge::input::Key;

    use super::{ActionBindings, reflected_property_offset};
    use crate::ue::uobject::NativeProperty;

    const MISERY: ActionBindings = ActionBindings {
        keys: &[
            (0x57, "ForwardInput"),
            (0x53, "BackwardInput"),
            (0x41, "LeftInput"),
            (0x44, "RightInput"),
            (0x45, "InteractInput"),
        ],
        yaw: "TurnInput",
        pitch: "LookupDownInput",
    };

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
    fn key_bindings_map_wasde() {
        let action = |vk: u16| MISERY.keys.iter().find(|(k, _)| *k == vk).map(|(_, n)| *n);
        assert_eq!(action(Key(0x57).0), Some("ForwardInput"));
        assert_eq!(action(Key(0x45).0), Some("InteractInput"));
        assert_eq!(action(0x99), None);
    }
}
