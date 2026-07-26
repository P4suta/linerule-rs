//! WndProc dispatch logic.
//!
//! RefCell rule: while a `borrow_mut()` is held, do NOT call a synchronously
//! re-entrant Win32 API (`SendMessageW` / `DestroyWindow` / `MessageBoxW`);
//! `PostMessageW` / `PostQuitMessage` are async and safe. A violation panics in
//! `borrow_mut`, caught by `overlay_wnd_proc`'s `catch_unwind`.

#![forbid(unsafe_code)]

use linerule_core::{
    ActionBatch, DeviceLostOutcome, HudFrame, Logical, Mode, OverlayAction, OverlayFrame, Point,
    RejectReason, ScreenRect, TickEffect, TickInput, apply_hud_envelope, frame,
    hud_distance_opacity, hud_frame, is_device_lost_hresult, record_device_lost_failure, tick,
};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    WM_APP, WM_DESTROY, WM_DISPLAYCHANGE, WM_DPICHANGED, WM_HOTKEY, WM_LBUTTONDOWN, WM_MBUTTONDOWN,
    WM_NCHITTEST, WM_PAINT, WM_RBUTTONDOWN,
};

use crate::cursor_tracker;
use crate::error::{PlatformError, Result};
use crate::messages::{HTTRANSPARENT, WM_APP_REASSERT_TOPMOST, WM_APP_TICK};
use crate::monitor_info;
use crate::overlay_state::OverlayWndState;
use crate::win32_ffi;

/// Asserts `WM_APP_TICK` is in the `WM_APP` band; also keeps the `WM_APP`
/// import used.
const _: () = assert!(WM_APP_TICK >= WM_APP);

/// `NotifyRejected` toast lifetime (ms); refreshed while a held hotkey repeats,
/// fades 3 s after release.
const REJECT_TOAST_MS: i64 = 3_000;

