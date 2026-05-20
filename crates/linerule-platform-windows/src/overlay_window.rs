//! 透明 click-through な topmost オーバーレイ HWND のライフサイクル管理。
//!
//! Phase D で `IDCompositionDesktopDevice` / `IDCompositionTarget` を attach し
//! た renderer を `OverlayWndState` 側に install する。Phase E で同じ HWND を
//! `RegisterHotKey` の target にして `WM_HOTKEY` を受信する（message-only HWND
//! を作らない設計 — D1 in `docs/plans/...`）。

#![forbid(unsafe_code)]

use core::ptr::NonNull;

use linerule_core::input::chord;
use linerule_core::input::win32_vk::chord_to_win32;
use linerule_core::{HotkeyMap, HudConfig, Logical, OverlayAction, ScreenRect, TapStepConfig};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{
    WINDOW_EX_STYLE, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_NOREDIRECTIONBITMAP, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};
use windows::core::w;

use crate::error::{PlatformError, Result};
use crate::overlay_state::{HotkeyConflict, HotkeyFailure, OverlayWndState};
use crate::win32_ffi::hotkey as hotkey_ffi;
use crate::{ex_style_snapshot, win32_ffi, window_class};

/// linerule overlay window の組み合わせ ex-style。
///
/// - [`WS_EX_LAYERED`] + [`WS_EX_NOREDIRECTIONBITMAP`]: DWM が GPU 直結で
///   per-pixel α 合成する layered window
/// - [`WS_EX_TRANSPARENT`]: DWM レベルでクリックを下層に通す（click-through）
/// - [`WS_EX_NOACTIVATE`]: 通常 focus を奪わない
/// - [`WS_EX_TOOLWINDOW`]: Alt+Tab / taskbar から除外
/// - [`WS_EX_TOPMOST`]: 常時最前面
pub const OVERLAY_EX_STYLE: WINDOW_EX_STYLE = WINDOW_EX_STYLE(
    WS_EX_LAYERED.0
        | WS_EX_TRANSPARENT.0
        | WS_EX_NOREDIRECTIONBITMAP.0
        | WS_EX_NOACTIVATE.0
        | WS_EX_TOOLWINDOW.0
        | WS_EX_TOPMOST.0,
);

/// 透明 click-through オーバーレイ HWND。Drop で `UnregisterHotKey` 群と
/// `DestroyWindow` を呼ぶ RAII ハンドル。
pub struct OverlayWindow {
    hwnd: HWND,
    /// `Box::into_raw` 由来のポインタ。実 drop は WM_NCDESTROY 経由で
    /// `win32_ffi::take_userdata` が回収する。
    state: NonNull<OverlayWndState>,
}

// SAFETY equivalent: HWND は thread-affinity を持つので明示的に Send/Sync を実装しない。

impl OverlayWindow {
    /// 指定した monitor bounds に重なる位置・サイズで HWND を作成する。
    /// デフォルトの `TickWorld::INITIAL` (mode = Off) で起動する。
    ///
    /// # Errors
    /// `RegisterClassExW` / `CreateWindowExW` / `GetModuleHandleW` が失敗したとき。
    pub fn new(monitor: ScreenRect<Logical>, hud_config: HudConfig) -> Result<Self> {
        Self::new_with_initial_world(
            monitor,
            hud_config,
            linerule_core::input::tick::TickWorld::INITIAL,
        )
    }

    /// 指定 monitor bounds + 任意 `TickWorld` で HWND を作成する。
    /// `--initial-mode horizontal` 等の経路で起動時 mode を上書きする場合に使う。
    ///
    /// # Errors
    /// `RegisterClassExW` / `CreateWindowExW` / `GetModuleHandleW` が失敗したとき。
    pub fn new_with_initial_world(
        monitor: ScreenRect<Logical>,
        hud_config: HudConfig,
        initial_world: linerule_core::input::tick::TickWorld,
    ) -> Result<Self> {
        let _atom = window_class::ensure_registered()?;

        let state_box = Box::new(OverlayWndState::new_with_initial_world(
            tracing::info_span!("overlay_window", class = "linerule-rs-overlay"),
            monitor,
            hud_config,
            initial_world,
        ));
        let state_ptr = Box::into_raw(state_box);

        let width = i32::try_from(monitor.width).unwrap_or(i32::MAX);
        let height = i32::try_from(monitor.height).unwrap_or(i32::MAX);

        let create_result = win32_ffi::create_window(
            OVERLAY_EX_STYLE,
            window_class::OVERLAY_CLASS_NAME,
            w!("linerule"),
            WS_POPUP,
            monitor.left(),
            monitor.top(),
            width,
            height,
            state_ptr,
        );

        match create_result {
            Ok(hwnd) => {
                ex_style_snapshot::capture(hwnd, "after CreateWindowExW");
                // `WS_EX_LAYERED + WS_EX_NOREDIRECTIONBITMAP + DComp` は仕様上
                // 「dcomp content の commit で表示される」が、実環境では
                // ShowWindow を明示呼びしないと visible にならないケースが
                // 確認されている (Phase I 実機検証)。SW_SHOWNOACTIVATE +
                // WS_EX_NOACTIVATE の二重防壁で focus 奪取は防ぐ。
                win32_ffi::show_window_noactivate(hwnd);

                // SAFETY-equivalent: NonNull<_> は Box::into_raw の戻り値で常に non-null
                let state = NonNull::new(state_ptr).expect("Box::into_raw is never null");
                Ok(Self { hwnd, state })
            },
            Err(e) => {
                // CreateWindowExW 失敗時は WM_NCCREATE が呼ばれていない可能性が
                // ある（Win32 仕様上、呼ばれてから失敗するケースもあるが、保守的に
                // 失敗を確認したらこちらで box を回収する）。`take_userdata` で
                // 既に WM_NCDESTROY 経由で回収されていれば no-op。
                win32_ffi::drop_userdata_raw(state_ptr);
                Err(e)
            },
        }
    }

