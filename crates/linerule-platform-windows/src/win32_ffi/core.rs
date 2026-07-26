//! FFI boundary: thin safe wrappers over the windows crate's `unsafe` Win32/COM
//! APIs. Other modules call only the safe functions here.

#![allow(
    unsafe_code,
    reason = "FFI boundary; windows crate Win32/COM APIs are all unsafe fn."
)]

use std::panic::{AssertUnwindSafe, catch_unwind};

use windows::Win32::Foundation::{HINSTANCE, HMODULE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::{HMONITOR, MONITORINFO, MonitorFromPoint};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CREATESTRUCTW, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GWLP_USERDATA,
    GetMessageW, GetSystemMetrics, GetWindowLongPtrW, MSG, PostQuitMessage, RegisterClassExW,
    SW_SHOWNOACTIVATE, SetWindowLongPtrW, ShowWindow, TranslateMessage, WINDOW_EX_STYLE,
    WINDOW_STYLE, WM_NCCREATE, WM_NCDESTROY, WNDCLASSEXW, WNDPROC,
};
use windows::core::PCWSTR;

use crate::error::{PlatformError, Result, decode_last_error};
use crate::overlay_state::OverlayWndState;

// ---- module handle ---------------------------------------------------------

/// Module handle of the current process, for the `hInstance` arg of
/// `CreateWindowExW` / `RegisterClassExW`.
pub fn module_handle() -> Result<HINSTANCE> {
    // SAFETY: standard call; PCWSTR::null() returns the current process HMODULE.
    let h: HMODULE =
        unsafe { GetModuleHandleW(PCWSTR::null()) }.map_err(|e| PlatformError::BadHr {
            operation: "GetModuleHandleW",
            hr: e.code().0,
        })?;
    Ok(HINSTANCE(h.0))
}

// ---- class registration ----------------------------------------------------

/// Safe wrapper over `RegisterClassExW`. Returns the class atom on success.
pub fn register_class(name: PCWSTR, wnd_proc: WNDPROC) -> Result<u16> {
    let h_instance = module_handle()?;
    let wc = WNDCLASSEXW {
        cbSize: u32::try_from(core::mem::size_of::<WNDCLASSEXW>()).unwrap_or(u32::MAX),
        lpfnWndProc: wnd_proc,
        hInstance: h_instance,
        lpszClassName: name,
        ..Default::default()
    };

    // SAFETY: `wc` fully initialized, pointer args valid. Returns 0 on failure.
    let atom = unsafe { RegisterClassExW(&wc) };
    if atom == 0 {
        return Err(last_error("RegisterClassExW"));
    }
    Ok(atom)
}

// ---- window lifecycle ------------------------------------------------------

/// Safe wrapper over `CreateWindowExW`. A stack-owned `CreatePayload` hands the
/// boxed state to the synchronous `WM_NCCREATE` callback.
#[allow(
    clippy::too_many_arguments,
    reason = "mirrors the Win32 API argument shape; flat is clearer for callers"
)]
pub fn create_window(
    ex_style: WINDOW_EX_STYLE,
    class_name: PCWSTR,
    title: PCWSTR,
    style: WINDOW_STYLE,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    state: Box<OverlayWndState>,
) -> Result<HWND> {
    let h_instance = module_handle()?;
    let mut payload = CreatePayload { state: Some(state) };
    // SAFETY: all args in valid range. Returns null HWND on failure.
    let hwnd = unsafe {
        CreateWindowExW(
            ex_style,
            class_name,
            title,
            style,
            x,
            y,
            width,
            height,
            None,
            None,
            Some(h_instance),
            Some((&raw mut payload).cast()),
        )
    }
    .map_err(|e| PlatformError::BadHr {
        operation: "CreateWindowExW",
        hr: e.code().0,
    })?;
    if hwnd.0.is_null() {
        return Err(PlatformError::NullHandle {
            operation: "CreateWindowExW",
        });
    }
    Ok(hwnd)
}