/// Dispatch a message other than WM_NCCREATE / WM_NCDESTROY.
///
/// `Some(LRESULT)` is the WndProc result; `None` asks the caller to fall back
/// to `DefWindowProcW`.
#[must_use]
pub fn dispatch(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> Option<LRESULT> {
    win32_ffi::with_userdata(hwnd, |state| {
        dispatch_with_state(state, hwnd, msg, wparam, lparam)
    })
    .flatten()
}

fn dispatch_with_state(
    state: &OverlayWndState,
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> Option<LRESULT> {
    match msg {
        WM_NCHITTEST => {
            if let Some(count) = state.tick_nchit() {
                tracing::trace!(
                    parent: state.span(),
                    count,
                    "WM_NCHITTEST -> HTTRANSPARENT"
                );
            }
            Some(LRESULT(HTTRANSPARENT as isize))
        },
        WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN => {
            let count = state.tick_click();
            tracing::warn!(
                parent: state.span(),
                msg = format_args!("{msg:#06x}").to_string(),
                count,
                "click reached overlay (click-through failed)"
            );
            None
        },
        WM_HOTKEY => {
            let id = wparam_as_hotkey_id(wparam);
            match state.action_for(id) {
                Some(action) => {
                    if let Err(e) = state.hotkey_sender().send(action) {
                        tracing::error!(parent: state.span(), id, ?action, error = %e,
                            "hotkey channel disconnected; dropping action");
                    } else {
                        tracing::debug!(parent: state.span(), id, ?action,
                            "WM_HOTKEY queued");
                        state.request_tick(hwnd);
                    }
                },
                None => {
                    tracing::warn!(parent: state.span(), id,
                        "WM_HOTKEY received for unknown id");
                },
            }
            Some(LRESULT(0))
        },
        WM_APP_TICK => {
            let (result, hotkey_backlog) = apply_tick(state);
            if let Err(e) = result {
                tracing::error!(parent: state.span(), error = %e,
                    "tick processing failed");
            }
            tracing::trace!(
                target: "linerule_render_tick",
                parent: state.span(),
                hotkey_backlog,
                "render tick processed"
            );
            state.complete_tick(hotkey_backlog);
            Some(LRESULT(0))
        },
        WM_DPICHANGED => {
            // Apply the OS-suggested rect; no font/HUD recalc needed since DComp
            // applies per-monitor DPI in the compositor.
            let new_rect = win32_ffi::rect_from_wm_dpichanged_lparam(lparam);
            let width = new_rect.right.saturating_sub(new_rect.left);
            let height = new_rect.bottom.saturating_sub(new_rect.top);
            let x_dpi = u32::try_from(wparam.0 & 0xFFFF).unwrap_or(0);
            let y_dpi = u32::try_from((wparam.0 >> 16) & 0xFFFF).unwrap_or(0);
            match win32_ffi::set_window_pos_rect(hwnd, new_rect.left, new_rect.top, width, height) {
                Ok(()) => {
                    tracing::info!(
                        target: "monitor.dpichanged",
                        parent: state.span(),
                        x_dpi,
                        y_dpi,
                        left = new_rect.left,
                        top = new_rect.top,
                        width,
                        height,
                        "WM_DPICHANGED applied"
                    );
                },
                Err(e) => {
                    tracing::warn!(parent: state.span(), error = %e,
                        "SetWindowPos failed on WM_DPICHANGED");
                },
            }
            Some(LRESULT(0))
        },
        WM_DISPLAYCHANGE => {
            // No cache to invalidate; the active monitor is re-resolved per tick
            // by `follow_active_monitor`. Log only, then fall back.
            let bpp = u32::try_from(wparam.0 & 0xFFFF).unwrap_or(0);
            tracing::info!(
                target: "monitor.displaychange",
                parent: state.span(),
                bpp,
                "WM_DISPLAYCHANGE received"
            );
            None
        },
        WM_APP_REASSERT_TOPMOST => {
            // SetWindowPos(HWND_TOPMOST) must run on the UI thread, hence the
            // post from the ForegroundHook callback. No-op when already topmost
            // (avoids z-order flicker).
            if let Err(e) = win32_ffi::accessibility::reassert_topmost(hwnd) {
                tracing::warn!(parent: state.span(), error = %e,
                    "reassert_topmost failed (foreground hook)");
            } else {
                tracing::trace!(parent: state.span(),
                    "topmost ensured after foreground change");
            }
            Some(LRESULT(0))
        },
        WM_PAINT => {
            // DComp drives drawing; return 0 to validate without painting.
            Some(LRESULT(0))
        },
        WM_DESTROY => {
            win32_ffi::post_quit(0);
            Some(LRESULT(0))
        },
        _ => None,
    }
}

/// The `WM_HOTKEY` `wparam` is the hotkey id; confines the lossy usize → i32
/// conversion to one place.
fn wparam_as_hotkey_id(wparam: WPARAM) -> i32 {
    i32::try_from(wparam.0).unwrap_or(i32::MAX)
}

/// One tick: cursor poll → hotkey drain → `tick::step` → `apply_effects`. The
/// elapsed time is fed to `FrameTimingTracker::record_tick` for HUD telemetry.
fn apply_tick(state: &OverlayWndState) -> (Result<()>, bool) {
    let tick_start = std::time::Instant::now();
    let polled_cursor = match cursor_tracker::poll() {
        Ok(cursor) => {
            if state.set_cursor_poll_available(true) {
                tracing::info!(
                    parent: state.span(),
                    "cursor polling recovered after session became available"
                );
            }
            Some(cursor)
        },
        Err(error) => {
            if state.set_cursor_poll_available(false) {
                tracing::warn!(
                    parent: state.span(),
                    %error,
                    "cursor polling unavailable; retaining the previous sample"
                );
            }
            None
        },
    };
    follow_active_monitor(state, polled_cursor);
    let drained_hotkeys = state.drain_hotkeys();
    let hotkey_backlog = drained_hotkeys.is_full();
    let now_ms = state.now_ms();
    let input = TickInput {
        now_ms,
        polled_cursor,
        drained_hotkeys,
    };
    let world = state.tick_world_snapshot();
    let telemetry_refresh = state.hud_config().telemetry_refresh;
    let (next_world, effects) = tick(world, &input, telemetry_refresh, state.anim_config());
    state.store_tick_world(next_world);
    let retry_result = if should_retry_renderers(
        state.device_lost_count().get(),
        world.state.mode,
        next_world.state.mode,
        &input.drained_hotkeys,
    ) {
        retry_disabled_renderers(state)
    } else {
        Ok(())
    };
    let result = retry_result.and_then(|()| apply_effects(state, effects.iter()));
    let elapsed = tick_start.elapsed();
    let over_budget = is_over_budget(elapsed, crate::render_timing::refresh_rate_hz());
    state
        .frame_timing()
        .borrow_mut()
        .record_tick(elapsed, over_budget);
    (result, hotkey_backlog)
}

fn should_retry_renderers(
    device_lost_count: u8,
    previous_mode: Mode,
    next_mode: Mode,
    actions: &ActionBatch,
) -> bool {
    device_lost_count == u8::MAX
        && matches!(previous_mode, Mode::Off)
        && !matches!(next_mode, Mode::Off)
        && actions
            .iter()
            .any(|action| matches!(action, OverlayAction::ToggleOnOff))
}

fn retry_disabled_renderers(state: &OverlayWndState) -> Result<()> {
    tracing::info!(
        target: "renderer.device_lost",
        parent: state.span(),
        "display request is retrying the disabled graphics pipeline"
    );
    match rebuild_renderers(state, crate::win32_ffi::graphics::GraphicsBackend::Auto) {
        Ok(()) => {
            state.device_lost_count().set(0);
            state.push_notification(
                linerule_core::NotificationClass::Info,
                "Drawing recovered".to_owned(),
                3_000,
            );
            Ok(())
        },
        Err(error) => {
            state.push_notification(
                linerule_core::NotificationClass::Error,
                "Drawing retry failed; hide and show to retry".to_owned(),
                5_000,
            );
            Err(error)
        },
    }
}

/// Resolve the active monitor from the cursor and update `state` if it changed.
/// `bounds_for_point` uses `MONITOR_DEFAULTTONEAREST`, so an off-screen cursor
/// still maps to a monitor. On error, warn and keep the previous monitor.
fn follow_active_monitor(state: &OverlayWndState, polled_cursor: Option<Point<Logical>>) {
    let Some(cursor) = polled_cursor else {
        return;
    };
    match monitor_info::bounds_for_point(cursor) {
        Ok(new_monitor) => {
            if new_monitor != state.monitor() {
                tracing::debug!(
                    target: "monitor.follow",
                    parent: state.span(),
                    new_left = new_monitor.left(),
                    new_top = new_monitor.top(),
                    new_width = new_monitor.width,
                    new_height = new_monitor.height,
                    "active monitor changed"
                );
                state.set_monitor(new_monitor);
            }
        },
        Err(e) => {
            tracing::warn!(parent: state.span(), error = %e,
                "bounds_for_point failed; keeping previous monitor");
        },
    }
}

/// True if `elapsed` exceeds `warn_ratio * (1000 / refresh_hz)` ms; counts
/// toward the HUD `drops` counter.
fn is_over_budget(elapsed: std::time::Duration, refresh_hz: u32) -> bool {
    let hz = refresh_hz.max(1);
    let warn_ratio = linerule_core::RenderConfig::DEFAULT.warn_ratio;
    let budget_ms = warn_ratio * 1000.0 / f64::from(hz);
    elapsed.as_secs_f64() * 1000.0 > budget_ms
}

/// Apply each `TickEffect` to the platform in order.
fn apply_effects(
    state: &OverlayWndState,
    effects: impl IntoIterator<Item = TickEffect>,
) -> Result<()> {
    for effect in effects {
        match effect {
            TickEffect::Quit => {
                tracing::info!(parent: state.span(), "Quit requested via tick");
                win32_ffi::post_quit(0);
            },
            TickEffect::DrawOverlay {
                mode,
                cursor,
                config,
                sample,
            } => {
                let frame = frame(mode, config, cursor, state.monitor(), sample);
                with_device_lost_recovery(state, "DrawOverlay", &|s| {
                    apply_overlay_frame(s, &frame)
                })?;
            },
            TickEffect::ClearOverlay => {
                with_device_lost_recovery(state, "ClearOverlay", &|s| {
                    apply_overlay_frame(s, &OverlayFrame::EMPTY)
                })?;
            },
            TickEffect::RefreshHud { state: s, tier } => {
                let hz = crate::render_timing::refresh_rate_hz();
                let notifications = build_notifications(state);
                let telemetry = state.frame_timing().borrow().snapshot();
                let hotkeys = state.hotkeys();
                let frame = hud_frame(
                    s,
                    *state.hud_config(),
                    state.monitor(),
                    hz,
                    &notifications,
                    &hotkeys,
                    telemetry,
                    tier,
                );
                // Cache the panel rect actually applied so `SetHudOpacity`'s
                // distance fade works against the bounds of whichever tier
                // (chip or full) is currently on screen.
                state.set_hud_panel_rect(panel_rect_of(&frame));
                with_device_lost_recovery(state, "RefreshHud", &|st| apply_hud_frame(st, &frame))?;
            },
            TickEffect::SetHudOpacity {
                state: s,
                cursor,
                envelope,
            } => {
                // Apply distance fade x envelope via `SpriteVisual::SetOpacity`;
                // cursor moves / envelope progress don't redraw the surface
                // (baked color alpha is RefreshHud's job).
                let distance = hud_distance_opacity(
                    s,
                    cursor,
                    state.hud_panel_rect(),
                    state.hud_config().fade_decay_px,
                );
                apply_hud_opacity(state, apply_hud_envelope(distance, envelope))?;
            },
            TickEffect::LogStateChanged { action, mode } => {
                tracing::info!(
                    parent: state.span(),
                    ?action,
                    ?mode,
                    "state changed"
                );
            },
            TickEffect::NotifyRejected { reason } => match reason {
                RejectReason::AdjustWhileOff => {
                    // Core carries only the semantic reason; the configured
                    let hotkeys = state.hotkeys();
                    let chord = hotkeys
                        .get(linerule_core::Command::ToggleOnOff)
                        .unwrap_or("the configured shortcut");
                    state.push_notification(
                        linerule_core::NotificationClass::Info,
                        format!("Overlay is off — {chord} to show"),
                        REJECT_TOAST_MS,
                    );
                },
            },
        }
    }
    Ok(())
}

fn apply_overlay_frame(state: &OverlayWndState, frame: &OverlayFrame) -> Result<()> {
    let using_blur_fallback = {
        let mut slot = state.renderer().borrow_mut();
        if let Some(renderer) = slot.as_mut() {
            renderer.apply(frame)?;
            renderer.uses_blur_fallback()
        } else {
            false
        }
    };
    if using_blur_fallback {
        state.push_notification(
            linerule_core::NotificationClass::Warn,
            "Backdrop Blur unavailable; using Dim".to_owned(),
            5_000,
        );
    }
    Ok(())
}

fn apply_hud_frame(state: &OverlayWndState, frame: &HudFrame) -> Result<()> {
    if let Some(renderer) = state.hud_renderer().borrow_mut().as_mut() {
        renderer.apply(frame)?;
    }
    Ok(())
}

/// Wndproc-side wrapper for `WinrtHudRenderer::set_opacity`; no-op before the
/// renderer is attached.
fn apply_hud_opacity(state: &OverlayWndState, opacity: f32) -> Result<()> {
    if let Some(renderer) = state.hud_renderer().borrow_mut().as_mut() {
        renderer.set_opacity(opacity)?;
    }
    Ok(())
}

/// Wrap a render op with device-lost recovery: on a device-lost HRESULT,
/// rebuild the renderers and retry once; disable drawing after 3 consecutive
/// failures while the controller, tray, and settings remain alive.
///
/// `op` must release its `borrow_mut` before returning Err (the `apply_*_frame`
/// helpers scope it to an `if let`), so the rebuild can re-borrow without a
/// `BorrowMutError` (see overlay_state.rs RefCell rule).
fn with_device_lost_recovery(
    state: &OverlayWndState,
    operation: &'static str,
    op: &dyn Fn(&OverlayWndState) -> Result<()>,
) -> Result<()> {
    with_device_lost_recovery_using(state, operation, op, &|state, backend| {
        rebuild_renderers(state, backend)
    })
}

fn with_device_lost_recovery_using(
    state: &OverlayWndState,
    operation: &'static str,
    op: &dyn Fn(&OverlayWndState) -> Result<()>,
    rebuild: &dyn Fn(&OverlayWndState, crate::win32_ffi::graphics::GraphicsBackend) -> Result<()>,
) -> Result<()> {
    match op(state) {
        Ok(()) => {
            state.device_lost_count().set(0);
            Ok(())
        },
        Err(e) => {
            state.frame_timing().borrow_mut().record_timeout();
            let Some(hr) = device_lost_hr(&e) else {
                return Err(e);
            };
            let prev = state.device_lost_count().get();
            match record_device_lost_failure(prev) {
                DeviceLostOutcome::Retry { next } => {
                    tracing::warn!(
                        target: "renderer.device_lost",
                        parent: state.span(),
                        operation,
                        hr = format_args!("{hr:#010x}").to_string(),
                        consecutive = next,
                        "device-lost detected; rebuilding pipeline and retrying once"
                    );
                    state.device_lost_count().set(next);
                    let backend = if next >= 2 {
                        crate::win32_ffi::graphics::GraphicsBackend::Warp
                    } else {
                        crate::win32_ffi::graphics::GraphicsBackend::Auto
                    };
                    rebuild(state, backend)?;
                    op(state)?;
                    state.device_lost_count().set(0);
                    state.push_notification(
                        linerule_core::NotificationClass::Info,
                        "Drawing recovered".to_owned(),
                        3_000,
                    );
                    Ok(())
                },
                DeviceLostOutcome::Degrade => {
                    tracing::error!(
                        target: "renderer.device_lost",
                        parent: state.span(),
                        operation,
                        hr = format_args!("{hr:#010x}").to_string(),
                        "device-lost exhausted; disabling drawing while controller remains alive"
                    );
                    state.device_lost_count().set(u8::MAX);
                    state.disable_renderers();
                    state.push_notification(
                        linerule_core::NotificationClass::Error,
                        "Drawing disabled after repeated graphics failures".to_owned(),
                        5_000,
                    );
                    Ok(())
                },
            }
        },
    }
}

/// The HRESULT from a `PlatformError::BadHr`, if it is a device-lost code.
fn device_lost_hr(e: &PlatformError) -> Option<i32> {
    match e {
        PlatformError::BadHr { hr, .. } if is_device_lost_hresult(*hr) => Some(*hr),
        _ => None,
    }
}

/// Rebuild the overlay + HUD renderers and reinstall them. The old renderers
/// Drop on replacement, releasing their COM objects.
fn rebuild_renderers(
    state: &OverlayWndState,
    backend: crate::win32_ffi::graphics::GraphicsBackend,
) -> Result<()> {
    let hwnd = state.hwnd().ok_or(PlatformError::NullHandle {
        operation: "rebuild_renderers: HWND unset",
    })?;
    let (overlay, hud) =
        crate::renderer_backend::build_backends(hwnd, state.hud_config(), backend)?;
    state.install_renderer(overlay);
    state.install_hud_renderer(hud);
    tracing::info!(
        target: "renderer.device_lost",
        parent: state.span(),
        backend = "winrt",
        "renderers rebuilt successfully"
    );
    Ok(())
}

/// Build the current HUD notification list.
fn build_notifications(state: &OverlayWndState) -> Vec<linerule_core::HudNotification> {
    state.live_notifications()
}

/// Round a `HudFrame`'s panel rect to `ScreenRect<Logical>`; cached as the
/// distance-fade target since panel size differs between chip and full tier.
fn panel_rect_of(frame: &HudFrame) -> ScreenRect<Logical> {
    #[allow(
        clippy::cast_possible_truncation,
        reason = "screen-space px fit the f32 mantissa; rounded result stays in i32 range"
    )]
    let left = frame.panel_left.round() as i32;
    #[allow(clippy::cast_possible_truncation, reason = "ditto")]
    let top = frame.panel_top.round() as i32;
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "panel dimensions are positive screen-space px"
    )]
    let w = frame.panel_width.round().max(0.0) as u32;
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "ditto"
    )]
    let h = frame.panel_height.round().max(0.0) as u32;
    ScreenRect::new(Point::<Logical>::new(left, top), w, h)
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    use std::cell::{Cell, RefCell};
    use std::time::Duration;

    use super::*;

    fn test_state() -> OverlayWndState {
        OverlayWndState::new(
            tracing::Span::none(),
            ScreenRect::new(Point::new(0, 0), 1920, 1080),
            linerule_core::HudConfig::DEFAULT,
            linerule_core::AnimConfig::DEFAULT,
        )
    }

    #[test]
    fn wparam_to_id_truncates_safely() {
        assert_eq!(wparam_as_hotkey_id(WPARAM(1)), 1);
        assert_eq!(wparam_as_hotkey_id(WPARAM(7)), 7);
        // usize::MAX saturates to i32::MAX without panicking.
        assert_eq!(wparam_as_hotkey_id(WPARAM(usize::MAX)), i32::MAX);
    }

    /// Boundary: `i32::MAX` passes through exactly.
    #[test]
    fn wparam_to_id_at_i32_max_boundary_is_preserved() {
        assert_eq!(
            wparam_as_hotkey_id(WPARAM(i32::MAX as usize)),
            i32::MAX,
            "i32::MAX boundary should pass through exactly"
        );
    }

    #[test]
    fn render_budget_handles_normal_over_budget_and_zero_refresh_rates() {
        assert!(!is_over_budget(Duration::from_millis(1), 60));
        assert!(is_over_budget(Duration::from_millis(14), 60));
        assert!(!is_over_budget(Duration::from_millis(100), 0));
        assert!(is_over_budget(Duration::from_millis(801), 0));
    }

    #[test]
    fn panel_rect_rounds_origins_and_clamps_negative_dimensions() {
        let frame = HudFrame {
            panel_left: -1.6,
            panel_top: 2.4,
            panel_width: -10.0,
            panel_height: 20.6,
            background: linerule_core::Rgba::TRANSPARENT,
            corner_radius: 0.0,
            opacity: 1.0,
            rules: Vec::new(),
            rows: Vec::new(),
        };
        let rectangle = panel_rect_of(&frame);
        assert_eq!((rectangle.left(), rectangle.top()), (-2, 2));
        assert_eq!((rectangle.width, rectangle.height), (0, 21));
    }

    #[test]
    fn effect_dispatch_covers_overlay_hud_opacity_logging_and_rejection() {
        let state = test_state();
        let config = linerule_core::OverlayConfig::DEFAULT;
        let effects = [
            TickEffect::DrawOverlay {
                mode: linerule_core::Mode::Horizontal,
                cursor: Point::new(960, 540),
                config,
                sample: linerule_core::OverlaySample::settled(config),
            },
            TickEffect::ClearOverlay,
            TickEffect::RefreshHud {
                state: linerule_core::State::DEFAULT,
                tier: linerule_core::HudTier::Full,
            },
            TickEffect::SetHudOpacity {
                state: linerule_core::State::DEFAULT,
                cursor: Point::new(960, 540),
                envelope: 128,
            },
            TickEffect::LogStateChanged {
                action: linerule_core::OverlayAction::CycleMode,
                mode: linerule_core::Mode::Horizontal,
            },
            TickEffect::NotifyRejected {
                reason: RejectReason::AdjustWhileOff,
            },
        ];
        apply_effects(&state, effects).expect("effect dispatch without renderers");
        let notifications = build_notifications(&state);
        assert_eq!(notifications.len(), 1);
        assert!(notifications[0].message.contains("Ctrl+Alt+H"));
    }

    #[test]
    fn device_lost_recovery_covers_success_non_device_retry_and_degrade() {
        let state = test_state();
        state.device_lost_count().set(2);
        with_device_lost_recovery(&state, "success", &|_| Ok(())).expect("successful operation");
        assert_eq!(state.device_lost_count().get(), 0);

        let ordinary = PlatformError::BadHr {
            operation: "fixture",
            hr: -1,
        };
        assert_eq!(
            with_device_lost_recovery(&state, "ordinary", &|_| Err(ordinary.clone())),
            Err(ordinary)
        );

        let device_removed = i32::from_ne_bytes(0x887A_0005_u32.to_ne_bytes());
        let lost = PlatformError::BadHr {
            operation: "fixture",
            hr: device_removed,
        };

        let auto_calls = Cell::new(0_u8);
        let auto_backends = RefCell::new(Vec::new());
        with_device_lost_recovery_using(
            &state,
            "hardware retry",
            &|_| {
                auto_calls.set(auto_calls.get().saturating_add(1));
                if auto_calls.get() == 1 {
                    Err(lost.clone())
                } else {
                    Ok(())
                }
            },
            &|_, backend| {
                auto_backends.borrow_mut().push(backend);
                Ok(())
            },
        )
        .expect("first device loss rebuilds the hardware-preferred pipeline");
        assert_eq!(auto_calls.get(), 2);
        assert_eq!(
            *auto_backends.borrow(),
            [crate::win32_ffi::graphics::GraphicsBackend::Auto]
        );
        assert_eq!(state.device_lost_count().get(), 0);
        assert!(
            build_notifications(&state)
                .iter()
                .any(|notification| notification.message.contains("Drawing recovered"))
        );

        state.device_lost_count().set(1);
        let warp_backends = RefCell::new(Vec::new());
        assert_eq!(
            with_device_lost_recovery_using(
                &state,
                "WARP retry",
                &|_| Err(lost.clone()),
                &|_, backend| {
                    warp_backends.borrow_mut().push(backend);
                    Ok(())
                },
            ),
            Err(lost.clone())
        );
        assert_eq!(
            *warp_backends.borrow(),
            [crate::win32_ffi::graphics::GraphicsBackend::Warp]
        );
        assert_eq!(state.device_lost_count().get(), 2);

        state.device_lost_count().set(0);
        let rebuild_failure = PlatformError::Invariant {
            operation: "injected renderer rebuild",
        };
        assert_eq!(
            with_device_lost_recovery_using(
                &state,
                "failed rebuild",
                &|_| Err(lost.clone()),
                &|_, _| Err(rebuild_failure.clone()),
            ),
            Err(rebuild_failure)
        );
        assert_eq!(state.device_lost_count().get(), 1);

        state.device_lost_count().set(2);
        let degraded_rebuild_calls = Cell::new(0_u8);
        with_device_lost_recovery_using(&state, "degrade", &|_| Err(lost.clone()), &|_, _| {
            degraded_rebuild_calls.set(degraded_rebuild_calls.get().saturating_add(1));
            Ok(())
        })
        .expect("third loss degrades drawing");
        assert_eq!(degraded_rebuild_calls.get(), 0);
        assert_eq!(state.device_lost_count().get(), u8::MAX);
        assert!(
            build_notifications(&state)
                .iter()
                .any(|notification| notification.message.contains("Drawing disabled"))
        );
        assert_eq!(device_lost_hr(&lost), Some(device_removed));
        assert_eq!(device_lost_hr(&PlatformError::AlreadyRunning), None);
    }

    #[test]
    fn disabled_renderer_retries_only_on_an_explicit_off_to_on_toggle() {
        let toggle = ActionBatch::try_from_actions([OverlayAction::ToggleOnOff])
            .expect("single toggle fits");
        let unrelated = ActionBatch::try_from_actions([OverlayAction::CycleEffect])
            .expect("single action fits");

        assert!(should_retry_renderers(
            u8::MAX,
            Mode::Off,
            Mode::Horizontal,
            &toggle
        ));
        assert!(!should_retry_renderers(
            2,
            Mode::Off,
            Mode::Horizontal,
            &toggle
        ));
        assert!(!should_retry_renderers(
            u8::MAX,
            Mode::Horizontal,
            Mode::Off,
            &toggle
        ));
        assert!(!should_retry_renderers(
            u8::MAX,
            Mode::Off,
            Mode::Horizontal,
            &unrelated
        ));
    }

    proptest! {
        /// The result is never negative (the `i32::MAX` upper bound holds by
        /// type).
        #[test]
        fn wparam_to_id_stays_in_i32_positive_range(raw in any::<usize>()) {
            let id = wparam_as_hotkey_id(WPARAM(raw));
            prop_assert!(id >= 0);
        }

        /// Values up to `i32::MAX` are preserved exactly.
        #[test]
        fn wparam_to_id_preserves_small_values(small in 0i32..=i32::MAX) {
            let id = wparam_as_hotkey_id(WPARAM(small as usize));
            prop_assert_eq!(id, small);
        }

        /// Values above `i32::MAX` saturate to `i32::MAX`.
        #[test]
        fn wparam_to_id_saturates_above_i32_max(huge in (i32::MAX as usize + 1)..=usize::MAX) {
            let id = wparam_as_hotkey_id(WPARAM(huge));
            prop_assert_eq!(id, i32::MAX);
        }
    }
}
