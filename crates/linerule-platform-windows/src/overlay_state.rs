//! Per-overlay-HWND instance state.
//!
//! Allocated via `Box::into_raw`, stored in `GWLP_USERDATA`, and recovered by
//! the WndProc through `win32_ffi::get_userdata` as `NonNull<OverlayWndState>`.
//!
//! RefCell rule: mutable fields are `RefCell`s shared across the single UI
//! thread. While a `borrow_mut()` is held, do NOT call a Win32 API that
//! re-enters synchronously (`SendMessageW` / `DestroyWindow` / `MessageBoxW` /
//! `BringWindowToTop` dispatch `WM_*` on the same stack); `PostMessageW` is
//! async and safe. A violation panics in `borrow_mut` and is caught by
//! `overlay_wnd_proc`'s `catch_unwind`.
//!
//! On `WM_NCDESTROY`, `take_userdata` reclaims the `Box`, dropping the
//! renderers and releasing their COM objects.

#![forbid(unsafe_code)]

use std::cell::{Cell, OnceCell, RefCell};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Instant;

use linerule_core::input::tick::TickWorld;
use linerule_core::{
    AnimConfig, ChordError, HotkeyMap, HudConfig, HudNotification, Logical, NotificationClass,
    OverlayAction, Point, ScreenRect,
};
use tracing::Span;
use windows::Win32::Foundation::HWND;

use crate::winrt_composition_renderer::WinrtCompositionRenderer;
use crate::winrt_hud_renderer::WinrtHudRenderer;

/// A failed hotkey registration, retained for the HUD conflict list.
#[derive(Debug, Clone)]
pub struct HotkeyConflict {
    /// Chord string from user config (e.g. `"Ctrl+Alt+R"`).
    pub spec: &'static str,
    /// Action this chord was bound to.
    pub action: OverlayAction,
    /// Why registration failed.
    pub reason: HotkeyFailure,
}

/// Reason a hotkey registration failed.
#[derive(Debug, Clone)]
pub enum HotkeyFailure {
    /// Chord string failed to parse.
    ChordParse(ChordError),
    /// `RegisterHotKey` failed (usually `ERROR_HOTKEY_ALREADY_REGISTERED`).
    RegisterHotKey {
        /// HRESULT / GetLastError at failure (informational).
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
    /// Transition timing config, passed to `tick::step` every tick.
    anim_config: AnimConfig,
    /// HUD panel rect actually applied by the last `RefreshHud`. The panel
    /// size differs between chip and full tier, so the `SetHudOpacity`
    /// distance fade computes against this cache (recomputing from config
    /// would only ever yield the fixed full-tier size).
    hud_panel_rect: Cell<ScreenRect<Logical>>,
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
    /// New instance state initialized with `TickWorld::INITIAL` (mode = Off).
    #[must_use]
    pub fn new(
        log_span: Span,
        monitor: ScreenRect<Logical>,
        hud_config: HudConfig,
        anim_config: AnimConfig,
    ) -> Self {
        Self::new_with_initial_world(
            log_span,
            monitor,
            hud_config,
            anim_config,
            TickWorld::INITIAL,
        )
    }

    /// New instance state initialized with an explicit `TickWorld`, used to
    /// override the startup mode (e.g. `--initial-mode`).
    #[must_use]
    pub fn new_with_initial_world(
        log_span: Span,
        monitor: ScreenRect<Logical>,
        hud_config: HudConfig,
        anim_config: AnimConfig,
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
            hud_panel_rect: Cell::new(initial_hud_panel_rect(&hud_config, monitor)),
            hud_config,
            anim_config,
            notifications: RefCell::new(Vec::new()),
            frame_timing: RefCell::new(crate::frame_timing::FrameTimingTracker::new()),
            device_lost_count: Cell::new(0),
            hwnd: OnceCell::new(),
            start_time: Instant::now(),
        }
    }

    /// Mutable access to the HUD telemetry tracker.
    #[must_use]
    pub fn frame_timing(&self) -> &RefCell<crate::frame_timing::FrameTimingTracker> {
        &self.frame_timing
    }

    /// Queue a toast, evicted by `live_notifications` after `lifetime_ms`.
    /// Pass `i64::MAX` to persist it.
    ///
    /// If a toast with the same `(class, message)` already exists, its
    /// lifetime is refreshed instead of stacking a duplicate (holding a
    /// repeatable hotkey would otherwise pile up the same rejection toast on
    /// every key repeat).
    pub fn push_notification(&self, class: NotificationClass, message: String, lifetime_ms: i64) {
        let until_ms = self.now_ms().saturating_add(lifetime_ms);
        let mut q = self.notifications.borrow_mut();
        q.retain(|n| !(n.class == class && n.message == message));
        q.push(HudNotification {
            class,
            message,
            until_ms,
        });
    }

