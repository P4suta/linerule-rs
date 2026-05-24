//! WndProc dispatch ロジック (forbid(unsafe_code))。
//!
//! 実 FFI 入口 `unsafe extern "system" fn overlay_wnd_proc` は `win32_ffi.rs`
//! 側にあり、本ファイルは `dispatch()` 関数として呼び出される。WM_NCCREATE で
//! `GWLP_USERDATA` に Box を仕込む処理も `win32_ffi.rs` 側にあるため、本
//! ファイルでは取り出した state ref を使うだけで `unsafe` は出現しない。
//!
//! ## RefCell borrow ルール
//!
//! `OverlayWndState` の `RefCell` フィールドは本ファイル内でのみ
//! `borrow_mut()` する。borrow 中に Win32 API の **同期再入**（`SendMessageW`
//! / `DestroyWindow` / `MessageBoxW` 系）を呼ばないこと。`PostMessageW` /
//! `PostQuitMessage` は async なので OK。違反時は `RefCell::borrow_mut` が
//! panic し、`win32_ffi::overlay_wnd_proc` の `catch_unwind` が拾って
//! `DefWindowProcW` にフォールバックする。

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

use crate::composition_renderer::CompositionRenderer;
use crate::cursor_tracker;
use crate::error::{PlatformError, Result};
use crate::hud_renderer::HudRenderer;
use crate::messages::{HTTRANSPARENT, WM_APP_QUIT_TIMER, WM_APP_REASSERT_TOPMOST, WM_APP_TICK};
use crate::monitor_info;
use crate::overlay_state::OverlayWndState;
use crate::win32_ffi;

/// `WM_APP_TICK` の数値が `WM_APP` 帯にあることを const にしてリンカへ示す
/// （`WM_APP` import 未使用警告を抑える兼ねた sanity check）。
const _: () = assert!(WM_APP_TICK >= WM_APP);

/// WM_NCCREATE / WM_NCDESTROY 以外のメッセージを dispatch する純粋関数。
///
/// 戻り値:
/// - `Some(LRESULT)`: 当該メッセージを処理した。返り値はそのまま `WndProc` の戻り値になる。
/// - `None`: 処理せず `DefWindowProcW` にフォールバックすることを呼び出し側に依頼。
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
            // CI smoke test 用の auto-quit message (boot.rs の duration thread
            // から発行)。`Ctrl+Alt+Q` 経由の Quit と同等の挙動として
            // `PostQuitMessage(0)` を呼ぶ。
            tracing::info!(parent: state.span(), "auto-quit timer fired (--duration-ms)");
            win32_ffi::post_quit(0);
            Some(LRESULT(0))
        },
        WM_DPICHANGED => {
            // Per-Monitor DPI Aware V2 で受信。`lparam` は `RECT*` (OS が推奨
            // する新 window rect、physical px ベースだが Win32 が logical 換算
            // 済みで渡す)。`wparam` HIWORD/LOWORD は新 Y/X DPI (通常同値)。
            // overlay は virtual-screen 全体に張られていて、DComp が compositor
            // 側で per-monitor DPI を適用するため font/HUD layout の再計算は
            // 不要 (DWrite text format は DIPs 入力, HudConfig は logical px)。
            // ここでは OS 推奨 rect で SetWindowPos するだけ (issue #44)。
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
            // 解像度や monitor 構成が変化した。active monitor cache は
            // `apply_tick::follow_active_monitor` (issue #46) で per-tick に
            // 再解決されるため invalidate 不要。event の可観測性のため log
            // だけ残し DefWindowProcW にフォールバックする。
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
            // ForegroundHook の callback (OS hook thread) から PostMessage で
            // 届く。実 SetWindowPos(HWND_TOPMOST) は UI thread 必須なのでここで
            // 実行する (ADR-0012)。
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
            // dcomp が描画を駆動するので WM_PAINT で paint する必要はない。
            // DefWindowProcW が ValidateRect 相当の処理をしてくれるが、明示的に
            // 0 を返してログ noise を避ける。
            Some(LRESULT(0))
        },
        WM_DESTROY => {
            win32_ffi::post_quit(0);
            Some(LRESULT(0))
        },
        WM_NCDESTROY => {
            // `Box<OverlayWndState>` を取り戻して drop。win32_ffi 側で
            // GWLP_USERDATA を 0 に戻し Box::from_raw する。
            let _ = win32_ffi::take_userdata(hwnd);
            None
        },
        _ => None,
    }
}

