//! Overlay HWND ごとに 1 つ存在する instance state。
//!
//! `Box::into_raw` で確保したアドレスが `GWLP_USERDATA` に格納され、WndProc
//! から `win32_ffi::get_userdata` 経由で `NonNull<OverlayWndState>` として
//! 取り出される。本ファイル自体は `#![forbid(unsafe_code)]`。
//!
//! ## RefCell 不変条件
//!
//! 可変フィールド (renderers / `tick_world` / hotkey maps / notifications 等) は
//! [`RefCell`] で保持される。WndProc は単一 UI thread からのみ呼ばれるため、
//! 通常の RefCell 規則に従えば安全に共有できる。ただし以下を守ること:
//!
//! - `borrow_mut()` の保持中に Win32 API のうち **同期再入** を起こすものを
//!   呼ばないこと。具体的には `SendMessageW` / `DestroyWindow` / `MessageBoxW`
//!   系 / `BringWindowToTop` 等は同 stack で `WM_*` を発火する。`PostMessageW`
//!   は async なので OK。
//! - 違反は [`RefCell::borrow_mut`] の panic で必ず検出され、
//!   `overlay_wnd_proc` の `catch_unwind` で吸収される（visual が一瞬欠ける
//!   程度の影響に閉じる）。
//!
//! ## RAII
//!
//! `Box<OverlayWndState>` が `WM_NCDESTROY` 経由で `take_userdata` により
//! reclaim されると、renderers が抱える COM オブジェクトも Drop で確実に
//! Release される。

#![forbid(unsafe_code)]

use std::cell::{Cell, OnceCell, RefCell};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Instant;

use linerule_core::input::tick::TickWorld;
use linerule_core::{
    ChordError, HotkeyMap, HudConfig, HudNotification, Logical, NotificationClass, OverlayAction,
    ScreenRect,
};
use tracing::Span;
use windows::Win32::Foundation::HWND;

use crate::winrt_composition_renderer::WinrtCompositionRenderer;
use crate::winrt_hud_renderer::WinrtHudRenderer;

/// 1 つの hotkey 登録に失敗した理由。HUD に列挙表示するために保持する。
#[derive(Debug, Clone)]
pub struct HotkeyConflict {
    /// ユーザー設定の chord 文字列（例: `"Ctrl+Alt+R"`）。
    pub spec: &'static str,
    /// この chord が割り当てられていた action。
    pub action: OverlayAction,
    /// 失敗理由。
    pub reason: HotkeyFailure,
}

/// `HotkeyConflict::reason` のバリアント。
#[derive(Debug, Clone)]
pub enum HotkeyFailure {
    /// chord 文字列の解析に失敗。
    ChordParse(ChordError),
    /// `RegisterHotKey` 失敗（多くは `ERROR_HOTKEY_ALREADY_REGISTERED`）。
    RegisterHotKey {
        /// 失敗時の HRESULT / GetLastError 値（参考情報）。
        hresult: i32,
    },
}

/// Overlay + HUD renderers. The HUD borrows the overlay's WinRT pipeline, so
/// `hud` is declared first to Drop (and Release its COM objects) before
/// `overlay`. Each keeps its own `RefCell` for independent borrows.
struct Renderers {
    hud: RefCell<Option<WinrtHudRenderer>>,
    overlay: RefCell<Option<WinrtCompositionRenderer>>,
}

/// Hotkey subsystem state: the action channel plus the id→action lookup, the
/// display chord map, and the registration-conflict list. Each mutable field
/// keeps its own `RefCell`.
struct Hotkeys {
    /// `WM_HOTKEY` sends actions here; `Sender::send` takes `&self`.
    sender: Sender<OverlayAction>,
    /// `WM_APP_TICK` drains actions from here; `try_recv` takes `&self`.
    inbox: Receiver<OverlayAction>,
    /// hotkey id → action, filled once by `register_hotkeys`, then read-only.
    id_to_action: RefCell<HashMap<i32, OverlayAction>>,
    /// Chord display strings for the HUD hotkey-help rows.
    display_map: RefCell<HotkeyMap>,
    /// Chords whose registration / parse failed, shown as persistent HUD warnings.
    conflicts: RefCell<Vec<HotkeyConflict>>,
}

