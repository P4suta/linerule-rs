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
use linerule_core::{HudFrame, Logical, OverlayFrame, Point, ScreenRect, State, hud_frame, render};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    WM_APP, WM_DESTROY, WM_HOTKEY, WM_LBUTTONDOWN, WM_MBUTTONDOWN, WM_NCDESTROY, WM_NCHITTEST,
    WM_PAINT, WM_RBUTTONDOWN,
};

use crate::cursor_tracker;
use crate::error::Result;
use crate::messages::{HTTRANSPARENT, WM_APP_TICK};
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
pub fn dispatch(hwnd: HWND, msg: u32, wparam: WPARAM, _lparam: LPARAM) -> Option<LRESULT> {
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
fn apply_tick(state: &OverlayWndState) -> Result<()> {
    let polled_cursor = cursor_tracker::poll();
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
    apply_effects(state, &effects)
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
                apply_overlay_frame(state, &frame)?;
            },
            TickEffect::ClearOverlay => {
                apply_overlay_frame(state, &OverlayFrame::EMPTY)?;
            },
            TickEffect::RefreshHud(s) => {
                let hz = crate::render_timing::refresh_rate_hz();
                let notifications = build_notifications(state);
                let frame = hud_frame(s, *state.hud_config(), state.monitor(), hz, &notifications);
                apply_hud_frame(state, &frame)?;
            },
            TickEffect::SetHudOpacity { state: s, cursor } => {
                // PR 3 では HUD opacity を `HudFrame` の色に bake する設計のため、
                // visual 単位 opacity 更新は行わない（描画器単位で 1 frame 再描画
                // するコスト > ベース不透明度のままの視覚差、と判断）。fade 反映は
                // 次の RefreshHud で `hud_frame()` が computed opacity を入れる
                // 拡張で対応する。本 handler では tracing のみ。
                let _ = hud_fade::compute_opacity(
                    s,
                    cursor,
                    hud_panel_rect(state),
                    state.hud_config().fade_decay_px,
                );
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
    use super::*;

    #[test]
    fn wparam_to_id_truncates_safely() {
        assert_eq!(wparam_as_hotkey_id(WPARAM(1)), 1);
        assert_eq!(wparam_as_hotkey_id(WPARAM(7)), 7);
        // usize::MAX を渡しても i32::MAX に潰れて panic しない
        assert_eq!(wparam_as_hotkey_id(WPARAM(usize::MAX)), i32::MAX);
    }
}
