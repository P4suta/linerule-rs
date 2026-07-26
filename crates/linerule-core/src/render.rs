//! Pure renderer: state + cursor + monitor bounds into an [`OverlayFrame`].
//! No I/O, no platform calls. [`frame`] is the only entry point.

pub mod hud_frame;
mod overlay_frame;

pub use hud_frame::{
    HudFontKey, HudFrame, HudNotification, HudRow, HudRule, HudTelemetry, HudTier,
    NotificationClass, hud_frame,
};
pub use overlay_frame::{Brush, Geometry, Layer, OverlayFrame};

use serde::Serialize;

use crate::{
    anim::Lerp,
    color::{Rgba, perceptual},
    config::OverlayConfig,
    geometry::{Logical, Point, ScreenRect},
    state::{Mode, SurroundEffect},
};

// No corner mode indicator: the HUD chip (`crate::render::hud_frame::HudTier::Chip`)
// already shows the mode letter + values in that corner.

/// Per-tick interpolated render inputs from the tick pipeline (`crate::input::tick`).
///
/// Integers only so the carrying `TickEffect` stays `Eq + Hash`. Settled
/// (`OverlaySample::settled`) renders byte-identically to rendering straight from
/// the config.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub struct OverlaySample {
    /// Master envelope `0..=255`: show/hide / mode-switch fade, `255` = fully
    /// shown. Applied perceptually ([`perceptual::smooth`]) to all alpha.
    pub master: u8,
    /// Slit thickness in logical px (glides during bumps).
    pub thickness_px: u16,
    /// Mask opacity byte, pre-perceptual ([`crate::color::Opacity::get`] domain).
    pub mask_alpha: u8,
    /// Style crossfade `0..=255`: `0` = dim mask color, `255` = white wash.
    /// Settled at [`SurroundEffect::mix_target`].
    pub style_mix: u8,
}

impl OverlaySample {
    /// Sample with every channel settled at the config's values.
    #[must_use]
    pub const fn settled(config: OverlayConfig) -> Self {
        Self {
            master: u8::MAX,
            thickness_px: config.thickness.get(),
            mask_alpha: config.opacity.get(),
            style_mix: config.effect.mix_target(),
        }
    }
}

