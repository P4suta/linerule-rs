//! Pure time-driven interpolation for overlay/HUD transitions.
//!
//! Integer endpoints only, so transition state stays `Eq + Hash + Serialize`
//! inside `TickWorld` (`f32` would break those derives). Sampling is per-tick
//! with injected `now_ms`; [`Transition::retarget`] re-bases from the current
//! sampled value so a held key mid-flight glides smoothly instead of snapping.

use serde::Serialize;

/// Integer-endpoint interpolation; small unsigned scalars (`u8`, `u16`) keep
/// transition state `Eq + Hash` inside `TickWorld`.
pub trait Lerp: Copy + Eq {
    /// Interpolate `from → to` at `t ∈ [0, 1]`, rounding to nearest.
    #[must_use]
    fn lerp(from: Self, to: Self, t: f32) -> Self;
}

impl Lerp for u8 {
    fn lerp(from: Self, to: Self, t: f32) -> Self {
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "lerp_f32 clamps to the endpoints' min..=max, within u8 range"
        )]
        let v = lerp_f32(f32::from(from), f32::from(to), t).round() as Self;
        v
    }
}

impl Lerp for u16 {
    fn lerp(from: Self, to: Self, t: f32) -> Self {
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "lerp_f32 clamps to the endpoints' min..=max, within u16 range"
        )]
        let v = lerp_f32(f32::from(from), f32::from(to), t).round() as Self;
        v
    }
}

/// `from + (to - from) * t`, `t` clamped to `[0, 1]` (NaN → 0); result clamped
/// to the endpoint interval so integer casts stay in range.
fn lerp_f32(from: f32, to: f32, t: f32) -> f32 {
    let t = if t.is_finite() {
        t.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let v = (to - from).mul_add(t, from);
    v.clamp(from.min(to), from.max(to))
}

/// Cubic ease-out `1 - (1 - t)³`. `t` clamped to `[0, 1]`; non-finite → `0`.
#[must_use]
pub(crate) fn ease_out(t: f32) -> f32 {
    if !t.is_finite() || t <= 0.0 {
        return 0.0;
    }
    if t >= 1.0 {
        return 1.0;
    }
    let inv = 1.0 - t;
    (inv * inv).mul_add(-inv, 1.0)
}

/// Timed transition between integer endpoints; `sample(now_ms)` returns the
/// eased value. Settled once `from == to` or duration elapsed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub struct Transition<T: Lerp> {
    /// Start value.
    pub from: T,
    /// Target value.
    pub to: T,
    /// Start time (ms, same axis as the tick's `now_ms`).
    pub start_ms: i64,
    /// Duration (ms). `0` = instant.
    pub duration_ms: u16,
}

impl<T: Lerp> Transition<T> {
    /// Transition settled at `value`; `sample` always returns it.
    #[must_use]
    pub const fn settled(value: T) -> Self {
        Self {
            from: value,
            to: value,
            start_ms: 0,
            duration_ms: 0,
        }
    }

