//! User-facing configuration. Every tunable is a compile-time constant,
//! exposed as `const DEFAULT: Self`. There is no file parser, no environment
//! lookup; reconfiguration means recompiling.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::color::{Opacity, Rgba, Thickness};

/// Mask color + thickness + opacity. Composed into a [`crate::state::State`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OverlayConfig {
    /// Color of the dim layers above and below (or beside) the slit.
    pub mask_color: Rgba,
    /// Slit width in logical pixels.
    pub thickness: Thickness,
    /// Mask opacity (perceptual-mapped on output).
    pub opacity: Opacity,
}

impl OverlayConfig {
    /// Default mask: `DEFAULT_MASK` × `Thickness::DEFAULT` × `Opacity::DEFAULT`.
    pub const DEFAULT: Self = Self {
        mask_color: Rgba::DEFAULT_MASK,
        thickness: Thickness::DEFAULT,
        opacity: Opacity::DEFAULT,
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
    /// Hold time beyond which `ToggleVisible` is treated as a long-press undo.
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
    pub const DEFAULT: Self = Self {
        background: Rgba::new(0x10, 0x12, 0x18, 0xD8),
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

/// HUD configuration root.
//
// `Deserialize` is omitted; see [`HudFonts`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct HudConfig {
    /// Base HUD opacity (0–1).
    pub base_opacity: f32,
    /// Distance (logical pixels) at which the HUD fades by `1 - 1/e`.
    pub fade_decay_px: f32,
    /// Interval for refreshing telemetry rows.
    pub telemetry_refresh: Duration,
    /// HUD bounding rectangle.
    pub geometry: HudGeometry,
    /// HUD panel padding.
    pub padding: HudPadding,
    /// HUD font sizes/families.
    pub fonts: HudFonts,
    /// HUD palette.
    pub colors: HudColors,
}

impl HudConfig {
    /// Default HUD tunables.
    pub const DEFAULT: Self = Self {
        base_opacity: 0.875,
        fade_decay_px: 120.0,
        telemetry_refresh: Duration::from_millis(200),
        geometry: HudGeometry::DEFAULT,
        padding: HudPadding::DEFAULT,
        fonts: HudFonts::DEFAULT,
        colors: HudColors::DEFAULT,
    };
}

impl Default for HudConfig {
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
}

impl UserConfig {
    /// Default user configuration — every sub-config at its `DEFAULT`.
    pub const DEFAULT: Self = Self {
        overlay: OverlayConfig::DEFAULT,
        hotkeys: crate::input::hotkey_map::HotkeyMap::DEFAULT,
        input: InputConfig::DEFAULT,
        hud: HudConfig::DEFAULT,
        render: RenderConfig::DEFAULT,
    };
}

impl Default for UserConfig {
    fn default() -> Self {
        Self::DEFAULT
    }
}
