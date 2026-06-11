//! User-facing configuration. Every tunable is a compile-time constant,
//! exposed as `const DEFAULT: Self`. There is no file parser, no environment
//! lookup; reconfiguration means recompiling.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::color::{BlurAmount, Opacity, Rgba, Thickness};
use crate::state::SurroundEffect;

/// Surround effect + thickness + opacity + blur amount. Composed into a
/// [`crate::state::State`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OverlayConfig {
    /// Treatment of the region around the slit (mask color is derived from it).
    pub effect: SurroundEffect,
    /// Slit width in logical pixels.
    pub thickness: Thickness,
    /// Mask opacity (perceptual-mapped on output). Inert under the `Blur`
    /// effect — see [`blur`](Self::blur).
    pub opacity: Opacity,
    /// Backdrop-blur amount (Gaussian σ, logical px). Only meaningful under the
    /// `Blur` effect, where the opacity hotkeys retarget onto it.
    pub blur: BlurAmount,
}

impl OverlayConfig {
    /// Default surround: `DimBlack` × `Thickness::DEFAULT` × `Opacity::DEFAULT`
    /// × `BlurAmount::DEFAULT`.
    pub const DEFAULT: Self = Self {
        effect: SurroundEffect::DimBlack,
        thickness: Thickness::DEFAULT,
        opacity: Opacity::DEFAULT,
        blur: BlurAmount::DEFAULT,
    };
}

impl Default for OverlayConfig {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Granularity of a single tap step. The values generalize for future
/// continuous controls; today they're just the bump magnitudes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TapStepConfig {
    /// Pixels per `BumpThickness` tap.
    pub thickness: i32,
    /// Bytes per `BumpOpacity` tap.
    pub opacity: i32,
}

impl TapStepConfig {
    /// Default tap step (`thickness = 8 px`, `opacity = 8`).
    pub const DEFAULT: Self = Self {
        thickness: 8,
        opacity: 8,
    };
}

impl Default for TapStepConfig {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Hold-to-repeat timing parameters consumed by
/// [`crate::input::hold::step`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RepeatConfig {
    /// Delay before the first repeat fires after the initial press.
    pub initial_delay: Duration,
    /// Hold time beyond which `ToggleOnOff` is treated as a long-press undo.
    pub long_press_threshold: Duration,
    /// Steady interval for the `Slow` cadence.
    pub slow_repeat_interval: Duration,
    /// Polling interval used while in `AwaitingRelease`.
    pub release_poll: Duration,
}

impl RepeatConfig {
    /// Default timings tuned for comfortable text-row tracking.
    pub const DEFAULT: Self = Self {
        initial_delay: Duration::from_millis(250),
        long_press_threshold: Duration::from_millis(250),
        slow_repeat_interval: Duration::from_millis(400),
        release_poll: Duration::from_millis(50),
    };
}

