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
    /// パネル角丸半径 (logical px)。
    pub corner_radius: f32,
    /// 全体の不透明度 (0.0–1.0)。`SetHudOpacity` で per-frame に更新される。
    pub opacity: f32,
    /// テキスト以外の塗り矩形 (divider 等)。rows より先に描画される。
    pub rules: Vec<HudRule>,
    /// 描画する行。
    pub rows: Vec<HudRow>,
}

/// HUD 内の塗り矩形 1 本 (divider 等)。座標は rows と同じく絶対 logical px。
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct HudRule {
    /// 左上 x (logical px)。
    pub left: f32,
    /// 左上 y (logical px)。
    pub top: f32,
    /// 幅 (logical px)。
    pub width: f32,
    /// 高さ (logical px)。
    pub height: f32,
    /// 塗り色。
    pub color: Rgba,
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

/// HUD の表示形態。
///
/// - `Chip`: 常駐の極小 1 行ステータス (`H · 28px · 67%`)。デフォルト。
/// - `Full`: ホットキーガイドつきのフルパネル。起動直後の数秒と、
///   `ToggleHudDetail` ホットキーでの明示トグルでのみ表示される。
///
/// 「操作方法を教える UI」はフル側に集約し、普段の読書中は画面の主張を
/// チップ 1 行まで下げる、という設計判断 (カーソル=読書の道具なので
/// hover 系のトリガーは採らない)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HudTier {
    /// 常駐ステータスチップ。
    Chip,
    /// ホットキーガイドつきフルパネル。
    Full,
}

impl HudTier {
    /// チップ ⇄ フルの反転。
    #[must_use]
    pub const fn toggle(self) -> Self {
        match self {
            Self::Chip => Self::Full,
            Self::Full => Self::Chip,
        }
    }
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

/// HUD telemetry の per-tick snapshot。`hud_frame()` の telemetry 行に表示する
/// 3 指標を運ぶ純粋 ADT。platform 側 (`linerule-platform-windows::frame_timing`)
/// が計測した値を core に値で渡す (ADR-0012、選択肢 c)。
///
/// cs `HudTelemetry.cs` と同じ意味論:
/// - `tick_p99_ms`: 直近 N フレームの tick 経過時間の 99 パーセンタイル (ms)。
/// - `frames_dropped`: render budget (`warn_ratio` × frame budget) を超えた tick の累計。
/// - `commit_timeouts`: `IDCompositionDevice::Commit` 失敗 / timeout の累計。
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct HudTelemetry {
    /// 直近窓の p99 tick latency (ms)。
    pub tick_p99_ms: f32,
    /// budget 超過 frame の累計。
    pub frames_dropped: u64,
    /// dcomp commit 失敗の累計。
    pub commit_timeouts: u64,
}

impl HudTelemetry {
    /// 計測前のゼロ値。`hud_frame()` の telemetry 引数として渡せる sentinel。
    /// test helper / boot 直後の "no samples yet" 状態に使う。
    pub const ZERO: Self = Self {
        tick_p99_ms: 0.0,
        frames_dropped: 0,
        commit_timeouts: 0,
    };
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
///     linerule_core::HudTier::Full,
/// );
/// // 右上アンカー: パネル右端は monitor 右端から margin だけ左
/// let expected_right = 1920.0 - HudConfig::DEFAULT.geometry.margin;
/// assert!((frame.panel_left + frame.panel_width - expected_right).abs() < 0.5);
/// // 5 baseline + 1 header + 9 hotkey rows = 15 行 (Style / HUD detail / Quit 含む)
/// assert!(frame.rows.len() >= 15);
/// ```
#[must_use]
#[allow(
    clippy::too_many_arguments,
    reason = "引数は既存 (refresh_hz/notifications/hotkeys/telemetry) に表示形態\
              (tier) を 1 つ追加したもので、グルーピング struct を作るより\
              呼び出し側が読みやすい"
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
        HudTier::Chip => chip_frame(state, hud, monitor, notifications),
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

/// フルパネル (ホットキーガイドつき) のレイアウト。
#[allow(
    clippy::too_many_arguments,
    clippy::too_many_lines,
    reason = "row 構築は逐次的でラインアウト計算が局所的に追跡できる方が読みやすい。\
              分割すると `y` 累積を渡し回す必要があり可読性が落ちる"
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
        text: format!("Mode: {}", mode_label(state.mode)),
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

