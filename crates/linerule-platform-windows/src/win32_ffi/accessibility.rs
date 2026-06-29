//! Accessibility hook (`SetWinEventHook`) watching foreground changes to
//! re-assert the overlay's topmost z-order.
//!
//! `HWND` is `!Send`, so it crosses to the hook thread as an isize via
//! `AtomicIsize`. The callback only `PostMessageW(WM_APP_REASSERT_TOPMOST)`;
//! `SetWindowPos` runs on the UI thread.

#![allow(
    unsafe_code,
    reason = "FFI boundary; SetWinEventHook/UnhookWinEvent/SetWindowPos/PostMessageW are unsafe fn."
)]

use core::ffi::c_void;
use core::sync::atomic::{AtomicIsize, Ordering};
use std::panic::{AssertUnwindSafe, catch_unwind};

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent};
use windows::Win32::UI::WindowsAndMessaging::{
    EVENT_SYSTEM_FOREGROUND, HWND_TOPMOST, PostMessageW, SET_WINDOW_POS_FLAGS, SWP_NOACTIVATE,
    SWP_NOMOVE, SWP_NOSIZE, SetWindowPos, WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS,
    WS_EX_TOPMOST,
};

use crate::error::{PlatformError, Result};
use crate::messages::WM_APP_REASSERT_TOPMOST;

/// Overlay HWND shared with the hook thread as an isize (`HWND` is `!Send`).
/// 0 = uninstalled / no target.
static TARGET_HWND: AtomicIsize = AtomicIsize::new(0);

/// Registers foreground-change notifications for overlay `target`. The returned
/// `HWINEVENTHOOK` must be released via `unhook_win_event`.
pub fn set_foreground_hook(target: HWND) -> Result<HWINEVENTHOOK> {
    TARGET_HWND.store(target.0 as isize, Ordering::SeqCst);
    // SAFETY: args in valid SDK range; callback is a static fn pointer. 0/0
    // watches all processes and threads.
    let hook = unsafe {
        SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            None,
            Some(on_foreground_event),
            0,
            0,
            WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
        )
    };
    if hook.0.is_null() {
        TARGET_HWND.store(0, Ordering::SeqCst);
        return Err(PlatformError::NullHandle {
            operation: "SetWinEventHook",
        });
    }
    Ok(hook)
}

/// Removes a registered hook.
pub fn unhook_win_event(hook: HWINEVENTHOOK) -> Result<()> {
    // SAFETY: hook is from set_foreground_hook; the caller excludes null.
    let ok = unsafe { UnhookWinEvent(hook) };
    TARGET_HWND.store(0, Ordering::SeqCst);
    if !ok.as_bool() {
        return Err(PlatformError::BadHr {
            operation: "UnhookWinEvent",
            hr: 0,
        });
    }
    Ok(())
}

/// Whether `GWL_EXSTYLE` has the `WS_EX_TOPMOST` bit (set while the window sits
/// in the topmost band).
#[must_use]
pub fn is_topmost(hwnd: HWND) -> bool {
    let bits = u32::try_from(crate::win32_ffi::get_ex_style(hwnd)).unwrap_or(u32::MAX);
    bits & WS_EX_TOPMOST.0 != 0
}

/// Restores the overlay's topmost z-order without stealing focus or moving it.
///
/// No-op when `WS_EX_TOPMOST` is already set: an unconditional `SetWindowPos`
/// on every foreground change causes a brief flicker from DWM z-order churn.
pub fn reassert_topmost(hwnd: HWND) -> Result<()> {
    if is_topmost(hwnd) {
        return Ok(());
    }
    let flags: SET_WINDOW_POS_FLAGS = SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE;
    // SAFETY: hwnd is a valid OverlayWindow HWND. HWND_TOPMOST is a WinAPI constant.
    unsafe { SetWindowPos(hwnd, Some(HWND_TOPMOST), 0, 0, 0, 0, flags) }.map_err(|e| {
        PlatformError::BadHr {
            operation: "SetWindowPos(HWND_TOPMOST)",
            hr: e.code().0,
        }
    })
}

/// `SetWinEventHook` callback on the OS hook thread; only `PostMessageW`s the
/// UI thread.
extern "system" fn on_foreground_event(
    _hook: HWINEVENTHOOK,
    _event: u32,
    _hwnd: HWND,
    _object_id: i32,
    _child_id: i32,
    _thread_id: u32,
    _time: u32,
) {
    // catch_unwind: a panic must not unwind across the FFI callback boundary.
    let _ = catch_unwind(AssertUnwindSafe(|| {
        let raw = TARGET_HWND.load(Ordering::SeqCst);
        if raw == 0 {
            return;
        }
        let target = HWND(raw as *mut c_void);
        // SAFETY: PostMessageW is thread-safe; target is the live overlay HWND.
        let _ =
            unsafe { PostMessageW(Some(target), WM_APP_REASSERT_TOPMOST, WPARAM(0), LPARAM(0)) };
    }));
}

// No unit tests: the real hook/SetWindowPos calls need native Windows and
// mutate the global TARGET_HWND.
