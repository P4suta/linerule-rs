//! 周期的に「アプリ動いてるよ」イベントを `tracing::info!(target = "Heartbeat")`
//! に流す bg thread。`Drop` で停止。

#![forbid(unsafe_code)]
#![cfg(windows)]

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

/// `Drop` で停止する heartbeat thread。
pub struct Heartbeat {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl Heartbeat {
    /// 5 秒ごとに heartbeat を吐く thread を起動する。
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
        // 1 秒刻みで stop を確認しつつ 5 秒待つ
        for _ in 0..5 {
            if stop.load(Ordering::Acquire) {
                break;
            }
            thread::sleep(interval / 5);
        }
    }
    tracing::info!(target: "Heartbeat", "heartbeat thread exited");
}
