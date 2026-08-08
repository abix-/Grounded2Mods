//! Main-thread job queue. Parallel to `ueforge::pe_queue`.
//!
//! HTTP POST handlers run on the listener thread. Anything that
//! touches Unity state (calling into Mono via the bridge,
//! invoking methods, etc.) must run on the Unity main thread or
//! Unity panics. The pattern: POST handlers queue closures
//! here; the C# shim's `Update()` drains the queue every frame.
//!
//! Lock-free fast path: an `AtomicUsize` shadow of the queue
//! length lets the per-frame drain bail with a single relaxed
//! load when nothing is queued.

use std::sync::atomic::{AtomicUsize, Ordering};

use parking_lot::Mutex;

type Job = Box<dyn FnOnce() + Send + 'static>;

pub struct MainThreadQueue {
    inner: Mutex<Vec<Job>>,
    pending: AtomicUsize,
}

impl MainThreadQueue {
    pub const fn new() -> Self {
        Self {
            inner: Mutex::new(Vec::new()),
            pending: AtomicUsize::new(0),
        }
    }

    /// Enqueue a job. Returns the new pending count.
    pub fn push<F>(&self, job: F) -> usize
    where
        F: FnOnce() + Send + 'static,
    {
        let mut g = self.inner.lock();
        g.push(Box::new(job));
        let n = g.len();
        self.pending.store(n, Ordering::Relaxed);
        n
    }

    /// Drain up to `budget` jobs. Returns how many ran. Called
    /// once per frame from the shim's `Update`.
    pub fn drain(&self, budget: usize) -> usize {
        // Fast path: nothing to do.
        if self.pending.load(Ordering::Relaxed) == 0 {
            return 0;
        }

        let mut jobs: Vec<Job> = Vec::with_capacity(budget);
        {
            let mut g = self.inner.lock();
            let take = g.len().min(budget);
            jobs.extend(g.drain(0..take));
            let remaining = g.len();
            self.pending.store(remaining, Ordering::Relaxed);
        }

        let n = jobs.len();
        for job in jobs {
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(job));
        }
        n
    }

    /// Enqueue `f` and block until the drain runs it, up to
    /// `timeout`. The run-on-main-and-wait pattern every op that
    /// touches Unity needs (on IL2CPP an off-main native call is
    /// a process crash, not an exception).
    pub fn run<T, F>(&self, label: &str, timeout: std::time::Duration, f: F) -> Result<T, String>
    where
        T: Send + 'static,
        F: FnOnce() -> T + Send + 'static,
    {
        use std::sync::Arc;
        let result: Arc<Mutex<Option<T>>> = Arc::new(Mutex::new(None));
        let r2 = result.clone();
        self.push(move || {
            *r2.lock() = Some(f());
        });
        let deadline = std::time::Instant::now() + timeout;
        loop {
            if let Some(r) = result.lock().take() {
                return Ok(r);
            }
            if std::time::Instant::now() >= deadline {
                return Err(format!("{label}: main-thread queue timed out"));
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    pub fn len(&self) -> usize {
        self.pending.load(Ordering::Relaxed)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for MainThreadQueue {
    fn default() -> Self {
        Self::new()
    }
}

pub static MAIN_QUEUE: MainThreadQueue = MainThreadQueue::new();