/// Stack-owned handoff used only during the synchronous `CreateWindowExW`
/// call. `WM_NCCREATE` takes the box; if that message is never delivered the
/// payload drops it normally.
struct CreatePayload {
    state: Option<Box<OverlayWndState>>,
}

/// Safe wrapper over `DestroyWindow`. May fail (called from Drop) without aborting.
pub fn destroy_window(hwnd: HWND) -> Result<()> {
    // SAFETY: hwnd is a valid HWND owned by OverlayWindow.
    unsafe { DestroyWindow(hwnd) }.map_err(|e| PlatformError::BadHr {
        operation: "DestroyWindow",
        hr: e.code().0,
    })
}

/// Repositions the overlay HWND via `SetWindowPos` with `SWP_NOACTIVATE`
/// (no focus steal) | `SWP_NOZORDER` (preserve WS_EX_TOPMOST). For WM_DPICHANGED.
///
/// # Errors
/// When `SetWindowPos` fails.
pub fn set_window_pos_rect(hwnd: HWND, x: i32, y: i32, width: i32, height: i32) -> Result<()> {
    use windows::Win32::UI::WindowsAndMessaging::{SWP_NOACTIVATE, SWP_NOZORDER, SetWindowPos};
    // SAFETY: valid overlay-owned hwnd; insertafter = None makes no z-order change.
    unsafe {
        SetWindowPos(
            hwnd,
            None,
            x,
            y,
            width,
            height,
            SWP_NOACTIVATE | SWP_NOZORDER,
        )
    }
    .map_err(|e| PlatformError::BadHr {
        operation: "SetWindowPos",
        hr: e.code().0,
    })
}

/// Reads a `WM_DPICHANGED` `lparam` as the OS-recommended new window `RECT`.
/// Deref kept here since `wndproc::dispatch` is `#![forbid(unsafe_code)]`.
///
/// # Safety
/// Call only when `msg == WM_DPICHANGED`; otherwise `lparam` is not a `RECT*` (UB).
pub fn rect_from_wm_dpichanged_lparam(lparam: LPARAM) -> windows::Win32::Foundation::RECT {
    // SAFETY: WM_DPICHANGED lparam is an OS-provided valid `RECT*`.
    unsafe { *(lparam.0 as *const windows::Win32::Foundation::RECT) }
}

/// Safe wrapper over `ShowWindow(hwnd, SW_SHOWNOACTIVATE)`.
///
/// A `WS_EX_LAYERED + WS_EX_NOREDIRECTIONBITMAP + DComp` overlay HWND may not
/// show without an explicit `ShowWindow`; `SW_SHOWNOACTIVATE` avoids focus steal.
/// The BOOL return only reports prior visibility, not failure, so it is discarded.
#[allow(
    clippy::disallowed_methods,
    reason = "the only place ShowWindow is allowed; callers deny it via clippy::disallowed_methods"
)]
pub fn show_window_noactivate(hwnd: HWND) {
    // SAFETY: valid overlay-owned hwnd; this API returns no failure HRESULT.
    let _ = unsafe { ShowWindow(hwnd, SW_SHOWNOACTIVATE) };
}

// ---- GWLP_USERDATA (instance state) ----------------------------------------

/// Borrow instance state only for the duration of `f`. The higher-ranked
/// callback prevents a reference from escaping the HWND-owned lifetime.
pub fn with_userdata<R>(
    hwnd: HWND,
    f: impl for<'state> FnOnce(&'state OverlayWndState) -> R,
) -> Option<R> {
    // SAFETY: plain read; the null check below handles pre-WM_NCCREATE calls.
    let raw = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut OverlayWndState;
    if raw.is_null() {
        return None;
    }
    // SAFETY: the pointer is installed from a Box during WM_NCCREATE, remains
    // owned by this HWND, and is cleared only during WM_NCDESTROY on the same
    // UI thread. The callback cannot return a borrow tied to this reference.
    Some(f(unsafe { &*raw }))
}

// ---- message dispatch ------------------------------------------------------

/// Safe wrapper over `DefWindowProcW`.
pub fn def_window_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // SAFETY: standard Win32 message forwarding; args come straight from WndProc.
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

