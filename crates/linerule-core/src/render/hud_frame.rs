//! Pure ADT and layout function for the HUD frame.
//!
//! Computes panel position, background, opacity, and text-row layout; the
//! platform layer (`linerule-platform-windows::hud_renderer`) draws it with
//! `DWrite` + `D2D`. Kept separate from [`crate::render::OverlayFrame`] because
//! the HUD needs text, which `Layer`'s closed enum does not carry.

use serde::Serialize;

use crate::color::Rgba;
use crate::config::HudConfig;
use crate::geometry::{Logical, ScreenRect};
use crate::input::hotkey_map::HotkeyMap;
use crate::state::{Mode, State};

/// HUD panel plus its text rows. Coordinates are `f32` logical pixels;
/// snapping to integer pixels is the platform layer's job.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HudFrame {
    /// Panel top-left x (logical px).
    pub panel_left: f32,
    /// Panel top-left y (logical px).
    pub panel_top: f32,
    /// Panel width (logical px).
    pub panel_width: f32,
    /// Panel height (logical px).
    pub panel_height: f32,
    /// Panel background color.
    pub background: Rgba,
    /// Panel corner radius (logical px).
    pub corner_radius: f32,
    /// Overall opacity (0.0-1.0); updated per-frame by `SetHudOpacity`.
    pub opacity: f32,
    /// Non-text fill rects (dividers etc.); drawn before the rows.
    pub rules: Vec<HudRule>,
    /// Rows to draw.
    pub rows: Vec<HudRow>,
}

/// One filled rect inside the HUD (divider etc.). Coordinates are absolute
/// logical px, same space as the rows.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct HudRule {
    /// Top-left x (logical px).
    pub left: f32,
    /// Top-left y (logical px).
    pub top: f32,
    /// Width (logical px).
    pub width: f32,
    /// Height (logical px).
    pub height: f32,
    /// Fill color.
    pub color: Rgba,
}

/// One HUD text row.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HudRow {
    /// Text layout rect top-left x (logical px).
    pub origin_x: f32,
    /// Text layout rect top-left y (logical px).
    pub origin_y: f32,
    /// Text to draw.
    pub text: String,
    /// Font size (logical pt).
    pub font_size: f32,
    /// Logical font-family key, resolved to a real family by the platform layer.
    pub font: HudFontKey,
    /// Text color.
    pub color: Rgba,
}

/// Logical font-family key, resolved by the platform layer to
/// [`crate::config::HudFonts::title_family`] / [`crate::config::HudFonts::mono_family`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HudFontKey {
    /// Proportional (title, status, body).
    Title,
    /// Monospace (telemetry and other numeric rows).
    Mono,
}

/// HUD presentation tier.
///
/// - `Chip`: persistent one-line status (`H · 28px · 67%`). The default.
/// - `Full`: full panel with the hotkey guide. Shown only for a few seconds
///   after startup and on an explicit `ToggleHudDetail` hotkey.
///
/// The teaching UI is concentrated on the full tier so that during normal
/// reading the HUD claims no more than one chip line (the cursor is a reading
/// tool here, so hover-style triggers are deliberately not used).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HudTier {
    /// Persistent status chip.
    Chip,
    /// Full panel with the hotkey guide.
    Full,
}

impl HudTier {
    /// Flip chip ⇄ full.
    #[must_use]
    pub const fn toggle(self) -> Self {
        match self {
            Self::Chip => Self::Full,
            Self::Full => Self::Chip,
        }
    }
}

/// Short-lived message shown at the panel's bottom edge (recoverable error,
/// hotkey conflict, device-lost rebuild, etc.).
///
/// `until_ms` is a monotonic time (ms): the notification is dropped once
/// `now_ms >= until_ms`. Pass `i64::MAX` to keep it permanent.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct HudNotification {
    /// Message class; drives the color.
    pub class: NotificationClass,
    /// Text to display.
    pub message: String,
    /// Expiry time (ms, monotonic).
    pub until_ms: i64,
}

/// [`HudNotification`] class. Maps to the HUD palette: `Info` -> `accent`,
/// `Warn` -> `hint`, `Error` -> a palette-external red.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationClass {
    /// Informational.
    Info,
    /// Warning (e.g. hotkey conflict).
    Warn,
    /// Error (e.g. device-lost rebuild failure).
    Error,
}

/// Per-tick telemetry snapshot for the HUD's telemetry row. Measured by the
/// platform layer (`linerule-platform-windows::frame_timing`) and passed by value.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct HudTelemetry {
    /// p99 tick latency over the recent window (ms).
    pub tick_p99_ms: f32,
    /// Cumulative count of ticks that exceeded the render budget.
    pub frames_dropped: u64,
    /// Cumulative count of composition commit failures / timeouts.
    pub commit_timeouts: u64,
}

