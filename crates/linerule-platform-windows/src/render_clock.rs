//! Demand-driven vsync pacing with a one-message tick gate.

#![forbid(unsafe_code)]
#![cfg(windows)]

#[cfg(test)]
use std::sync::atomic::AtomicU64;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use windows::Win32::Foundation::HWND;

use crate::error::Result;
use crate::messages::WM_APP_TICK;
use crate::win32_ffi::pacer;

const PACER_BACKOFF: Duration = Duration::from_millis(16);

struct Shared {
    stop: AtomicBool,
    enabled: AtomicBool,
    tick_pending: AtomicBool,
    #[cfg(test)]
    pacing_attempts: AtomicU64,
    wait_lock: Mutex<()>,
    wake: Condvar,
}

/// UI-thread control handle. Clones share the pacer state.
#[derive(Clone)]
pub(crate) struct RenderClockControl {
    shared: Arc<Shared>,
}

impl RenderClockControl {
    /// Wake the pacer and enqueue at most one immediate tick.
    pub(crate) fn request_tick(&self, target: HWND) {
        self.shared.enabled.store(true, Ordering::Release);
        self.shared.wake.notify_one();
        post_if_clear(&self.shared, target);
    }

    /// Mark the current message consumed and choose whether vsync pacing stays
    /// active. `false` is the Off + hidden steady state.
    pub(crate) fn complete_tick(&self, keep_running: bool) {
        self.shared.tick_pending.store(false, Ordering::Release);
        self.shared.enabled.store(keep_running, Ordering::Release);
        if keep_running {
            self.shared.wake.notify_one();
        }
    }
}

/// RAII owner of the demand-driven pacer thread.
pub(crate) struct RenderClock {
    shared: Arc<Shared>,
    handle: Option<JoinHandle<()>>,
}

impl RenderClock {
    pub(crate) fn spawn(target: HWND) -> Result<Self> {
        let shared = Arc::new(Shared {
            stop: AtomicBool::new(false),
            enabled: AtomicBool::new(true),
            tick_pending: AtomicBool::new(false),
            #[cfg(test)]
            pacing_attempts: AtomicU64::new(0),
            wait_lock: Mutex::new(()),
            wake: Condvar::new(),
        });
        let thread_shared = Arc::clone(&shared);
        let hwnd = target.0 as isize;
        let handle = thread::Builder::new()
            .name("linerule-pacer".into())
            .spawn(move || {
                let target = HWND(hwnd as *mut _);
                pacer_loop(&thread_shared, target);
            })
            .map_err(|_| crate::error::PlatformError::LastError {
                operation: "thread::Builder::spawn",
                code: 0,
                symbol: "thread spawn failed",
            })?;
        Ok(Self {
            shared,
            handle: Some(handle),
        })
    }

    pub(crate) fn control(&self) -> RenderClockControl {
        RenderClockControl {
            shared: Arc::clone(&self.shared),
        }
    }
}

impl Drop for RenderClock {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Release);
        self.shared.wake.notify_all();
        if let Some(handle) = self.handle.take()
            && let Err(error) = handle.join()
        {
            tracing::error!(?error, "render-clock thread panicked during shutdown");
        }
    }
}

fn pacer_loop(shared: &Shared, target: HWND) {
    tracing::info!(target: "RenderClock", "demand-driven pacer started");
    while !shared.stop.load(Ordering::Acquire) {
        wait_until_enabled(shared);
        if shared.stop.load(Ordering::Acquire) {
            break;
        }
        #[cfg(test)]
        shared.pacing_attempts.fetch_add(1, Ordering::Relaxed);
        if let Err(error) = pacer::dwm_flush() {
            tracing::warn!(%error, "DwmFlush failed; backing off");
            thread::sleep(PACER_BACKOFF);
            continue;
        }
        if shared.enabled.load(Ordering::Acquire) {
            post_if_clear(shared, target);
        }
    }
    tracing::info!(target: "RenderClock", "demand-driven pacer stopped");
}

fn wait_until_enabled(shared: &Shared) {
    let mut guard = match shared.wait_lock.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    while !shared.enabled.load(Ordering::Acquire) && !shared.stop.load(Ordering::Acquire) {
        guard = match shared.wake.wait(guard) {
            Ok(next) => next,
            Err(poisoned) => poisoned.into_inner(),
        };
    }
}

fn post_if_clear(shared: &Shared, target: HWND) {
    if shared
        .tick_pending
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return;
    }
    if let Err(error) = pacer::post_message(target, WM_APP_TICK) {
        shared.tick_pending.store(false, Ordering::Release);
        tracing::warn!(%error, "PostMessageW(WM_APP_TICK) failed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "10-second release soak; exercised by native Windows CI"]
    fn off_hidden_wait_posts_no_render_ticks_for_ten_seconds() {
        let shared = Arc::new(Shared {
            stop: AtomicBool::new(false),
            enabled: AtomicBool::new(false),
            tick_pending: AtomicBool::new(false),
            wait_lock: Mutex::new(()),
            wake: Condvar::new(),
            pacing_attempts: AtomicU64::new(0),
        });
        let worker_shared = Arc::clone(&shared);
        let worker = thread::spawn(move || {
            pacer_loop(&worker_shared, HWND(std::ptr::null_mut()));
        });

        thread::sleep(Duration::from_secs(10));
        let attempts = shared.pacing_attempts.load(Ordering::Relaxed);
        shared.stop.store(true, Ordering::Release);
        shared.wake.notify_all();
        worker.join().expect("pacer soak worker must exit");

        assert_eq!(
            attempts, 0,
            "Off + hidden must not attempt pacing or enqueue a render tick"
        );
    }
}
