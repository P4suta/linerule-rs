//! Vsync pacing: a background thread waits on `DwmFlush()` and posts
//! `WM_APP_TICK` to the UI thread. `Drop` sets the stop flag and joins.

#![forbid(unsafe_code)]
#![cfg(windows)]

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use windows::Win32::Foundation::HWND;

use crate::error::Result;
use crate::messages::WM_APP_TICK;
use crate::win32_ffi::pacer;

/// Backoff after a `DwmFlush` failure (~one 60Hz frame), to avoid a hot loop.
const PACER_BACKOFF: Duration = Duration::from_millis(16);

/// `DwmFlush`-based pacer that posts `WM_APP_TICK` to a target HWND. Stops on
/// `Drop`.
pub struct RenderClock {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl RenderClock {
    /// Spawn a new pacer thread.
    ///
    /// # Errors
    /// When `std::thread::Builder::spawn` fails.
    pub fn spawn(target: HWND) -> Result<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let stop_clone = Arc::clone(&stop);
        // HWND is !Send, but PostMessageW is thread-safe; pass it across the
        // thread boundary as an isize.
        let hwnd_isize = target.0 as isize;
        let handle = thread::Builder::new()
            .name("linerule-pacer".into())
            .spawn(move || {
                let target = HWND(hwnd_isize as *mut _);
                pacer_loop(stop_clone, target);
            })
            .map_err(|_| crate::error::PlatformError::LastError {
                operation: "thread::Builder::spawn",
                code: 0,
                symbol: "thread spawn failed",
            })?;
        Ok(Self {
            stop,
            handle: Some(handle),
        })
    }
}

impl Drop for RenderClock {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(handle) = self.handle.take()
            && let Err(e) = handle.join()
        {
            tracing::warn!(?e, "render_clock pacer thread panicked during join");
        }
    }
}

fn pacer_loop(stop: Arc<AtomicBool>, target: HWND) {
    tracing::info!(target: "RenderClock", "pacer thread started");
    while !stop.load(Ordering::Acquire) {
        if let Err(e) = pacer::dwm_flush() {
            tracing::warn!(error = %e, "DwmFlush failed; backing off");
            // Avoid burning CPU when DwmFlush fails immediately (e.g. compositor
            // stopped).
            thread::sleep(PACER_BACKOFF);
            continue;
        }
        if stop.load(Ordering::Acquire) {
            break;
        }
        if let Err(e) = pacer::post_message(target, WM_APP_TICK) {
            tracing::warn!(error = %e, "PostMessageW(WM_APP_TICK) failed");
        }
    }
    tracing::info!(target: "RenderClock", "pacer thread exited");
}
