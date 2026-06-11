//! Lifecycle of the transparent, click-through, topmost overlay HWND.
//!
//! Attaches the WinRT composition renderer and installs it into
//! `OverlayWndState`, and uses the same HWND as the `RegisterHotKey` target for
//! `WM_HOTKEY` (no separate message-only HWND).

#![forbid(unsafe_code)]

use core::ptr::NonNull;

use linerule_core::input::chord;
use linerule_core::input::win32_vk::chord_to_win32;
use linerule_core::{
    AnimConfig, HotkeyMap, HudConfig, Logical, OverlayAction, ScreenRect, TapStepConfig,
};
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

/// Combined ex-style for the overlay window.
///
/// - `WS_EX_LAYERED` + `WS_EX_NOREDIRECTIONBITMAP` — DWM-composited per-pixel alpha (layered window)
/// - `WS_EX_TRANSPARENT` — click-through at the DWM level
/// - `WS_EX_NOACTIVATE` — never steals focus
/// - `WS_EX_TOOLWINDOW` — excluded from Alt+Tab / taskbar
/// - `WS_EX_TOPMOST` — always on top
pub const OVERLAY_EX_STYLE: WINDOW_EX_STYLE = WINDOW_EX_STYLE(
    WS_EX_LAYERED.0
        | WS_EX_TRANSPARENT.0
        | WS_EX_NOREDIRECTIONBITMAP.0
        | WS_EX_NOACTIVATE.0
        | WS_EX_TOOLWINDOW.0
        | WS_EX_TOPMOST.0,
);

/// RAII handle for the transparent click-through overlay HWND; Drop calls
/// `UnregisterHotKey` and `DestroyWindow`.
pub struct OverlayWindow {
    hwnd: HWND,
    /// Pointer from `Box::into_raw`; reclaimed via `win32_ffi::take_userdata`
    /// on WM_NCDESTROY.
    state: NonNull<OverlayWndState>,
}

// SAFETY: HWND is thread-affine, so no Send/Sync is implemented.

impl OverlayWindow {
    /// Create the HWND covering `monitor`, starting at `TickWorld::INITIAL`
    /// (mode = Off).
    ///
    /// # Errors
    /// If `RegisterClassExW` / `CreateWindowExW` / `GetModuleHandleW` fail.
    pub fn new(
        monitor: ScreenRect<Logical>,
        hud_config: HudConfig,
        anim_config: AnimConfig,
    ) -> Result<Self> {
        Self::new_with_initial_world(
            monitor,
            hud_config,
            anim_config,
            linerule_core::input::tick::TickWorld::INITIAL,
        )
    }

    /// Create the HWND covering `monitor` with an explicit `TickWorld`, used to
    /// override the startup mode (e.g. `--initial-mode horizontal`).
    ///
    /// # Errors
    /// If `RegisterClassExW` / `CreateWindowExW` / `GetModuleHandleW` fail.
    pub fn new_with_initial_world(
        monitor: ScreenRect<Logical>,
        hud_config: HudConfig,
        anim_config: AnimConfig,
        initial_world: linerule_core::input::tick::TickWorld,
    ) -> Result<Self> {
        let _atom = window_class::ensure_registered()?;

        let state_box = Box::new(OverlayWndState::new_with_initial_world(
            tracing::info_span!("overlay_window", class = "linerule-rs-overlay"),
            monitor,
            hud_config,
            anim_config,
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
                // Explicit ShowWindow is needed for the window to become visible;
                // SW_SHOWNOACTIVATE + WS_EX_NOACTIVATE both prevent focus steal.
                win32_ffi::show_window_noactivate(hwnd);

                // SAFETY: Box::into_raw is never null.
                let state = NonNull::new(state_ptr).expect("Box::into_raw is never null");
                // Shelve the HWND so device-lost rebuild can call
                // `WinrtCompositionRenderer::new(hwnd)` again.
                win32_ffi::state_ref(state).set_hwnd(hwnd);
                Ok(Self { hwnd, state })
            },
            Err(e) => {
                // On CreateWindowExW failure, reclaim the box here; no-op if
                // WM_NCDESTROY already reclaimed it via `take_userdata`.
                win32_ffi::drop_userdata_raw(state_ptr);
                Err(e)
            },
        }
    }

    /// The inner HWND.
    #[must_use]
    pub fn hwnd(&self) -> HWND {
        self.hwnd
    }