/// WndProc instance state, referenced from every message handler.
///
/// Field declaration order is Drop order. `Renderers` is placed where the
/// renderers used to be so the HUD still Drops before the overlay.
pub struct OverlayWndState {
    log_span: Span,
    nchit_count: AtomicU64,
    click_count: AtomicU64,
    renderers: Renderers,
    /// Pure tick-pipeline state, `borrow_mut` per `WM_APP_TICK`.
    tick_world: RefCell<TickWorld>,
    hotkeys: Hotkeys,
    /// Active monitor bounds, re-resolved per tick from the cursor; the HUD
    /// panel anchors to it.
    monitor: RefCell<ScreenRect<Logical>>,
    /// HUD look / timing config.
    hud_config: HudConfig,
    /// Short-lived runtime toasts (device-lost rebuild / DPI change), evicted on
    /// expiry by `live_notifications`.
    notifications: RefCell<Vec<HudNotification>>,
    /// HUD telemetry tracker (p99 / drops / commit timeouts).
    frame_timing: RefCell<crate::frame_timing::FrameTimingTracker>,
    /// Consecutive device-lost failures; reset on success, Quit at 3.
    device_lost_count: Cell<u8>,
    /// Overlay HWND, set once after `CreateWindowExW`; reused by device-lost
    /// rebuild.
    hwnd: OnceCell<HWND>,
    /// Process start, the origin for `now_ms`.
    start_time: Instant,
}

impl OverlayWndState {
    /// 新しい instance state を構築する。デフォルトの `TickWorld::INITIAL`
    /// (mode = Off) で初期化される。
    #[must_use]
    pub fn new(log_span: Span, monitor: ScreenRect<Logical>, hud_config: HudConfig) -> Self {
        Self::new_with_initial_world(log_span, monitor, hud_config, TickWorld::INITIAL)
    }

    /// 任意の `TickWorld` で初期化する。`--initial-mode` 等で起動時 mode を
    /// 上書きする経路で使う (CI smoke test が slit 描画パスを cover するため
    /// `Mode::Horizontal` で起動する用途)。
    #[must_use]
    pub fn new_with_initial_world(
        log_span: Span,
        monitor: ScreenRect<Logical>,
        hud_config: HudConfig,
        initial_world: TickWorld,
    ) -> Self {
        let (sender, inbox) = channel::<OverlayAction>();
        Self {
            log_span,
            nchit_count: AtomicU64::new(0),
            click_count: AtomicU64::new(0),
            renderers: Renderers {
                hud: RefCell::new(None),
                overlay: RefCell::new(None),
            },
            tick_world: RefCell::new(initial_world),
            hotkeys: Hotkeys {
                sender,
                inbox,
                id_to_action: RefCell::new(HashMap::new()),
                display_map: RefCell::new(HotkeyMap::DEFAULT),
                conflicts: RefCell::new(Vec::new()),
            },
            monitor: RefCell::new(monitor),
            hud_config,
            notifications: RefCell::new(Vec::new()),
            frame_timing: RefCell::new(crate::frame_timing::FrameTimingTracker::new()),
            device_lost_count: Cell::new(0),
            hwnd: OnceCell::new(),
            start_time: Instant::now(),
        }
    }

    /// HUD telemetry tracker への可変アクセス。`wndproc` から record_tick /
    /// record_timeout / snapshot を呼ぶ。
    #[must_use]
    pub fn frame_timing(&self) -> &RefCell<crate::frame_timing::FrameTimingTracker> {
        &self.frame_timing
    }

    /// 短寿命 toast を queue する。`lifetime_ms` 経過後に `live_notifications`
    /// で除去される。永続させたい場合は `i64::MAX` を渡す。
    pub fn push_notification(&self, class: NotificationClass, message: String, lifetime_ms: i64) {
        let until_ms = self.now_ms().saturating_add(lifetime_ms);
        self.notifications.borrow_mut().push(HudNotification {
            class,
            message,
            until_ms,
        });
    }

