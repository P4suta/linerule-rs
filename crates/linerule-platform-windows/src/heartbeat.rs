//! Background thread that periodically logs a liveness event to
//! `tracing::info!(target = "Heartbeat")`. Stops on `Drop`.

#![forbid(unsafe_code)]
#![cfg(windows)]

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

/// Heartbeat thread; stops on `Drop`.
pub struct Heartbeat {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl Heartbeat {
    /// Spawn a thread that emits a heartbeat every 5 seconds.
    #[must_use]
    pub fn spawn() -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let stop_clone = Arc::clone(&stop);
        let handle = thread::Builder::new()
            .name("linerule-heartbeat".into())
            .spawn(move || heartbeat_loop(stop_clone))
            .ok();
        Self { stop, handle }
    }
}

impl Drop for Heartbeat {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn heartbeat_loop(stop: Arc<AtomicBool>) {
    tracing::info!(target: "Heartbeat", "heartbeat thread started");
    let interval = Duration::from_secs(5);
    while !stop.load(Ordering::Acquire) {
        tracing::info!(target: "Heartbeat", "alive");
        // Wait 5s, checking the stop flag each second.
        for _ in 0..5 {
            if stop.load(Ordering::Acquire) {
                break;
            }
            thread::sleep(interval / 5);
        }
    }
    tracing::info!(target: "Heartbeat", "heartbeat thread exited");
}
