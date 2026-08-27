use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use modforge::input::{Axis, Button, InputSurface, Key, PlayerCommand, dispatch_player_commands};

#[derive(Debug, PartialEq)]
enum Seen {
    Axis(Axis, f32, f32),
    Key(Key, bool),
}

#[derive(Default)]
struct RecordingSurface {
    seen: Mutex<Vec<Seen>>,
    batches: AtomicUsize,
}

impl InputSurface for RecordingSurface {
    fn name(&self) -> &'static str {
        "recording"
    }

    fn click(&self, _button: Button, _x: i32, _y: i32) -> Result<(), String> {
        unreachable!()
    }

    fn move_abs(&self, _x: i32, _y: i32) -> Result<(), String> {
        unreachable!()
    }

    fn key(&self, key: Key, down: bool) -> Result<(), String> {
        self.seen.lock().unwrap().push(Seen::Key(key, down));
        Ok(())
    }

    fn axis(&self, axis: Axis, value: f32, delta_time: f32) -> Result<(), String> {
        self.seen
            .lock()
            .unwrap()
            .push(Seen::Axis(axis, value, delta_time));
        Ok(())
    }

    fn commands(&self, commands: &[PlayerCommand]) -> Result<(), String> {
        self.batches.fetch_add(1, Ordering::Relaxed);
        for command in commands {
            match *command {
                PlayerCommand::Axis {
                    axis,
                    value,
                    delta_time,
                } => {
                    self.axis(axis, value, delta_time)?;
                }
                PlayerCommand::Key { key, down } => self.key(key, down)?,
            }
        }
        Ok(())
    }
}

#[test]
fn player_commands_reach_the_surface_in_order() {
    let surface = RecordingSurface::default();
    dispatch_player_commands(
        &surface,
        [
            PlayerCommand::axis(Axis::MoveForward, 1.0, 0.016),
            PlayerCommand::axis(Axis::MouseX, -3.0, 0.016),
            PlayerCommand::key(Key::parse("e").unwrap(), true),
            PlayerCommand::key(Key::parse("e").unwrap(), false),
            PlayerCommand::axis(Axis::MoveForward, 0.0, 0.016),
        ],
    )
    .unwrap();

    assert_eq!(surface.batches.load(Ordering::Relaxed), 1);
    assert_eq!(
        *surface.seen.lock().unwrap(),
        vec![
            Seen::Axis(Axis::MoveForward, 1.0, 0.016),
            Seen::Axis(Axis::MouseX, -3.0, 0.016),
            Seen::Key(Key(0x45), true),
            Seen::Key(Key(0x45), false),
            Seen::Axis(Axis::MoveForward, 0.0, 0.016),
        ]
    );
}

#[test]
fn movement_axes_parse_by_player_meaning() {
    assert_eq!(Axis::parse("move_forward").unwrap(), Axis::MoveForward);
    assert_eq!(Axis::parse("move_right").unwrap(), Axis::MoveRight);
}