    /// `now_ms` 時点で expire していない toast の snapshot を返す。expire 済みは
    /// 同時に内部から除去する (`retain` で eviction)。
    pub fn live_notifications(&self) -> Vec<HudNotification> {
        let now = self.now_ms();
        let mut q = self.notifications.borrow_mut();
        q.retain(|n| now < n.until_ms);
        q.clone()
    }

    /// この HWND の tracing span を借りる。
    pub fn span(&self) -> &Span {
        &self.log_span
    }

    /// `WM_NCHITTEST` を 1 回受信したとカウントし、サンプリング閾値（先頭 3 件
    /// または 200 件ごと）に該当すれば `true` を返す。
    #[must_use]
    pub fn tick_nchit(&self) -> Option<u64> {
        let n = self.nchit_count.fetch_add(1, Ordering::Relaxed) + 1;
        if n <= 3 || n.is_multiple_of(200) {
            Some(n)
        } else {
            None
        }
    }

    /// `WM_LBUTTONDOWN` 系を受信した（= click-through 失敗）。
    pub fn tick_click(&self) -> u64 {
        self.click_count.fetch_add(1, Ordering::Relaxed) + 1
    }

    /// `attach_compositor` で構築された overlay renderer を仕込む。
    pub fn install_renderer(&self, renderer: WinrtCompositionRenderer) {
        *self.renderers.overlay.borrow_mut() = Some(renderer);
    }

    /// レンダラへの可変アクセス（WndProc の `WM_APP_TICK` ハンドラから利用）。
    pub fn renderer(&self) -> &RefCell<Option<WinrtCompositionRenderer>> {
        &self.renderers.overlay
    }

    /// `attach_compositor` で構築された HUD renderer を仕込む。
    pub fn install_hud_renderer(&self, renderer: WinrtHudRenderer) {
        *self.renderers.hud.borrow_mut() = Some(renderer);
    }

    /// HUD レンダラへの可変アクセス（`RefreshHud` / `SetHudOpacity` 効果適用用）。
    pub fn hud_renderer(&self) -> &RefCell<Option<WinrtHudRenderer>> {
        &self.renderers.hud
    }

    /// 現在の tick world snapshot を取り出す。
    #[must_use]
    pub fn tick_world_snapshot(&self) -> TickWorld {
        *self.tick_world.borrow()
    }

    /// tick world を書き戻す。
    pub fn store_tick_world(&self, world: TickWorld) {
        *self.tick_world.borrow_mut() = world;
    }

    /// hotkey sender を借りる（`WM_HOTKEY` ハンドラから利用）。
    pub fn hotkey_sender(&self) -> &Sender<OverlayAction> {
        &self.hotkeys.sender
    }

    /// hotkey id に対応する `OverlayAction` を引く。
    #[must_use]
    pub fn action_for(&self, id: i32) -> Option<OverlayAction> {
        self.hotkeys.id_to_action.borrow().get(&id).copied()
    }

    /// `register_hotkeys` から hotkey id と action の対応を仕込む。
    ///
    /// debug build では同 id を二重登録すると `debug_assert!` で即捕捉する。
    /// release では `HashMap::insert` の上書き挙動に委ねる (last-write-wins)。
    pub fn record_hotkey(&self, id: i32, action: OverlayAction) {
        let prev = self.hotkeys.id_to_action.borrow_mut().insert(id, action);
        debug_assert!(
            prev.is_none(),
            "duplicate hotkey id {id} registered (prev action: {prev:?}); \
             this is a bug — each `RegisterHotKey` call must use a unique id"
        );
    }

    /// 現在登録済みの hotkey id 一覧。Drop で `UnregisterHotKey` する際に使う。
    pub fn registered_hotkey_ids(&self) -> Vec<i32> {
        self.hotkeys.id_to_action.borrow().keys().copied().collect()
    }

    /// hotkey 登録失敗を記録する。
    pub fn record_hotkey_conflict(&self, conflict: HotkeyConflict) {
        self.hotkeys.conflicts.borrow_mut().push(conflict);
    }

