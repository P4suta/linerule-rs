//! HUD frame の純粋 ADT とレイアウト関数。
//!
//! プラットフォーム側 (`linerule-platform-windows::hud_renderer`) が `DWrite` +
//! `D2D` で描画する際に必要な「パネル位置・背景・不透明度・テキスト行配置」
//! を提供する。テキスト描画自体は platform-windows 側の責務だが、レイアウト
//! 計算は純粋関数で記述して `linerule-core` の coverage / mutation testing
//! の対象に含める。
//!
//! [`crate::render::OverlayFrame`] (`Layer { Brush::Solid, Geometry::Rect }`) と
//! 分離している理由: HUD はテキスト描画 (`DWrite` 必須) のため `Layer` の閉じた
//! 表現に Text variant を足すと exhaustive match の意味が崩れ、
//! `composition_renderer` の `decompose` が単一型に「色塗りと文字描画」を混在
//! させる事故を起こすため。(ADR-0002 §5)

use serde::Serialize;

use crate::color::Rgba;
use crate::config::HudConfig;
use crate::geometry::{Logical, ScreenRect};
use crate::state::{Mode, State};

/// HUD パネル + 行群。プラットフォーム側はパネルを塗って各 row を描画する。
///
/// 座標系は logical pixel 上の `f32`。整数ピクセル境界に揃えるのは
/// プラットフォーム側の責務。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HudFrame {
    /// HUD パネルの左上 x (logical px)。
    pub panel_left: f32,
    /// HUD パネルの左上 y (logical px)。
    pub panel_top: f32,
    /// パネル幅 (logical px)。
    pub panel_width: f32,
    /// パネル高 (logical px)。
    pub panel_height: f32,
    /// パネル背景色。
    pub background: Rgba,
    /// 全体の不透明度 (0.0–1.0)。`SetHudOpacity` で per-frame に更新される。
    pub opacity: f32,
    /// 描画する行。
    pub rows: Vec<HudRow>,
}

/// HUD の 1 行。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HudRow {
    /// テキストレイアウト矩形の左上 x (logical px)。
    pub origin_x: f32,
    /// テキストレイアウト矩形の左上 y (logical px)。
    pub origin_y: f32,
    /// 描画する文字列。
    pub text: String,
    /// フォントサイズ (logical pt)。
    pub font_size: f32,
    /// フォント family のロジカルキー（platform 側で実 family 名に解決）。
    pub font: HudFontKey,
    /// 文字色。
    pub color: Rgba,
}

/// HUD で使うフォント family の論理キー。
///
/// プラットフォーム側で [`crate::config::HudFonts::title_family`] /
/// [`crate::config::HudFonts::mono_family`] に解決される。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HudFontKey {
    /// プロポーショナル系（タイトル・状態・本文）。
    Title,
    /// 等幅系（テレメトリ等の数値表示）。
    Mono,
}

/// HUD の panel 下端に表示する短寿命メッセージ。`Recoverable` な runtime error /
/// hotkey 競合 / device-lost rebuild 等の即時通知を出す経路。
///
/// `until_ms` は monotonic 時刻 (ms) — `now_ms >= until_ms` で `drain_expired_*`
/// により消去される。永続表示したい場合は `i64::MAX` を渡す (hotkey conflict
/// は config 経由なので永続)。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct HudNotification {
    /// メッセージの種類。色分け表示に使う。
    pub class: NotificationClass,
    /// 表示文字列 (例: `"Ctrl+Alt+R → already in use"`)。
    pub message: String,
    /// この notification が消える時刻 (ms, monotonic)。
    pub until_ms: i64,
}

/// [`HudNotification`] の種類。HUD palette とのマッピング:
///
/// - `Info` → `HudColors::accent`
/// - `Warn` → `HudColors::hint`
/// - `Error` → `Rgba::new(0xFF, 0x6B, 0x6B, 0xFF)` (palette 外、`hint` より強い赤)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationClass {
    /// 情報通知 (例: `"DPI changed to 150%"`)。
    Info,
    /// 警告 (例: hotkey 競合)。
    Warn,
    /// エラー (例: device-lost rebuild 失敗)。
    Error,
}

