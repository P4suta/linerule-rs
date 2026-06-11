//! User-facing configuration. Every tunable is a compile-time constant,
//! exposed as `const DEFAULT: Self`. There is no file parser, no environment
//! lookup; reconfiguration means recompiling.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::color::{Opacity, Rgba, Thickness};

/// How the area *outside* the slit is rendered.
///
/// The canonical cycle is `Dim → Bright → Dim`. `Blur` is reserved for a
/// future Gaussian-blur surround; it is intentionally left out of [`cycle`]
/// and is not yet drawn by the platform layer (the renderer falls back to a
/// tinted solid). The planned blur path (screen capture → D2D
/// `CLSID_D2D1GaussianBlur` → tinted overlay) is described on
/// [`crate::render::Brush::Blur`] and in the README.
///
/// [`cycle`]: SurroundStyle::cycle
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurroundStyle {
    /// Darken the surround (the original behavior): a translucent dark mask.
    #[default]
    Dim,
    /// Wash the surround in a light/near-white veil instead of darkening it.
    Bright,
    /// Reserved: blur the surround. Not part of [`SurroundStyle::cycle`] yet
    /// and not implemented in the renderer (falls back to a solid tint).
    Blur,
}

impl SurroundStyle {
    /// Advance to the next *implemented* style (`Dim → Bright → Dim`).
    ///
    /// `Blur` is reserved and excluded from the cycle until its render path
    /// lands; if a config somehow holds `Blur`, cycling normalizes it back to
    /// `Dim`. Enabling blur later is a one-line change here.
    #[must_use]
    pub const fn cycle(self) -> Self {
        match self {
            Self::Dim => Self::Bright,
            Self::Bright | Self::Blur => Self::Dim,
        }
    }

    /// Style crossfade target for the renderer's `style_mix` channel:
    /// `Dim` = `0`, `Bright` = `255`. The reserved `Blur` falls back to the
    /// dim mask (= `0`), matching the solid-tint fallback in the renderer.
    #[must_use]
    pub const fn mix_target(self) -> u8 {
        match self {
            Self::Dim | Self::Blur => 0,
            Self::Bright => u8::MAX,
        }
    }
}

/// Mask color + thickness + opacity + surround style. Composed into a
/// [`crate::state::State`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OverlayConfig {
    /// Color of the dim layers above and below (or beside) the slit.
    pub mask_color: Rgba,
    /// Slit width in logical pixels.
    pub thickness: Thickness,
    /// Mask opacity (perceptual-mapped on output).
    pub opacity: Opacity,
    /// How the surround (everything but the slit) is rendered.
    #[serde(default)]
    pub surround_style: SurroundStyle,
}

impl OverlayConfig {
    /// Default mask: `DEFAULT_MASK` × `Thickness::DEFAULT` × `Opacity::DEFAULT`
    /// × `SurroundStyle::Dim`.
    pub const DEFAULT: Self = Self {
        mask_color: Rgba::DEFAULT_MASK,
        thickness: Thickness::DEFAULT,
        opacity: Opacity::DEFAULT,
        surround_style: SurroundStyle::Dim,
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
    /// HUD sits on top of the overlay mask in `DComp` z-order, so the mask
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
    reason = "ミリ秒単位を field 名で明示する (`_ms` suffix) 方が呼び出し側の誤用を防ぐ"
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

/// Root configuration aggregate. Per ADR-0015 this is a compile-time
/// constant; there is no runtime config-file load path.
//
// `Deserialize` is omitted; see [`HudFonts`].
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

    // cs parity: HUD's base opacity is 0.875. Pinning this default closes a
    // mutation-gate hole — `HudConfig::DEFAULT.base_opacity` is otherwise only
    // sampled by the platform layer's render path, which mutation testing can
    // perturb without any in-process test catching the regression.
    #[test]
    fn hud_default_base_opacity_is_pinned_at_cs_value() {
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
    fn surround_style_default_is_dim() {
        assert_eq!(SurroundStyle::default(), SurroundStyle::Dim);
        assert_eq!(OverlayConfig::DEFAULT.surround_style, SurroundStyle::Dim);
    }

    #[test]
    fn surround_style_cycle_toggles_dim_and_bright() {
        assert_eq!(SurroundStyle::Dim.cycle(), SurroundStyle::Bright);
        assert_eq!(SurroundStyle::Bright.cycle(), SurroundStyle::Dim);
    }

    /// アニメ既定値を pin する。「速く・控えめ」の設計制約: 全トランジション
    /// 200ms 未満。値が伸びる方向の変更 (もたつき) を検知する。
    #[test]
    fn anim_defaults_are_pinned_under_200ms() {
        let a = AnimConfig::DEFAULT;
        assert_eq!(a.overlay_fade_ms, 160);
        assert_eq!(a.value_glide_ms, 130);
        assert_eq!(a.hud_swap_ms, 140);
        assert_eq!(a.startup_full_hud_ms, 5_000);
        assert!(a.overlay_fade_ms < 200 && a.value_glide_ms < 200 && a.hud_swap_ms < 200);
    }

    /// `mix_target` の対応を pin する: Dim=0 / Bright=255 / Blur=0 (dim fallback)。
    #[test]
    fn surround_style_mix_target_mapping_is_pinned() {
        assert_eq!(SurroundStyle::Dim.mix_target(), 0);
        assert_eq!(SurroundStyle::Bright.mix_target(), 255);
        assert_eq!(SurroundStyle::Blur.mix_target(), 0);
    }

    #[test]
    fn surround_style_cycle_normalizes_reserved_blur_to_dim() {
        // `Blur` is reserved and not part of the user-facing cycle yet.
        assert_eq!(SurroundStyle::Blur.cycle(), SurroundStyle::Dim);
    }

    #[test]
    fn overlay_config_deserializes_without_surround_style_field() {
        // `#[serde(default)]` keeps older payloads (pre-surround_style) loadable.
        // Build the payload by stripping the key from a serialized DEFAULT so the
        // test stays agnostic to the inner field representations.
        let mut value = serde_json::to_value(OverlayConfig::DEFAULT).expect("serialize");
        value
            .as_object_mut()
            .expect("config serializes as a JSON object")
            .remove("surround_style");
        let cfg: OverlayConfig = serde_json::from_value(value).expect("legacy config loads");
        assert_eq!(cfg.surround_style, SurroundStyle::Dim);
    }
}