    /// hotkey 競合の一覧。
    pub fn hotkey_conflicts(&self) -> Vec<HotkeyConflict> {
        self.hotkeys.conflicts.borrow().clone()
    }

    /// 受信 channel から OverlayAction を drain する。
    pub fn drain_hotkeys(&self) -> Vec<OverlayAction> {
        let mut out = Vec::new();
        while let Ok(a) = self.hotkeys.inbox.try_recv() {
            out.push(a);
        }
        out
    }

    /// 現在の active monitor bounds の snapshot。`ScreenRect` は `Copy` なので
    /// 借用のたびに値を取り出して返す。
    #[must_use]
    pub fn monitor(&self) -> ScreenRect<Logical> {
        *self.monitor.borrow()
    }

    /// active monitor bounds を上書きする。tick ごとに cursor 位置から解決した
    /// 最新値を反映するために使う。
    pub fn set_monitor(&self, monitor: ScreenRect<Logical>) {
        *self.monitor.borrow_mut() = monitor;
    }

    /// HUD 設定を借りる。
    #[must_use]
    pub fn hud_config(&self) -> &HudConfig {
        &self.hud_config
    }

    /// `register_hotkeys` から HUD 表示用に chord map を仕込む。
    pub fn record_hotkeys(&self, hotkeys: HotkeyMap) {
        *self.hotkeys.display_map.borrow_mut() = hotkeys;
    }

    /// HUD 表示用の chord map を借りる。`hud_frame()` の操作説明 rows に渡す。
    #[must_use]
    pub fn hotkeys(&self) -> HotkeyMap {
        *self.hotkeys.display_map.borrow()
    }

    /// 起動時刻からの経過 ms。`tick::step` の `now_ms` に使う。
    #[must_use]
    pub fn now_ms(&self) -> i64 {
        i64::try_from(self.start_time.elapsed().as_millis()).unwrap_or(i64::MAX)
    }

    /// device-lost 連続失敗カウンタへのアクセサ。`wndproc` の rebuild path で
    /// `get()` / `set()` を使う。`Cell` なので shared ref で更新可。
    #[must_use]
    pub fn device_lost_count(&self) -> &Cell<u8> {
        &self.device_lost_count
    }

    /// overlay HWND を 1 度だけ設定する (`OverlayWindow::new` 後に呼ぶ)。
    /// 既に設定済みの場合は無視 (`OnceCell::set` の挙動)。
    pub fn set_hwnd(&self, hwnd: HWND) {
        let _ = self.hwnd.set(hwnd);
    }

    /// 設定済みであれば overlay HWND を返す。device-lost rebuild の経路で
    /// `WinrtCompositionRenderer::new(hwnd)` に渡すために使う。
    #[must_use]
    pub fn hwnd(&self) -> Option<HWND> {
        self.hwnd.get().copied()
    }
}

impl core::fmt::Debug for OverlayWndState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("OverlayWndState")
            .field("nchit_count", &self.nchit_count.load(Ordering::Relaxed))
            .field("click_count", &self.click_count.load(Ordering::Relaxed))
            .field(
                "id_to_action.len",
                &self.hotkeys.id_to_action.borrow().len(),
            )
            .field("monitor", &self.monitor.borrow())
            .finish_non_exhaustive()
    }
}

// `Sender` と `Receiver` が `Send + !Sync` であることから [`OverlayWndState`] は
// 自動で `!Sync` になり、UI thread 越しの shared 参照を型レベルで防ぐ。
// HWND の thread-affinity が型に伝わる狙い。

#[cfg(test)]
mod tests {
    use super::*;
    use linerule_core::Point;

    fn fresh_state() -> OverlayWndState {
        let monitor = ScreenRect::new(Point::new(0, 0), 1920, 1080);
        OverlayWndState::new(Span::none(), monitor, HudConfig::DEFAULT)
    }

    #[test]
    fn nchit_first_three_samples_are_emitted() {
        let s = fresh_state();
        assert_eq!(s.tick_nchit(), Some(1));
        assert_eq!(s.tick_nchit(), Some(2));
        assert_eq!(s.tick_nchit(), Some(3));
    }