/// Safe wrapper over `PostQuitMessage(exit_code)`.
pub fn post_quit(exit_code: i32) {
    // SAFETY: plain POST; cannot fail.
    unsafe { PostQuitMessage(exit_code) };
}

/// Pumps one message: `Some(true)` processed, `Some(false)` WM_QUIT,
/// `None` GetMessageW returned -1 (API error).
pub fn pump_one() -> Option<bool> {
    let mut msg = MSG::default();
    // SAFETY: msg is a zero-init out param.
    let r = unsafe { GetMessageW(&mut msg, None, 0, 0) };
    match r.0 {
        0 => Some(false),
        -1 => None,
        _ => {
            // SAFETY: msg was initialized by GetMessageW on success.
            unsafe {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            Some(true)
        },
    }
}

// ---- cursor position -------------------------------------------------------

/// Safe wrapper over `GetCursorPos`.
pub fn cursor_pos() -> Result<linerule_core::Point<linerule_core::Logical>> {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;
    let mut pt = POINT::default();
    // SAFETY: pt is a zero-init out param.
    unsafe { GetCursorPos(&mut pt) }.map_err(|e| PlatformError::BadHr {
        operation: "GetCursorPos",
        hr: e.code().0,
    })?;
    Ok(linerule_core::Point::new(pt.x, pt.y))
}

// ---- monitor info ----------------------------------------------------------

/// Virtual-screen bounds `(left, top, width, height)` covering all monitors.
pub fn virtual_screen_metrics() -> (i32, i32, i32, i32) {
    use windows::Win32::UI::WindowsAndMessaging::{
        SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
    };
    // SAFETY: four read-only API calls.
    unsafe {
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
        )
    }
}

// ---- WndProc entry point ---------------------------------------------------

/// The linerule overlay WndProc. On WM_NCCREATE stores instance state in
/// `GWLP_USERDATA`; otherwise delegates to `crate::wndproc::dispatch`, absorbing
/// any panic via `catch_unwind` and falling back to `DefWindowProcW`.
///
/// # Safety
/// Install only as a window class `lpfnWndProc`: `hwnd` must be a valid window of
/// that class and, for `WM_NCCREATE`, `lparam` must point to a `CREATESTRUCTW`
/// whose `lpCreateParams` points to the live `CreatePayload` supplied by
/// [`create_window`].
pub unsafe extern "system" fn overlay_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_NCCREATE {
        // SAFETY: per Win32, the WM_NCCREATE lparam is a CREATESTRUCTW*.
        let cs = unsafe { &*(lparam.0 as *const CREATESTRUCTW) };
        let payload = cs.lpCreateParams.cast::<CreatePayload>();
        // SAFETY: `create_window` passes a live stack `CreatePayload` and
        // CreateWindowExW delivers WM_NCCREATE synchronously before returning.
        let state = unsafe { &mut *payload }
            .state
            .take()
            .ok_or(())
            .map(Box::into_raw);
        let Ok(raw) = state else {
            return LRESULT(0);
        };
        // SAFETY: `raw` is the unique Box handoff taken from CreatePayload;
        // this callback installs it exactly once on its new HWND owner.
        unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, raw as isize) };
        return def_window_proc(hwnd, msg, wparam, lparam);
    }

    if msg == WM_NCDESTROY {
        // Let Windows finish default non-client teardown while the state is
        // still live, then atomically detach and reclaim the Box in this FFI
        // callback. No safe raw-pointer deallocation API is exposed.
        let result = def_window_proc(hwnd, msg, wparam, lparam);
        // SAFETY: WM_NCDESTROY is delivered once for this HWND. The slot holds
        // only the Box::into_raw value installed during WM_NCCREATE.
        let raw = unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0) } as *mut OverlayWndState;
        if !raw.is_null() {
            let cleanup = catch_unwind(AssertUnwindSafe(|| {
                // SAFETY: the atomic slot clear above gives this callback sole
                // ownership of the original Box allocation.
                drop(unsafe { Box::from_raw(raw) });
            }));
            if cleanup.is_err() {
                tracing::error!("overlay state Drop panicked during WM_NCDESTROY");
            }
        }
        return result;
    }

    let result = catch_unwind(AssertUnwindSafe(|| {
        crate::wndproc::dispatch(hwnd, msg, wparam, lparam)
    }));

    match result {
        Ok(Some(lresult)) => lresult,
        Ok(None) => def_window_proc(hwnd, msg, wparam, lparam),
        Err(payload) => {
            let message = payload
                .downcast_ref::<&str>()
                .copied()
                .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
                .unwrap_or("<non-string panic payload>");
            tracing::error!(message, "overlay WndProc caught a panic");
            // Never unwind across the FFI callback boundary.
            def_window_proc(hwnd, msg, wparam, lparam)
        },
    }
}