    /// 内部 HWND を借りる。Phase F の `RenderClock::spawn` 等から target を取得
    /// するときに使う。
    #[must_use]
    pub fn hwnd(&self) -> HWND {
        self.hwnd
    }

    /// instance state を借りる（テスト / 診断用）。
    #[must_use]
    pub fn state(&self) -> &OverlayWndState {
        win32_ffi::state_ref(self.state)
    }

    /// Phase D + G: DirectComposition + Direct2D の visual tree を attach し、
    /// overlay slit 用 `CompositionRenderer` と HUD 用 `HudRenderer` を
    /// `OverlayWndState` 側に install する。
    ///
    /// # Errors
    /// D3D11 / DXGI / D2D / DComp / DWrite のいずれかの初期化に失敗したとき。
    pub fn attach_dcomp(&mut self) -> Result<()> {
        let renderer = crate::composition_renderer::CompositionRenderer::new(self.hwnd)?;
        let hud_config = *self.state().hud_config();
        let hud_renderer = crate::hud_renderer::HudRenderer::new(renderer.pipeline(), &hud_config)?;
        ex_style_snapshot::capture(self.hwnd, "after attach_dcomp");
        self.state().install_renderer(renderer);
        self.state().install_hud_renderer(hud_renderer);
        Ok(())
    }

    /// Phase E: `HotkeyMap` の chord を順に解析・`RegisterHotKey` し、成功した
    /// 組を `OverlayWndState::record_hotkey` に積む。失敗した chord は warn +
    /// `record_hotkey_conflict` で残し、続きを継続する。
    ///
    /// # Errors
    /// 通常は失敗しない（個別の chord 失敗は内部で warn して conflict に積む）。
    /// 将来 OS レベルで catastrophic に失敗する API を呼ぶようになった場合のみ
    /// `Err` を返す可能性を残す。
    pub fn register_hotkeys(&self, hotkeys: &HotkeyMap, tap_step: TapStepConfig) -> Result<()> {
        let bumps = (tap_step.thickness, tap_step.opacity);
        let pairs: [(i32, &'static str, OverlayAction); 7] = [
            (1, hotkeys.cycle_mode, OverlayAction::CycleMode),
            (2, hotkeys.toggle_visible, OverlayAction::ToggleVisible),
            (3, hotkeys.thicker, OverlayAction::BumpThickness(bumps.0)),
            (4, hotkeys.thinner, OverlayAction::BumpThickness(-bumps.0)),
            (5, hotkeys.more_opaque, OverlayAction::BumpOpacity(bumps.1)),
            (6, hotkeys.less_opaque, OverlayAction::BumpOpacity(-bumps.1)),
            (7, hotkeys.quit, OverlayAction::Quit),
        ];
        for (id, spec, action) in pairs {
            self.register_one(id, spec, action);
        }
        Ok(())
    }

    fn register_one(&self, id: i32, spec: &'static str, action: OverlayAction) {
        let state = self.state();
        let chord = match chord::parse(spec) {
            Ok(c) => c,
            Err(err) => {
                tracing::warn!(spec, ?action, ?err, "chord parse failed; skipping hotkey");
                state.record_hotkey_conflict(HotkeyConflict {
                    spec,
                    action,
                    reason: HotkeyFailure::ChordParse(err),
                });
                return;
            },
        };
        let (mods, vk) = chord_to_win32(chord);
        match hotkey_ffi::register_hotkey(self.hwnd, id, mods, vk) {
            Ok(()) => {
                state.record_hotkey(id, action);
                tracing::info!(spec, ?action, id, "hotkey registered");
            },
            Err(err) => {
                let hresult = match err {
                    PlatformError::BadHr { hr, .. } => hr,
                    _ => 0,
                };
                tracing::warn!(
                    spec,
                    ?action,
                    ?err,
                    "RegisterHotKey failed; skipping hotkey"
                );
                state.record_hotkey_conflict(HotkeyConflict {
                    spec,
                    action,
                    reason: HotkeyFailure::RegisterHotKey { hresult },
                });
            },
        }
    }
}

impl Drop for OverlayWindow {
    fn drop(&mut self) {
        // HWND が生きているうちに UnregisterHotKey を済ませる
        let ids = self.state().registered_hotkey_ids();
        for id in ids {
            if let Err(e) = hotkey_ffi::unregister_hotkey(self.hwnd, id) {
                tracing::warn!(id, error = %e, "UnregisterHotKey failed during OverlayWindow::drop");
            }
        }
        // `DestroyWindow` が WM_NCDESTROY を発火し、`crate::wndproc::dispatch`
        // が `win32_ffi::take_userdata` を呼んで Box<OverlayWndState> を回収する。
        if let Err(e) = win32_ffi::destroy_window(self.hwnd) {
            tracing::warn!(error = %e, "DestroyWindow failed during OverlayWindow::drop");
        }
    }
}

#[cfg(test)]
mod tests {
    //! `OverlayWindow::new` 自体は HWND 作成を伴い Windows 環境必須なので、
    //! ここでは pure const `OVERLAY_EX_STYLE` の bit pattern を検証する。
    //! ex-style の組み合わせは PR で簡単に変わるため、各 flag が確実に立って
    //! いることを test で固定する (ADR-0003 系: Win32 設計判断の test 化)。