impl HudTelemetry {
    /// All-zero value for the "no samples yet" state (boot, tests).
    pub const ZERO: Self = Self {
        tick_p99_ms: 0.0,
        frames_dropped: 0,
        commit_timeouts: 0,
    };
}

/// Build a HUD frame from `State`, `HudConfig`, monitor, refresh Hz, and
/// notifications.
///
/// The panel is anchored top-right (`geometry.margin` from the monitor corner).
/// Rows, top to bottom: title, status (Mode), body (Thickness, Opacity, Effect),
/// telemetry (Refresh Hz), hotkey help (filtered to the keys that are
/// currently actionable), then one row per notification.
///
/// `notifications` must already have expired entries removed; `hud_frame` does
/// no time checks, only layout.
///
/// # Examples
///
/// ```
/// use linerule_core::{
///     HotkeyMap, HudConfig, HudTelemetry, Mode, Point, ScreenRect, State, hud_frame,
/// };
///
/// let monitor = ScreenRect::new(Point::new(0, 0), 1920, 1080);
/// let frame = hud_frame(
///     State::with_mode(Mode::Horizontal),
///     HudConfig::DEFAULT,
///     monitor,
///     144,
///     &[],
///     HotkeyMap::DEFAULT,
///     HudTelemetry::ZERO,
///     linerule_core::HudTier::Full,
/// );
/// // Top-right anchor: panel right edge is `margin` left of the monitor right.
/// let expected_right = 1920.0 - HudConfig::DEFAULT.geometry.margin;
/// assert!((frame.panel_left + frame.panel_width - expected_right).abs() < 0.5);
/// // While on: 6 baseline + 1 header + 9 hotkey rows = 16 rows (Quit included).
/// assert!(frame.rows.len() >= 16);
/// ```
#[must_use]
#[allow(
    clippy::too_many_arguments,
    reason = "config + per-tick state are distinct inputs; a grouping struct \
              would read worse at the call site than the flat list"
)]
pub fn hud_frame(
    state: State,
    hud: HudConfig,
    monitor: ScreenRect<Logical>,
    refresh_hz: u32,
    notifications: &[HudNotification],
    hotkeys: HotkeyMap,
    telemetry: HudTelemetry,
    tier: HudTier,
) -> HudFrame {
    match tier {
        HudTier::Chip => chip_frame(state, hud, monitor, notifications, hotkeys),
        HudTier::Full => full_frame(
            state,
            hud,
            monitor,
            refresh_hz,
            notifications,
            hotkeys,
            telemetry,
        ),
    }
}

/// Layout for the full panel (with the hotkey guide).
#[allow(
    clippy::too_many_arguments,
    clippy::too_many_lines,
    reason = "row construction is a sequential layout walk; splitting it would \
              thread the `y` cursor through helpers and read worse"
)]
fn full_frame(
    state: State,
    hud: HudConfig,
    monitor: ScreenRect<Logical>,
    refresh_hz: u32,
    notifications: &[HudNotification],
    hotkeys: HotkeyMap,
    telemetry: HudTelemetry,
) -> HudFrame {
    let panel_left = monitor_right(monitor) - hud.geometry.margin - hud.geometry.width;
    let panel_top = monitor_top(monitor) + hud.geometry.margin;

    let mut cur = RowCursor::new(panel_left + hud.padding.edge, panel_top + hud.padding.edge);
    let title = HudFontKey::Title;
    let mono = HudFontKey::Mono;

    cur.push(
        "linerule".to_string(),
        hud.fonts.title,
        title,
        hud.colors.foreground,
    );
    cur.advance(hud.padding.section);

    cur.push(
        format!("Mode: {}", mode_label(state.mode)),
        hud.fonts.status,
        title,
        hud.colors.foreground,
    );
    cur.advance(hud.padding.row);

    cur.push(
        format!("Thickness: {} px", state.config.thickness.get()),
        hud.fonts.body,
        title,
        hud.colors.subtle,
    );
    cur.advance(hud.padding.row);

    // Under Blur, the opacity hotkeys retarget onto the σ amount, so the same
    // row slot shows the derived σ (px) instead of opacity.
    let intensity = if state.config.effect.is_blur() {
        format!("Blur: {} px", state.config.blur.to_std_dev().round())
    } else {
        format!("Opacity: {}", state.config.opacity.get())
    };
    cur.push(intensity, hud.fonts.body, title, hud.colors.subtle);
    cur.advance(hud.padding.row);

    cur.push(
        format!("Effect: {}", state.config.effect.label()),
        hud.fonts.body,
        title,
        hud.colors.subtle,
    );
    cur.advance(hud.padding.section);

    // `·` is U+00B7; spacing is fixed for monospace alignment.
    cur.push(
        format!(
            "{refresh_hz}Hz · p99 {:.2}ms · drops {} · stalls {}",
            telemetry.tick_p99_ms, telemetry.frames_dropped, telemetry.commit_timeouts,
        ),
        hud.fonts.telemetry,
        mono,
        hud.colors.accent,
    );
    cur.advance(hud.padding.section);

    // A 1px palette-colored divider between the status block and the hotkey
    // guide, centered in the section gap; it does not move the row layout.
    let rules = vec![HudRule {
        left: panel_left + hud.padding.edge,
        top: hud.padding.section.mul_add(-0.5, cur.y),
        width: hud.padding.edge.mul_add(-2.0, hud.geometry.width),
        height: 1.0,
        color: hud.colors.divider,
    }];

    cur.push(
        "Hotkeys".to_string(),
        hud.fonts.body,
        title,
        hud.colors.foreground,
    );
    cur.advance(hud.padding.row);
    // The guide lists only the keys that are currently actionable: while Off
    // the axis/effect/value keys are rejected by the reducer anyway, so they
    // are hidden until `ToggleOnOff` brings the overlay back.
    let on = !matches!(state.mode, Mode::Off);
    let hotkey_lines: [(&str, &str, bool); 9] = [
        ("Mode cycle", hotkeys.cycle_mode, on),
        ("Effect cycle", hotkeys.cycle_effect, on),
        ("On/Off", hotkeys.toggle_on_off, true),
        ("Thicker", hotkeys.thicker, on),
        ("Thinner", hotkeys.thinner, on),
        ("More opaque", hotkeys.more_opaque, on),
        ("Less opaque", hotkeys.less_opaque, on),
        ("HUD detail", hotkeys.toggle_hud, true),
        ("Quit", hotkeys.quit, true),
    ];
    for (label, chord, actionable) in hotkey_lines {
        if !actionable {
            continue;
        }
        cur.push(
            format!("{label:<12} {chord}"),
            hud.fonts.telemetry,
            mono,
            hud.colors.subtle,
        );
        cur.advance(hud.padding.row);
    }
    // Extra gap separating the hotkey section from notifications.
    cur.advance(hud.padding.section - hud.padding.row);

    for notification in notifications {
        cur.push(
            notification.message.clone(),
            hud.fonts.telemetry,
            title,
            notification_color(notification.class, hud),
        );
        cur.advance(hud.padding.row);
    }

    HudFrame {
        panel_left,
        panel_top,
        panel_width: hud.geometry.width,
        panel_height: hud.geometry.height,
        background: hud.colors.background,
        corner_radius: hud.corner_radius,
        opacity: hud.base_opacity,
        rules,
        rows: cur.rows,
    }
}