impl Default for RepeatConfig {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Aggregated input timing config.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct InputConfig {
    /// Per-tap magnitudes.
    pub tap_step: TapStepConfig,
    /// Hold-to-repeat timings.
    pub repeat: RepeatConfig,
}

impl InputConfig {
    /// Default tap-step × default repeat.
    pub const DEFAULT: Self = Self {
        tap_step: TapStepConfig::DEFAULT,
        repeat: RepeatConfig::DEFAULT,
    };
}

impl Default for InputConfig {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Render-budget tunables.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RenderConfig {
    /// Fraction of the frame budget above which a warning is logged.
    pub warn_ratio: f64,
    /// Fallback refresh rate (Hz) when display probing fails.
    pub fallback_refresh_hz: i32,
}

impl RenderConfig {
    /// Default render budget (`warn_ratio = 0.8`, `fallback_refresh_hz = 60`).
    pub const DEFAULT: Self = Self {
        warn_ratio: 0.8,
        fallback_refresh_hz: 60,
    };
}

impl Default for RenderConfig {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// HUD bounding box in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct HudGeometry {
    /// HUD panel width.
    pub width: f32,
    /// HUD panel height.
    pub height: f32,
    /// Margin from the screen edge.
    pub margin: f32,
}

impl HudGeometry {
    /// Default HUD bounds (`520 × 560` panel with `24 px` margin).
    pub const DEFAULT: Self = Self {
        width: 520.0,
        height: 560.0,
        margin: 24.0,
    };
}

impl Default for HudGeometry {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Padding inside the HUD panel (logical pixels).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct HudPadding {
    /// Padding between content and the panel edge.
    pub edge: f32,
    /// Padding between major sections.
    pub section: f32,
    /// Padding between rows of text.
    pub row: f32,
}

impl HudPadding {
    /// Default padding (`edge = 24`, `section = 16`, `row = 8`).
    pub const DEFAULT: Self = Self {
        edge: 24.0,
        section: 16.0,
        row: 8.0,
    };
}

impl Default for HudPadding {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// HUD font sizes (logical points) and font families.
//
// `Deserialize` is omitted because `&'static str` fields cannot satisfy
// `Deserialize<'de>` for arbitrary `'de`. Compile-time const only.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct HudFonts {
    /// Title row size.
    pub title: f32,
    /// Status row size.
    pub status: f32,
    /// Body text size.
    pub body: f32,
    /// Telemetry footer size.
    pub telemetry: f32,
    /// Proportional family used for titles/body.
    pub title_family: &'static str,
    /// Monospace family used for telemetry.
    pub mono_family: &'static str,
}

impl HudFonts {
    /// Default font sizes and families (Segoe UI + Cascadia Mono).
    pub const DEFAULT: Self = Self {
        title: 24.0,
        status: 22.0,
        body: 20.0,
        telemetry: 18.0,
        title_family: "Segoe UI",
        mono_family: "Cascadia Mono",
    };
}

impl Default for HudFonts {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// HUD palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct HudColors {
    /// Panel background.
    pub background: Rgba,
    /// Foreground text.
    pub foreground: Rgba,
    /// Subtle / muted text.
    pub subtle: Rgba,
    /// Accent (interactive emphasis).
    pub accent: Rgba,
    /// Hint / warning emphasis.
    pub hint: Rgba,
    /// Divider rule.
    pub divider: Rgba,
}

impl HudColors {
    /// Default dark palette.
    ///
    /// `background.alpha` is `0xEB` (≈ 92%): a slight translucency lets the
    /// desktop / overlay mask breathe through the panel (Fluent-style acrylic
    /// feel) while staying dark enough that text contrast is unaffected. The
    /// HUD sits on top of the overlay mask in composition z-order, so the mask
    /// darkens the panel marginally when active — intended. Per-frame fade is
    /// still applied via [`HudConfig::base_opacity`] / `compute_opacity` on
    /// top of this.
    pub const DEFAULT: Self = Self {
        background: Rgba::new(0x10, 0x12, 0x18, 0xEB),
        foreground: Rgba::new(0xE6, 0xE9, 0xEF, 0xFF),
        subtle: Rgba::new(0x9A, 0xA0, 0xAE, 0xFF),
        accent: Rgba::new(0x6C, 0x9F, 0xFF, 0xFF),
        hint: Rgba::new(0xFF, 0xC1, 0x6C, 0xFF),
        divider: Rgba::new(0x2A, 0x2E, 0x38, 0xFF),
    };
}

impl Default for HudColors {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Geometry of the persistent status chip (the default, low-key HUD tier).
///
/// The chip is a one-line mono status (`H · 28px · 67%`) anchored top-right;
/// the full guide panel only appears at startup or via the HUD-detail hotkey.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct HudChip {
    /// Chip text size (logical pt, mono family).
    pub font_size: f32,
    /// Horizontal padding around the text.
    pub pad_x: f32,
    /// Vertical padding around the text.
    pub pad_y: f32,
}

impl HudChip {
    /// Default chip metrics.
    pub const DEFAULT: Self = Self {
        font_size: 13.0,
        pad_x: 10.0,
        pad_y: 6.0,
    };
}

impl Default for HudChip {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// HUD configuration root.
//
// `Deserialize` is omitted; see [`HudFonts`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct HudConfig {
    /// Base HUD opacity (0–1).
    pub base_opacity: f32,
    /// Distance (logical pixels) at which the HUD fades by `1 - 1/e`.
    pub fade_decay_px: f32,
    /// Panel corner radius (logical pixels, Fluent-style rounding).
    pub corner_radius: f32,
    /// Interval for refreshing telemetry rows.
    pub telemetry_refresh: Duration,
    /// HUD bounding rectangle (full tier).
    pub geometry: HudGeometry,
    /// HUD panel padding.
    pub padding: HudPadding,
    /// HUD font sizes/families.
    pub fonts: HudFonts,
    /// HUD palette.
    pub colors: HudColors,
    /// Persistent status chip metrics (chip tier).
    pub chip: HudChip,
}

impl HudConfig {
    /// Default HUD tunables.
    pub const DEFAULT: Self = Self {
        base_opacity: 0.875,
        fade_decay_px: 120.0,
        corner_radius: 8.0,
        telemetry_refresh: Duration::from_millis(200),
        geometry: HudGeometry::DEFAULT,
        padding: HudPadding::DEFAULT,
        fonts: HudFonts::DEFAULT,
        colors: HudColors::DEFAULT,
        chip: HudChip::DEFAULT,
    };
}

impl Default for HudConfig {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Transition timing tunables (milliseconds).
///
/// The design constraint is "fast, subtle, never sluggish": every duration is
/// a short ease-out glide well under 200 ms, so motion reads as
/// responsiveness rather than decoration. `0` disables a transition
/// (instant), which doubles as the CI / determinism escape hatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[allow(
    clippy::struct_field_names,
    reason = "the `_ms` suffix spells out the unit at every call site"
)]
pub struct AnimConfig {
    /// Show/hide, mode switch, and style crossfade duration.
    pub overlay_fade_ms: u16,
    /// Thickness / opacity bump glide duration (held keys retarget mid-glide,
    /// so repeats merge into one continuous motion).
    pub value_glide_ms: u16,
    /// HUD chip ⇄ full presentation swap fade duration.
    pub hud_swap_ms: u16,
    /// How long the full HUD (hotkey guide) stays up after startup before
    /// collapsing to the chip.
    pub startup_full_hud_ms: u32,
}

impl AnimConfig {
    /// Default transition timings.
    pub const DEFAULT: Self = Self {
        overlay_fade_ms: 160,
        value_glide_ms: 130,
        hud_swap_ms: 140,
        startup_full_hud_ms: 5_000,
    };
}

impl Default for AnimConfig {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Root configuration aggregate. Compile-time constant; no runtime config-file
/// load path. `Deserialize` is omitted; see [`HudFonts`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct UserConfig {
    /// Overlay (mask + slit) configuration.
    pub overlay: OverlayConfig,
    /// Hotkey chord assignments.
    pub hotkeys: crate::input::hotkey_map::HotkeyMap,
    /// Input timing (tap step / hold repeat).
    pub input: InputConfig,
    /// HUD configuration.
    pub hud: HudConfig,
    /// Render-budget tunables.
    pub render: RenderConfig,
    /// Transition timings.
    pub anim: AnimConfig,
}

impl UserConfig {
    /// Default user configuration — every sub-config at its `DEFAULT`.
    pub const DEFAULT: Self = Self {
        overlay: OverlayConfig::DEFAULT,
        hotkeys: crate::input::hotkey_map::HotkeyMap::DEFAULT,
        input: InputConfig::DEFAULT,
        hud: HudConfig::DEFAULT,
        render: RenderConfig::DEFAULT,
        anim: AnimConfig::DEFAULT,
    };
}

impl Default for UserConfig {
    fn default() -> Self {
        Self::DEFAULT
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // base_opacity is otherwise only read by the platform render path; pin it here.
    #[test]
    fn hud_default_base_opacity_is_pinned() {
        assert!((HudConfig::DEFAULT.base_opacity - 0.875).abs() < f32::EPSILON);
    }

    #[test]
    fn hud_default_fade_decay_px_is_pinned() {
        assert!((HudConfig::DEFAULT.fade_decay_px - 120.0).abs() < f32::EPSILON);
    }

    #[test]
    fn hud_default_corner_radius_is_pinned_at_fluent_8px() {
        assert!((HudConfig::DEFAULT.corner_radius - 8.0).abs() < f32::EPSILON);
    }

    /// HUD 背景は僅かに半透明 (0xEB ≈ 92%)。完全不透明 (0xFF) に戻すと Fluent
    /// 的な抜け感が消え、0x80 級まで下げると overlay 暗幕の透けで可読性が落ちる。
    /// どちらの方向の事故も pin で検知する。
    #[test]
    fn hud_default_background_alpha_is_pinned_slightly_translucent() {
        assert_eq!(HudColors::DEFAULT.background.a, 0xEB);
    }

    #[test]
    fn hud_default_telemetry_refresh_is_pinned_at_200ms() {
        assert_eq!(
            HudConfig::DEFAULT.telemetry_refresh,
            Duration::from_millis(200)
        );
    }

    #[test]
    fn overlay_default_effect_is_dim_black_with_default_blur() {
        use crate::color::BlurAmount;
        use crate::state::SurroundEffect;
        assert_eq!(OverlayConfig::DEFAULT.effect, SurroundEffect::DimBlack);
        assert_eq!(OverlayConfig::DEFAULT.blur, BlurAmount::DEFAULT);
    }

    /// Pin the animation defaults. Design constraint "fast, subtle": every
    /// transition stays under 200 ms; catches changes that drift sluggish.
    #[test]
    fn anim_defaults_are_pinned_under_200ms() {
        let a = AnimConfig::DEFAULT;
        assert_eq!(a.overlay_fade_ms, 160);
        assert_eq!(a.value_glide_ms, 130);
        assert_eq!(a.hud_swap_ms, 140);
        assert_eq!(a.startup_full_hud_ms, 5_000);
        assert!(a.overlay_fade_ms < 200 && a.value_glide_ms < 200 && a.hud_swap_ms < 200);
    }
}
