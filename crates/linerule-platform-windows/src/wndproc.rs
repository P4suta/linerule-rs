//! WndProc dispatch logic.
//!
//! The FFI entry point `overlay_wnd_proc` lives in `win32_ffi.rs` and calls
//! `dispatch()` here with an already-recovered state ref, so this file needs no
//! `unsafe`.
//!
//! RefCell rule: while a `borrow_mut()` is held, do NOT call a synchronously
//! re-entrant Win32 API (`SendMessageW` / `DestroyWindow` / `MessageBoxW`);
//! `PostMessageW` / `PostQuitMessage` are async and safe. A violation panics in
//! `borrow_mut`, caught by `win32_ffi::overlay_wnd_proc`'s `catch_unwind`, which
//! falls back to `DefWindowProcW`.

#![forbid(unsafe_code)]

use linerule_core::input::hud_fade;
use linerule_core::input::tick::{TickEffect, TickInput, step};
use linerule_core::{
    DeviceLostOutcome, HudFrame, Logical, OverlayAction, OverlayFrame, Point, ScreenRect, State,
    hud_frame, is_device_lost_hresult, record_device_lost_failure, render,
};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    WM_APP, WM_DESTROY, WM_DISPLAYCHANGE, WM_DPICHANGED, WM_HOTKEY, WM_LBUTTONDOWN, WM_MBUTTONDOWN,
    WM_NCDESTROY, WM_NCHITTEST, WM_PAINT, WM_RBUTTONDOWN,
};

use crate::cursor_tracker;
use crate::error::{PlatformError, Result};
use crate::messages::{HTTRANSPARENT, WM_APP_QUIT_TIMER, WM_APP_REASSERT_TOPMOST, WM_APP_TICK};
use crate::monitor_info;
use crate::overlay_state::OverlayWndState;
use crate::win32_ffi;

/// Asserts `WM_APP_TICK` is in the `WM_APP` band; also keeps the `WM_APP`
/// import used.
const _: () = assert!(WM_APP_TICK >= WM_APP);

