//! `--duration-ms` CI smoke test 用の自動終了タイマー。
//!
//! 別 thread で `thread::sleep(duration)` 後に `PostMessageW(hwnd,
//! WM_APP_QUIT_TIMER, 0, 0)` を発行し、UI thread の wndproc が
//! `PostQuitMessage(0)` に変換することで、`Ctrl+Alt+Q` 経由 quit と同じ
//! graceful な終了 flow を自動化する (Phase α GUI smoke test)。
//!
//! 設計は `RenderClock` と同じ「別 thread + `JoinHandle` を `Drop` で join」
//! パターン。pacer thread とは独立してライフサイクル管理する。

#![forbid(unsafe_code)]
#![cfg(windows)]

use std::thread::{self, JoinHandle};
use std::time::Duration;

use windows::Win32::Foundation::HWND;

use crate::error::{PlatformError, Result};
use crate::messages::WM_APP_QUIT_TIMER;
use crate::win32_ffi::pacer;

/// `duration` 経過後に overlay HWND に `WM_APP_QUIT_TIMER` を 1 回 post する
/// タイマー。`Drop` で thread join。
pub struct AutoQuitTimer {
    handle: Option<JoinHandle<()>>,
}

impl AutoQuitTimer {
    /// 指定 duration 後に発火する 1-shot timer thread を spawn する。
    ///
    /// # Errors
    /// `std::thread::Builder::spawn` の生成に失敗したとき。
    pub fn spawn(target: HWND, duration: Duration) -> Result<Self> {
        // HWND は !Send だが、PostMessageW 自体は thread-safe (Windows 仕様)。
        // HWND を isize 化して thread 境界を越える (render_clock と同じパターン)。
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
        if let Some(handle) = self.handle.take() {
            if let Err(e) = handle.join() {
                tracing::warn!(?e, "auto-quit timer thread panicked during join");
            }
        }
    }
}