/// Average monospace advance width (em ratio), used to estimate chip width.
/// Lets core finish layout without `DWrite` measurements — Cascadia Mono's
/// advance is roughly 0.6em; we err on the wide side.
const MONO_ADVANCE_RATIO: f32 = 0.62;

/// Layout for the resident chip (one status row + toast rows right below).
///
/// Unlike the full panel, width fits the content (`MONO_ADVANCE_RATIO`
/// estimate). Panel origin and size are rounded to integer logical px so
/// small text doesn't blur from sub-pixel placement.
fn chip_frame(
    state: State,
    hud: HudConfig,
    monitor: ScreenRect<Logical>,
    notifications: &[HudNotification],
    hotkeys: HotkeyMap,
) -> HudFrame {
    let chip = hud.chip;
    let margin = hud.geometry.margin;
    #[allow(
        clippy::cast_precision_loss,
        reason = "screen-space px fits comfortably in the f32 mantissa"
    )]
    let monitor_right = (monitor.left() + i32::try_from(monitor.width).unwrap_or(i32::MAX)) as f32;
    #[allow(
        clippy::cast_precision_loss,
        reason = "screen-space px fits comfortably in the f32 mantissa"
    )]
    let monitor_top = monitor.top() as f32;

    let status = chip_text(state, hotkeys);

    // Width = longest of the status and toast rows (estimated) + horizontal padding.
    let mut text_width = estimate_mono_width(&status, chip.font_size);
    for n in notifications {
        text_width = text_width.max(estimate_mono_width(&n.message, chip.font_size));
    }
    let panel_width = chip.pad_x.mul_add(2.0, text_width).ceil();

    let row_advance = chip.font_size + hud.padding.row;
    #[allow(
        clippy::cast_precision_loss,
        reason = "notification count is single-digit"
    )]
    let toast_height = notifications.len() as f32 * row_advance;
    let panel_height = (chip.pad_y.mul_add(2.0, chip.font_size) + toast_height).ceil();

    let panel_left = (monitor_right - margin - panel_width).round();
    let panel_top = (monitor_top + margin).round();

    let x = panel_left + chip.pad_x;
    let mut y = panel_top + chip.pad_y;
    let mut rows = Vec::with_capacity(1 + notifications.len());
    rows.push(HudRow {
        origin_x: x,
        origin_y: y,
        text: status,
        font_size: chip.font_size,
        font: HudFontKey::Mono,
        color: hud.colors.foreground,
    });
    y += row_advance;
    for notification in notifications {
        rows.push(HudRow {
            origin_x: x,
            origin_y: y,
            text: notification.message.clone(),
            font_size: chip.font_size,
            font: HudFontKey::Mono,
            color: notification_color(notification.class, hud),
        });
        y += row_advance;
    }

    HudFrame {
        panel_left,
        panel_top,
        panel_width,
        panel_height,
        background: hud.colors.background,
        corner_radius: hud.corner_radius,
        opacity: hud.base_opacity,
        rules: Vec::new(),
        rows,
    }
}

