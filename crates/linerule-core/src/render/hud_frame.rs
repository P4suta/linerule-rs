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
use crate::input::hotkey_map::HotkeyMap;
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
/// use linerule_core::{HotkeyMap, HudConfig, Point, ScreenRect, State, hud_frame};
///
/// let monitor = ScreenRect::new(Point::new(0, 0), 1920, 1080);
/// let frame = hud_frame(
///     State::DEFAULT,
///     HudConfig::DEFAULT,
///     monitor,
///     144,
///     &[],
///     HotkeyMap::DEFAULT,
/// );
/// // 右上アンカー: パネル右端は monitor 右端から margin だけ左
/// let expected_right = 1920.0 - HudConfig::DEFAULT.geometry.margin;
/// assert!((frame.panel_left + frame.panel_width - expected_right).abs() < 0.5);
/// // 5 baseline + 1 header + 7 hotkey rows = 13 行 (Quit 含む)
/// assert!(frame.rows.len() >= 13);
/// ```
#[must_use]
#[allow(
    clippy::too_many_lines,
    reason = "row 構築は逐次的でラインアウト計算が局所的に追跡できる方が読みやすい。\
              分割すると `y` 累積を渡し回す必要があり可読性が落ちる"
)]
pub fn hud_frame(
    state: State,
    hud: HudConfig,
    monitor: ScreenRect<Logical>,
    refresh_hz: u32,
    notifications: &[HudNotification],
    hotkeys: HotkeyMap,
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

    // Hotkey help section. C# 版相当の操作説明を panel に常時表示する。
    // section header (body サイズ, title font) → 7 hotkey rows (telemetry サイズ,
    // mono font) で chord 表記を揃える。Quit は emergency 退避手段なので必ず出す。
    rows.push(HudRow {
        origin_x: x,
        origin_y: y,
        text: "Hotkeys".to_string(),
        font_size: hud.fonts.body,
        font: HudFontKey::Title,
        color: hud.colors.foreground,
    });
    y += hud.fonts.body + hud.padding.row;

    let hotkey_lines: [(&str, &str); 7] = [
        ("Mode cycle", hotkeys.cycle_mode),
        ("Show/Hide", hotkeys.toggle_visible),
        ("Thicker", hotkeys.thicker),
        ("Thinner", hotkeys.thinner),
        ("More opaque", hotkeys.more_opaque),
        ("Less opaque", hotkeys.less_opaque),
        ("Quit", hotkeys.quit),
    ];
    for (label, chord) in hotkey_lines {
        rows.push(HudRow {
            origin_x: x,
            origin_y: y,
            text: format!("{label:<12} {chord}"),
            font_size: hud.fonts.telemetry,
            font: HudFontKey::Mono,
            color: hud.colors.subtle,
        });
        y += hud.fonts.telemetry + hud.padding.row;
    }
    // section の終わりに余白を入れて notifications との視認分離を作る
    y += hud.padding.section - hud.padding.row;

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

    /// `hud_frame()` を default 引数で呼ぶ test helper。Phase ζ で hotkeys 引数が
    /// 必須化されたため、12+ 件の test を一行で書き直せるよう小さな wrapper を置く。
    fn default_frame(state: State, refresh_hz: u32, notifications: &[HudNotification]) -> HudFrame {
        hud_frame(
            state,
            HudConfig::DEFAULT,
            monitor(),
            refresh_hz,
            notifications,
            HotkeyMap::DEFAULT,
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
            f.rows.len() >= 13,
            "expected at least 13 rows (5 baseline + 1 header + 7 hotkeys), got {}",
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
            .find(|r| r.text.contains("144"))
            .expect("refresh row");
        assert_eq!(telemetry.font, HudFontKey::Mono);
        assert!(telemetry.text.starts_with("Refresh:"));
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
        // baseline 5 + hotkey help 1+7 = 13 + 2 notifications = 15 rows or more
        assert!(f.rows.len() >= 15, "rows: {:?}", f.rows);
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
        // baseline 5 (title + status + thickness + opacity + telemetry)
        // + 1 hotkey help header + 7 hotkey rows (cycle / show / thicker /
        // thinner / more / less / quit) = 13 rows
        assert_eq!(f.rows.len(), 13);
    }

    /// 各 row の `origin_y` を `HudConfig::DEFAULT` 由来の算術で pin する。
    ///
    /// `hud_frame` 内部の `y += font + padding` 累積が単一の `+=` / `+`
    /// 演算子変更 (mutation) でズレた場合に確実に検知するための回帰テスト。
    /// 既存の ordering test (`origin_y[i] <= origin_y[i+1]`) は ordering を
    /// 守るが値域を pin しないので、`+= title + section` を `*= title + section`
    /// に変えるような mutation を捕捉できなかった (Phase ε mutation baseline)。
    ///
    /// 期待値はすべて `HudConfig::DEFAULT` から手計算:
    /// - `panel_top` = `monitor_top + margin` = `0 + 24` = `24`
    /// - row 0 (Title)            `y0 = 24 + 24 (edge)` = `48`
    /// - row 1 (Status)           `y1 = 48 + 24 (title font) + 16 (section)` = `88`
    /// - row 2 (Thickness)        `y2 = 88 + 22 (status font) + 8 (row)` = `118`
    /// - row 3 (Opacity)          `y3 = 118 + 20 (body font) + 8 (row)` = `146`
    /// - row 4 (Telemetry)        `y4 = 146 + 20 (body font) + 16 (section)` = `182`
    /// - row 5 (Hotkeys header)   `y5 = 182 + 18 (telemetry) + 16 (section)` = `216`
    /// - row 6 (Mode cycle)       `y6 = 216 + 20 (body font) + 8 (row)` = `244`
    /// - row 7..12 (Hotkey rows)  `y{n+1} = y{n} + 18 (telemetry) + 8 (row)` = `+26 each`
    ///
    /// `HudConfig::DEFAULT` 自体が変わったらこの test を更新する (回帰検知の
    /// 重みを残すために、寛容な許容差ではなく `EPSILON` 級で pin する)。
    #[test]
    fn row_origin_y_pins_default_layout_arithmetic() {
        let f = default_frame(State::DEFAULT, 60, &[]);
        assert_eq!(
            f.rows.len(),
            13,
            "5 baseline + 1 header + 7 hotkeys expected"
        );

        // `panel_top` itself は monitor_top + margin。
        assert!(
            (f.panel_top - 24.0).abs() < 0.001,
            "panel_top expected 24.0, got {}",
            f.panel_top
        );

        // row 0..5: baseline + Hotkeys header
        let baseline_y = [48.0_f32, 88.0, 118.0, 146.0, 182.0, 216.0];
        // row 6..12: 7 hotkey rows, starting at 244 with +26 step
        let hotkey_y = [244.0_f32, 270.0, 296.0, 322.0, 348.0, 374.0, 400.0];
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

    /// notification rows の `origin_y` を pin する。hotkey help section の後ろに
    /// section 余白を挟んで notifications が並ぶ。
    ///
    /// 期待値:
    /// - 最後の hotkey row (Quit) の y = 400 (上 test 参照)
    /// - hotkey loop 終了後 `y += telemetry(18) + row(8) + (section - row)(8)` = +34 → 434
    /// - notification[0] y = 434
    /// - notification[1] y = `434 + 18 (telemetry) + 8 (row)` = `460`
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
        assert_eq!(f.rows.len(), 15, "13 baseline+hotkey + 2 notification rows");
        let actual_n1 = f.rows[13].origin_y;
        let actual_n2 = f.rows[14].origin_y;
        assert!(
            (actual_n1 - 434.0).abs() < 0.001,
            "notification[0] origin_y expected 434.0, got {actual_n1}"
        );
        assert!(
            (actual_n2 - 460.0).abs() < 0.001,
            "notification[1] origin_y expected 460.0, got {actual_n2}"
        );
    }

    /// `hotkeys` 引数で渡した chord 文字列が各 hotkey row に正しく反映されることを
    /// pin する。custom `HotkeyMap` を渡したら row の text が変わることを確認 (これが
    /// 効かないと「HUD 操作説明が常に DEFAULT 表示」という degenerate state が
    /// 発生する。Phase ζ の主要機能の retainer test)。
    #[test]
    fn hotkey_help_rows_reflect_hotkey_map_argument() {
        // DEFAULT と完全に異なる chord にして substring 混同を避ける (`Ctrl+Alt+R`
        // のような短い prefix が `Ctrl+Alt+Right` にマッチする問題を回避)。
        let custom = HotkeyMap {
            cycle_mode: "Ctrl+Shift+M",
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
        );
        let texts: Vec<&str> = f.rows.iter().map(|r| r.text.as_str()).collect();
        // hotkey rows は telemetry 行の後の 1 header + 7 rows
        let cycle_row = texts
            .iter()
            .find(|t| t.contains("Mode cycle"))
            .expect("cycle row");
        assert!(cycle_row.contains("Ctrl+Shift+M"), "cycle row: {cycle_row}");
        let quit_row = texts.iter().find(|t| t.contains("Quit")).expect("quit row");
        assert!(quit_row.contains("Ctrl+Shift+X"), "quit row: {quit_row}");
        // DEFAULT chord は custom map に上書きされて表面化しないこと
        for r in &f.rows {
            assert!(
                !r.text.contains("Ctrl+Alt+"),
                "custom map should never surface any DEFAULT Ctrl+Alt+* chord: {}",
                r.text
            );
        }
    }
}