    #[test]
    fn nchit_samples_4_through_199_are_suppressed() {
        let s = fresh_state();
        let _ = s.tick_nchit();
        let _ = s.tick_nchit();
        let _ = s.tick_nchit();
        for n in 4..=199 {
            assert_eq!(s.tick_nchit(), None, "n={n} should be suppressed");
        }
    }

    #[test]
    fn nchit_sample_emitted_every_200() {
        let s = fresh_state();
        for _ in 1..=199 {
            let _ = s.tick_nchit();
        }
        assert_eq!(s.tick_nchit(), Some(200));
        for n in 201..=399 {
            assert_eq!(s.tick_nchit(), None, "n={n} should be suppressed");
        }
        assert_eq!(s.tick_nchit(), Some(400));
    }

    #[test]
    fn click_counter_increments_monotonically() {
        let s = fresh_state();
        assert_eq!(s.tick_click(), 1);
        assert_eq!(s.tick_click(), 2);
        assert_eq!(s.tick_click(), 3);
    }

    #[test]
    fn click_counter_independent_from_nchit_counter() {
        let s = fresh_state();
        let _ = s.tick_nchit();
        let _ = s.tick_nchit();
        let _ = s.tick_click();
        assert_eq!(s.tick_nchit(), Some(3));
        assert_eq!(s.tick_click(), 2);
    }

    #[test]
    fn hotkey_pump_round_trips_actions() {
        let s = fresh_state();
        // sender → receiver round trip
        s.hotkey_sender()
            .send(OverlayAction::CycleMode)
            .expect("sender alive");
        s.hotkey_sender()
            .send(OverlayAction::Quit)
            .expect("sender alive");
        let drained = s.drain_hotkeys();
        assert_eq!(drained, vec![OverlayAction::CycleMode, OverlayAction::Quit]);
        // 2 回目 drain は空
        assert!(s.drain_hotkeys().is_empty());
    }

    #[test]
    fn record_hotkey_populates_action_for_lookup() {
        let s = fresh_state();
        s.record_hotkey(1, OverlayAction::CycleMode);
        s.record_hotkey(2, OverlayAction::Quit);
        assert_eq!(s.action_for(1), Some(OverlayAction::CycleMode));
        assert_eq!(s.action_for(2), Some(OverlayAction::Quit));
        assert_eq!(s.action_for(99), None);
        let mut ids = s.registered_hotkey_ids();
        ids.sort_unstable();
        assert_eq!(ids, vec![1, 2]);
    }

    #[test]
    fn tick_world_round_trips() {
        let s = fresh_state();
        let mut w = s.tick_world_snapshot();
        w.frame_seq = 42;
        s.store_tick_world(w);
        assert_eq!(s.tick_world_snapshot().frame_seq, 42);
    }

    #[test]
    fn record_hotkey_conflict_is_observable() {
        let s = fresh_state();
        s.record_hotkey_conflict(HotkeyConflict {
            spec: "Ctrl+Alt+Bogus",
            action: OverlayAction::Quit,
            reason: HotkeyFailure::ChordParse(ChordError::Empty),
        });
        let conflicts = s.hotkey_conflicts();
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].spec, "Ctrl+Alt+Bogus");
        assert_eq!(conflicts[0].action, OverlayAction::Quit);
        assert!(matches!(
            conflicts[0].reason,
            HotkeyFailure::ChordParse(ChordError::Empty)
        ));
    }

    #[test]
    fn set_monitor_replaces_returned_bounds() {
        let s = fresh_state();
        let initial = s.monitor();
        let next = ScreenRect::new(Point::new(1920, 0), 2560, 1440);
        assert_ne!(initial, next);
        s.set_monitor(next);
        assert_eq!(s.monitor(), next);
    }

    #[test]
    fn now_ms_is_monotonic_and_nonnegative() {
        let s = fresh_state();
        let a = s.now_ms();
        let b = s.now_ms();
        assert!(a >= 0, "elapsed should be non-negative: {a}");
        assert!(b >= a, "elapsed should be monotonic: {a} -> {b}");
    }
}
