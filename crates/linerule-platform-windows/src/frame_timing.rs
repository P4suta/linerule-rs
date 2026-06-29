//! Fixed-window (256-frame) tracker feeding HUD `HudTelemetry`.
//! Callers pass elapsed `Duration` (keeps module pure); p99 sorts a copy at
//! `snapshot()` time. `frames_dropped` / `commit_timeouts` are monotonic.

#![forbid(unsafe_code)]

use std::collections::VecDeque;
use std::time::Duration;

use linerule_core::HudTelemetry;

/// p99 window size; 256 frames ~= 4s @ 60Hz, ~= 1.8s @ 144Hz.
const WINDOW_CAPACITY: usize = 256;

/// Rolling HUD telemetry tracker: `record_tick` per frame, `snapshot` to read.
#[derive(Debug)]
pub struct FrameTimingTracker {
    /// Fixed-window samples of per-tick elapsed time.
    samples: VecDeque<Duration>,
    /// Monotonic count of over-budget ticks.
    frames_dropped: u64,
    /// Monotonic count of failed composition commits.
    commit_timeouts: u64,
    _phantom: (),
}

impl FrameTimingTracker {
    /// Empty tracker; reports p99 = 0.0 with no samples.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            samples: VecDeque::new(),
            frames_dropped: 0,
            commit_timeouts: 0,
            _phantom: (),
        }
    }

    /// Append one tick's elapsed time; bump drop counter if over budget.
    /// Caller decides `over_budget`; this module does not know the budget.
    pub fn record_tick(&mut self, elapsed: Duration, over_budget: bool) {
        if self.samples.len() >= WINDOW_CAPACITY {
            self.samples.pop_front();
        }
        self.samples.push_back(elapsed);
        if over_budget {
            self.frames_dropped = self.frames_dropped.saturating_add(1);
        }
    }

    /// Record one composition commit failure/timeout. No callers yet: WinRT
    /// auto-commits via the DispatcherQueue, so telemetry stays 0.
    pub fn record_timeout(&mut self) {
        self.commit_timeouts = self.commit_timeouts.saturating_add(1);
    }

    /// Snapshot for the HUD; `tick_p99_ms` is the window p99, 0.0 if empty.
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

/// 99th percentile of the window's samples in ms (f32); 0.0 if empty.
/// Index is `((n - 1) * 99) / 100`.
fn p99_ms(samples: &VecDeque<Duration>) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let mut copy: Vec<Duration> = samples.iter().copied().collect();
    copy.sort_unstable();
    let n = copy.len();
    let idx = ((n - 1) * 99) / 100;
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
        /// p99 always falls within `[0, max_sample_ms]`.
        #[test]
        fn p99_stays_within_sample_range(samples in proptest::collection::vec(1u64..=1000, 1..=300)) {
            let mut t = FrameTimingTracker::new();
            for s in &samples {
                t.record_tick(ms(*s), false);
            }
            // Account for window truncation at WINDOW_CAPACITY.
            let kept: Vec<u64> = samples.iter().rev().take(WINDOW_CAPACITY).copied().collect();
            let &max = kept.iter().max().unwrap();
            let max_f32 = max as f32;
            let p99 = t.snapshot().tick_p99_ms;
            prop_assert!(p99 >= 0.0);
            prop_assert!(p99 <= max_f32 + 0.001);
        }

        /// Uniform samples yield p99 equal to that value.
        #[test]
        fn p99_with_uniform_samples_equals_sample(v in 1u64..=500, n in 1usize..=256) {
            let mut t = FrameTimingTracker::new();
            for _ in 0..n {
                t.record_tick(ms(v), false);
            }
            let v_f32 = v as f32;
            let p99 = t.snapshot().tick_p99_ms;
            prop_assert!((p99 - v_f32).abs() < 0.001);
        }

        /// commit_timeouts is monotonic non-decreasing.
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