/// `State` + `HudConfig` + monitor + refresh Hz + notifications から HUD frame を
/// 組み立てる。
///
/// 配置はパネル右上にアンカー（モニタ右上から `geometry.margin` だけ離れた位置）。
/// 行は上から順に: title / status (Mode) / body (Thickness, Opacity) / divider /
/// telemetry (Refresh Hz) / 続けて notifications を 1 件 1 行で append。
///
/// `notifications` は呼び出し側で expire 済みを除去した snapshot を渡す前提
/// (`hud_frame` 自体は時刻判定をしない、純粋にレイアウトのみ)。
///
/// # Examples
///
/// ```
/// use linerule_core::{HudConfig, Point, ScreenRect, State, hud_frame};
///
/// let monitor = ScreenRect::new(Point::new(0, 0), 1920, 1080);
/// let frame = hud_frame(State::DEFAULT, HudConfig::DEFAULT, monitor, 144, &[]);
/// // 右上アンカー: パネル右端は monitor 右端から margin だけ左
/// let expected_right = 1920.0 - HudConfig::DEFAULT.geometry.margin;
/// assert!((frame.panel_left + frame.panel_width - expected_right).abs() < 0.5);
/// // 4 行以上 (title + status + thickness + opacity + telemetry)
/// assert!(frame.rows.len() >= 5);
/// ```
#[must_use]
pub fn hud_frame(
    state: State,
    hud: HudConfig,
    monitor: ScreenRect<Logical>,
    refresh_hz: u32,
    notifications: &[HudNotification],
) -> HudFrame {
    let panel_width = hud.geometry.width;
    let panel_height = hud.geometry.height;
    let margin = hud.geometry.margin;
    #[allow(
        clippy::cast_precision_loss,
        reason = "screen-space px は f32 mantissa に余裕で収まる"
    )]
    let monitor_right = (monitor.left() + i32::try_from(monitor.width).unwrap_or(i32::MAX)) as f32;
    #[allow(
        clippy::cast_precision_loss,
        reason = "screen-space px は f32 mantissa に余裕で収まる"
    )]
    let monitor_top = monitor.top() as f32;

    let panel_left = monitor_right - margin - panel_width;
    let panel_top = monitor_top + margin;

    let mut rows = Vec::with_capacity(6);
    let mut y = panel_top + hud.padding.edge;
    let x = panel_left + hud.padding.edge;

    // Title
    rows.push(HudRow {
        origin_x: x,
        origin_y: y,
        text: "linerule".to_string(),
        font_size: hud.fonts.title,
        font: HudFontKey::Title,
        color: hud.colors.foreground,
    });
    y += hud.fonts.title + hud.padding.section;

    // Status: Mode
    rows.push(HudRow {
        origin_x: x,
        origin_y: y,
        text: format!("Mode: {}", mode_label(state.mode, state.visible)),
        font_size: hud.fonts.status,
        font: HudFontKey::Title,
        color: hud.colors.foreground,
    });
    y += hud.fonts.status + hud.padding.row;

    // Body: Thickness
    rows.push(HudRow {
        origin_x: x,
        origin_y: y,
        text: format!("Thickness: {} px", state.config.thickness.get()),
        font_size: hud.fonts.body,
        font: HudFontKey::Title,
        color: hud.colors.subtle,
    });
    y += hud.fonts.body + hud.padding.row;

    // Body: Opacity
    rows.push(HudRow {
        origin_x: x,
        origin_y: y,
        text: format!("Opacity: {}", state.config.opacity.get()),
        font_size: hud.fonts.body,
        font: HudFontKey::Title,
        color: hud.colors.subtle,
    });
    y += hud.fonts.body + hud.padding.section;

    // Telemetry: Refresh Hz (mono family)
    rows.push(HudRow {
        origin_x: x,
        origin_y: y,
        text: format!("Refresh: {refresh_hz} Hz"),
        font_size: hud.fonts.telemetry,
        font: HudFontKey::Mono,
        color: hud.colors.accent,
    });
    y += hud.fonts.telemetry + hud.padding.section;

    // Notifications (短寿命 toast or 永続 conflict 表示)。
    // 行間は `padding.row`、 font は telemetry size を使う (status より控えめ)。
    for notification in notifications {
        rows.push(HudRow {
            origin_x: x,
            origin_y: y,
            text: notification.message.clone(),
            font_size: hud.fonts.telemetry,
            font: HudFontKey::Title,
            color: notification_color(notification.class, hud),
        });
        y += hud.fonts.telemetry + hud.padding.row;
    }

    HudFrame {
        panel_left,
        panel_top,
        panel_width,
        panel_height,
        background: hud.colors.background,
        opacity: hud.base_opacity,
        rows,
    }
}