/// `WM_HOTKEY` の `wparam` は hotkey ID（`RegisterHotKey` で渡した `i32`）。
/// usize → i32 への lossy 変換を 1 箇所に閉じ込める。
fn wparam_as_hotkey_id(wparam: WPARAM) -> i32 {
    i32::try_from(wparam.0).unwrap_or(i32::MAX)
}

/// 1 tick 分の処理: cursor poll → hotkey drain → `tick::step` → `apply_effects`。
/// frame timing は先頭で `Instant::now()` を取り、末尾の elapsed を
/// `FrameTimingTracker::record_tick` に流す (HUD telemetry の p99 / drops 用)。
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

/// cursor 位置から active monitor を解決し、現 cache と異なれば
/// `state.set_monitor` で更新する (issue #46)。`bounds_for_point` は
/// `MonitorFromPoint(MONITOR_DEFAULTTONEAREST)` 経由で remote desktop 等の
/// 画面外 cursor にも fallback する。失敗時は warn だけ出して現状維持。
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

/// elapsed が `RenderConfig::DEFAULT.warn_ratio * (1000 / refresh_hz)` を
/// 超えたら over-budget と判定する。値は HUD の `drops` カウンタに反映される。
fn is_over_budget(elapsed: std::time::Duration, refresh_hz: u32) -> bool {
    let hz = refresh_hz.max(1);
    let warn_ratio = linerule_core::RenderConfig::DEFAULT.warn_ratio;
    let budget_ms = warn_ratio * 1000.0 / f64::from(hz);
    elapsed.as_secs_f64() * 1000.0 > budget_ms
}

/// `TickEffect` を順に platform へ反映する。
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
                // cursor 距離から fade opacity を pure 関数で計算し、HUD visual
                // の `IDCompositionVisual3::SetOpacity2` で multiplicative に
                // 適用する (issue #47)。`frame.opacity` の bake (色 alpha) は
                // RefreshHud 側で別軸として保持されるので、cursor 移動だけで
                // surface 再描画は走らない。
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

/// `HudRenderer::set_opacity` の wndproc-side wrapper。`RefCell` の `borrow_mut`
/// を 1 箇所に閉じ込め、renderer 未 attach の起動直後 (Phase D 前) でも no-op
/// で済むようにする。
fn apply_hud_opacity(state: &OverlayWndState, opacity: f32) -> Result<()> {
    if let Some(renderer) = state.hud_renderer().borrow_mut().as_mut() {
        renderer.set_opacity(opacity)?;
    }
    Ok(())
}

/// `apply_overlay_frame` / `apply_hud_frame` を device-lost rebuild で wrap する
/// (issue #45)。失敗 HRESULT が DXGI/D2D の device-lost 系なら一度 renderer を
/// 作り直して 1 度だけ retry する。連続 3 回で `OverlayAction::Quit` を要求。
///
/// `op` は `Fn(&OverlayWndState) -> Result<()>` で、`apply_*_frame` ヘルパーを
/// 渡す。closure 内で `borrow_mut` を `if let` scope に閉じ込めているので、
/// Err 復帰時には borrow は drop 済み → `install_renderer` で再 borrow しても
/// `BorrowMutError` にならない (overlay_state.rs の RefCell 不変条件)。
fn with_device_lost_recovery(
    state: &OverlayWndState,
    operation: &'static str,
    op: &dyn Fn(&OverlayWndState) -> Result<()>,
) -> Result<()> {
    match op(state) {
        Ok(()) => {
            // 成功で連続失敗カウンタを reset。
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
                    // Quit 経路は async (次 tick で drain される) なので、この
                    // tick の Err は propagate して caller に通知する。
                    Err(e)
                },
            }
        },
    }
}