    // Telemetry (mono family). cs HudRenderer.cs:408 と byte-for-byte 一致:
    //   "{Hz}Hz · p99 {ms:F2}ms · drops {} · stalls {}"
    // `·` は U+00B7 MIDDLE DOT。Mono フォントで等幅整列を保つため "Hz" の前後の
    // 空白は cs と合わせる ("60Hz" は数値直後、 "p99 1.23ms" は数値前後ともスペース)。
    rows.push(HudRow {
        origin_x: x,
        origin_y: y,
        text: format!(
            "{refresh_hz}Hz · p99 {:.2}ms · drops {} · stalls {}",
            telemetry.tick_p99_ms, telemetry.frames_dropped, telemetry.commit_timeouts,
        ),
        font_size: hud.fonts.telemetry,
        font: HudFontKey::Mono,
        color: hud.colors.accent,
    });
    y += hud.fonts.telemetry + hud.padding.section;

    // ステータス群とホットキーガイドの間に 1px の divider を引く (palette の
    // divider 色)。section 余白の中点に置き、行レイアウトの y 累積は変えない。
    let rules = vec![HudRule {
        left: x,
        top: hud.padding.section.mul_add(-0.5, y),
        width: hud.padding.edge.mul_add(-2.0, panel_width),
        height: 1.0,
        color: hud.colors.divider,
    }];

    // Hotkey help section. C# 版相当の操作説明を panel に常時表示する。
    // section header (body サイズ, title font) → 8 hotkey rows (telemetry サイズ,
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

    let hotkey_lines: [(&str, &str); 9] = [
        ("Mode cycle", hotkeys.cycle_mode),
        ("On/Off", hotkeys.toggle_on_off),
        ("Thicker", hotkeys.thicker),
        ("Thinner", hotkeys.thinner),
        ("More opaque", hotkeys.more_opaque),
        ("Less opaque", hotkeys.less_opaque),
        ("Style", hotkeys.style_cycle),
        ("HUD detail", hotkeys.toggle_hud),
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
        corner_radius: hud.corner_radius,
        opacity: hud.base_opacity,
        rules,
        rows,
    }
}

/// 等幅フォントの平均文字送り (em 比)。チップ幅の概算に使う。DWrite の実測
/// なしで core 側がレイアウトを完結させるための見積もり値 — Cascadia Mono の
/// advance はおよそ 0.6em で、余裕側 (広め) に倒してある。
const MONO_ADVANCE_RATIO: f32 = 0.62;

/// 常駐チップ (1 行ステータス + 直下の toast 行) のレイアウト。
///
/// フルパネルと違い、幅は内容にフィットさせる (`MONO_ADVANCE_RATIO` 概算)。
/// パネル座標・寸法は整数 logical px に丸め、小サイズのテキストが sub-pixel
/// 配置でにじまないようにする。
fn chip_frame(
    state: State,
    hud: HudConfig,
    monitor: ScreenRect<Logical>,
    notifications: &[HudNotification],
) -> HudFrame {
    let chip = hud.chip;
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

    let status = chip_text(state);

    // 幅 = ステータス行と toast 行のうち最長のもの (概算) + 左右 padding。
    let mut text_width = estimate_mono_width(&status, chip.font_size);
    for n in notifications {
        text_width = text_width.max(estimate_mono_width(&n.message, chip.font_size));
    }
    let panel_width = chip.pad_x.mul_add(2.0, text_width).ceil();

    let row_advance = chip.font_size + hud.padding.row;
    #[allow(
        clippy::cast_precision_loss,
        reason = "notification 件数は一桁オーダー"
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

/// 等幅前提のテキスト幅概算 (logical px)。
fn estimate_mono_width(text: &str, font_size: f32) -> f32 {
    #[allow(clippy::cast_precision_loss, reason = "HUD テキストは数十文字オーダー")]
    let chars = text.chars().count() as f32;
    chars * font_size * MONO_ADVANCE_RATIO
}

/// チップのステータス文字列: `H · 28px · 67%` (Off 中は `Off`)。
/// `%` は知覚値ではなく保存バイトの百分率 (0xAA → 67%)。
fn chip_text(state: State) -> String {
    let letter = match state.mode {
        Mode::Off => return "Off".to_string(),
        Mode::Horizontal => "H",
        Mode::Vertical => "V",
    };
    let opacity_pct = percent_of_byte(state.config.opacity.get());
    format!(
        "{letter} · {}px · {opacity_pct}%",
        state.config.thickness.get()
    )
}

/// `byte / 255` の百分率 (四捨五入)。
fn percent_of_byte(byte: u8) -> u32 {
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "0..=255 を 100 倍しても f32 で exact、round 後は 0..=100"
    )]
    let pct = (f32::from(byte) * 100.0 / 255.0).round() as u32;
    pct
}

