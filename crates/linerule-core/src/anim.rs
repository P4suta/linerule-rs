//! Pure time-driven interpolation for overlay / HUD transitions.
//!
//! A [`Transition<T>`] glides between two **integer** endpoints over a fixed
//! duration. Integer endpoints are a deliberate constraint: transition state
//! lives inside `TickWorld`, which derives `Eq + Hash + Serialize` — storing
//! `f32` there would break those derives. Sampling happens per tick with the
//! injected `now_ms`, so the module stays free of clocks and I/O.
//!
//! Cancellation/retargeting is a single rule: [`Transition::retarget`]
//! re-bases the glide *from the currently sampled value*, so a held bump key
//! whose repeats land mid-flight keeps moving smoothly instead of
//! stair-stepping or snapping back.

use serde::Serialize;

/// Integer endpoint interpolation. Implementors are small unsigned scalars
/// (`u8` for opacity bytes / envelopes, `u16` for thickness px) so transition
/// state stays `Eq + Hash` inside `TickWorld`.
pub trait Lerp: Copy + Eq {
    /// Interpolate `from → to` at `t ∈ [0, 1]` (callers clamp `t`), rounding
    /// to the nearest representable value.
    #[must_use]
    fn lerp(from: Self, to: Self, t: f32) -> Self;
}

impl Lerp for u8 {
    fn lerp(from: Self, to: Self, t: f32) -> Self {
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "lerp_f32 は両端点の min..=max に clamp 済み、u8 域に収まる"
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
            reason = "lerp_f32 は両端点の min..=max に clamp 済み、u16 域に収まる"
        )]
        let v = lerp_f32(f32::from(from), f32::from(to), t).round() as Self;
        v
    }
}

/// `from + (to - from) * t`、`t` は `[0, 1]` に clamp (NaN → 0)。結果は
/// 両端点の張る区間に clamp するので、整数キャストが域外に出ることはない。
fn lerp_f32(from: f32, to: f32, t: f32) -> f32 {
    let t = if t.is_finite() {
        t.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let v = (to - from).mul_add(t, from);
    v.clamp(from.min(to), from.max(to))
}

/// Cubic ease-out: `1 - (1 - t)³`。`t` は `[0, 1]` に clamp、非有限は `0`
/// に倒す ([`crate::color::perceptual`] のガードと同じ流儀)。
#[must_use]
pub fn ease_out(t: f32) -> f32 {
    if !t.is_finite() || t <= 0.0 {
        return 0.0;
    }
    if t >= 1.0 {
        return 1.0;
    }
    let inv = 1.0 - t;
    (inv * inv).mul_add(-inv, 1.0)
}

/// 整数エンドポイント間の時間遷移。`sample(now_ms)` が ease-out 済みの現在値
/// を返す。`from == to` または期間満了で「settled」(以後どの時刻でも `to`)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub struct Transition<T: Lerp> {
    /// 開始値。
    pub from: T,
    /// 目標値。
    pub to: T,
    /// 開始時刻 (ms, tick の `now_ms` と同じ時間軸)。
    pub start_ms: i64,
    /// 期間 (ms)。`0` = 即時 (CI / アニメ無効化の逃げ道)。
    pub duration_ms: u16,
}

impl<T: Lerp> Transition<T> {
    /// 最初から `value` に settle した遷移。`sample` はどの時刻でも `value`。
    #[must_use]
    pub const fn settled(value: T) -> Self {
        Self {
            from: value,
            to: value,
            start_ms: 0,
            duration_ms: 0,
        }
    }

    /// 進行率 `[0, 1]` (easing 前の線形値)。`duration_ms == 0` は常に `1.0`、
    /// `now_ms < start_ms` は `0.0`。
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
            reason = "elapsed は 0..u16::MAX に bound 済み、f32 で exact"
        )]
        let e = elapsed as f32;
        e / f32::from(self.duration_ms)
    }

    /// `now_ms` 時点の値 (ease-out 適用後)。
    #[must_use]
    pub fn sample(self, now_ms: i64) -> T {
        T::lerp(self.from, self.to, ease_out(self.progress(now_ms)))
    }

    /// まだ動いているか。settle 済み (`from == to` または期間満了) なら `false`。
    /// `true` の間は呼び出し側が毎 tick 再描画を続ける必要がある。
    #[must_use]
    pub fn is_live(self, now_ms: i64) -> bool {
        self.from != self.to && self.progress(now_ms) < 1.0
    }

    /// 目標を差し替える。**現在のサンプル値から** re-base するので、飛行中に
    /// 新しい目標が届いても値は連続に滑る (held-key の連続グライド保証)。
    /// 同一目標への retarget は no-op (既存の飛行を再スタートさせない)。
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

    // ---- ease_out ---------------------------------------------------------

    #[test]
    fn ease_out_endpoints() {
        assert!(ease_out(0.0).abs() < 1e-6);
        assert!((ease_out(1.0) - 1.0).abs() < 1e-6);
    }

    /// `ease_out(0.5)` の具体値を pin する: `1 - 0.5³ = 0.875`。
    /// `inv*inv*inv` の演算子 mutation (`*` → `+` 等) を spot で catch する。
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

    // ---- Transition -------------------------------------------------------

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

    /// ease-out なので進行率 0.5 時点で線形 (50%) より先へ進んでいる。
    /// `ease_out` を恒等関数に mutate すると 100 になり検出できる。
    #[test]
    fn sample_midpoint_is_past_linear_midpoint() {
        let t = tr(0, 200, 0, 160);
        let v = t.sample(80);
        // ease_out(0.5) = 0.875 → 200 * 0.875 = 175
        assert_eq!(v, 175, "expected eased value 175, got {v}");
    }

    proptest! {
        /// 任意時刻で sample は from..=to (順序正規化済み) の範囲内。
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

        /// `from <= to` のとき sample は now に対して単調非減少 (逆向きは鏡映)。
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

        /// retarget の連続性: 差し替え直後のサンプル値は差し替え前と一致する。
        /// held bump key が飛行中に何度着弾しても値が飛ばない保証の核。
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

        /// retarget 後は新しい目標へ向かい、期間満了で到達する。
        #[test]
        fn retarget_reaches_new_target(
            from in any::<u8>(), to in any::<u8>(), new_to in any::<u8>(),
            dur in 1_u16..2_000, new_dur in 1_u16..2_000,
            elapsed in 0_i64..3_000,
        ) {
            let t = Transition { from, to, start_ms: 0, duration_ms: dur };
            let r = t.retarget(elapsed, new_to, new_dur);
            prop_assert_eq!(r.sample(elapsed + i64::from(new_dur)), new_to);
        }

        /// ease_out は任意入力で panic せず [0, 1] を返し、[0,1] 内で単調。
        #[test]
        fn ease_out_total_and_monotone(t1 in 0.0_f32..=1.0, t2 in 0.0_f32..=1.0) {
            let (lo, hi) = if t1 <= t2 { (t1, t2) } else { (t2, t1) };
            let a = ease_out(lo);
            let b = ease_out(hi);
            prop_assert!((0.0..=1.0).contains(&a));
            prop_assert!((0.0..=1.0).contains(&b));
            prop_assert!(a <= b, "ease_out must be non-decreasing");
        }

        /// u16 実装も端点一致 (thickness px チャネル用)。
        #[test]
        fn u16_lerp_endpoints(from in any::<u16>(), to in any::<u16>()) {
            prop_assert_eq!(u16::lerp(from, to, 0.0), from);
            prop_assert_eq!(u16::lerp(from, to, 1.0), to);
        }
    }
}
