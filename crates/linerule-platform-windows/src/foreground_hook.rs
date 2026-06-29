//! RAII guard that re-asserts the overlay's topmost z-order on foreground
//! changes: a `WS_EX_TOPMOST` overlay can still drop behind an app that comes
//! to the foreground. Posts `WM_APP_REASSERT_TOPMOST` to the UI thread.
//! `WINEVENT_SKIPOWNPROCESS` suppresses own-process events, so the callback
//! needs no HWND comparison.

#![forbid(unsafe_code)]

#[cfg(target_os = "windows")]
use windows::Win32::Foundation::HWND;
#[cfg(target_os = "windows")]
use windows::Win32::UI::Accessibility::HWINEVENTHOOK;

#[cfg(target_os = "windows")]
use crate::error::Result;
#[cfg(target_os = "windows")]
use crate::win32_ffi::accessibility;

/// RAII guard for foreground-change notifications; `Drop` calls `UnhookWinEvent`.
///
/// `target` reaches the callback via an AtomicIsize global, so only the
/// last-installed HWND gets events. The overlay is a singleton, so this is fine.
#[cfg(target_os = "windows")]
pub struct ForegroundHook {
    hook: HWINEVENTHOOK,
}

#[cfg(target_os = "windows")]
impl ForegroundHook {
    /// Install the hook so the callback posts `WM_APP_REASSERT_TOPMOST` to `target`.
    ///
    /// # Errors
    /// `SetWinEventHook` returned null (`PlatformError::NullHandle`).
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
