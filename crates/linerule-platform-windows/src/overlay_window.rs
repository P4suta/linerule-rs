//! Lifecycle of the transparent, click-through, topmost overlay HWND.
//!
//! Same HWND serves as renderer host and `RegisterHotKey`/`WM_HOTKEY` target.

#![forbid(unsafe_code)]

use linerule_core::{
    AnimConfig, Command, HotkeyBindings, HudConfig, Logical, NotificationClass, ScreenRect, State,
    TapStepConfig, TickWorld, chord_to_win32,
};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{
    WINDOW_EX_STYLE, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_NOREDIRECTIONBITMAP, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};
use windows::core::w;

use crate::error::{PlatformError, Result};
use crate::overlay_state::OverlayWndState;
use crate::win32_ffi::hotkey as hotkey_ffi;
use crate::{ex_style_snapshot, win32_ffi, window_class};

/// Combined ex-style: layered + no-redirection (per-pixel alpha), transparent
/// (click-through), no-activate, tool-window, topmost.
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
}

// SAFETY: HWND is thread-affine, so no Send/Sync is implemented.

impl OverlayWindow {
    /// Create the HWND covering `monitor` with an explicit caller-owned
    /// `TickWorld`.
    ///
    /// # Errors
    /// If `RegisterClassExW` / `CreateWindowExW` / `GetModuleHandleW` fail.
    pub fn new_with_initial_world(
        monitor: ScreenRect<Logical>,
        hud_config: HudConfig,
        anim_config: AnimConfig,
        initial_world: TickWorld,
    ) -> Result<Self> {
        let _atom = window_class::ensure_registered()?;

        let state = Box::new(OverlayWndState::new_with_initial_world(
            tracing::info_span!("overlay_window", class = "linerule-rs-overlay"),
            monitor,
            hud_config,
            anim_config,
            initial_world,
        ));
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
            state,
        );