/// Estimated text width assuming monospace (logical px).
fn estimate_mono_width(text: &str, font_size: f32) -> f32 {
    #[allow(clippy::cast_precision_loss, reason = "HUD text is tens of characters")]
    let chars = text.chars().count() as f32;
    chars * font_size * MONO_ADVANCE_RATIO
}

/// Chip status string: nothing but a key hint. Mode, effect, and values are
/// all visible on screen, so the chip only keeps the path to the full guide
/// alive — the HUD-detail chord while on (`Ctrl+Alt+K`), the restore chord
/// while off (`Off · Ctrl+Alt+H`).
fn chip_text(state: State, hotkeys: HotkeyMap) -> String {
    match state.mode {
        Mode::Off => format!("Off · {}", hotkeys.toggle_on_off),
        Mode::Horizontal | Mode::Vertical => hotkeys.toggle_hud.to_string(),
    }
}

/// Sequential row layout: pushes a [`HudRow`] at the running cursor, then the
/// caller advances `y`. Keeps `y` accumulation in one place.
struct RowCursor {
    x: f32,
    y: f32,
    rows: Vec<HudRow>,
}

impl RowCursor {
    fn new(x: f32, y: f32) -> Self {
        Self {
            x,
            y,
            rows: Vec::with_capacity(16),
        }
    }

    /// Push a row at the cursor and advance past its font height. Pair with
    /// `advance` to add the trailing row/section padding.
    fn push(&mut self, text: String, font_size: f32, font: HudFontKey, color: Rgba) {
        self.rows.push(HudRow {
            origin_x: self.x,
            origin_y: self.y,
            text,
            font_size,
            font,
            color,
        });
        self.y += font_size;
    }

    /// Advance `y` by trailing padding after a row, or a standalone gap.
    fn advance(&mut self, delta: f32) {
        self.y += delta;
    }
}

#[allow(
    clippy::cast_precision_loss,
    reason = "screen-space px fits f32 mantissa with room to spare"
)]
fn monitor_right(monitor: ScreenRect<Logical>) -> f32 {
    (monitor.left() + i32::try_from(monitor.width).unwrap_or(i32::MAX)) as f32
}

#[allow(
    clippy::cast_precision_loss,
    reason = "screen-space px fits f32 mantissa with room to spare"
)]
const fn monitor_top(monitor: ScreenRect<Logical>) -> f32 {
    monitor.top() as f32
}

/// Map a [`NotificationClass`] to a color from `HudConfig::colors`.
const fn notification_color(class: NotificationClass, hud: HudConfig) -> Rgba {
    match class {
        NotificationClass::Info => hud.colors.accent,
        NotificationClass::Warn => hud.colors.hint,
        NotificationClass::Error => Rgba::new(0xFF, 0x6B, 0x6B, 0xFF),
    }
}