/// Build the frame for the current tick.
///
/// `cursor` and `monitor` (the rect of the screen the cursor is on) are in
/// logical pixels. Takes `mode` + `config` (mirroring the `DrawOverlay`
/// payload) rather than a full `State`. `sample` carries the interpolated
/// per-tick values.
///
/// # Examples
///
/// `Mode::Off` (the default) renders nothing:
///
/// ```
/// use linerule_core::{frame, Mode, OverlayConfig, OverlaySample, Point, ScreenRect};
/// let monitor = ScreenRect::new(Point::new(0, 0), 1920, 1080);
/// let sample = OverlaySample::settled(OverlayConfig::DEFAULT);
/// let out = frame(Mode::Off, OverlayConfig::DEFAULT, Point::new(0, 0), monitor, sample);
/// assert!(out.is_empty());
/// ```
///
/// In an active mode, the frame has two dim halves (cursor mid-screen):
///
/// ```
/// use linerule_core::{frame, Mode, OverlayConfig, OverlaySample, Point, ScreenRect};
/// let monitor = ScreenRect::new(Point::new(0, 0), 1920, 1080);
/// let sample = OverlaySample::settled(OverlayConfig::DEFAULT);
/// let out = frame(Mode::Horizontal, OverlayConfig::DEFAULT, Point::new(960, 540), monitor, sample);
/// assert_eq!(out.layer_count(), 2);
/// ```
#[must_use]
#[inline]
pub fn frame(
    mode: Mode,
    config: OverlayConfig,
    cursor: Point<Logical>,
    monitor: ScreenRect<Logical>,
    sample: OverlaySample,
) -> OverlayFrame {
    match mode {
        Mode::Off => OverlayFrame::EMPTY,
        Mode::Horizontal => slit_frame(Axis::Horizontal, cursor, monitor, config, sample),
        Mode::Vertical => slit_frame(Axis::Vertical, cursor, monitor, config, sample),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Axis {
    Horizontal,
    Vertical,
}

fn slit_frame(
    axis: Axis,
    cursor: Point<Logical>,
    monitor: ScreenRect<Logical>,
    config: OverlayConfig,
    sample: OverlaySample,
) -> OverlayFrame {
    let brush = surround_brush(config, sample);
    let thickness = i32::from(sample.thickness_px);
    let (before, after) = split_around(axis_value(axis, cursor), thickness);

    OverlayFrame::from_slit(axis, monitor, before, after, brush)
}

/// Brush for the surround bands: `Solid` for dim/white-wash, `Blur` for blur.
///
/// Flat effects: `style_mix` crossfades base RGB between dim mask and white
/// wash; alpha is the opacity byte scaled by the master envelope. Blur carries
/// no tint, only the σ amount.
fn surround_brush(config: OverlayConfig, sample: OverlaySample) -> Brush {
    if config.effect.is_blur() {
        Brush::Blur {
            amount: config.blur,
            opacity: sample.master,
        }
    } else {
        let base = mix_rgb(
            SurroundEffect::DimBlack.mask_color(),
            SurroundEffect::WhiteWash.mask_color(),
            sample.style_mix,
        );
        Brush::Solid(base.with_alpha(composite_alpha(sample.mask_alpha, sample.master)))
    }
}

/// Per-channel RGB crossfade by `mix ∈ 0..=255` (alpha is set by the caller).
fn mix_rgb(from: Rgba, to: Rgba, mix: u8) -> Rgba {
    let t = f32::from(mix) / 255.0;
    Rgba::new(
        u8::lerp(from.r, to.r, t),
        u8::lerp(from.g, to.g, t),
        u8::lerp(from.b, to.b, t),
        0,
    )
}

/// Perceptual alpha composition: opacity byte through the CIE L\* curve (like
/// [`crate::color::Opacity::to_perceptual_byte`]), then the master envelope
/// via the gamma curve. At `master == 255` the envelope is exactly `1.0`, so
/// the result is byte-identical to `to_perceptual_byte()`.
fn composite_alpha(mask_alpha: u8, master: u8) -> u8 {
    let linear = f32::from(mask_alpha) / 255.0;
    let envelope = perceptual::smooth(f32::from(master) / 255.0);
    let scaled = (perceptual::l_star(linear) * envelope * 255.0)
        .clamp(0.0, 255.0)
        .round();
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "scaled is finite and clamped to [0, 255]"
    )]
    let byte = scaled as u8;
    byte
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

/// Clipped rectangle from `(left, top, right, bottom)`; `None` when width or
/// height clips to zero.
pub(crate) fn band(left: i32, top: i32, right: i32, bottom: i32) -> Option<ScreenRect<Logical>> {
    let width = u32::try_from((right - left).max(0)).ok()?;
    let height = u32::try_from((bottom - top).max(0)).ok()?;
    if width == 0 || height == 0 {
        return None;
    }
    Some(ScreenRect::new(Point::new(left, top), width, height))
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use super::*;

    fn monitor() -> ScreenRect<Logical> {
        ScreenRect::new(Point::new(0, 0), 1920, 1080)
    }

    /// Calls `frame` with a settled sample.
    fn settled_frame(mode: Mode, config: OverlayConfig, cursor: Point<Logical>) -> OverlayFrame {
        frame(
            mode,
            config,
            cursor,
            monitor(),
            OverlaySample::settled(config),
        )
    }

    fn first_layer(frame: OverlayFrame) -> Layer {
        frame.layers().next().expect("non-empty frame")
    }

    #[test]
    fn off_mode_emits_empty_frame() {
        let f = settled_frame(Mode::Off, OverlayConfig::DEFAULT, Point::new(0, 0));
        assert!(f.is_empty());
    }

    #[test]
    fn horizontal_mode_emits_two_dim_layers() {
        let f = settled_frame(
            Mode::Horizontal,
            OverlayConfig::DEFAULT,
            Point::new(960, 540),
        );
        assert_eq!(f.layer_count(), 2);
    }

    #[test]
    fn horizontal_layers_cover_full_width() {
        let f = settled_frame(
            Mode::Horizontal,
            OverlayConfig::DEFAULT,
            Point::new(960, 540),
        );
        let bands = f
            .layers()
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
        let f = settled_frame(Mode::Horizontal, OverlayConfig::DEFAULT, Point::new(960, 0));
        assert!(f.layer_count() <= 2);
    }

    // ---- surround effect (DimBlack / WhiteWash / Blur) --------------------

    fn surround_color_of(mode: Mode, effect: SurroundEffect) -> Rgba {
        let config = OverlayConfig {
            effect,
            ..OverlayConfig::DEFAULT
        };
        let f = settled_frame(mode, config, Point::new(960, 540));
        // First layer is a surround half (cursor centered).
        match first_layer(f).brush {
            Brush::Solid(c) => c,
            Brush::Blur { .. } => panic!("surround must be a solid brush in flat effects"),
        }
    }

    #[test]
    fn dim_black_effect_keeps_the_dark_mask_color() {
        let c = surround_color_of(Mode::Horizontal, SurroundEffect::DimBlack);
        assert_eq!((c.r, c.g, c.b), (0x00, 0x00, 0x00));
        assert!(c.a > 0);
    }

    #[test]
    fn white_wash_effect_washes_the_surround_white() {
        let c = surround_color_of(Mode::Horizontal, SurroundEffect::WhiteWash);
        assert_eq!((c.r, c.g, c.b), (0xFF, 0xFF, 0xFF));
        assert!(c.a > 0, "white wash keeps the opacity-derived alpha");
    }

    #[test]
    fn dim_and_white_wash_surrounds_differ() {
        let dim = surround_color_of(Mode::Horizontal, SurroundEffect::DimBlack);
        let wash = surround_color_of(Mode::Horizontal, SurroundEffect::WhiteWash);
        assert_ne!(
            dim, wash,
            "the surround fill must visibly change between effects"
        );
    }

    #[test]
    fn blur_effect_uses_blur_brush_for_every_surround_band() {
        let config = OverlayConfig {
            effect: SurroundEffect::Blur,
            ..OverlayConfig::DEFAULT
        };
        let f = settled_frame(Mode::Horizontal, config, Point::new(960, 540));
        assert!(!f.is_empty());
        for layer in f.layers() {
            assert!(
                matches!(layer.brush, Brush::Blur { .. }),
                "blur surround must use Brush::Blur, got {:?}",
                layer.brush
            );
        }
    }

    #[test]
    fn opacity_does_not_affect_the_blur_brush() {
        // Blur carries no tint, so the surround brush is byte-identical at MIN
        // and MAX opacity.
        let blur_brush_at = |opacity| {
            let config = OverlayConfig {
                effect: SurroundEffect::Blur,
                opacity,
                ..OverlayConfig::DEFAULT
            };
            first_layer(settled_frame(
                Mode::Horizontal,
                config,
                Point::new(960, 540),
            ))
            .brush
        };
        assert_eq!(
            blur_brush_at(crate::color::Opacity::MAX),
            blur_brush_at(crate::color::Opacity::MIN)
        );
    }

    #[test]
    fn blur_brush_carries_the_config_blur_amount() {
        use crate::color::BlurAmount;
        let amount = BlurAmount::DEFAULT.saturating_add(8);
        let config = OverlayConfig {
            effect: SurroundEffect::Blur,
            blur: amount,
            ..OverlayConfig::DEFAULT
        };
        let Brush::Blur { amount: got, .. } = first_layer(settled_frame(
            Mode::Horizontal,
            config,
            Point::new(960, 540),
        ))
        .brush
        else {
            panic!("blur surround must be Brush::Blur");
        };
        assert_eq!(
            got, amount,
            "surround_brush must thread config.blur into the brush"
        );
    }

    /// The master envelope reaches the blur brush as its opacity, so show/hide
    /// fades work under Blur without rebuilding the sprite pool.
    #[test]
    fn blur_brush_carries_the_master_envelope_as_opacity() {
        let config = OverlayConfig {
            effect: SurroundEffect::Blur,
            ..OverlayConfig::DEFAULT
        };
        let mut sample = OverlaySample::settled(config);
        sample.master = 0x40;
        let f = frame(
            Mode::Horizontal,
            config,
            Point::new(960, 540),
            monitor(),
            sample,
        );
        let Brush::Blur { opacity, .. } = first_layer(f).brush else {
            panic!("blur surround must be Brush::Blur");
        };
        assert_eq!(opacity, 0x40, "sample.master must reach Brush::Blur");
        let settled = settled_frame(Mode::Horizontal, config, Point::new(960, 540));
        let Brush::Blur { opacity, .. } = first_layer(settled).brush else {
            panic!("blur surround must be Brush::Blur");
        };
        assert_eq!(opacity, u8::MAX, "settled blur is fully shown");
    }

    // ---- Vertical mode ---------------------------------------------------

    #[test]
    fn vertical_mode_emits_two_dim_layers() {
        let f = settled_frame(Mode::Vertical, OverlayConfig::DEFAULT, Point::new(960, 540));
        assert_eq!(f.layer_count(), 2);
    }

    #[test]
    fn vertical_layers_cover_full_height() {
        let f = settled_frame(Mode::Vertical, OverlayConfig::DEFAULT, Point::new(960, 540));
        let bands = f
            .layers()
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
        let f = settled_frame(Mode::Vertical, OverlayConfig::DEFAULT, Point::new(0, 540));
        // The left dim band collapses (zero width), leaving the right dim.
        assert!(f.layer_count() <= 2);
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
        // Center can go below zero on DPI/wrap-around edges; (hi - lo) == thickness.
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

    // ---- OverlaySample (transition channels) ------------------------------

    /// Settled-sample mask alpha (`master = 255`) is byte-identical to
    /// `Opacity::to_perceptual_byte()`; catches 1-bit drift from transitions.
    #[test]
    fn settled_sample_alpha_is_byte_identical_to_perceptual_byte() {
        for byte in [1_u8, 0x40, 0x80, 0xAA, 0xFF] {
            let expected = crate::color::Opacity::try_new(byte)
                .expect("non-zero")
                .to_perceptual_byte();
            assert_eq!(
                composite_alpha(byte, u8::MAX),
                expected,
                "composite_alpha({byte:#04x}, 255) must equal to_perceptual_byte()"
            );
        }
    }

    /// Thickness comes from the sample, not the config (mid-glide
    /// intermediate values become the slit width directly).
    #[test]
    fn sample_thickness_overrides_config() {
        let config = OverlayConfig::DEFAULT; // thickness = 28
        let sample = OverlaySample {
            thickness_px: 100,
            ..OverlaySample::settled(config)
        };
        let f = frame(
            Mode::Horizontal,
            config,
            Point::new(960, 540),
            monitor(),
            sample,
        );
        let bands: Vec<_> = f
            .layers()
            .map(|l| match l.geometry {
                Geometry::Rect(r) => r,
            })
            .collect();
        // Gap between the top/bottom bands (the slit) = sample.thickness_px
        let gap = bands[1].top() - bands[0].bottom();
        assert_eq!(gap, 100, "slit width must come from the sample");
    }

    /// At an intermediate `style_mix` the surround is a color between Dim and
    /// Bright.
    #[test]
    fn midway_style_mix_blends_between_dim_and_bright() {
        let config = OverlayConfig::DEFAULT; // mask_color = black
        let sample = OverlaySample {
            style_mix: 128,
            ..OverlaySample::settled(config)
        };
        let f = frame(
            Mode::Horizontal,
            config,
            Point::new(960, 540),
            monitor(),
            sample,
        );
        let c = match first_layer(f).brush {
            Brush::Solid(c) => c,
            Brush::Blur { .. } => panic!("surround must be solid"),
        };
        assert!(
            c.r > 0x40 && c.r < 0xC0,
            "midway mix should be grey-ish, got r={:#04x}",
            c.r
        );
        assert_eq!((c.r, c.g), (c.g, c.b), "blend stays achromatic");
    }

    /// At `master = 0` (fully faded out) every layer's alpha is 0.
    #[test]
    fn master_zero_yields_fully_transparent_layers() {
        let config = OverlayConfig::DEFAULT;
        let sample = OverlaySample {
            master: 0,
            ..OverlaySample::settled(config)
        };
        let f = frame(
            Mode::Horizontal,
            config,
            Point::new(960, 540),
            monitor(),
            sample,
        );
        for layer in f.layers() {
            match layer.brush {
                Brush::Solid(c) => assert_eq!(c.a, 0, "master=0 must zero all alpha"),
                Brush::Blur { .. } => panic!("solid brushes only"),
            }
        }
    }
}
#[test]
fn master_envelope_reduces_partial_mask_alpha() {
    let partial = composite_alpha(128, 128);
    let full = composite_alpha(128, u8::MAX);
    assert!(partial > 0);
    assert!(partial < full, "partial={partial}, full={full}");
}