/// [`NotificationClass`] を [`HudConfig::colors`] のパレットに mapping する。
const fn notification_color(class: NotificationClass, hud: HudConfig) -> Rgba {
    match class {
        NotificationClass::Info => hud.colors.accent,
        NotificationClass::Warn => hud.colors.hint,
        NotificationClass::Error => Rgba::new(0xFF, 0x6B, 0x6B, 0xFF),
    }
}

/// `State` の mode + visible を 1 つの表示ラベルに畳む。`visible == false` は
/// hidden を上書き表示。
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

    #[test]
    fn panel_anchored_top_right_with_margin() {
        let f = hud_frame(State::DEFAULT, HudConfig::DEFAULT, monitor(), 60, &[]);
        let expected_right = 1920.0_f32 - HudConfig::DEFAULT.geometry.margin;
        assert!((f.panel_left + f.panel_width - expected_right).abs() < 0.5);
        assert!((f.panel_top - HudConfig::DEFAULT.geometry.margin).abs() < 0.5);
    }

    #[test]
    fn default_state_rows_are_present_and_ordered_top_to_bottom() {
        let f = hud_frame(State::DEFAULT, HudConfig::DEFAULT, monitor(), 144, &[]);
        assert!(
            f.rows.len() >= 5,
            "expected at least 5 rows, got {}",
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

    #[test]
    fn mode_label_reflects_state() {
        let mut s = State::DEFAULT;
        s.mode = Mode::Horizontal;
        let f = hud_frame(s, HudConfig::DEFAULT, monitor(), 60, &[]);
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
        let f = hud_frame(s, HudConfig::DEFAULT, monitor(), 60, &[]);
        assert!(
            f.rows.iter().any(|r| r.text == "Mode: Hidden"),
            "rows: {:?}",
            f.rows
        );
    }

    #[test]
    fn refresh_hz_appears_in_telemetry_row_with_mono_font() {
        let f = hud_frame(State::DEFAULT, HudConfig::DEFAULT, monitor(), 144, &[]);
        let telemetry = f
            .rows
            .iter()
            .find(|r| r.text.contains("144"))
            .expect("refresh row");
        assert_eq!(telemetry.font, HudFontKey::Mono);
        assert!(telemetry.text.starts_with("Refresh:"));
    }

    #[test]
    fn opacity_reflects_base_opacity_from_config() {
        let f = hud_frame(State::DEFAULT, HudConfig::DEFAULT, monitor(), 60, &[]);
        assert!((f.opacity - HudConfig::DEFAULT.base_opacity).abs() < f32::EPSILON);
    }

    #[test]
    fn rows_fit_within_panel_horizontally() {
        let f = hud_frame(State::DEFAULT, HudConfig::DEFAULT, monitor(), 60, &[]);
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
    fn notifications_appended_below_telemetry_row() {
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
        let f = hud_frame(
            State::DEFAULT,
            HudConfig::DEFAULT,
            monitor(),
            60,
            &[warn, info],
        );
        // baseline 5 rows + 2 notifications = 7 rows or more
        assert!(f.rows.len() >= 7, "rows: {:?}", f.rows);
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
        let f = hud_frame(State::DEFAULT, HudConfig::DEFAULT, monitor(), 60, &[]);
        // baseline 5 rows (title + status + thickness + opacity + telemetry)
        assert_eq!(f.rows.len(), 5);
    }
}
