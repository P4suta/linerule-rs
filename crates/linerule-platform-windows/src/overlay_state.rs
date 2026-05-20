//! Overlay HWND ごとに 1 つ存在する instance state。
//!
//! `Box::into_raw` で確保したアドレスが `GWLP_USERDATA` に格納され、WndProc
//! から `win32_ffi::get_userdata` 経由で `NonNull<OverlayWndState>` として
//! 取り出される。本ファイル自体は `#![forbid(unsafe_code)]`。
//!
//! ## RefCell 不変条件
//!
//! `renderer` / `tick_world` / `id_to_action` / `hotkey_conflicts` は
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
//! reclaim されると、`renderer: RefCell<Option<CompositionRenderer>>` の中の
//! COM オブジェクトも Drop で確実に Release される (ADR-0002 §4)。

#![forbid(unsafe_code)]

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Instant;

use linerule_core::input::tick::TickWorld;
use linerule_core::{ChordError, ChordSpec, HudConfig, Logical, OverlayAction, ScreenRect};
use tracing::Span;

use crate::composition_renderer::CompositionRenderer;

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

/// WndProc に流れ込むメッセージごとに参照される instance state。
pub struct OverlayWndState {
    log_span: Span,
    nchit_count: AtomicU64,
    click_count: AtomicU64,
    /// Phase D で attach される DComp + D2D renderer。`attach_dcomp` で `Some` に
    /// なり、Drop で COM オブジェクトが Release される。
    renderer: RefCell<Option<CompositionRenderer>>,
    /// Pure tick pipeline の累積 state。`WM_APP_TICK` ハンドラから per-tick で
    /// `borrow_mut()` される。
    tick_world: RefCell<TickWorld>,
    /// `WM_HOTKEY` ハンドラが action を流す出口。`Sender::send(&self, _)` は
    /// shared ref で呼べるため `RefCell` は不要。
    hotkey_sender: Sender<OverlayAction>,
    /// `WM_APP_TICK` ハンドラが action を drain する入口。`Receiver::try_recv`
    /// も shared ref で呼べる。
    hotkey_inbox: Receiver<OverlayAction>,
    /// hotkey id → action の lookup。`register_hotkeys` で一度埋めた後、
    /// WndProc 側は読み取りのみ。
    id_to_action: RefCell<HashMap<i32, OverlayAction>>,
    /// 現在 overlay が掛かっている monitor の bounds（PR 3 で multi-monitor 化）。
    monitor: ScreenRect<Logical>,
    /// HUD の見た目・タイミング設定。
    hud_config: HudConfig,
    /// `RegisterHotKey` / chord parse に失敗した chord のリスト。PR 2 で HUD に
    /// 列挙される。
    hotkey_conflicts: RefCell<Vec<HotkeyConflict>>,
    /// プロセス起動時刻。`tick::step` に渡す `now_ms` を計算するための原点。
    start_time: Instant,
}

impl OverlayWndState {
    /// 新しい instance state を構築する。
    #[must_use]
    pub fn new(log_span: Span, monitor: ScreenRect<Logical>, hud_config: HudConfig) -> Self {
        let (sender, receiver) = channel::<OverlayAction>();
        Self {
            log_span,
            nchit_count: AtomicU64::new(0),
            click_count: AtomicU64::new(0),
            renderer: RefCell::new(None),
            tick_world: RefCell::new(TickWorld::INITIAL),
            hotkey_sender: sender,
            hotkey_inbox: receiver,
            id_to_action: RefCell::new(HashMap::new()),
            monitor,
            hud_config,
            hotkey_conflicts: RefCell::new(Vec::new()),
            start_time: Instant::now(),
        }
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

    /// `attach_dcomp` で構築された `CompositionRenderer` を仕込む。
    pub fn install_renderer(&self, renderer: CompositionRenderer) {
        *self.renderer.borrow_mut() = Some(renderer);
    }

    /// レンダラへの可変アクセス（WndProc の `WM_APP_TICK` ハンドラから利用）。
    pub fn renderer(&self) -> &RefCell<Option<CompositionRenderer>> {
        &self.renderer
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
        &self.hotkey_sender
    }

    /// hotkey id に対応する `OverlayAction` を引く。
    #[must_use]
    pub fn action_for(&self, id: i32) -> Option<OverlayAction> {
        self.id_to_action.borrow().get(&id).copied()
    }

    /// `register_hotkeys` から hotkey id と action の対応を仕込む。
    pub fn record_hotkey(&self, id: i32, action: OverlayAction) {
        self.id_to_action.borrow_mut().insert(id, action);
    }

    /// 現在登録済みの hotkey id 一覧。Drop で `UnregisterHotKey` する際に使う。
    pub fn registered_hotkey_ids(&self) -> Vec<i32> {
        self.id_to_action.borrow().keys().copied().collect()
    }

    /// hotkey 登録失敗を記録する。
    pub fn record_hotkey_conflict(&self, conflict: HotkeyConflict) {
        self.hotkey_conflicts.borrow_mut().push(conflict);
    }

    /// hotkey 競合の一覧。
    pub fn hotkey_conflicts(&self) -> Vec<HotkeyConflict> {
        self.hotkey_conflicts.borrow().clone()
    }

    /// 受信 channel から OverlayAction を drain する。
    pub fn drain_hotkeys(&self) -> Vec<OverlayAction> {
        let mut out = Vec::new();
        while let Ok(a) = self.hotkey_inbox.try_recv() {
            out.push(a);
        }
        out
    }

    /// 現在 monitor bounds（PR 3 で `RefCell` 化予定）。
    #[must_use]
    pub fn monitor(&self) -> ScreenRect<Logical> {
        self.monitor
    }

    /// HUD 設定を借りる。
    #[must_use]
    pub fn hud_config(&self) -> &HudConfig {
        &self.hud_config
    }

    /// 起動時刻からの経過 ms。`tick::step` の `now_ms` に使う。
    #[must_use]
    pub fn now_ms(&self) -> i64 {
        i64::try_from(self.start_time.elapsed().as_millis()).unwrap_or(i64::MAX)
    }
}

impl core::fmt::Debug for OverlayWndState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("OverlayWndState")
            .field("nchit_count", &self.nchit_count.load(Ordering::Relaxed))
            .field("click_count", &self.click_count.load(Ordering::Relaxed))
            .field("id_to_action.len", &self.id_to_action.borrow().len())
            .field("monitor", &self.monitor)
            .finish_non_exhaustive()
    }
}

// `Sender` と `Receiver` が `Send + !Sync` であることから [`OverlayWndState`] は
// 自動で `!Sync` になり、UI thread 越しの shared 参照を型レベルで防ぐ。
// HWND の thread-affinity が型に伝わる狙い（ADR-0002 §7）。
// `ChordSpec` は将来 [`HotkeyConflict`] 拡張で使うため import を維持。
#[allow(dead_code, reason = "ChordSpec は HUD 表示拡張 (PR 2) で参照する")]
const _: fn() = || {
    let _: Option<ChordSpec> = None;
};

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
    fn now_ms_is_monotonic_and_nonnegative() {
        let s = fresh_state();
        let a = s.now_ms();
        let b = s.now_ms();
        assert!(a >= 0, "elapsed should be non-negative: {a}");
        assert!(b >= a, "elapsed should be monotonic: {a} -> {b}");
    }
}
