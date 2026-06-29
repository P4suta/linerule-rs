//! RAII guard that re-asserts the overlay's topmost z-order on foreground
//! changes.
//!
//! A `WS_EX_TOPMOST` overlay can still drop behind another app that comes to
//! the foreground (Alt+Tab, `SetForegroundWindow`). This hook watches
//! `SetWinEventHook(EVENT_SYSTEM_FOREGROUND)` and posts `WM_APP_REASSERT_TOPMOST`
//! to the UI thread, which runs the actual `SetWindowPos(HWND_TOPMOST)`.
//!
//! `WINEVENT_SKIPOWNPROCESS` suppresses own-process events, so no HWND
//! comparison is needed in the callback.

#![forbid(unsafe_code)]

#[cfg(target_os = "windows")]
use windows::Win32::Foundation::HWND;
#[cfg(target_os = "windows")]
use windows::Win32::UI::Accessibility::HWINEVENTHOOK;

#[cfg(target_os = "windows")]
use crate::error::Result;
#[cfg(target_os = "windows")]
use crate::win32_ffi::accessibility;

/// RAII guard for foreground-change notifications; `Drop` always calls
/// `UnhookWinEvent`.
///
/// The `target` HWND is shared with the callback via an AtomicIsize global, so
/// with multiple installs only the last-installed HWND receives events. The
/// overlay is a singleton, so this is fine.
#[cfg(target_os = "windows")]
pub struct ForegroundHook {
    hook: HWINEVENTHOOK,
}

#[cfg(target_os = "windows")]
impl ForegroundHook {
    /// Install `SetWinEventHook` so the callback posts `WM_APP_REASSERT_TOPMOST`
    /// to `target`.
    ///
    /// # Errors
    /// When `SetWinEventHook` returns null (`PlatformError::NullHandle`).
    pub fn install(target: HWND) -> Result<Self> {
        let hook = accessibility::set_foreground_hook(target)?;
        tracing::info!("ForegroundHook installed for topmost re-assertion");
        Ok(Self { hook })
    }
}

#[cfg(target_os = "windows")]
impl Drop for ForegroundHook {
    fn drop(&mut self) {
        if !self.hook.0.is_null()
            && let Err(e) = accessibility::unhook_win_event(self.hook)
        {
            tracing::warn!(error = %e, "UnhookWinEvent failed during ForegroundHook::drop");
        }
    }
}
