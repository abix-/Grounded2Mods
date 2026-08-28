use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use modforge::input::{InputSurface, Key, PlayerCommand};
use modforge::route::{GameNavigation, Path, PathPoint, PlayerObservation, Position};
use unityforge::input::UnityInputSurface;
use unityforge::main_thread_queue::MainThreadQueue;
use unityforge::navigation::UnityNavigation;

fn run_while_draining<T: Send + 'static>(
    queue: &'static MainThreadQueue,
    action: impl FnOnce() -> T + Send + 'static,
) -> T {
    let handle = thread::spawn(action);
    let deadline = Instant::now() + Duration::from_secs(1);
    while !handle.is_finished() {
        queue.drain(16);
        assert!(Instant::now() < deadline, "Unity test operation timed out");
        thread::yield_now();
    }
    handle.join().unwrap()
}

#[test]
fn unity_surface_forwards_shared_player_commands_and_observation() {
    let queue = Box::leak(Box::new(MainThreadQueue::new()));
    let received = Arc::new(Mutex::new(Vec::new()));
    let recorded = received.clone();
    let surface = Box::leak(Box::new(UnityInputSurface::new(
        "test Unity input",
        queue,
        move |commands| {
            recorded.lock().unwrap().extend_from_slice(commands);
            Ok(())
        },
        || {
            Ok(PlayerObservation {
                position: Position::new(1.0, 2.0, 3.0),
                yaw_deg: 90.0,
                pitch_deg: -5.0,
            })
        },
    )));
    let commands = vec![
        PlayerCommand::key(Key(0x57), true),
        PlayerCommand::mouse_delta(4, -2),
        PlayerCommand::key(Key(0x45), true),
        PlayerCommand::key(Key(0x45), false),
    ];
    let sent_commands = commands.clone();

    let command_surface = &*surface;
    run_while_draining(queue, move || {
        command_surface.commands(&sent_commands).unwrap()
    });
    assert_eq!(*received.lock().unwrap(), commands);

    let observation_surface = &*surface;
    let observation =
        run_while_draining(queue, move || observation_surface.observe_player().unwrap());
    assert_eq!(observation.position, Position::new(1.0, 2.0, 3.0));
    assert_eq!(observation.yaw_deg, 90.0);
    assert_eq!(observation.pitch_deg, -5.0);
}

#[test]
fn unity_navigation_returns_the_shared_path_format() {
    let queue = Box::leak(Box::new(MainThreadQueue::new()));
    let navigation = Box::leak(Box::new(UnityNavigation::new(queue, |start, goal| {
        Path::new(vec![PathPoint::new(start), PathPoint::new(goal)])
    })));
    let start = Position::new(1.0, 2.0, 3.0);
    let goal = Position::new(10.0, 20.0, 30.0);

    let navigation_ref = &*navigation;
    let path = run_while_draining(queue, move || {
        navigation_ref.find_path(start, goal).unwrap()
    });
    assert_eq!(path.points()[0].position, start);
    assert_eq!(path.points()[1].position, goal);
}