    /// Snapshot of toasts unexpired at `now_ms`; evicts expired ones in place.
    pub fn live_notifications(&self) -> Vec<HudNotification> {
        let now = self.now_ms();
        let mut q = self.notifications.borrow_mut();
        q.retain(|n| now < n.until_ms);
        q.clone()
    }

    /// This HWND's tracing span.
    pub fn span(&self) -> &Span {
        &self.log_span
    }

    /// Count one `WM_NCHITTEST`; returns the count at sampling thresholds
    /// (first 3, then every 200th), else `None`.
    #[must_use]
    pub fn tick_nchit(&self) -> Option<u64> {
        let n = self.nchit_count.fetch_add(1, Ordering::Relaxed) + 1;
        if n <= 3 || n.is_multiple_of(200) {
            Some(n)
        } else {
            None
        }
    }

    /// Count one button-down message (a click-through failure).
    pub fn tick_click(&self) -> u64 {
        self.click_count.fetch_add(1, Ordering::Relaxed) + 1
    }

    /// Install the overlay renderer built by `attach_compositor`.
    pub fn install_renderer(&self, renderer: WinrtCompositionRenderer) {
        *self.renderers.overlay.borrow_mut() = Some(renderer);
    }

    /// Mutable access to the overlay renderer.
    pub fn renderer(&self) -> &RefCell<Option<WinrtCompositionRenderer>> {
        &self.renderers.overlay
    }

    /// Install the HUD renderer built by `attach_compositor`.
    pub fn install_hud_renderer(&self, renderer: WinrtHudRenderer) {
        *self.renderers.hud.borrow_mut() = Some(renderer);
    }

    /// Mutable access to the HUD renderer.
    pub fn hud_renderer(&self) -> &RefCell<Option<WinrtHudRenderer>> {
        &self.renderers.hud
    }

    /// Current tick world snapshot.
    #[must_use]
    pub fn tick_world_snapshot(&self) -> TickWorld {
        *self.tick_world.borrow()
    }

    /// Store the tick world.
    pub fn store_tick_world(&self, world: TickWorld) {
        *self.tick_world.borrow_mut() = world;
    }

    /// The hotkey sender.
    pub fn hotkey_sender(&self) -> &Sender<OverlayAction> {
        &self.hotkeys.sender
    }

    /// Action bound to a hotkey id.
    #[must_use]
    pub fn action_for(&self, id: i32) -> Option<OverlayAction> {
        self.hotkeys.id_to_action.borrow().get(&id).copied()
    }

    /// Record a hotkey id → action mapping.
    ///
    /// Each id must be unique; a duplicate trips `debug_assert!` in debug and
    /// is last-write-wins in release.
    pub fn record_hotkey(&self, id: i32, action: OverlayAction) {
        let prev = self.hotkeys.id_to_action.borrow_mut().insert(id, action);
        debug_assert!(
            prev.is_none(),
            "duplicate hotkey id {id} registered (prev action: {prev:?}); \
             this is a bug — each `RegisterHotKey` call must use a unique id"
        );
    }

    /// Registered hotkey ids, used to `UnregisterHotKey` on Drop.
    pub fn registered_hotkey_ids(&self) -> Vec<i32> {
        self.hotkeys.id_to_action.borrow().keys().copied().collect()
    }

    /// Record a failed hotkey registration.
    pub fn record_hotkey_conflict(&self, conflict: HotkeyConflict) {
        self.hotkeys.conflicts.borrow_mut().push(conflict);
    }

    /// The hotkey conflict list.
    pub fn hotkey_conflicts(&self) -> Vec<HotkeyConflict> {
        self.hotkeys.conflicts.borrow().clone()
    }

    /// Drain queued actions from the inbox channel.
    pub fn drain_hotkeys(&self) -> Vec<OverlayAction> {
        let mut out = Vec::new();
        while let Ok(a) = self.hotkeys.inbox.try_recv() {
            out.push(a);
        }
        out
    }

    /// Snapshot of the active monitor bounds.
    #[must_use]
    pub fn monitor(&self) -> ScreenRect<Logical> {
        *self.monitor.borrow()
    }

    /// Replace the active monitor bounds (re-resolved per tick from the cursor).
    pub fn set_monitor(&self, monitor: ScreenRect<Logical>) {
        *self.monitor.borrow_mut() = monitor;
    }

    /// The HUD config.
    #[must_use]
    pub fn hud_config(&self) -> &HudConfig {
        &self.hud_config
    }