        match create_result {
            Ok(hwnd) => {
                ex_style_snapshot::capture(hwnd, "after CreateWindowExW");
                // Explicit ShowWindow is needed for the window to become visible;
                // SW_SHOWNOACTIVATE + WS_EX_NOACTIVATE both prevent focus steal.
                win32_ffi::show_window_noactivate(hwnd);

                win32_ffi::with_userdata(hwnd, |state| state.set_hwnd(hwnd)).ok_or(
                    PlatformError::NullHandle {
                        operation: "GWLP_USERDATA after CreateWindowExW",
                    },
                )??;
                Ok(Self { hwnd })
            },
            Err(e) => Err(e),
        }
    }

    /// The inner HWND.
    #[must_use]
    pub fn hwnd(&self) -> HWND {
        self.hwnd
    }

    fn with_state<R>(
        &self,
        operation: &'static str,
        f: impl for<'state> FnOnce(&'state OverlayWndState) -> R,
    ) -> Result<R> {
        win32_ffi::with_userdata(self.hwnd, f).ok_or(PlatformError::NullHandle { operation })
    }

    /// Attach the WinRT composition visual tree and install the overlay / HUD
    /// renderers into `OverlayWndState`.
    ///
    /// # Errors
    /// If D3D11 / DXGI / D2D / DWrite / WinRT composition init fails.
    pub fn attach_compositor(&mut self) -> Result<()> {
        let hud_config = self.with_state("attach_compositor", |state| *state.hud_config())?;
        let (overlay, hud) = crate::renderer_backend::build_backends(
            self.hwnd,
            &hud_config,
            crate::win32_ffi::graphics::GraphicsBackend::Auto,
        )?;
        ex_style_snapshot::capture(self.hwnd, "after attach_compositor");
        self.with_state("attach_compositor", |state| {
            state.install_renderer(overlay);
            state.install_hud_renderer(hud);
        })?;
        tracing::info!(backend = "winrt", "composition backend attached");
        Ok(())
    }

    /// Keep the window/controller alive with drawing disabled until the user
    /// next changes the ruler from Off to visible.
    pub fn disable_rendering_until_display_retry(&self, message: String) -> Result<()> {
        self.with_state("disable_rendering_until_display_retry", |state| {
            state.disable_renderers();
            state.device_lost_count().set(u8::MAX);
            state.push_notification(NotificationClass::Error, message, 10_000);
        })
    }

    /// Validate and register the entire binding set. Any registration failure
    /// unregisters every chord from this attempt before returning.
    ///
    /// # Errors
    /// A validation or OS registration error after rollback.
    pub fn register_hotkeys(
        &self,
        hotkeys: &HotkeyBindings,
        tap_step: TapStepConfig,
    ) -> Result<()> {
        hotkeys.validate()?;
        let old = self.with_state("register_hotkeys snapshot", OverlayWndState::hotkeys)?;
        let old_ids = self.with_state(
            "register_hotkeys registered IDs",
            OverlayWndState::registered_hotkey_ids,
        )?;
        for id in &old_ids {
            if let Err(error) = hotkey_ffi::unregister_hotkey(self.hwnd, *id) {
                tracing::warn!(id, %error, "old hotkey was already unavailable during transaction");
            }
        }

        let mappings = match self.register_set(hotkeys, tap_step) {
            Ok(mappings) => mappings,
            Err(error) => {
                if !old_ids.is_empty()
                    && let Err(rollback) = self.register_set(&old, tap_step)
                {
                    return Err(PlatformError::HotkeyRollback {
                        original: error.to_string(),
                        rollback: rollback.to_string(),
                    });
                }
                return Err(error);
            },
        };
        self.with_state("register_hotkeys commit", |state| {
            state.replace_hotkeys(hotkeys.clone(), mappings);
        })
    }

    fn register_set(
        &self,
        hotkeys: &HotkeyBindings,
        tap_step: TapStepConfig,
    ) -> Result<Vec<(i32, linerule_core::OverlayAction)>> {
        let parsed = hotkeys.validate()?;
        let mut registered = Vec::with_capacity(parsed.len());
        let mut mappings = Vec::with_capacity(parsed.len());
        for (index, (command, chord)) in parsed.into_iter().enumerate() {
            let id = i32::try_from(index + 1).unwrap_or(i32::MAX);
            let action = command.action(tap_step);
            let repeatable = matches!(
                command,
                Command::Thicker | Command::Thinner | Command::MoreOpaque | Command::LessOpaque
            );
            let (modifiers, key) = chord_to_win32(chord);
            if let Err(error) =
                hotkey_ffi::register_hotkey(self.hwnd, id, modifiers, key, repeatable)
            {
                for registered_id in registered {
                    if let Err(rollback_error) =
                        hotkey_ffi::unregister_hotkey(self.hwnd, registered_id)
                    {
                        tracing::error!(
                            id = registered_id,
                            error = %rollback_error,
                            "hotkey rollback failed"
                        );
                    }
                }
                return Err(PlatformError::HotkeyRegistration {
                    command,
                    source: Box::new(error),
                });
            }
            tracing::info!(spec = %chord, ?command, id, "hotkey registered");
            registered.push(id);
            mappings.push((id, action));
        }
        Ok(mappings)
    }

    pub fn push_notification(
        &self,
        class: NotificationClass,
        message: String,
        lifetime_ms: i64,
    ) -> Result<()> {
        self.with_state("push_notification", |state| {
            state.push_notification(class, message, lifetime_ms);
            state.request_tick(self.hwnd);
        })
    }

    pub fn state_snapshot(&self) -> Result<State> {
        self.with_state("state_snapshot", |state| state.tick_world_snapshot().state)
    }

    pub fn telemetry_snapshot(&self) -> Result<(linerule_core::HudTelemetry, usize)> {
        self.with_state("telemetry_snapshot", |state| {
            let tracker = state.frame_timing().borrow();
            (tracker.snapshot(), tracker.sample_count())
        })
    }

    pub fn install_render_clock(
        &self,
        control: crate::render_clock::RenderClockControl,
    ) -> Result<()> {
        self.with_state("install_render_clock", |state| {
            state.install_render_clock(control)
        })?
    }

    pub fn enqueue_action(&self, action: linerule_core::OverlayAction) -> Result<()> {
        self.with_state("enqueue_action", |state| {
            state
                .hotkey_sender()
                .send(action)
                .map_err(|_| PlatformError::Invariant {
                    operation: "OverlayWindow action receiver",
                })?;
            state.request_tick(self.hwnd);
            Ok(())
        })?
    }
}

impl Drop for OverlayWindow {
    fn drop(&mut self) {
        // Unregister hotkeys while the HWND is still alive.
        let ids = win32_ffi::with_userdata(self.hwnd, OverlayWndState::registered_hotkey_ids)
            .unwrap_or_default();
        for id in ids {
            if let Err(e) = hotkey_ffi::unregister_hotkey(self.hwnd, id) {
                tracing::warn!(id, error = %e, "UnregisterHotKey failed during OverlayWindow::drop");
            }
        }
        // `DestroyWindow` fires WM_NCDESTROY, whose FFI callback clears the
        // userdata slot and reclaims the `Box<OverlayWndState>` in place.
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
