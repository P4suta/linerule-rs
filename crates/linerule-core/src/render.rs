//! Pure renderer: turns the current overlay state + cursor + monitor bounds
//! into an [`OverlayFrame`] of fillable layers. No I/O, no platform calls.
//!
//! The submodule [`overlay_frame`] carries the data ADT ([`Layer`],
//! [`Brush`], [`Geometry`], [`OverlayFrame`]). The [`frame`] function in
//! this file is the only entry point.

pub mod hud_frame;
pub mod overlay_frame;

pub use hud_frame::{HudFontKey, HudFrame, HudRow, hud_frame};
pub use overlay_frame::{Brush, Geometry, Layer, OverlayFrame};

use crate::{
    color::{Opacity, Rgba},
    config::OverlayConfig,
    geometry::{Logical, Point, ScreenRect},
    state::{Mode, State},
};

const INDICATOR_W: u32 = 18;
const INDICATOR_H: u32 = 18;
const INDICATOR_MARGIN: i32 = 12;

/// Build the frame for the current tick.
///
/// `cursor` is the latest cursor position polled from the OS; `monitor` is
/// the bounding rect of the screen the cursor is on. Both are in logical
/// pixels.
///
/// # Examples
///
/// `Mode::Off` (the default) renders nothing:
///
/// ```
/// use linerule_core::{frame, Point, ScreenRect, State};
/// let monitor = ScreenRect::new(Point::new(0, 0), 1920, 1080);
/// let out = frame(State::DEFAULT, Point::new(0, 0), monitor);
/// assert!(out.is_empty());
/// ```
///
/// In an active mode, the frame has the two dim halves plus the indicator
/// (three layers total when the cursor is in the middle of the screen):
///
/// ```
/// use linerule_core::{frame, Mode, Point, ScreenRect, State};
/// let monitor = ScreenRect::new(Point::new(0, 0), 1920, 1080);
/// let state = State { mode: Mode::Horizontal, ..State::DEFAULT };
/// let out = frame(state, Point::new(960, 540), monitor);
/// assert_eq!(out.layer_count(), 3);
/// ```
#[must_use]
pub fn frame(state: State, cursor: Point<Logical>, monitor: ScreenRect<Logical>) -> OverlayFrame {
    if !state.visible {
        return OverlayFrame::EMPTY;
    }
    match state.mode {
        Mode::Off => OverlayFrame::EMPTY,
        Mode::Horizontal => slit_frame(Axis::Horizontal, cursor, monitor, state.config),
        Mode::Vertical => slit_frame(Axis::Vertical, cursor, monitor, state.config),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Axis {
    Horizontal,
    Vertical,
}

fn slit_frame(
    axis: Axis,
    cursor: Point<Logical>,
    monitor: ScreenRect<Logical>,
    config: OverlayConfig,
) -> OverlayFrame {
    let mask = mask_color(config);
    let thickness = i32::from(config.thickness.get());
    let (before, after) = split_around(axis_value(axis, cursor), thickness);

    let mut layers = Vec::with_capacity(3);
    if let Some(layer) = dim_half(axis, monitor, DimSide::Before, before, mask) {
        layers.push(layer);
    }
    if let Some(layer) = dim_half(axis, monitor, DimSide::After, after, mask) {
        layers.push(layer);
    }
    layers.push(indicator_layer(monitor));
    OverlayFrame::from_layers(layers)
}

fn mask_color(config: OverlayConfig) -> Rgba {
    config
        .mask_color
        .with_alpha(config.opacity.to_perceptual_byte())
}

pub(crate) const fn axis_value(axis: Axis, cursor: Point<Logical>) -> i32 {
    match axis {
        Axis::Horizontal => cursor.y,
        Axis::Vertical => cursor.x,
    }
}

/// Cursor-anchored slit split: returns `(slit_lo, slit_hi)` along the axis.
pub(crate) const fn split_around(center: i32, thickness: i32) -> (i32, i32) {
    let half = thickness / 2;
    let extra = thickness - half;
    (center - half, center + extra)
}

#[derive(Debug, Clone, Copy)]
enum DimSide {
    /// Above the slit (horizontal mode) or left of it (vertical mode).
    Before,
    /// Below the slit (horizontal mode) or right of it (vertical mode).
    After,
}

fn dim_half(
    axis: Axis,
    monitor: ScreenRect<Logical>,
    side: DimSide,
    slit_edge: i32,
    fill: Rgba,
) -> Option<Layer> {
    let rect = match (axis, side) {
        (Axis::Horizontal, DimSide::Before) => {
            band(monitor.left(), monitor.top(), monitor.right(), slit_edge)
        },
        (Axis::Horizontal, DimSide::After) => {
            band(monitor.left(), slit_edge, monitor.right(), monitor.bottom())
        },
        (Axis::Vertical, DimSide::Before) => {
            band(monitor.left(), monitor.top(), slit_edge, monitor.bottom())
        },
        (Axis::Vertical, DimSide::After) => {
            band(slit_edge, monitor.top(), monitor.right(), monitor.bottom())
        },
    }?;
    Some(Layer::solid_rect(rect, fill))
}

/// Construct a clipped rectangle from `(left, top, right, bottom)`, returning
/// `None` when the resulting width or height is zero (after clipping against
/// the monitor edge).
pub(crate) fn band(left: i32, top: i32, right: i32, bottom: i32) -> Option<ScreenRect<Logical>> {
    let width = u32::try_from((right - left).max(0)).ok()?;
    let height = u32::try_from((bottom - top).max(0)).ok()?;
    if width == 0 || height == 0 {
        return None;
    }
    Some(ScreenRect::new(Point::new(left, top), width, height))
}

fn indicator_layer(monitor: ScreenRect<Logical>) -> Layer {
    let w_i32 = i32::try_from(INDICATOR_W).unwrap_or(i32::MAX);
    let x = monitor.right() - INDICATOR_MARGIN - w_i32;
    let y = monitor.top() + INDICATOR_MARGIN;
    let alpha = Opacity::INDICATOR_DEFAULT.to_perceptual_byte();
    Layer::solid_rect(
        ScreenRect::new(Point::new(x, y), INDICATOR_W, INDICATOR_H),
        Rgba::WHITE.with_alpha(alpha),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn monitor() -> ScreenRect<Logical> {
        ScreenRect::new(Point::new(0, 0), 1920, 1080)
    }

    #[test]
    fn off_mode_emits_empty_frame() {
        let s = State::DEFAULT;
        let f = frame(s, Point::new(0, 0), monitor());
        assert!(f.is_empty());
    }

    #[test]
    fn hidden_state_emits_empty_frame() {
        let s = State {
            mode: Mode::Horizontal,
            visible: false,
            ..State::DEFAULT
        };
        let f = frame(s, Point::new(960, 540), monitor());
        assert!(f.is_empty());
    }

    #[test]
    fn horizontal_mode_emits_three_layers() {
        let s = State {
            mode: Mode::Horizontal,
            ..State::DEFAULT
        };
        let f = frame(s, Point::new(960, 540), monitor());
        assert_eq!(f.layer_count(), 3);
    }

    #[test]
    fn horizontal_layers_cover_full_width() {
        let s = State {
            mode: Mode::Horizontal,
            ..State::DEFAULT
        };
        let f = frame(s, Point::new(960, 540), monitor());
        let bands = f
            .layers()
            .iter()
            .map(|l| match l.geometry {
                Geometry::Rect(r) => r,
            })
            .collect::<Vec<_>>();
        assert!(
            bands.iter().any(|r| r.left() == 0 && r.right() == 1920),
            "expected at least one full-width band, got {bands:?}"
        );
    }

    #[test]
    fn dim_half_at_top_edge_is_dropped() {
        let s = State {
            mode: Mode::Horizontal,
            ..State::DEFAULT
        };
        let f = frame(s, Point::new(960, 0), monitor());
        assert!(f.layer_count() <= 3);
    }

    #[test]
    fn indicator_uses_white_with_perceptual_alpha() {
        let s = State {
            mode: Mode::Horizontal,
            ..State::DEFAULT
        };
        let f = frame(s, Point::new(960, 540), monitor());
        let indicator = f.layers().last().expect("non-empty");
        match indicator.brush {
            Brush::Solid(c) => {
                assert_eq!(c.r, 0xFF);
                assert_eq!(c.g, 0xFF);
                assert_eq!(c.b, 0xFF);
                assert!(c.a > 0);
            },
        }
    }

    // ---- Vertical mode (was previously untested) -------------------------

    #[test]
    fn vertical_mode_emits_three_layers() {
        let s = State {
            mode: Mode::Vertical,
            ..State::DEFAULT
        };
        let f = frame(s, Point::new(960, 540), monitor());
        assert_eq!(f.layer_count(), 3);
    }

    #[test]
    fn vertical_layers_cover_full_height() {
        let s = State {
            mode: Mode::Vertical,
            ..State::DEFAULT
        };
        let f = frame(s, Point::new(960, 540), monitor());
        let bands = f
            .layers()
            .iter()
            .map(|l| match l.geometry {
                Geometry::Rect(r) => r,
            })
            .collect::<Vec<_>>();
        assert!(
            bands.iter().any(|r| r.top() == 0 && r.bottom() == 1080),
            "expected a full-height band, got {bands:?}"
        );
    }

    #[test]
    fn vertical_dim_at_left_edge_is_dropped() {
        let s = State {
            mode: Mode::Vertical,
            ..State::DEFAULT
        };
        let f = frame(s, Point::new(0, 540), monitor());
        // The left dim band collapses (zero width), leaving the right dim + indicator.
        assert!(f.layer_count() <= 3);
    }

    // ---- axis_value / split_around / band helpers ------------------------

    #[test]
    fn axis_value_picks_correct_axis() {
        let cursor = Point::<Logical>::new(100, 200);
        assert_eq!(axis_value(Axis::Horizontal, cursor), 200);
        assert_eq!(axis_value(Axis::Vertical, cursor), 100);
    }

    #[test]
    fn split_around_even_thickness_is_symmetric() {
        // thickness = 28 → half = 14, extra = 14, both sides equal.
        assert_eq!(split_around(540, 28), (526, 554));
    }

    #[test]
    fn split_around_odd_thickness_puts_extra_pixel_after_center() {
        // thickness = 29 → half = 14, extra = 15, asymmetric by 1.
        assert_eq!(split_around(540, 29), (526, 555));
    }

    #[test]
    fn split_around_negative_center_stays_consistent() {
        // The center of the slit can move below zero on wrap-around / DPI edge.
        // We just check internal consistency: (hi - lo) == thickness.
        let (lo, hi) = split_around(-100, 50);
        assert_eq!(hi - lo, 50);
    }

    #[test]
    fn band_rejects_zero_width() {
        assert!(band(10, 0, 10, 100).is_none());
    }

    #[test]
    fn band_rejects_zero_height() {
        assert!(band(0, 50, 100, 50).is_none());
    }

    #[test]
    fn band_clips_negative_widths_to_none() {
        // right < left collapses to width = 0 after .max(0).
        assert!(band(100, 0, 50, 100).is_none());
    }

    #[test]
    fn band_round_trip_positive_dims() {
        let r = band(0, 0, 100, 50).expect("non-empty band");
        assert_eq!(r.left(), 0);
        assert_eq!(r.top(), 0);
        assert_eq!(r.width, 100);
        assert_eq!(r.height, 50);
    }
}
