//! Vsync ペーシング: 別 thread で `DwmFlush()` を待ち、UI thread に
//! `WM_APP_TICK` を `PostMessageW` で送る。
//!
//! Drop で stop flag → `join` → 解放。dcomp / D2D には一切触らない（pacer は
//! 別 thread）。

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

/// `DwmFlush` 失敗時の backoff 間隔。60Hz 1 フレーム相当（~16ms）。hot-loop
/// 化を防ぐと同時に、UI thread の `WM_APP_TICK` 流量が極端に変動するのを抑える。
const PACER_BACKOFF: Duration = Duration::from_millis(16);

/// 別 thread で `DwmFlush` ベースの tick を生成し、指定 HWND に
/// `WM_APP_TICK` を送り続けるペーサ。`Drop` で停止する。
pub struct RenderClock {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl RenderClock {
    /// 新しい pacer thread を起動する。
    ///
    /// # Errors
    /// `std::thread::Builder` の生成に失敗したとき。
    pub fn spawn(target: HWND) -> Result<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let stop_clone = Arc::clone(&stop);
        // HWND は !Send だが、pacer thread が触るのは PostMessageW の引数だけで、
        // PostMessageW 自体は thread-safe (Windows 仕様)。HWND を usize 化して
        // thread 境界を越える。
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
        if let Some(handle) = self.handle.take() {
            if let Err(e) = handle.join() {
                tracing::warn!(?e, "render_clock pacer thread panicked during join");
            }
        }
    }
}

fn pacer_loop(stop: Arc<AtomicBool>, target: HWND) {
    tracing::info!(target: "RenderClock", "pacer thread started");
    while !stop.load(Ordering::Acquire) {
        if let Err(e) = pacer::dwm_flush() {
            tracing::warn!(error = %e, "DwmFlush failed; backing off");
            // hot-loop 回避: DwmFlush が即座に失敗を返す状況（compositor 停止
            // 中など）で CPU を食い潰さないよう ~16ms 待つ。stop flag も観測する。
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