    use super::*;

    #[test]
    fn overlay_ex_style_contains_layered() {
        assert_ne!(
            OVERLAY_EX_STYLE.0 & WS_EX_LAYERED.0,
            0,
            "OVERLAY_EX_STYLE must include WS_EX_LAYERED (DComp per-pixel alpha)"
        );
    }

    #[test]
    fn overlay_ex_style_contains_no_redirection_bitmap() {
        assert_ne!(
            OVERLAY_EX_STYLE.0 & WS_EX_NOREDIRECTIONBITMAP.0,
            0,
            "OVERLAY_EX_STYLE must include WS_EX_NOREDIRECTIONBITMAP (skip DWM redirection)"
        );
    }

    #[test]
    fn overlay_ex_style_contains_transparent() {
        assert_ne!(
            OVERLAY_EX_STYLE.0 & WS_EX_TRANSPARENT.0,
            0,
            "OVERLAY_EX_STYLE must include WS_EX_TRANSPARENT (click-through)"
        );
    }

    #[test]
    fn overlay_ex_style_contains_no_activate() {
        assert_ne!(
            OVERLAY_EX_STYLE.0 & WS_EX_NOACTIVATE.0,
            0,
            "OVERLAY_EX_STYLE must include WS_EX_NOACTIVATE (no focus steal)"
        );
    }

    #[test]
    fn overlay_ex_style_contains_tool_window() {
        assert_ne!(
            OVERLAY_EX_STYLE.0 & WS_EX_TOOLWINDOW.0,
            0,
            "OVERLAY_EX_STYLE must include WS_EX_TOOLWINDOW (Alt+Tab/taskbar exclusion)"
        );
    }

    #[test]
    fn overlay_ex_style_contains_topmost() {
        assert_ne!(
            OVERLAY_EX_STYLE.0 & WS_EX_TOPMOST.0,
            0,
            "OVERLAY_EX_STYLE must include WS_EX_TOPMOST (always on top)"
        );
    }

    /// `OVERLAY_EX_STYLE` には 6 つの WS_EX_* フラグだけが立っている。
    /// 余計な flag が混入していないか (例: 誤って WS_EX_APPWINDOW を加えると
    /// taskbar に出てしまう) を XOR check で監視する。
    #[test]
    fn overlay_ex_style_has_no_unintended_flags() {
        let expected = WS_EX_LAYERED.0
            | WS_EX_TRANSPARENT.0
            | WS_EX_NOREDIRECTIONBITMAP.0
            | WS_EX_NOACTIVATE.0
            | WS_EX_TOOLWINDOW.0
            | WS_EX_TOPMOST.0;
        assert_eq!(
            OVERLAY_EX_STYLE.0, expected,
            "OVERLAY_EX_STYLE has unexpected extra flags"
        );
    }
}