/// Fold `Mode` into a display label; "not shown" is solely `Mode::Off`.
const fn mode_label(mode: Mode) -> &'static str {
    match mode {
        Mode::Off => "Off",
        Mode::Horizontal => "Horizontal",
        Mode::Vertical => "Vertical",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Point;

    fn monitor() -> ScreenRect<Logical> {
        ScreenRect::new(Point::new(0, 0), 1920, 1080)
    }

    /// Call `hud_frame()` (full tier) with default `HotkeyMap` / `HudTelemetry`.
    fn default_frame(state: State, refresh_hz: u32, notifications: &[HudNotification]) -> HudFrame {
        hud_frame(
            state,
            HudConfig::DEFAULT,
            monitor(),
            refresh_hz,
            notifications,
            HotkeyMap::DEFAULT,
            HudTelemetry::ZERO,
            HudTier::Full,
        )
    }

    /// Test helper for the chip tier.
    fn chip(state: State, notifications: &[HudNotification]) -> HudFrame {
        hud_frame(
            state,
            HudConfig::DEFAULT,
            monitor(),
            60,
            notifications,
            HotkeyMap::DEFAULT,
            HudTelemetry::ZERO,
            HudTier::Chip,
        )
    }

    #[test]
    fn panel_anchored_top_right_with_margin() {
        let f = default_frame(State::DEFAULT, 60, &[]);
        let expected_right = 1920.0_f32 - HudConfig::DEFAULT.geometry.margin;
        assert!((f.panel_left + f.panel_width - expected_right).abs() < 0.5);
        assert!((f.panel_top - HudConfig::DEFAULT.geometry.margin).abs() < 0.5);
    }

    #[test]
    fn active_state_rows_are_present_and_ordered_top_to_bottom() {
        let f = default_frame(State::with_mode(Mode::Horizontal), 144, &[]);
        assert!(
            f.rows.len() >= 16,
            "expected at least 16 rows (6 baseline + 1 header + 9 hotkeys), got {}",
            f.rows.len()
        );
        for w in f.rows.windows(2) {
            assert!(
                w[0].origin_y <= w[1].origin_y,
                "rows should be top-to-bottom: {} then {}",
                w[0].text,
                w[1].text
            );
        }
    }

    /// While Off, the guide lists only the actionable keys: On/Off, HUD
    /// detail, and Quit. The adjustment keys (axis/effect/values) are hidden
    /// because the reducer rejects them anyway.
    #[test]
    fn off_state_guide_hides_inactive_hotkeys() {
        let f = default_frame(State::DEFAULT, 60, &[]);
        // 6 baseline + 1 header + 3 actionable hotkeys = 10 rows.
        assert_eq!(f.rows.len(), 10, "rows: {:?}", f.rows);
        let texts: Vec<&str> = f.rows.iter().map(|r| r.text.as_str()).collect();
        for present in ["On/Off", "HUD detail", "Quit"] {
            assert!(
                texts.iter().any(|t| t.contains(present)),
                "{present} must stay visible while Off: {texts:?}"
            );
        }
        for hidden in [
            "Mode cycle",
            "Effect cycle",
            "Thicker",
            "Thinner",
            "More opaque",
            "Less opaque",
        ] {
            assert!(
                !texts.iter().any(|t| t.contains(hidden)),
                "{hidden} must be hidden while Off: {texts:?}"
            );
        }
    }

    /// Default (Dim) effect shows an Opacity row.
    #[test]
    fn flat_effect_shows_opacity_row() {
        let mut s = State::DEFAULT;
        s.mode = Mode::Horizontal;
        let f = default_frame(s, 60, &[]);
        assert!(
            f.rows.iter().any(|r| r.text.starts_with("Opacity:")),
            "rows: {:?}",
            f.rows
        );
        assert!(!f.rows.iter().any(|r| r.text.starts_with("Blur:")));
    }

    /// Under Blur, the same slot becomes a Blur σ row and the Opacity row is
    /// gone; row count stays the same.
    #[test]
    fn blur_effect_swaps_opacity_row_for_blur_amount() {
        use crate::state::SurroundEffect;
        let mut s = State::DEFAULT;
        s.mode = Mode::Horizontal;
        s.config.effect = SurroundEffect::Blur;
        let baseline = {
            let mut d = State::DEFAULT;
            d.mode = Mode::Horizontal;
            default_frame(d, 60, &[]).rows.len()
        };
        let f = default_frame(s, 60, &[]);
        assert_eq!(f.rows.len(), baseline, "row count must stay stable");
        assert!(
            f.rows
                .iter()
                .any(|r| r.text == format!("Blur: {} px", s.config.blur.to_std_dev().round())),
            "rows: {:?}",
            f.rows
        );
        assert!(
            !f.rows.iter().any(|r| r.text.starts_with("Opacity:")),
            "Opacity row must be gone under Blur"
        );
    }

    #[test]
    fn mode_label_reflects_state() {
        let f = default_frame(State::with_mode(Mode::Horizontal), 60, &[]);
        assert!(
            f.rows.iter().any(|r| r.text == "Mode: Horizontal"),
            "rows: {:?}",
            f.rows
        );
    }

    #[test]
    fn off_state_shows_mode_off() {
        let f = default_frame(State::DEFAULT, 60, &[]);
        assert!(
            f.rows.iter().any(|r| r.text == "Mode: Off"),
            "rows: {:?}",
            f.rows
        );
    }

    #[test]
    fn refresh_hz_appears_in_telemetry_row_with_mono_font() {
        let f = default_frame(State::DEFAULT, 144, &[]);
        let telemetry = f
            .rows
            .iter()
            .find(|r| r.text.contains("144Hz"))
            .expect("refresh row");
        assert_eq!(telemetry.font, HudFontKey::Mono);
        assert!(
            telemetry.text.starts_with("144Hz · p99 "),
            "telemetry row should start with refresh Hz + p99 (cs format): {}",
            telemetry.text
        );
    }

    /// Pin the telemetry row format and that values are interpolated.
    #[test]
    fn telemetry_line_format_is_pinned() {
        let t = HudTelemetry {
            tick_p99_ms: 1.23,
            frames_dropped: 7,
            commit_timeouts: 2,
        };
        let f = hud_frame(
            State::DEFAULT,
            HudConfig::DEFAULT,
            monitor(),
            60,
            &[],
            HotkeyMap::DEFAULT,
            t,
            HudTier::Full,
        );
        let row = f
            .rows
            .iter()
            .find(|r| r.text.contains("Hz"))
            .expect("telemetry row");
        assert_eq!(row.text, "60Hz · p99 1.23ms · drops 7 · stalls 2");
        assert_eq!(row.font, HudFontKey::Mono);
    }

    /// Pin the serde serialization of `HudTelemetry::ZERO` for snapshot stability.
    #[test]
    fn hud_telemetry_zero_serde_round_trip() {
        let json = serde_json::to_string(&HudTelemetry::ZERO).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["tick_p99_ms"], 0.0);
        assert_eq!(parsed["frames_dropped"], 0);
        assert_eq!(parsed["commit_timeouts"], 0);
    }

    #[test]
    fn opacity_reflects_base_opacity_from_config() {
        let f = default_frame(State::DEFAULT, 60, &[]);
        assert!((f.opacity - HudConfig::DEFAULT.base_opacity).abs() < f32::EPSILON);
    }

    #[test]
    fn rows_fit_within_panel_horizontally() {
        let f = default_frame(State::DEFAULT, 60, &[]);
        let panel_right = f.panel_left + f.panel_width;
        for r in &f.rows {
            assert!(
                r.origin_x >= f.panel_left,
                "row origin_x {} should be >= panel_left {}",
                r.origin_x,
                f.panel_left
            );
            assert!(
                r.origin_x < panel_right,
                "row origin_x {} should be < panel_right {}",
                r.origin_x,
                panel_right
            );
        }
    }

    #[test]
    fn notifications_appended_below_hotkey_help_section() {
        let warn = HudNotification {
            class: NotificationClass::Warn,
            message: "Ctrl+Alt+R → already in use".to_string(),
            until_ms: i64::MAX,
        };
        let info = HudNotification {
            class: NotificationClass::Info,
            message: "Device rebuilt".to_string(),
            until_ms: 1_000,
        };
        let f = default_frame(State::with_mode(Mode::Horizontal), 60, &[warn, info]);
        // 16 baseline+hotkey rows + 2 notifications.
        assert!(f.rows.len() >= 18, "rows: {:?}", f.rows);
        let n1 = &f.rows[f.rows.len() - 2];
        let n2 = &f.rows[f.rows.len() - 1];
        assert_eq!(n1.text, "Ctrl+Alt+R → already in use");
        assert_eq!(n2.text, "Device rebuilt");
        assert!(n2.origin_y > n1.origin_y);
    }

    #[test]
    fn notification_color_maps_per_class() {
        let hud = HudConfig::DEFAULT;
        assert_eq!(
            notification_color(NotificationClass::Info, hud),
            hud.colors.accent
        );
        assert_eq!(
            notification_color(NotificationClass::Warn, hud),
            hud.colors.hint
        );
        // Error is palette-external, biased red
        let err = notification_color(NotificationClass::Error, hud);
        assert!(err.r > err.g && err.r > err.b);
    }

    #[test]
    fn empty_notifications_preserve_default_row_count() {
        let f = default_frame(State::with_mode(Mode::Horizontal), 60, &[]);
        // While on: 6 baseline + 1 hotkey header + 9 hotkey rows = 16.
        assert_eq!(f.rows.len(), 16);
    }

    /// Pin each row's `origin_y` against `HudConfig::DEFAULT`-derived layout
    /// arithmetic, so a single `+=` / `+` operator mutation in the `y`
    /// accumulation is caught (the ordering test alone cannot pin the values).
    /// Expected values, hand-derived from `HudConfig::DEFAULT`:
    /// - `panel_top` = `monitor_top + margin` = `0 + 24` = `24`
    /// - row 0 (Title)            `y0 = 24 + 24 (edge)` = `48`
    /// - row 1 (Mode)             `y1 = 48 + 24 (title font) + 16 (section)` = `88`
    /// - row 2 (Thickness)        `y2 = 88 + 22 (status font) + 8 (row)` = `118`
    /// - row 3 (Opacity/Blur)     `y3 = 118 + 20 (body font) + 8 (row)` = `146`
    /// - row 4 (Effect)           `y4 = 146 + 20 (body font) + 8 (row)` = `174`
    /// - row 5 (Telemetry)        `y5 = 174 + 20 (body font) + 16 (section)` = `210`
    /// - row 6 (Hotkeys header)   `y6 = 210 + 18 (telemetry) + 16 (section)` = `244`
    /// - row 7..15 (Hotkey rows)  `y{n+1} = y{n} + 18 (telemetry) + 8 (row)` = `+26 each`
    ///
    /// Update if `HudConfig::DEFAULT` changes.
    #[test]
    fn row_origin_y_pins_default_layout_arithmetic() {
        let f = default_frame(State::with_mode(Mode::Horizontal), 60, &[]);
        assert_eq!(
            f.rows.len(),
            16,
            "6 baseline + 1 header + 9 hotkeys expected while on"
        );

        // panel_top == monitor_top + margin.
        assert!(
            (f.panel_top - 24.0).abs() < 0.001,
            "panel_top expected 24.0, got {}",
            f.panel_top
        );

        // rows 0..6: baseline + Hotkeys header.
        let baseline_y = [48.0_f32, 88.0, 118.0, 146.0, 174.0, 210.0, 244.0];
        // rows 7..15: 9 hotkey rows, starting at 272 with +26 step.
        let hotkey_y = [
            272.0_f32, 298.0, 324.0, 350.0, 376.0, 402.0, 428.0, 454.0, 480.0,
        ];
        let expected_y: Vec<f32> = baseline_y.iter().chain(hotkey_y.iter()).copied().collect();
        for (i, exp) in expected_y.iter().enumerate() {
            let actual = f.rows[i].origin_y;
            assert!(
                (actual - exp).abs() < 0.001,
                "row {i} ({:?}): expected origin_y = {exp}, got {actual}",
                f.rows[i].text
            );
        }
    }

    /// Pin the divider rule: a 1px palette-colored line at the midpoint of the
    /// section gap (16px) between the telemetry row and the hotkey guide
    /// (`244 - 8 = 236`), spanning the panel inside the edge padding.
    #[test]
    fn divider_rule_pins_position_and_color() {
        let f = default_frame(State::DEFAULT, 60, &[]);
        assert_eq!(f.rules.len(), 1, "exactly one divider rule expected");
        let rule = f.rules[0];
        assert!(
            (rule.top - 236.0).abs() < 0.001,
            "divider top expected 236.0, got {}",
            rule.top
        );
        assert!((rule.height - 1.0).abs() < 0.001);
        let edge = HudConfig::DEFAULT.padding.edge;
        assert!((rule.left - (f.panel_left + edge)).abs() < 0.001);
        assert!((rule.width - edge.mul_add(-2.0, f.panel_width)).abs() < 0.001);
        assert_eq!(rule.color, HudConfig::DEFAULT.colors.divider);
    }

    /// Pins that `corner_radius` flows from config straight into the frame.
    #[test]
    fn corner_radius_flows_from_config() {
        let f = default_frame(State::DEFAULT, 60, &[]);
        assert!((f.corner_radius - HudConfig::DEFAULT.corner_radius).abs() < f32::EPSILON);
    }

    // ---- chip tier ---------------------------------------------------------

    /// While off the chip is a single row pointing at the restore key.
    #[test]
    fn chip_shows_restore_key_when_mode_is_off() {
        let f = chip(State::DEFAULT, &[]);
        assert_eq!(f.rows.len(), 1);
        assert_eq!(
            f.rows[0].text,
            format!("Off · {}", HotkeyMap::DEFAULT.toggle_on_off)
        );
        assert_eq!(f.rows[0].font, HudFontKey::Mono);
    }

    /// While active the chip is nothing but the HUD-detail key hint: mode,
    /// effect, and values are all visible on screen.
    #[test]
    fn chip_status_text_is_just_the_detail_key_hint() {
        for mode in [Mode::Horizontal, Mode::Vertical] {
            let f = chip(State::with_mode(mode), &[]);
            assert_eq!(f.rows[0].text, HotkeyMap::DEFAULT.toggle_hud);
        }
    }

    /// The chip ignores effect and value state entirely — it stays the same
    /// key hint under Blur (no σ, no percentages).
    #[test]
    fn chip_is_state_value_agnostic() {
        use crate::state::SurroundEffect;
        let mut s = State::with_mode(Mode::Horizontal);
        s.config.effect = SurroundEffect::Blur;
        let f = chip(s, &[]);
        assert_eq!(f.rows[0].text, HotkeyMap::DEFAULT.toggle_hud);
    }

    /// The chip anchors top-right on integer px boundaries, far smaller than
    /// the full panel.
    #[test]
    fn chip_is_anchored_top_right_and_integer_aligned() {
        let f = chip(State::with_mode(Mode::Horizontal), &[]);
        let margin = HudConfig::DEFAULT.geometry.margin;
        assert!(
            ((f.panel_left + f.panel_width) - (1920.0 - margin)).abs() <= 1.0,
            "right edge ≈ monitor right - margin"
        );
        assert!((f.panel_top - margin).abs() < 0.001);
        assert!(f.panel_left.fract().abs() < 0.001, "integer-aligned left");
        assert!(f.panel_width.fract().abs() < 0.001, "integer width");
        assert!(f.panel_height.fract().abs() < 0.001, "integer height");
        assert!(
            f.panel_width < 200.0 && f.panel_height < 40.0,
            "chip must be small: {}x{}",
            f.panel_width,
            f.panel_height
        );
    }

    /// Toasts append below the chip row and grow the panel height; no full
    /// expansion.
    #[test]
    fn chip_appends_toasts_below_status_and_grows_height() {
        let toast = HudNotification {
            class: NotificationClass::Info,
            message: "Overlay is off — Ctrl+Alt+H to show".to_string(),
            until_ms: i64::MAX,
        };
        let plain = chip(State::DEFAULT, &[]);
        let with_toast = chip(State::DEFAULT, &[toast]);
        assert_eq!(with_toast.rows.len(), 2);
        assert!(
            with_toast.rows[1].origin_y > with_toast.rows[0].origin_y,
            "toast sits below the status row"
        );
        assert!(
            with_toast.panel_height > plain.panel_height,
            "toast grows the chip panel"
        );
        assert!(
            with_toast.panel_width > plain.panel_width,
            "long toast widens the chip panel"
        );
        assert_eq!(
            with_toast.rows[1].color,
            HudConfig::DEFAULT.colors.accent,
            "Info toast uses the accent color"
        );
    }

    /// `HudTier::toggle` round-trips back to the original.
    #[test]
    fn hud_tier_toggle_is_involutive() {
        assert_eq!(HudTier::Chip.toggle(), HudTier::Full);
        assert_eq!(HudTier::Full.toggle(), HudTier::Chip);
    }

    /// The chip shows neither guide rows (Hotkeys) nor a divider.
    #[test]
    fn chip_has_no_guide_rows_and_no_rules() {
        let f = chip(State::with_mode(Mode::Horizontal), &[]);
        assert!(f.rules.is_empty(), "no divider on the chip");
        assert!(
            !f.rows.iter().any(|r| r.text.contains("Hotkeys")),
            "no guide on the chip"
        );
    }

    /// Pin notification rows' `origin_y`: they follow the hotkey help section
    /// after a section gap (last hotkey row `Quit` y = 480, then
    /// `+18 (telemetry) + 8 (row) + 8 (section - row)` = 514). Update if
    /// `HudConfig::DEFAULT` changes.
    #[test]
    fn notification_origin_y_pins_default_layout_arithmetic() {
        let n1 = HudNotification {
            class: NotificationClass::Info,
            message: "first".to_string(),
            until_ms: i64::MAX,
        };
        let n2 = HudNotification {
            class: NotificationClass::Warn,
            message: "second".to_string(),
            until_ms: i64::MAX,
        };
        let f = default_frame(State::with_mode(Mode::Horizontal), 60, &[n1, n2]);
        assert_eq!(f.rows.len(), 18, "16 baseline+hotkey + 2 notification rows");
        let actual_n1 = f.rows[16].origin_y;
        let actual_n2 = f.rows[17].origin_y;
        assert!(
            (actual_n1 - 514.0).abs() < 0.001,
            "notification[0] origin_y expected 514.0, got {actual_n1}"
        );
        assert!(
            (actual_n2 - 540.0).abs() < 0.001,
            "notification[1] origin_y expected 540.0, got {actual_n2}"
        );
    }

    /// Pin that the `hotkeys` argument's chord strings reach the hotkey rows
    /// (otherwise the help would always show DEFAULT chords).
    #[test]
    fn hotkey_help_rows_reflect_hotkey_map_argument() {
        // Use chords fully distinct from DEFAULT so a short prefix like
        // `Ctrl+Alt+R` can't substring-match e.g. `Ctrl+Alt+Right`.
        let custom = HotkeyMap {
            cycle_mode: "Ctrl+Shift+M",
            cycle_effect: "Ctrl+Shift+E",
            toggle_on_off: "Ctrl+Shift+V",
            thicker: "Ctrl+Shift+T",
            thinner: "Ctrl+Shift+N",
            more_opaque: "Ctrl+Shift+O",
            less_opaque: "Ctrl+Shift+S",
            toggle_hud: "Ctrl+Shift+D",
            quit: "Ctrl+Shift+X",
        };
        let f = hud_frame(
            State::with_mode(Mode::Horizontal),
            HudConfig::DEFAULT,
            monitor(),
            60,
            &[],
            custom,
            HudTelemetry::ZERO,
            HudTier::Full,
        );
        let texts: Vec<&str> = f.rows.iter().map(|r| r.text.as_str()).collect();
        let cycle_row = texts
            .iter()
            .find(|t| t.contains("Mode cycle"))
            .expect("cycle row");
        assert!(cycle_row.contains("Ctrl+Shift+M"), "cycle row: {cycle_row}");
        let effect_row = texts
            .iter()
            .find(|t| t.contains("Effect cycle"))
            .expect("effect row");
        assert!(
            effect_row.contains("Ctrl+Shift+E"),
            "effect row: {effect_row}"
        );
        let quit_row = texts.iter().find(|t| t.contains("Quit")).expect("quit row");
        assert!(quit_row.contains("Ctrl+Shift+X"), "quit row: {quit_row}");
        // No DEFAULT chord should surface once the custom map overrides it.
        for r in &f.rows {
            assert!(
                !r.text.contains("Ctrl+Alt+"),
                "custom map should never surface any DEFAULT Ctrl+Alt+* chord: {}",
                r.text
            );
        }
    }
}
