//! Auto-quit timer for the `--duration-ms` CI smoke test.
//!
//! Background thread sleeps `duration`, then posts `WM_APP_QUIT_TIMER`; the
//! wndproc maps it to `PostQuitMessage(0)`.

#![forbid(unsafe_code)]
#![cfg(windows)]

use std::thread::{self, JoinHandle};
use std::time::Duration;

use windows::Win32::Foundation::HWND;

use crate::error::{PlatformError, Result};
use crate::messages::WM_APP_QUIT_TIMER;
use crate::win32_ffi::pacer;

/// One-shot timer that posts `WM_APP_QUIT_TIMER` to the overlay HWND after
/// `duration`. Joins the thread on `Drop`.
pub struct AutoQuitTimer {
    handle: Option<JoinHandle<()>>,
}

impl AutoQuitTimer {
    /// Spawn a one-shot timer thread that fires after `duration`.
    ///
    /// # Errors
    /// When `std::thread::Builder::spawn` fails.
    pub fn spawn(target: HWND, duration: Duration) -> Result<Self> {
        // HWND is !Send, but PostMessageW is thread-safe; pass it across the
        // thread boundary as an isize.
        let hwnd_isize = target.0 as isize;
        let handle = thread::Builder::new()
            .name("linerule-auto-quit".into())
            .spawn(move || {
                tracing::info!(target: "AutoQuitTimer", millis = duration.as_millis() as u64,
                    "auto-quit timer scheduled");
                thread::sleep(duration);
                let hwnd = HWND(hwnd_isize as *mut _);
                if let Err(e) = pacer::post_message(hwnd, WM_APP_QUIT_TIMER) {
                    tracing::warn!(target: "AutoQuitTimer", error = %e,
                        "PostMessageW(WM_APP_QUIT_TIMER) failed; process may not exit promptly");
                }
            })
            .map_err(|_| PlatformError::LastError {
                operation: "thread::Builder::spawn (auto-quit)",
                code: 0,
                symbol: "thread spawn failed",
            })?;
        Ok(Self {
            handle: Some(handle),
        })
    }
}

impl Drop for AutoQuitTimer {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take()
            && let Err(e) = handle.join()
        {
            tracing::warn!(?e, "auto-quit timer thread panicked during join");
        }
    }
}