/// Dispatch a message other than WM_NCCREATE / WM_NCDESTROY.
///
/// `Some(LRESULT)` is the WndProc result; `None` asks the caller to fall back
/// to `DefWindowProcW`.
#[must_use]
pub fn dispatch(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> Option<LRESULT> {
    let state_ptr = win32_ffi::get_userdata(hwnd)?;
    let state = win32_ffi::state_ref(state_ptr);

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
            if let Err(e) = apply_tick(state) {
                tracing::error!(parent: state.span(), error = %e,
                    "tick processing failed");
            }
            Some(LRESULT(0))
        },
        WM_APP_QUIT_TIMER => {
            // Auto-quit timer (`--duration-ms`); same effect as a Quit hotkey.
            tracing::info!(parent: state.span(), "auto-quit timer fired (--duration-ms)");
            win32_ffi::post_quit(0);
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
            // Posted by the ForegroundHook callback; the actual
            // SetWindowPos(HWND_TOPMOST) must run on the UI thread, so do it here.
            if let Err(e) = win32_ffi::accessibility::reassert_topmost(hwnd) {
                tracing::warn!(parent: state.span(), error = %e,
                    "reassert_topmost failed (foreground hook)");
            } else {
                tracing::trace!(parent: state.span(),
                    "topmost re-asserted after foreground change");
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
        WM_NCDESTROY => {
            // Reclaim and drop the `Box<OverlayWndState>` (clears GWLP_USERDATA).
            let _ = win32_ffi::take_userdata(hwnd);
            None
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
fn apply_tick(state: &OverlayWndState) -> Result<()> {
    let tick_start = std::time::Instant::now();
    let polled_cursor = cursor_tracker::poll();
    follow_active_monitor(state, polled_cursor);
    let drained_hotkeys = state.drain_hotkeys();
    let now_ms = state.now_ms();
    let input = TickInput {
        now_ms,
        polled_cursor,
        drained_hotkeys,
    };
    let world = state.tick_world_snapshot();
    let telemetry_refresh = state.hud_config().telemetry_refresh;
    let (next_world, effects) = step(world, &input, telemetry_refresh);
    state.store_tick_world(next_world);
    let result = apply_effects(state, &effects);
    let elapsed = tick_start.elapsed();
    let over_budget = is_over_budget(elapsed, crate::render_timing::refresh_rate_hz());
    state
        .frame_timing()
        .borrow_mut()
        .record_tick(elapsed, over_budget);
    result
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
fn apply_effects(state: &OverlayWndState, effects: &[TickEffect]) -> Result<()> {
    for effect in effects {
        match *effect {
            TickEffect::Quit => {
                tracing::info!(parent: state.span(), "Quit requested via tick");
                win32_ffi::post_quit(0);
            },
            TickEffect::DrawOverlay {
                mode,
                cursor,
                config,
            } => {
                let frame = render::frame(
                    State {
                        mode,
                        visible: true,
                        config,
                    },
                    cursor,
                    state.monitor(),
                );
                with_device_lost_recovery(state, "DrawOverlay", &|s| {
                    apply_overlay_frame(s, &frame)
                })?;
            },
            TickEffect::ClearOverlay => {
                with_device_lost_recovery(state, "ClearOverlay", &|s| {
                    apply_overlay_frame(s, &OverlayFrame::EMPTY)
                })?;
            },
            TickEffect::RefreshHud(s) => {
                let hz = crate::render_timing::refresh_rate_hz();
                let notifications = build_notifications(state);
                let telemetry = state.frame_timing().borrow().snapshot();
                let frame = hud_frame(
                    s,
                    *state.hud_config(),
                    state.monitor(),
                    hz,
                    &notifications,
                    state.hotkeys(),
                    telemetry,
                );
                with_device_lost_recovery(state, "RefreshHud", &|st| apply_hud_frame(st, &frame))?;
            },
            TickEffect::SetHudOpacity { state: s, cursor } => {
                // Compute fade opacity from cursor distance and apply it via
                // `SpriteVisual::SetOpacity`; cursor moves alone don't redraw the
                // surface (the baked color alpha is handled by RefreshHud).
                let opacity = hud_fade::compute_opacity(
                    s,
                    cursor,
                    hud_panel_rect(state),
                    state.hud_config().fade_decay_px,
                );
                apply_hud_opacity(state, opacity)?;
            },
            TickEffect::LogStateChanged {
                action,
                mode,
                visible,
            } => {
                tracing::info!(
                    parent: state.span(),
                    ?action,
                    ?mode,
                    visible,
                    "state changed"
                );
            },
        }
    }
    Ok(())
}

fn apply_overlay_frame(state: &OverlayWndState, frame: &OverlayFrame) -> Result<()> {
    if let Some(renderer) = state.renderer().borrow_mut().as_mut() {
        renderer.apply(frame)?;
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
/// rebuild the renderers and retry once; Quit after 3 consecutive failures.
///
/// `op` must release its `borrow_mut` before returning Err (the `apply_*_frame`
/// helpers scope it to an `if let`), so the rebuild can re-borrow without a
/// `BorrowMutError` (see overlay_state.rs RefCell rule).
fn with_device_lost_recovery(
    state: &OverlayWndState,
    operation: &'static str,
    op: &dyn Fn(&OverlayWndState) -> Result<()>,
) -> Result<()> {
    match op(state) {
        Ok(()) => {
            state.device_lost_count().set(0);
            Ok(())
        },
        Err(e) => {
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
                    rebuild_renderers(state)?;
                    op(state)
                },
                DeviceLostOutcome::Quit => {
                    tracing::error!(
                        target: "renderer.device_lost",
                        parent: state.span(),
                        operation,
                        hr = format_args!("{hr:#010x}").to_string(),
                        "device-lost exhausted (3 consecutive failures); requesting Quit"
                    );
                    if let Err(send_err) = state.hotkey_sender().send(OverlayAction::Quit) {
                        tracing::error!(parent: state.span(), error = %send_err,
                            "failed to send Quit after device-lost exhaustion");
                    }
                    // Quit is async (drained next tick); still propagate this
                    // tick's Err to the caller.
                    Err(e)
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
fn rebuild_renderers(state: &OverlayWndState) -> Result<()> {
    let hwnd = state.hwnd().ok_or(PlatformError::NullHandle {
        operation: "rebuild_renderers: HWND unset",
    })?;
    let (overlay, hud) = crate::renderer_backend::build_backends(hwnd, state.hud_config())?;
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

/// Build the `HudNotification` list from hotkey conflicts plus live toasts.
fn build_notifications(state: &OverlayWndState) -> Vec<linerule_core::HudNotification> {
    let conflicts = state.hotkey_conflicts();
    let mut out = Vec::with_capacity(conflicts.len() + 1);
    if !conflicts.is_empty() {
        out.push(linerule_core::HudNotification {
            class: linerule_core::NotificationClass::Warn,
            message: format!("Hotkey conflicts: {}", conflicts.len()),
            until_ms: i64::MAX,
        });
        for c in conflicts.iter().take(6) {
            let reason = match &c.reason {
                crate::overlay_state::HotkeyFailure::ChordParse(_) => "parse error",
                crate::overlay_state::HotkeyFailure::RegisterHotKey { .. } => "already in use",
            };
            out.push(linerule_core::HudNotification {
                class: linerule_core::NotificationClass::Warn,
                message: format!("  {} → {}", c.spec, reason),
                until_ms: i64::MAX,
            });
        }
    }
    out.extend(state.live_notifications());
    out
}

/// HUD panel bounds (logical px), computed like `hud_frame` and rounded to
/// `ScreenRect<Logical>` for `compute_opacity`.
fn hud_panel_rect(state: &OverlayWndState) -> ScreenRect<Logical> {
    let hud = state.hud_config();
    let monitor = state.monitor();
    let width = hud.geometry.width;
    let height = hud.geometry.height;
    let margin = hud.geometry.margin;
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        reason = "screen-space px fit the f32 mantissa; rounded result stays in i32 range"
    )]
    let monitor_right = monitor.left() + i32::try_from(monitor.width).unwrap_or(i32::MAX);
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        reason = "ditto"
    )]
    let panel_left = monitor_right - (margin + width).round() as i32;
    let panel_top = monitor.top() + margin.round() as i32;
    let w = width.round() as u32;
    let h = height.round() as u32;
    ScreenRect::new(Point::<Logical>::new(panel_left, panel_top), w, h)
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

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

    proptest! {
        /// The result always lies in `[0, i32::MAX]`.
        #[test]
        fn wparam_to_id_stays_in_i32_positive_range(raw in any::<usize>()) {
            let id = wparam_as_hotkey_id(WPARAM(raw));
            prop_assert!(id >= 0);
            prop_assert!(id <= i32::MAX);
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
