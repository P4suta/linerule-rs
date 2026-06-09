//! Tick latency と dropped frame の固定窓 tracker。HUD telemetry に
//! `HudTelemetry { tick_p99_ms, frames_dropped, commit_timeouts }` として供給する。
//!
//! 設計:
//! - サンプル窓は固定 256 frames (≒ 4 秒 @ 60Hz, ≒ 1.8 秒 @ 144Hz)。VecDeque の
//!   push_back + pop_front で O(1) 更新。
//! - p99 は snapshot 時に Vec にコピーして sort し index `(n-1) * 99 / 100` を取る。
//!   per-tick の計算ではない (snapshot は 200ms 毎にしか呼ばれない)。
//! - `Instant::now` は呼び出し側 (`wndproc::apply_tick`) で取って begin_tick /
//!   end_tick に渡すことで、本モジュールを純粋ロジックに保ち unit test 可能にする。
//! - `frames_dropped` / `commit_timeouts` は monotonic counter。リセットしない
//!   (cs HudTelemetry.cs と同じ意味論)。

#![forbid(unsafe_code)]

use std::collections::VecDeque;
use std::time::Duration;

use linerule_core::HudTelemetry;

/// p99 計算のサンプル窓サイズ。256 frames ≒ 4s @ 60Hz, ≒ 1.8s @ 144Hz。
const WINDOW_CAPACITY: usize = 256;

/// HUD telemetry の rolling tracker。`wndproc::apply_tick` の入口で
/// `begin_tick(now)`、出口で `end_tick(now, over_budget)` を呼び、`RefreshHud`
/// effect の処理時に `snapshot()` で [`HudTelemetry`] を取得する。
#[derive(Debug)]
pub struct FrameTimingTracker {
    /// 各 tick の elapsed (begin → end) の固定窓サンプル。
    samples: VecDeque<Duration>,
    /// budget 超過 tick の monotonic counter。
    frames_dropped: u64,
    /// dcomp commit 失敗の monotonic counter。
    commit_timeouts: u64,
    /// 直近の `begin_tick()` 時刻。`None` なら未開始 (or end_tick で reset 済み)。
    /// elapsed 計算は `end_tick(end) - tick_start` で行う。`Instant` を保持する
    /// と clock を mock しづらいので Duration ベースで callers が elapsed を渡す
    /// 設計にしている (`end_tick(elapsed, over_budget)`)。
    _phantom: (),
}

impl FrameTimingTracker {
    /// 空の tracker を作る。サンプル無しの状態は p99 = 0.0 を返す。
    #[must_use]
    pub const fn new() -> Self {
        Self {
            samples: VecDeque::new(),
            frames_dropped: 0,
            commit_timeouts: 0,
            _phantom: (),
        }
    }

    /// 1 tick の elapsed をサンプル窓に追加し、budget 超過なら drop counter を
    /// increment する。`over_budget` は caller (`wndproc`) が
    /// `RenderConfig::warn_ratio * (1000 / refresh_hz)` と elapsed を比較して
    /// 決める (本モジュールは budget を知らない — 純粋ロジック維持のため)。
    pub fn record_tick(&mut self, elapsed: Duration, over_budget: bool) {
        if self.samples.len() >= WINDOW_CAPACITY {
            self.samples.pop_front();
        }
        self.samples.push_back(elapsed);
        if over_budget {
            self.frames_dropped = self.frames_dropped.saturating_add(1);
        }
    }

    /// composition commit の失敗 / timeout を 1 件記録する。WinRT は DispatcherQueue
    /// で自動 commit するため、現状この経路の呼び出し元は無い (telemetry は 0 のまま)。
    pub fn record_timeout(&mut self) {
        self.commit_timeouts = self.commit_timeouts.saturating_add(1);
    }

    /// 現在の累積値から HUD 表示用 snapshot を作る。`tick_p99_ms` は
    /// 直近窓の 99 パーセンタイル。サンプル数 < 1 のときは 0.0。
    #[must_use]
    pub fn snapshot(&self) -> HudTelemetry {
        HudTelemetry {
            tick_p99_ms: p99_ms(&self.samples),
            frames_dropped: self.frames_dropped,
            commit_timeouts: self.commit_timeouts,
        }
    }
}

