//! Unity connection for the shared Modforge path format.

use std::sync::Arc;
use std::time::Duration;

use modforge::route::{GameNavigation, Path, Position};

use crate::main_thread_queue::{MAIN_QUEUE, MainThreadQueue};

const NAVIGATION_TIMEOUT: Duration = Duration::from_secs(3);
type FindPath = dyn Fn(Position, Position) -> Result<Path, String> + Send + Sync;

pub struct UnityNavigation {
    queue: &'static MainThreadQueue,
    find_path: Arc<FindPath>,
}

impl UnityNavigation {
    pub fn new(
        queue: &'static MainThreadQueue,
        find_path: impl Fn(Position, Position) -> Result<Path, String> + Send + Sync + 'static,
    ) -> Self {
        Self {
            queue,
            find_path: Arc::new(find_path),
        }
    }

    pub fn main_queue(
        find_path: impl Fn(Position, Position) -> Result<Path, String> + Send + Sync + 'static,
    ) -> Self {
        Self::new(&MAIN_QUEUE, find_path)
    }
}

impl GameNavigation for UnityNavigation {
    fn find_path(&self, start: Position, goal: Position) -> Result<Path, String> {
        let find_path = self.find_path.clone();
        self.queue
            .run_result("Unity navigation path", NAVIGATION_TIMEOUT, move || {
                find_path(start, goal)
            })
    }
}
