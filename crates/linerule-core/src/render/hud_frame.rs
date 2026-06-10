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
    /// Overall opacity (0.0-1.0); updated per-frame by `SetHudOpacity`.
    pub opacity: f32,
    /// Rows to draw.
    pub rows: Vec<HudRow>,
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
/// telemetry (Refresh Hz), hotkey help, then one row per notification.
///
/// `notifications` must already have expired entries removed; `hud_frame` does
/// no time checks, only layout.
///
/// # Examples
///
/// ```
/// use linerule_core::{HotkeyMap, HudConfig, HudTelemetry, Point, ScreenRect, State, hud_frame};
///
/// let monitor = ScreenRect::new(Point::new(0, 0), 1920, 1080);
/// let frame = hud_frame(
///     State::DEFAULT,
///     HudConfig::DEFAULT,
///     monitor,
///     144,
///     &[],
///     HotkeyMap::DEFAULT,
///     HudTelemetry::ZERO,
/// );
/// // Top-right anchor: panel right edge is `margin` left of the monitor right.
/// let expected_right = 1920.0 - HudConfig::DEFAULT.geometry.margin;
/// assert!((frame.panel_left + frame.panel_width - expected_right).abs() < 0.5);
/// // 6 baseline + 1 header + 8 hotkey rows = 15 rows (Quit included).
/// assert!(frame.rows.len() >= 15);
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
        format!("Mode: {}", mode_label(state.mode, state.visible)),
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

    cur.push(
        "Hotkeys".to_string(),
        hud.fonts.body,
        title,
        hud.colors.foreground,
    );
    cur.advance(hud.padding.row);
    let hotkey_lines: [(&str, &str); 8] = [
        ("Mode cycle", hotkeys.cycle_mode),
        ("Effect cycle", hotkeys.cycle_effect),
        ("Show/Hide", hotkeys.toggle_visible),
        ("Thicker", hotkeys.thicker),
        ("Thinner", hotkeys.thinner),
        ("More opaque", hotkeys.more_opaque),
        ("Less opaque", hotkeys.less_opaque),
        ("Quit", hotkeys.quit),
    ];
    for (label, chord) in hotkey_lines {
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
        opacity: hud.base_opacity,
        rows: cur.rows,
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

/// Fold mode + visible into one label; `visible == false` shows "Hidden".
const fn mode_label(mode: Mode, visible: bool) -> &'static str {
    if !visible {
        return "Hidden";
    }
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

    /// Call `hud_frame()` with default `HotkeyMap` / `HudTelemetry` arguments.
    fn default_frame(state: State, refresh_hz: u32, notifications: &[HudNotification]) -> HudFrame {
        hud_frame(
            state,
            HudConfig::DEFAULT,
            monitor(),
            refresh_hz,
            notifications,
            HotkeyMap::DEFAULT,
            HudTelemetry::ZERO,
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
    fn default_state_rows_are_present_and_ordered_top_to_bottom() {
        let f = default_frame(State::DEFAULT, 144, &[]);
        assert!(
            f.rows.len() >= 15,
            "expected at least 15 rows (6 baseline + 1 header + 8 hotkeys), got {}",
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
        let mut s = State::DEFAULT;
        s.mode = Mode::Horizontal;
        let f = default_frame(s, 60, &[]);
        assert!(
            f.rows.iter().any(|r| r.text == "Mode: Horizontal"),
            "rows: {:?}",
            f.rows
        );
    }

    #[test]
    fn hidden_state_overrides_mode_label() {
        let mut s = State::DEFAULT;
        s.mode = Mode::Horizontal;
        s.visible = false;
        let f = default_frame(s, 60, &[]);
        assert!(
            f.rows.iter().any(|r| r.text == "Mode: Hidden"),
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
        let f = default_frame(State::DEFAULT, 60, &[warn, info]);
        // 15 baseline+hotkey rows + 2 notifications.
        assert!(f.rows.len() >= 17, "rows: {:?}", f.rows);
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
        let f = default_frame(State::DEFAULT, 60, &[]);
        // 6 baseline + 1 hotkey header + 8 hotkey rows = 15.
        assert_eq!(f.rows.len(), 15);
    }

    /// Pin each row's `origin_y` against `HudConfig::DEFAULT`-derived layout
    /// arithmetic; update if `HudConfig::DEFAULT` changes.
    #[test]
    fn row_origin_y_pins_default_layout_arithmetic() {
        let f = default_frame(State::DEFAULT, 60, &[]);
        assert_eq!(
            f.rows.len(),
            15,
            "6 baseline + 1 header + 8 hotkeys expected"
        );

        // panel_top == monitor_top + margin.
        assert!(
            (f.panel_top - 24.0).abs() < 0.001,
            "panel_top expected 24.0, got {}",
            f.panel_top
        );

        // rows 0..6: baseline + Hotkeys header.
        let baseline_y = [48.0_f32, 88.0, 118.0, 146.0, 174.0, 210.0, 244.0];
        // rows 7..14: 8 hotkey rows, starting at 272 with +26 step.
        let hotkey_y = [272.0_f32, 298.0, 324.0, 350.0, 376.0, 402.0, 428.0, 454.0];
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

    /// Pin notification rows' `origin_y`: they follow the hotkey help section
    /// after a section gap. Update if `HudConfig::DEFAULT` changes.
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
        let f = default_frame(State::DEFAULT, 60, &[n1, n2]);
        assert_eq!(f.rows.len(), 17, "15 baseline+hotkey + 2 notification rows");
        let actual_n1 = f.rows[15].origin_y;
        let actual_n2 = f.rows[16].origin_y;
        assert!(
            (actual_n1 - 488.0).abs() < 0.001,
            "notification[0] origin_y expected 488.0, got {actual_n1}"
        );
        assert!(
            (actual_n2 - 514.0).abs() < 0.001,
            "notification[1] origin_y expected 514.0, got {actual_n2}"
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
            toggle_visible: "Ctrl+Shift+V",
            thicker: "Ctrl+Shift+T",
            thinner: "Ctrl+Shift+N",
            more_opaque: "Ctrl+Shift+O",
            less_opaque: "Ctrl+Shift+S",
            quit: "Ctrl+Shift+X",
        };
        let f = hud_frame(
            State::DEFAULT,
            HudConfig::DEFAULT,
            monitor(),
            60,
            &[],
            custom,
            HudTelemetry::ZERO,
        );
        let texts: Vec<&str> = f.rows.iter().map(|r| r.text.as_str()).collect();
        let cycle_row = texts
            .iter()
            .find(|t| t.contains("Mode cycle"))
            .expect("cycle row");
        assert!(cycle_row.contains("Ctrl+Shift+M"), "cycle row: {cycle_row}");
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