    /// The transition timing config (`Copy`), passed to `tick::step`.
    #[must_use]
    pub fn anim_config(&self) -> AnimConfig {
        self.anim_config
    }

    /// The HUD panel rect applied by the last `RefreshHud`.
    #[must_use]
    pub fn hud_panel_rect(&self) -> ScreenRect<Logical> {
        self.hud_panel_rect.get()
    }

    /// Cache the actual panel rect when `RefreshHud` is applied.
    pub fn set_hud_panel_rect(&self, rect: ScreenRect<Logical>) {
        self.hud_panel_rect.set(rect);
    }

    /// Store the chord map shown in the HUD hotkey-help rows.
    pub fn record_hotkeys(&self, hotkeys: HotkeyMap) {
        *self.hotkeys.display_map.borrow_mut() = hotkeys;
    }

    /// The chord map for the HUD hotkey-help rows.
    #[must_use]
    pub fn hotkeys(&self) -> HotkeyMap {
        *self.hotkeys.display_map.borrow()
    }

    /// Milliseconds since process start; the `now_ms` for `tick::step`.
    #[must_use]
    pub fn now_ms(&self) -> i64 {
        i64::try_from(self.start_time.elapsed().as_millis()).unwrap_or(i64::MAX)
    }

    /// The consecutive device-lost failure counter (read/written on the
    /// `wndproc` rebuild path).
    #[must_use]
    pub fn device_lost_count(&self) -> &Cell<u8> {
        &self.device_lost_count
    }

    /// Set the overlay HWND once; ignored if already set (`OnceCell::set`).
    pub fn set_hwnd(&self, hwnd: HWND) {
        let _ = self.hwnd.set(hwnd);
    }

    /// The overlay HWND, if set. Used to rebuild the renderer on device-lost.
    #[must_use]
    pub fn hwnd(&self) -> Option<HWND> {
        self.hwnd.get().copied()
    }
}

/// Fallback rect right after startup (before the first `RefreshHud` lands),
/// sized for the full panel. The first tick's `RefreshHud` always overwrites
/// it, so a plausible initial value matters more than precision.
fn initial_hud_panel_rect(hud: &HudConfig, monitor: ScreenRect<Logical>) -> ScreenRect<Logical> {
    let monitor_right = monitor.left() + i32::try_from(monitor.width).unwrap_or(i32::MAX);
    #[allow(
        clippy::cast_possible_truncation,
        reason = "screen-space px; rounded result fits in i32"
    )]
    let panel_left = monitor_right - (hud.geometry.margin + hud.geometry.width).round() as i32;
    #[allow(clippy::cast_possible_truncation, reason = "ditto")]
    let panel_top = monitor.top() + hud.geometry.margin.round() as i32;
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "panel dimensions are positive screen-space px"
    )]
    let w = hud.geometry.width.round() as u32;
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "ditto"
    )]
    let h = hud.geometry.height.round() as u32;
    ScreenRect::new(Point::<Logical>::new(panel_left, panel_top), w, h)
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

// `Sender`/`Receiver` are `!Sync`, so `OverlayWndState` is `!Sync` too,
// enforcing HWND thread-affinity at the type level.

#[cfg(test)]
mod tests {
    use super::*;
    use linerule_core::Point;

    fn fresh_state() -> OverlayWndState {
        let monitor = ScreenRect::new(Point::new(0, 0), 1920, 1080);
        OverlayWndState::new(
            Span::none(),
            monitor,
            HudConfig::DEFAULT,
            AnimConfig::DEFAULT,
        )
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
        s.hotkey_sender()
            .send(OverlayAction::CycleMode)
            .expect("sender alive");
        s.hotkey_sender()
            .send(OverlayAction::Quit)
            .expect("sender alive");
        let drained = s.drain_hotkeys();
        assert_eq!(drained, vec![OverlayAction::CycleMode, OverlayAction::Quit]);
        // Second drain is empty.
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

    /// A toast with the same `(class, message)` doesn't stack; only its
    /// lifetime refreshes. Pins that holding a repeatable hotkey doesn't
    /// duplicate the rejection toast on every key repeat.
    #[test]
    fn push_notification_dedups_same_class_and_message() {
        let s = fresh_state();
        s.push_notification(NotificationClass::Info, "Overlay is off".to_string(), 3_000);
        s.push_notification(NotificationClass::Info, "Overlay is off".to_string(), 3_000);
        assert_eq!(
            s.live_notifications().len(),
            1,
            "duplicate toast must not stack"
        );
        // A different class coexists as a separate toast.
        s.push_notification(NotificationClass::Warn, "Overlay is off".to_string(), 3_000);
        assert_eq!(s.live_notifications().len(), 2);
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