/// `PlatformError::BadHr` を分解して、device-lost 系 HRESULT であれば値を返す。
fn device_lost_hr(e: &PlatformError) -> Option<i32> {
    match e {
        PlatformError::BadHr { hr, .. } if is_device_lost_hresult(*hr) => Some(*hr),
        _ => None,
    }
}

/// CompositionRenderer + HudRenderer を新規構築して state に install し直す
/// (issue #45)。古い renderer は `install_*` で差し替えられた時点で Drop され、
/// 古い COM オブジェクト (pipeline / visual / surface) は RAII で Release される。
fn rebuild_renderers(state: &OverlayWndState) -> Result<()> {
    let hwnd = state.hwnd().ok_or(PlatformError::NullHandle {
        operation: "rebuild_renderers: HWND unset",
    })?;
    let new_renderer = CompositionRenderer::new(hwnd)?;
    let new_hud = HudRenderer::new(new_renderer.pipeline(), state.hud_config())?;
    state.install_renderer(new_renderer);
    state.install_hud_renderer(new_hud);
    tracing::info!(
        target: "renderer.device_lost",
        parent: state.span(),
        "renderers rebuilt successfully"
    );
    Ok(())
}

/// `OverlayWndState` の hotkey 競合一覧 + 即時 toast を `HudNotification` の
/// 列に変換する。`hud_frame()` 側でレイアウト計算する純粋関数フローに統合
/// する (旧 `append_conflict_rows` の責務移譲、ADR-0009)。
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
    // 短寿命 runtime notifications (push_notification 経由) は OverlayWndState 側
    // で expire 済みを除去した snapshot を取得して結合する。
    out.extend(state.live_notifications());
    out
}

/// HUD パネルの bounds (logical px) を `hud_frame` と同じロジックで計算する。
/// `compute_opacity` に渡すために `ScreenRect<Logical>` (i32) に丸める。
fn hud_panel_rect(state: &OverlayWndState) -> ScreenRect<Logical> {
    let hud = state.hud_config();
    let monitor = state.monitor();
    let width = hud.geometry.width;
    let height = hud.geometry.height;
    let margin = hud.geometry.margin;
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        reason = "screen-space px は f32 mantissa に余裕で収まり、ceil の結果は i32 範囲内"
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
        // usize::MAX を渡しても i32::MAX に潰れて panic しない
        assert_eq!(wparam_as_hotkey_id(WPARAM(usize::MAX)), i32::MAX);
    }

    /// `wparam_as_hotkey_id` の境界条件: `i32::MAX` 以下の usize は完全保存、
    /// それを超える値は `i32::MAX` に saturate。負値はそもそも usize なので
    /// 起こらない。proptest で全範囲を網羅する。
    #[test]
    fn wparam_to_id_at_i32_max_boundary_is_preserved() {
        assert_eq!(
            wparam_as_hotkey_id(WPARAM(i32::MAX as usize)),
            i32::MAX,
            "i32::MAX boundary should pass through exactly"
        );
    }

    proptest! {
        /// 任意の usize 入力に対し、戻り値が `[0, i32::MAX]` の範囲に必ず収まる。
        #[test]
        fn wparam_to_id_stays_in_i32_positive_range(raw in any::<usize>()) {
            let id = wparam_as_hotkey_id(WPARAM(raw));
            prop_assert!(id >= 0);
            prop_assert!(id <= i32::MAX);
        }

        /// `i32::MAX` 以下の値は完全保存される (lossless round-trip)。
        #[test]
        fn wparam_to_id_preserves_small_values(small in 0i32..=i32::MAX) {
            let id = wparam_as_hotkey_id(WPARAM(small as usize));
            prop_assert_eq!(id, small);
        }

        /// `i32::MAX` を超える値はすべて `i32::MAX` に saturate する。
        #[test]
        fn wparam_to_id_saturates_above_i32_max(huge in (i32::MAX as usize + 1)..=usize::MAX) {
            let id = wparam_as_hotkey_id(WPARAM(huge));
            prop_assert_eq!(id, i32::MAX);
        }
    }
}