/// [`NotificationClass`] を [`HudConfig::colors`] のパレットに mapping する。
const fn notification_color(class: NotificationClass, hud: HudConfig) -> Rgba {
    match class {
        NotificationClass::Info => hud.colors.accent,
        NotificationClass::Warn => hud.colors.hint,
        NotificationClass::Error => Rgba::new(0xFF, 0x6B, 0x6B, 0xFF),
    }
}

/// `Mode` を表示ラベルに畳む。「非表示」は `Mode::Off` の一表現のみ。
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

    /// `hud_frame()` を default 引数 (フル表示) で呼ぶ test helper。Phase ζ で
    /// hotkeys 引数が必須化、PR 4 で telemetry 引数が必須化されたため、12+ 件の
    /// test を一行で書き直せるよう小さな wrapper を置く。telemetry は `ZERO` 既定。
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

    /// チップ表示の test helper。
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
    fn default_state_rows_are_present_and_ordered_top_to_bottom() {
        let f = default_frame(State::DEFAULT, 144, &[]);
        assert!(
            f.rows.len() >= 15,
            "expected at least 15 rows (5 baseline + 1 header + 9 hotkeys), got {}",
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

    /// cs `HudRenderer.cs:408` の format 文字列と byte-for-byte 一致するか pin する。
    /// `{Hz}Hz · p99 {ms:F2}ms · drops {} · stalls {}`。`·` は U+00B7 MIDDLE DOT。
    /// telemetry の値が反映されているかも同時に確認する。
    #[test]
    fn telemetry_line_format_matches_cs() {
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

    /// `HudTelemetry::ZERO` の serde round-trip。Snapshot 互換のため公開 ADT の
    /// serialize 安定性を pin する。
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
        // baseline 5 + hotkey help 1+9 = 15 + 2 notifications = 17 rows or more
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
        // baseline 5 (title + status + thickness + opacity + telemetry)
        // + 1 hotkey help header + 9 hotkey rows (cycle / on-off / thicker /
        // thinner / more / less / style / hud-detail / quit) = 15 rows
        assert_eq!(f.rows.len(), 15);
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
    /// - row 7..14 (Hotkey rows)  `y{n+1} = y{n} + 18 (telemetry) + 8 (row)` = `+26 each`
    ///
    /// `HudConfig::DEFAULT` 自体が変わったらこの test を更新する (回帰検知の
    /// 重みを残すために、寛容な許容差ではなく `EPSILON` 級で pin する)。
    #[test]
    fn row_origin_y_pins_default_layout_arithmetic() {
        let f = default_frame(State::DEFAULT, 60, &[]);
        assert_eq!(
            f.rows.len(),
            15,
            "5 baseline + 1 header + 9 hotkeys expected"
        );

        // `panel_top` itself は monitor_top + margin。
        assert!(
            (f.panel_top - 24.0).abs() < 0.001,
            "panel_top expected 24.0, got {}",
            f.panel_top
        );

        // row 0..5: baseline + Hotkeys header
        let baseline_y = [48.0_f32, 88.0, 118.0, 146.0, 182.0, 216.0];
        // row 6..14: 9 hotkey rows, starting at 244 with +26 step
        let hotkey_y = [
            244.0_f32, 270.0, 296.0, 322.0, 348.0, 374.0, 400.0, 426.0, 452.0,
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

    /// divider rule を pin する: telemetry 行とホットキーガイドの間の section
    /// 余白 (16px) の中点 = `216 - 8 = 208` に置かれ、左右 edge padding 内側を
    /// 横断する 1px の palette divider 色。
    #[test]
    fn divider_rule_pins_position_and_color() {
        let f = default_frame(State::DEFAULT, 60, &[]);
        assert_eq!(f.rules.len(), 1, "exactly one divider rule expected");
        let rule = f.rules[0];
        assert!(
            (rule.top - 208.0).abs() < 0.001,
            "divider top expected 208.0, got {}",
            rule.top
        );
        assert!((rule.height - 1.0).abs() < 0.001);
        let edge = HudConfig::DEFAULT.padding.edge;
        assert!((rule.left - (f.panel_left + edge)).abs() < 0.001);
        assert!((rule.width - edge.mul_add(-2.0, f.panel_width)).abs() < 0.001);
        assert_eq!(rule.color, HudConfig::DEFAULT.colors.divider);
    }

    /// `corner_radius` が config からそのまま frame に流れることを pin する。
    #[test]
    fn corner_radius_flows_from_config() {
        let f = default_frame(State::DEFAULT, 60, &[]);
        assert!((f.corner_radius - HudConfig::DEFAULT.corner_radius).abs() < f32::EPSILON);
    }

    // ---- chip tier ---------------------------------------------------------

    /// Off 中のチップは `Off` 1 行のみ。
    #[test]
    fn chip_shows_off_label_when_mode_is_off() {
        let f = chip(State::DEFAULT, &[]);
        assert_eq!(f.rows.len(), 1);
        assert_eq!(f.rows[0].text, "Off");
        assert_eq!(f.rows[0].font, HudFontKey::Mono);
    }

    /// アクティブ中のチップは `H · 28px · 67%` 形式 (0xAA → 67%)。
    #[test]
    fn chip_status_text_format_is_pinned() {
        let f = chip(State::with_mode(Mode::Horizontal), &[]);
        assert_eq!(f.rows[0].text, "H · 28px · 67%");
        let v = chip(State::with_mode(Mode::Vertical), &[]);
        assert!(
            v.rows[0].text.starts_with("V · "),
            "text: {}",
            v.rows[0].text
        );
    }

    /// チップは右上アンカーかつ整数 px 境界に乗る。フルパネルよりずっと小さい。
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

    /// toast はチップ行の下に追記され、パネル高が伸びる。フル展開はしない。
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

    /// `HudTier::toggle` は往復で元に戻る。
    #[test]
    fn hud_tier_toggle_is_involutive() {
        assert_eq!(HudTier::Chip.toggle(), HudTier::Full);
        assert_eq!(HudTier::Full.toggle(), HudTier::Chip);
    }

    /// チップにはガイド行 (Hotkeys) も divider も出ない。
    #[test]
    fn chip_has_no_guide_rows_and_no_rules() {
        let f = chip(State::with_mode(Mode::Horizontal), &[]);
        assert!(f.rules.is_empty(), "no divider on the chip");
        assert!(
            !f.rows.iter().any(|r| r.text.contains("Hotkeys")),
            "no guide on the chip"
        );
    }

    /// notification rows の `origin_y` を pin する。hotkey help section の後ろに
    /// section 余白を挟んで notifications が並ぶ。
    ///
    /// 期待値:
    /// - 最後の hotkey row (Quit) の y = 452 (上 test 参照)
    /// - hotkey loop 終了後 `y += telemetry(18) + row(8) + (section - row)(8)` = +34 → 486
    /// - notification[0] y = 486
    /// - notification[1] y = `486 + 18 (telemetry) + 8 (row)` = `512`
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
            (actual_n1 - 486.0).abs() < 0.001,
            "notification[0] origin_y expected 486.0, got {actual_n1}"
        );
        assert!(
            (actual_n2 - 512.0).abs() < 0.001,
            "notification[1] origin_y expected 512.0, got {actual_n2}"
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
            toggle_on_off: "Ctrl+Shift+V",
            thicker: "Ctrl+Shift+T",
            thinner: "Ctrl+Shift+N",
            more_opaque: "Ctrl+Shift+O",
            less_opaque: "Ctrl+Shift+S",
            style_cycle: "Ctrl+Shift+Y",
            toggle_hud: "Ctrl+Shift+D",
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
            HudTier::Full,
        );
        let texts: Vec<&str> = f.rows.iter().map(|r| r.text.as_str()).collect();
        // hotkey rows は telemetry 行の後の 1 header + 7 rows
        let cycle_row = texts
            .iter()
            .find(|t| t.contains("Mode cycle"))
            .expect("cycle row");
        assert!(cycle_row.contains("Ctrl+Shift+M"), "cycle row: {cycle_row}");
        let style_row = texts
            .iter()
            .find(|t| t.contains("Style"))
            .expect("style row");
        assert!(style_row.contains("Ctrl+Shift+Y"), "style row: {style_row}");
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
