//! Distance-driven HUD fade.
//!
//! The HUD panel fades out when the cursor (and thus the user's gaze) gets
//! close to it, and fades back in once the slit is far away. The fade is
//! purely a function of geometry, so it lives here in core.

use crate::{
    color::{Thickness, perceptual},
    geometry::{Logical, Point, ScreenRect},
    state::{Mode, State},
};

/// Returns the HUD opacity in `[0, 1]` for the current frame.
///
/// - When the slit is far from the HUD bounds, opacity approaches `1.0`.
/// - When the slit overlaps the HUD bounds, opacity is `0.0`.
/// - While `Mode::Off` there is no slit; the cursor itself (as a 1-px rect)
///   drives the distance, so the HUD still yields to the pointer.
#[must_use]
pub fn compute_opacity(
    state: State,
    cursor: Point<Logical>,
    hud: ScreenRect<Logical>,
    fade_decay_px: f32,
) -> f32 {
    let cx = px(cursor.x);
    let cy = px(cursor.y);
    let hl = px(hud.left());
    let hr = px(hud.right());
    let ht = px(hud.top());
    let hb = px(hud.bottom());

    let distance = match state.mode {
        Mode::Off => point_to_rect_distance(cx, cy, hl, ht, hr, hb),
        Mode::Horizontal => {
            let (lo, hi) = slit_range(cy, state.config.thickness);
            axis_gap(lo, hi, ht, hb)
        },
        Mode::Vertical => {
            let (lo, hi) = slit_range(cx, state.config.thickness);
            axis_gap(lo, hi, hl, hr)
        },
    };
    let linear = 1.0 - (-distance / fade_decay_px.max(1.0)).exp();
    perceptual::smooth(linear)
}

/// 距離フェード ([`compute_opacity`]) の結果に HUD フェードエンベロープを
/// 乗算する。エンベロープ byte は知覚カーブ ([`perceptual::smooth`]) を通す
/// ので立ち上がりが視覚的に自然になる。`envelope == 255` は恒等 (定常状態の
/// 不透明度を変えない)。
#[must_use]
pub fn apply_envelope(distance_opacity: f32, envelope: u8) -> f32 {
    distance_opacity * perceptual::smooth(f32::from(envelope) / 255.0)
}

pub(crate) fn slit_range(center: f32, thickness: Thickness) -> (f32, f32) {
    let t = px(i32::from(thickness.get()));
    let half = t * 0.5;
    (center - half, center + (t - half))
}

/// 1-D interval gap between half-open intervals `[a_lo, a_hi)` and
/// `[b_lo, b_hi)`. Returns `0` when they overlap.
pub(crate) fn axis_gap(a_lo: f32, a_hi: f32, b_lo: f32, b_hi: f32) -> f32 {
    (b_lo - a_hi).max(a_lo - b_hi).max(0.0)
}

/// 2-D point-to-rect distance. Returns `0` when the point lies inside the
/// rectangle.
pub(crate) fn point_to_rect_distance(x: f32, y: f32, rl: f32, rt: f32, rr: f32, rb: f32) -> f32 {
    let dx = axis_gap(x, x + 1.0, rl, rr);
    let dy = axis_gap(y, y + 1.0, rt, rb);
    dx.mul_add(dx, dy * dy).sqrt()
}

