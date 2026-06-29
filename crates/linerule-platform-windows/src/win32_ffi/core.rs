//! FFI boundary: thin safe wrappers over the windows crate's `unsafe` Win32/COM
//! APIs. Other modules call only the safe functions here.

#![allow(
    unsafe_code,
    reason = "FFI boundary; windows crate Win32/COM APIs are all unsafe fn."
)]

use core::ptr::NonNull;
use std::panic::{AssertUnwindSafe, catch_unwind};

use windows::Win32::Foundation::{HINSTANCE, HMODULE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::{HMONITOR, MONITORINFO, MonitorFromPoint};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CREATESTRUCTW, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GWLP_USERDATA,
    GetMessageW, GetSystemMetrics, GetWindowLongPtrW, MSG, PostQuitMessage, RegisterClassExW,
    SM_CXSCREEN, SM_CYSCREEN, SW_SHOWNOACTIVATE, SetWindowLongPtrW, ShowWindow, TranslateMessage,
    WINDOW_EX_STYLE, WINDOW_STYLE, WM_NCCREATE, WNDCLASSEXW, WNDPROC,
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

/// Safe wrapper over `CreateWindowExW`. `create_param` is the
/// `*mut OverlayWndState` (`Box::into_raw`) delivered to the WndProc via WM_NCCREATE.
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
    create_param: *mut OverlayWndState,
) -> Result<HWND> {
    let h_instance = module_handle()?;
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
            Some(create_param.cast()),
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

/// Stores the WM_NCCREATE `*mut OverlayWndState` (`Box::into_raw`) in `GWLP_USERDATA`.
pub fn set_userdata(hwnd: HWND, ptr: *mut OverlayWndState) {
    // SAFETY: hwnd valid; ptr is a caller-owned Box::into_raw result or null.
    unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, ptr as isize) };
}

/// Reads the `*mut OverlayWndState` from `GWLP_USERDATA` as `NonNull`.
/// `None` before WM_NCCREATE sets it.
pub fn get_userdata(hwnd: HWND) -> Option<NonNull<OverlayWndState>> {
    // SAFETY: plain read, made safe by the null check.
    let raw = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut OverlayWndState;
    NonNull::new(raw)
}

/// Clears `GWLP_USERDATA` and reclaims the stored Box. Call once in WM_NCDESTROY.
pub fn take_userdata(hwnd: HWND) -> Option<Box<OverlayWndState>> {
    // SAFETY: writes 0 and returns the prior value (atomic swap).
    let raw = unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0) } as *mut OverlayWndState;
    if raw.is_null() {
        return None;
    }
    // SAFETY: reclaims the WM_NCCREATE Box::into_raw value once.
    Some(unsafe { Box::from_raw(raw) })
}

/// Drops a Box when CreateWindowExW failed before WM_NCCREATE, so the pointer
/// never reached `GWLP_USERDATA`; the caller passes it directly.
#[allow(
    clippy::not_unsafe_ptr_arg_deref,
    reason = "kept a safe fn for cleanup-path ergonomics; the Box::from_raw contract is documented \
              in the SAFETY comment below and upheld by the single CreateWindowExW failure caller"
)]
pub fn drop_userdata_raw(ptr: *mut OverlayWndState) {
    if ptr.is_null() {
        return;
    }
    // SAFETY: caller passes a Box::into_raw result and guarantees no double free.
    drop(unsafe { Box::from_raw(ptr) });
}

/// Converts `NonNull<OverlayWndState>` to `&OverlayWndState`, valid only for one
/// WndProc dispatch.
pub fn state_ref<'a>(ptr: NonNull<OverlayWndState>) -> &'a OverlayWndState {
    // SAFETY: stable address from WM_NCCREATE; WndProc runs single-threaded and the
    // box lives until dispatch returns.
    unsafe { ptr.as_ref() }
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

/// Safe wrapper over `GetSystemMetrics(SM_CXSCREEN)`.
pub fn screen_width() -> i32 {
    // SAFETY: GetSystemMetrics is a read-only call with no argument validation.
    unsafe { GetSystemMetrics(SM_CXSCREEN) }
}

/// Safe wrapper over `GetSystemMetrics(SM_CYSCREEN)`.
pub fn screen_height() -> i32 {
    // SAFETY: as above.
    unsafe { GetSystemMetrics(SM_CYSCREEN) }
}

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
/// whose `lpCreateParams` is the `Box::into_raw(OverlayWndState)` pointer.
pub unsafe extern "system" fn overlay_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_NCCREATE {
        // SAFETY: per Win32, the WM_NCCREATE lparam is a CREATESTRUCTW*.
        let cs = unsafe { &*(lparam.0 as *const CREATESTRUCTW) };
        let raw = cs.lpCreateParams.cast::<OverlayWndState>();
        set_userdata(hwnd, raw);
        return def_window_proc(hwnd, msg, wparam, lparam);
    }

    let result = catch_unwind(AssertUnwindSafe(|| {
        crate::wndproc::dispatch(hwnd, msg, wparam, lparam)
    }));

    match result {
        Ok(Some(lresult)) => lresult,
        Ok(None) => def_window_proc(hwnd, msg, wparam, lparam),
        Err(_panic) => {
            // Swallow panic; keep the process alive.
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

/// Safe wrapper over `MonitorFromPoint(0, 0, MONITOR_DEFAULTTOPRIMARY)`.
pub fn primary_monitor() -> HMONITOR {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::Graphics::Gdi::MONITOR_DEFAULTTOPRIMARY;
    // SAFETY: MonitorFromPoint is a read-only API taking a point and flag.
    unsafe { MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY) }
}

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

// ---- DPI awareness ---------------------------------------------------------

/// Sets per-monitor-aware-V2 DPI awareness (call once at startup). V2 lets the
/// overlay HWND receive `WM_DPICHANGED` (Windows 10 1703+). Failure is non-fatal.
///
/// # Errors
/// When `SetProcessDpiAwarenessContext` returns `FALSE`.
pub fn set_dpi_aware() -> Result<()> {
    use windows::Win32::UI::HiDpi::{
        DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
    };
    // SAFETY: windows-rs constant arg; API is idempotent.
    unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) }.map_err(
        |e| PlatformError::BadHr {
            operation: "SetProcessDpiAwarenessContext",
            hr: e.code().0,
        },
    )
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