// ---- ex-style snapshot helpers ---------------------------------------------

/// Safe wrapper reading `GWL_EXSTYLE` (= -20) via `GetWindowLongPtrW`.
pub fn get_ex_style(hwnd: HWND) -> isize {
    use windows::Win32::UI::WindowsAndMessaging::GWL_EXSTYLE;
    // SAFETY: plain GWL_EXSTYLE read.
    unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) }
}

// ---- last-error helpers ----------------------------------------------------

/// Reads `GetLastError()` and builds a `PlatformError::LastError`.
pub fn last_error(operation: &'static str) -> PlatformError {
    use windows::Win32::Foundation::GetLastError;
    // SAFETY: GetLastError returns the last thread-local error with no side effects.
    let code = unsafe { GetLastError() }.0;
    PlatformError::LastError {
        operation,
        code,
        symbol: decode_last_error(code),
    }
}

// ---- monitor info ----------------------------------------------------------

/// Safe wrapper over `MonitorFromPoint(p, MONITOR_DEFAULTTONEAREST)`. Returns the
/// nearest monitor to any point (fine if the cursor is off all monitors).
pub fn monitor_from_point(x: i32, y: i32) -> HMONITOR {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::Graphics::Gdi::MONITOR_DEFAULTTONEAREST;
    // SAFETY: read-only API accepting any i32 coordinates.
    unsafe { MonitorFromPoint(POINT { x, y }, MONITOR_DEFAULTTONEAREST) }
}

/// Safe wrapper over `GetMonitorInfoW`.
pub fn get_monitor_info(hmonitor: HMONITOR) -> Result<MONITORINFO> {
    use windows::Win32::Graphics::Gdi::GetMonitorInfoW;
    let mut info = MONITORINFO {
        cbSize: u32::try_from(core::mem::size_of::<MONITORINFO>()).unwrap_or(u32::MAX),
        ..Default::default()
    };
    // SAFETY: info.cbSize is set correctly; hmonitor is expected valid.
    let ok = unsafe { GetMonitorInfoW(hmonitor, &mut info) };
    if !ok.as_bool() {
        return Err(last_error("GetMonitorInfoW"));
    }
    Ok(info)
}

// ---- display settings (refresh rate) ---------------------------------------

/// Reads the primary display's current `DEVMODEW` (HUD shows `dmDisplayFrequency`).
///
/// # Errors
/// When `EnumDisplaySettingsW` returns FALSE.
pub fn enum_display_settings_current() -> Result<windows::Win32::Graphics::Gdi::DEVMODEW> {
    use windows::Win32::Graphics::Gdi::{DEVMODEW, ENUM_CURRENT_SETTINGS, EnumDisplaySettingsW};
    let mut dm = DEVMODEW {
        dmSize: u16::try_from(core::mem::size_of::<DEVMODEW>()).unwrap_or(u16::MAX),
        ..Default::default()
    };
    // SAFETY: dm.dmSize is set correctly; device name = NULL (PCWSTR::null)
    // means the primary display per Win32. The out param is zero-init.
    let ok = unsafe { EnumDisplaySettingsW(PCWSTR::null(), ENUM_CURRENT_SETTINGS, &mut dm) };
    if !ok.as_bool() {
        return Err(last_error("EnumDisplaySettingsW"));
    }
    Ok(dm)
}