    /// Linear progress in `[0, 1]` (pre-easing). `duration_ms == 0` → `1.0`;
    /// before `start_ms` → `0.0`.
    #[must_use]
    pub fn progress(self, now_ms: i64) -> f32 {
        if self.duration_ms == 0 {
            return 1.0;
        }
        let elapsed = now_ms.saturating_sub(self.start_ms);
        if elapsed <= 0 {
            return 0.0;
        }
        if elapsed >= i64::from(self.duration_ms) {
            return 1.0;
        }
        #[allow(
            clippy::cast_precision_loss,
            reason = "elapsed is bounded to 0..u16::MAX, exact in f32"
        )]
        let e = elapsed as f32;
        e / f32::from(self.duration_ms)
    }

    /// Value at `now_ms` (after ease-out).
    #[must_use]
    pub fn sample(self, now_ms: i64) -> T {
        T::lerp(self.from, self.to, ease_out(self.progress(now_ms)))
    }

    /// Whether the transition is still moving. While `true`, callers must
    /// redraw every tick.
    #[must_use]
    pub fn is_live(self, now_ms: i64) -> bool {
        self.from != self.to && self.progress(now_ms) < 1.0
    }

    /// Swap the target, re-basing from the current sampled value so a mid-flight
    /// target change glides continuously. Same-target is a no-op (does not
    /// restart the flight).
    #[must_use]
    pub fn retarget(self, now_ms: i64, to: T, duration_ms: u16) -> Self {
        if self.to == to {
            return self;
        }
        Self {
            from: self.sample(now_ms),
            to,
            start_ms: now_ms,
            duration_ms,
        }
    }
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn tr(from: u8, to: u8, start_ms: i64, duration_ms: u16) -> Transition<u8> {
        Transition {
            from,
            to,
            start_ms,
            duration_ms,
        }
    }

    #[test]
    fn ease_out_endpoints() {
        assert!(ease_out(0.0).abs() < 1e-6);
        assert!((ease_out(1.0) - 1.0).abs() < 1e-6);
    }

    /// Pins `ease_out(0.5) = 0.875`; catches operator mutations in `inv*inv*inv`.
    #[test]
    fn ease_out_midpoint_value_is_pinned() {
        let v = ease_out(0.5);
        assert!(
            (v - 0.875).abs() < 1e-6,
            "ease_out(0.5): expected 0.875, got {v}"
        );
    }

    #[test]
    fn ease_out_handles_nan_and_out_of_range() {
        assert!(ease_out(f32::NAN).abs() < 1e-6);
        assert!(ease_out(-1.0).abs() < 1e-6);
        assert!(ease_out(f32::INFINITY).abs() < 1e-6, "non-finite maps to 0");
        assert!((ease_out(2.0) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn sample_at_start_is_from() {
        let t = tr(0, 200, 1_000, 160);
        assert_eq!(t.sample(1_000), 0);
    }

    #[test]
    fn sample_at_end_is_to() {
        let t = tr(0, 200, 1_000, 160);
        assert_eq!(t.sample(1_160), 200);
    }

    #[test]
    fn sample_clamps_before_start_and_after_end() {
        let t = tr(10, 250, 1_000, 160);
        assert_eq!(t.sample(0), 10, "before start clamps to from");
        assert_eq!(t.sample(i64::MAX), 250, "far future clamps to to");
    }

    #[test]
    fn zero_duration_is_instant() {
        let t = tr(0, 200, 1_000, 0);
        assert_eq!(t.sample(0), 200, "duration 0 settles immediately");
        assert!(!t.is_live(0));
    }

    #[test]
    fn settled_is_constant_and_not_live() {
        let t = Transition::settled(42_u8);
        for now in [i64::MIN, 0, i64::MAX] {
            assert_eq!(t.sample(now), 42);
            assert!(!t.is_live(now));
        }
    }

    #[test]
    fn is_live_during_flight_only() {
        let t = tr(0, 200, 1_000, 160);
        assert!(t.is_live(1_000));
        assert!(t.is_live(1_080));
        assert!(!t.is_live(1_160), "exactly at end = settled");
    }

    #[test]
    fn retarget_to_same_target_is_identity() {
        let t = tr(0, 200, 1_000, 160);
        assert_eq!(t.retarget(1_080, 200, 160), t, "mid-flight same target");
        let settled = Transition::settled(200_u8);
        assert_eq!(settled.retarget(5_000, 200, 160), settled);
    }

    /// At 0.5 progress the eased value is past the linear 50%; identity-mutating
    /// `ease_out` would yield 100 and be caught.
    #[test]
    fn sample_midpoint_is_past_linear_midpoint() {
        let t = tr(0, 200, 0, 160);
        let v = t.sample(80);
        // ease_out(0.5) = 0.875 → 200 * 0.875 = 175
        assert_eq!(v, 175, "expected eased value 175, got {v}");
    }

    proptest! {
        /// At any time, sample stays within from..=to (order-normalized).
        #[test]
        fn sample_stays_within_endpoints(
            from in any::<u8>(), to in any::<u8>(),
            start in -10_000_i64..10_000, dur in 0_u16..2_000,
            now in -20_000_i64..20_000,
        ) {
            let t = Transition { from, to, start_ms: start, duration_ms: dur };
            let v = t.sample(now);
            prop_assert!(v >= from.min(to) && v <= from.max(to));
        }

        /// When `from <= to`, sample is non-decreasing in now (mirrored for
        /// the reverse direction).
        #[test]
        fn sample_is_monotone_in_time(
            from in any::<u8>(), to in any::<u8>(),
            dur in 1_u16..2_000,
            n1 in 0_i64..5_000, n2 in 0_i64..5_000,
        ) {
            let t = Transition { from, to, start_ms: 0, duration_ms: dur };
            let (lo, hi) = if n1 <= n2 { (n1, n2) } else { (n2, n1) };
            let a = t.sample(lo);
            let b = t.sample(hi);
            if from <= to {
                prop_assert!(a <= b, "non-decreasing expected: {a} then {b}");
            } else {
                prop_assert!(a >= b, "non-increasing expected: {a} then {b}");
            }
        }

        /// Sample right after the swap equals the sample right before: the
        /// value never jumps at retarget.
        #[test]
        fn retarget_is_continuous_at_switch_time(
            from in any::<u8>(), to in any::<u8>(), new_to in any::<u8>(),
            dur in 1_u16..2_000, new_dur in 1_u16..2_000,
            elapsed in 0_i64..3_000,
        ) {
            let t = Transition { from, to, start_ms: 0, duration_ms: dur };
            let now = elapsed;
            let before = t.sample(now);
            let r = t.retarget(now, new_to, new_dur);
            prop_assert_eq!(r.sample(now), before, "value must not jump at retarget");
        }

        /// After retargeting to a different target, the value arrives when the
        /// new duration elapses.
        #[test]
        fn retarget_reaches_new_target(
            from in any::<u8>(), to in any::<u8>(), new_to in any::<u8>(),
            dur in 1_u16..2_000, new_dur in 1_u16..2_000,
            elapsed in 0_i64..3_000,
        ) {
            prop_assume!(new_to != to);
            let t = Transition { from, to, start_ms: 0, duration_ms: dur };
            let r = t.retarget(elapsed, new_to, new_dur);
            prop_assert_eq!(r.sample(elapsed + i64::from(new_dur)), new_to);
        }

        /// ease_out never panics, returns [0, 1], and is monotone on [0, 1].
        #[test]
        fn ease_out_total_and_monotone(t1 in 0.0_f32..=1.0, t2 in 0.0_f32..=1.0) {
            let (lo, hi) = if t1 <= t2 { (t1, t2) } else { (t2, t1) };
            let a = ease_out(lo);
            let b = ease_out(hi);
            prop_assert!((0.0..=1.0).contains(&a));
            prop_assert!((0.0..=1.0).contains(&b));
            prop_assert!(a <= b, "ease_out must be non-decreasing");
        }

        /// The u16 impl also matches endpoints (thickness px channel).
        #[test]
        fn u16_lerp_endpoints(from in any::<u16>(), to in any::<u16>()) {
            prop_assert_eq!(u16::lerp(from, to, 0.0), from);
            prop_assert_eq!(u16::lerp(from, to, 1.0), to);
        }
    }
}
