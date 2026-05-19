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
/// - When the overlay is not visible, the HUD shows in full (`1.0`).
/// - When the slit is far from the HUD bounds, opacity approaches `1.0`.
/// - When the slit overlaps the HUD bounds, opacity is `0.0`.
#[must_use]
pub fn compute_opacity(
    state: State,
    cursor: Point<Logical>,
    hud: ScreenRect<Logical>,
    fade_decay_px: f32,
) -> f32 {
    if !state.visible {
        return 1.0;
    }
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

fn slit_range(center: f32, thickness: Thickness) -> (f32, f32) {
    let t = px(i32::from(thickness.get()));
    let half = t * 0.5;
    (center - half, center + (t - half))
}

/// 1-D interval gap between half-open intervals `[a_lo, a_hi)` and
/// `[b_lo, b_hi)`. Returns `0` when they overlap.
fn axis_gap(a_lo: f32, a_hi: f32, b_lo: f32, b_hi: f32) -> f32 {
    (b_lo - a_hi).max(a_lo - b_hi).max(0.0)
}

/// 2-D point-to-rect distance. Returns `0` when the point lies inside the
/// rectangle.
fn point_to_rect_distance(x: f32, y: f32, rl: f32, rt: f32, rr: f32, rb: f32) -> f32 {
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
        State {
            mode: Mode::Horizontal,
            ..State::DEFAULT
        }
    }

    #[test]
    fn hidden_state_yields_full_opacity() {
        let s = State {
            visible: false,
            ..h_state()
        };
        let v = compute_opacity(s, Point::new(0, 0), hud_rect(), 120.0);
        assert!((v - 1.0).abs() < 1e-6);
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
}