impl Default for FrameTimingTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// 直近窓の Duration サンプルから 99 パーセンタイルを ms (f32) で返す。
/// 空のときは 0.0。サンプル数 n に対し index は `((n - 1) * 99) / 100`。
///
/// 純粋関数として切り出してあるので proptest 対象。窓サイズが 100 を超える
/// と percentile index の差分 (例: n=200 で `(199 * 99) / 100 = 197`、
/// n=256 で `(255 * 99) / 100 = 252`) が線形に増える挙動を test で pin する。
fn p99_ms(samples: &VecDeque<Duration>) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let mut copy: Vec<Duration> = samples.iter().copied().collect();
    copy.sort_unstable();
    let n = copy.len();
    let idx = ((n - 1) * 99) / 100;
    // 0..1000 ms 範囲なら f32 で十分。`as_secs_f32` は秒、 1000 倍で ms。
    copy[idx].as_secs_f32() * 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn p99_with_empty_samples_returns_zero() {
        let t = FrameTimingTracker::new();
        let s = t.snapshot();
        assert_eq!(s.tick_p99_ms, 0.0);
        assert_eq!(s.frames_dropped, 0);
        assert_eq!(s.commit_timeouts, 0);
    }

    #[test]
    fn p99_with_single_sample_equals_that_sample() {
        let mut t = FrameTimingTracker::new();
        t.record_tick(ms(7), false);
        assert!((t.snapshot().tick_p99_ms - 7.0).abs() < 0.001);
    }

    #[test]
    fn p99_correctly_picks_99th_percentile_at_100_samples() {
        let mut t = FrameTimingTracker::new();
        // 100 samples: 1..=100. p99 index = (100 - 1) * 99 / 100 = 98 → sorted[98] = 99
        for i in 1..=100u64 {
            t.record_tick(ms(i), false);
        }
        assert!(
            (t.snapshot().tick_p99_ms - 99.0).abs() < 0.001,
            "p99 of 1..=100 ms expected 99.0 ms"
        );
    }

    #[test]
    fn p99_correctly_picks_99th_percentile_at_window_capacity() {
        let mut t = FrameTimingTracker::new();
        // 256 samples: 1..=256. p99 index = (256 - 1) * 99 / 100 = 252 → sorted[252] = 253
        for i in 1..=256u64 {
            t.record_tick(ms(i), false);
        }
        assert!(
            (t.snapshot().tick_p99_ms - 253.0).abs() < 0.001,
            "p99 of 1..=256 ms expected 253.0 ms, got {}",
            t.snapshot().tick_p99_ms
        );
    }

    #[test]
    fn window_evicts_oldest_when_full() {
        let mut t = FrameTimingTracker::new();
        // 256 + 10 samples; oldest 10 should be evicted.
        for i in 1..=266u64 {
            t.record_tick(ms(i), false);
        }
        // p99 over 11..=266 ms: index = (256 - 1) * 99 / 100 = 252 → sorted[252] = 263
        assert!(
            (t.snapshot().tick_p99_ms - 263.0).abs() < 0.001,
            "after eviction p99 expected 263.0 ms, got {}",
            t.snapshot().tick_p99_ms
        );
    }

    #[test]
    fn dropped_increments_only_on_over_budget() {
        let mut t = FrameTimingTracker::new();
        t.record_tick(ms(5), false);
        t.record_tick(ms(20), true);
        t.record_tick(ms(8), false);
        t.record_tick(ms(30), true);
        assert_eq!(t.snapshot().frames_dropped, 2);
    }

    #[test]
    fn record_timeout_is_monotonic_for_simple_sequence() {
        let mut t = FrameTimingTracker::new();
        t.record_timeout();
        t.record_timeout();
        t.record_timeout();
        assert_eq!(t.snapshot().commit_timeouts, 3);
    }

    proptest! {
        /// 任意の長さのサンプル列に対し、p99 は常に `[0, max_sample_ms]` の範囲に
        /// 入る。範囲外を返したら percentile index 計算のバグ。
        #[test]
        fn p99_stays_within_sample_range(samples in proptest::collection::vec(1u64..=1000, 1..=300)) {
            let mut t = FrameTimingTracker::new();
            for s in &samples {
                t.record_tick(ms(*s), false);
            }
            // window cap 256 で truncate されることを考慮
            let kept: Vec<u64> = samples.iter().rev().take(WINDOW_CAPACITY).copied().collect();
            let &max = kept.iter().max().unwrap();
            #[allow(clippy::cast_precision_loss)]
            let max_f32 = max as f32;
            let p99 = t.snapshot().tick_p99_ms;
            prop_assert!(p99 >= 0.0);
            prop_assert!(p99 <= max_f32 + 0.001);
        }

        /// 全 sample が同じ値なら p99 もその値。
        #[test]
        fn p99_with_uniform_samples_equals_sample(v in 1u64..=500, n in 1usize..=256) {
            let mut t = FrameTimingTracker::new();
            for _ in 0..n {
                t.record_tick(ms(v), false);
            }
            #[allow(clippy::cast_precision_loss)]
            let v_f32 = v as f32;
            let p99 = t.snapshot().tick_p99_ms;
            prop_assert!((p99 - v_f32).abs() < 0.001);
        }

        /// commit_timeouts は monotonic non-decreasing。
        #[test]
        fn timeouts_are_monotonic(calls in 0u64..=500) {
            let mut t = FrameTimingTracker::new();
            let mut last = 0u64;
            for _ in 0..calls {
                t.record_timeout();
                let snap = t.snapshot();
                prop_assert!(snap.commit_timeouts >= last);
                last = snap.commit_timeouts;
            }
            prop_assert_eq!(t.snapshot().commit_timeouts, calls);
        }
    }
}