/// Pixel coordinate → `f32` conversion, localized so the precision-loss lint
/// only fires once in the crate, with a concrete invariant statement.
const fn px(p: i32) -> f32 {
    // Screen-pixel coordinates in linerule are bounded to `[i32::MIN, 2^16]`;
    // f32's 24-bit mantissa represents this range exactly. The clippy
    // precision-loss lint is theoretical, not actual, in this domain.
    #[allow(
        clippy::cast_precision_loss,
        reason = "screen pixels (<= 2^16) fit f32 mantissa exactly"
    )]
    let f = p as f32;
    f
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::OverlayConfig;

    fn hud_rect() -> ScreenRect<Logical> {
        ScreenRect::new(Point::new(1376, 24), 520, 560)
    }

    fn h_state() -> State {
        State::with_mode(Mode::Horizontal)
    }

    #[test]
    fn slit_far_from_hud_approaches_full_opacity() {
        let v = compute_opacity(h_state(), Point::new(960, 1000), hud_rect(), 120.0);
        assert!(v > 0.9, "expected > 0.9, got {v}");
    }

    #[test]
    fn slit_intersecting_hud_yields_zero_opacity() {
        let cursor = Point::new(0, 24 + 280);
        let v = compute_opacity(h_state(), cursor, hud_rect(), 120.0);
        assert!(v.abs() < 1e-3, "expected ~0, got {v}");
    }

    #[test]
    fn off_mode_uses_2d_distance() {
        let s = State {
            mode: Mode::Off,
            config: OverlayConfig::DEFAULT,
            ..State::DEFAULT
        };
        let far = compute_opacity(s, Point::new(0, 0), hud_rect(), 120.0);
        let inside = compute_opacity(s, Point::new(1500, 200), hud_rect(), 120.0);
        assert!(far > 0.9);
        assert!(inside.abs() < 1e-3);
    }

    // ---- apply_envelope ---------------------------------------------------

    /// `envelope == 255` は恒等、`0` は完全不可視。中間値は単調。
    #[test]
    fn apply_envelope_endpoints_and_monotonicity() {
        let base = 0.8_f32;
        assert!(
            (apply_envelope(base, 255) - base).abs() < 1e-6,
            "255 = identity"
        );
        assert!(apply_envelope(base, 0).abs() < 1e-6, "0 = invisible");
        let mid_lo = apply_envelope(base, 64);
        let mid_hi = apply_envelope(base, 192);
        assert!(mid_lo > 0.0 && mid_lo < mid_hi && mid_hi < base);
    }

    // ---- axis_gap --------------------------------------------------------

    #[test]
    fn axis_gap_overlapping_intervals_yields_zero() {
        // [0, 10) overlaps [5, 15) → gap = 0
        assert!((axis_gap(0.0, 10.0, 5.0, 15.0)).abs() < 1e-6);
    }

    #[test]
    fn axis_gap_touching_intervals_yields_zero() {
        // [0, 10) touches [10, 20) → gap = 0 (half-open)
        assert!((axis_gap(0.0, 10.0, 10.0, 20.0)).abs() < 1e-6);
    }

    #[test]
    fn axis_gap_a_before_b_yields_positive_gap() {
        // [0, 5) then [10, 20) → gap = 10 - 5 = 5
        assert!((axis_gap(0.0, 5.0, 10.0, 20.0) - 5.0).abs() < 1e-6);
    }

    #[test]
    fn axis_gap_b_before_a_is_symmetric() {
        assert!((axis_gap(10.0, 20.0, 0.0, 5.0) - 5.0).abs() < 1e-6);
    }

    // ---- point_to_rect_distance ------------------------------------------

    #[test]
    fn point_inside_rect_yields_zero_distance() {
        // Rect (0,0)-(100,100), point (50,50) → distance ~0
        let d = point_to_rect_distance(50.0, 50.0, 0.0, 0.0, 100.0, 100.0);
        assert!(d < 2.0, "expected ~0 (inside), got {d}");
    }

    #[test]
    fn point_outside_rect_to_the_right() {
        // Rect (0,0)-(100,100), point (200,50) → horizontal gap = 100
        let d = point_to_rect_distance(200.0, 50.0, 0.0, 0.0, 100.0, 100.0);
        assert!((d - 100.0).abs() < 2.0, "expected ~100, got {d}");
    }

    #[test]
    fn point_outside_rect_diagonally() {
        // Rect (0,0)-(100,100), point (200,200) → distance ~ sqrt(100^2 + 100^2) ≈ 141.42
        let d = point_to_rect_distance(200.0, 200.0, 0.0, 0.0, 100.0, 100.0);
        let expected = (100.0_f32).hypot(100.0);
        assert!((d - expected).abs() < 2.0, "expected ~{expected}, got {d}");
    }

    // ---- slit_range ------------------------------------------------------

    #[test]
    fn slit_range_centered_around_zero_with_even_thickness() {
        let t = crate::color::Thickness::try_new(28).unwrap();
        let (lo, hi) = slit_range(0.0, t);
        assert!((hi - lo - 28.0).abs() < 1e-6);
        // half = 14, extra = 14 → symmetric
        assert!((lo + 14.0).abs() < 1e-6);
        assert!((hi - 14.0).abs() < 1e-6);
    }

    /// The point is treated as a 1px-wide rect via `x + 1.0`: rect
    /// (2,0)-(100,100), point (0,50) gives dx ~ 1.0.
    #[test]
    fn point_to_rect_distance_pins_x_plus_one_unit_offset() {
        let d = point_to_rect_distance(0.0, 50.0, 2.0, 0.0, 100.0, 100.0);
        assert!(
            (d - 1.0).abs() < 0.01,
            "expected dx ≈ 1.0 (1px gap), got {d}"
        );
    }

    /// Pins the `y + 1.0` term: rect (0,2)-(100,100), point (50,0) gives
    /// dy ~ 1.0.
    #[test]
    fn point_to_rect_distance_pins_y_plus_one_unit_offset() {
        let d = point_to_rect_distance(50.0, 0.0, 0.0, 2.0, 100.0, 100.0);
        assert!(
            (d - 1.0).abs() < 0.01,
            "expected dy ≈ 1.0 (1px gap), got {d}"
        );
    }

    /// Pins the fade curve at one decay distance: in `Off` mode, a cursor
    /// ~120px from the HUD gives linear ~0.629, smooth ~0.811.
    #[test]
    fn compute_opacity_pins_fade_curve_at_one_decay_distance() {
        // hud_rect = (1376, 24, 520, 560): left = 1376, top = 24, right = 1896, bottom = 584.
        // cursor (1256, 50) is 120px left of the HUD left edge, so distance ~ 119.
        let s = State {
            mode: Mode::Off,
            ..State::DEFAULT
        };
        let v = compute_opacity(s, Point::new(1256, 50), hud_rect(), 120.0);
        // 1 - exp(-119/120) ~ 0.629, smooth(0.629) ~ 0.811.
        assert!(
            v > 0.70 && v < 0.90,
            "expected opacity ≈ 0.81 (one decay distance), got {v}"
        );
    }
}