    /// The instance state (test / diagnostics).
    #[must_use]
    pub fn state(&self) -> &OverlayWndState {
        win32_ffi::state_ref(self.state)
    }

    /// Attach the WinRT composition visual tree and install the overlay / HUD
    /// renderers into `OverlayWndState`.
    ///
    /// # Errors
    /// If any of D3D11 / DXGI / D2D / DWrite / WinRT composition init fails.
    /// There is no fallback, so failure is fatal.
    pub fn attach_compositor(&mut self) -> Result<()> {
        let hud_config = *self.state().hud_config();
        let (overlay, hud) = crate::renderer_backend::build_backends(self.hwnd, &hud_config)?;
        ex_style_snapshot::capture(self.hwnd, "after attach_compositor");
        self.state().install_renderer(overlay);
        self.state().install_hud_renderer(hud);
        tracing::info!(backend = "winrt", "composition backend attached");
        Ok(())
    }

    /// Parse and `RegisterHotKey` each chord in the `HotkeyMap`, recording the
    /// successes; failed chords are warned and pushed to the conflict list.
    ///
    /// # Errors
    /// Normally infallible (per-chord failures become conflicts). Reserved for a
    /// future catastrophic OS-level failure.
    pub fn register_hotkeys(&self, hotkeys: &HotkeyMap, tap_step: TapStepConfig) -> Result<()> {
        // Retain the chords as the source for the HUD hotkey-help rows.
        self.state().record_hotkeys(*hotkeys);
        let bumps = (tap_step.thickness, tap_step.opacity);
        // The `repeatable` field is `true` only for Bump actions (drops
        // `MOD_NOREPEAT`) to allow hold-to-repeat; Toggle actions stay `false`.
        let pairs: [(i32, &'static str, OverlayAction, bool); 9] = [
            (1, hotkeys.cycle_mode, OverlayAction::CycleMode, false),
            (2, hotkeys.toggle_on_off, OverlayAction::ToggleOnOff, false),
            (
                3,
                hotkeys.thicker,
                OverlayAction::BumpThickness(bumps.0),
                true,
            ),
            (
                4,
                hotkeys.thinner,
                OverlayAction::BumpThickness(-bumps.0),
                true,
            ),
            (
                5,
                hotkeys.more_opaque,
                OverlayAction::BumpOpacity(bumps.1),
                true,
            ),
            (
                6,
                hotkeys.less_opaque,
                OverlayAction::BumpOpacity(-bumps.1),
                true,
            ),
            (7, hotkeys.quit, OverlayAction::Quit, false),
            // Effect cycle is a discrete toggle ⇒ non-repeatable (MOD_NOREPEAT).
            (8, hotkeys.cycle_effect, OverlayAction::CycleEffect, false),
            // HUD detail (chip ⇄ full) is a discrete toggle ⇒ non-repeatable.
            (9, hotkeys.toggle_hud, OverlayAction::ToggleHudDetail, false),
        ];
        for (id, spec, action, repeatable) in pairs {
            self.register_one(id, spec, action, repeatable);
        }
        Ok(())
    }

    fn register_one(&self, id: i32, spec: &'static str, action: OverlayAction, repeatable: bool) {
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
        match hotkey_ffi::register_hotkey(self.hwnd, id, mods, vk, repeatable) {
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
        // Unregister hotkeys while the HWND is still alive.
        let ids = self.state().registered_hotkey_ids();
        for id in ids {
            if let Err(e) = hotkey_ffi::unregister_hotkey(self.hwnd, id) {
                tracing::warn!(id, error = %e, "UnregisterHotKey failed during OverlayWindow::drop");
            }
        }
        // `DestroyWindow` fires WM_NCDESTROY, which reclaims the
        // `Box<OverlayWndState>` via `win32_ffi::take_userdata`.
        if let Err(e) = win32_ffi::destroy_window(self.hwnd) {
            tracing::warn!(error = %e, "DestroyWindow failed during OverlayWindow::drop");
        }
    }
}

#[cfg(test)]
mod tests {
    //! `OverlayWindow::new` needs a real HWND, so these tests only pin the
    //! `OVERLAY_EX_STYLE` bit pattern.

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

    /// `OVERLAY_EX_STYLE` holds exactly the six expected WS_EX_* flags and no
    /// extras (e.g. a stray WS_EX_APPWINDOW would show it in the taskbar).
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
