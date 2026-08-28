use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use modforge::input::{Button, InputSurface, Key, PlayerCommand, dispatch_player_commands};
use modforge::route::{PlayerObservation, Position};

#[derive(Debug, PartialEq)]
enum Seen {
    MouseDelta(i32, i32),
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

    fn move_rel(&self, dx: i32, dy: i32) -> Result<(), String> {
        self.seen.lock().unwrap().push(Seen::MouseDelta(dx, dy));
        Ok(())
    }

    fn key(&self, key: Key, down: bool) -> Result<(), String> {
        self.seen.lock().unwrap().push(Seen::Key(key, down));
        Ok(())
    }

    fn commands(&self, commands: &[PlayerCommand]) -> Result<(), String> {
        self.batches.fetch_add(1, Ordering::Relaxed);
        for command in commands {
            match *command {
                PlayerCommand::MouseDelta { dx, dy } => self.move_rel(dx, dy)?,
                PlayerCommand::Key { key, down } => self.key(key, down)?,
            }
        }
        Ok(())
    }

    fn observe_player(&self) -> Result<PlayerObservation, String> {
        Ok(PlayerObservation {
            position: Position::new(1.0, 2.0, 3.0),
            yaw_deg: 90.0,
            pitch_deg: -12.0,
        })
    }
}

#[test]
fn player_commands_reach_the_surface_in_order() {
    let surface = RecordingSurface::default();
    dispatch_player_commands(
        &surface,
        [
            PlayerCommand::key(Key::parse("w").unwrap(), true),
            PlayerCommand::mouse_delta(-3, 2),
            PlayerCommand::key(Key::parse("e").unwrap(), true),
            PlayerCommand::key(Key::parse("e").unwrap(), false),
            PlayerCommand::key(Key::parse("w").unwrap(), false),
        ],
    )
    .unwrap();

    assert_eq!(surface.batches.load(Ordering::Relaxed), 1);
    assert_eq!(
        *surface.seen.lock().unwrap(),
        vec![
            Seen::Key(Key(0x57), true),
            Seen::MouseDelta(-3, 2),
            Seen::Key(Key(0x45), true),
            Seen::Key(Key(0x45), false),
            Seen::Key(Key(0x57), false),
        ]
    );
}

#[test]
fn player_command_batch_round_trips_for_the_control_plane() {
    let commands = vec![
        PlayerCommand::key(Key(0x57), true),
        PlayerCommand::mouse_delta(4, -2),
    ];
    let json = serde_json::to_value(&commands).unwrap();
    assert_eq!(json[0]["kind"], "key");
    assert_eq!(json[1]["kind"], "mouse_delta");
    assert_eq!(
        serde_json::from_value::<Vec<PlayerCommand>>(json).unwrap(),
        commands
    );
}

#[test]
fn player_observation_round_trips_with_yaw_and_pitch() {
    let observation = RecordingSurface::default().observe_player().unwrap();
    let json = serde_json::to_value(observation).unwrap();
    assert_eq!(json["yaw_deg"], 90.0);
    assert_eq!(json["pitch_deg"], -12.0);
    assert_eq!(
        serde_json::from_value::<PlayerObservation>(json).unwrap(),
        observation
    );
}
