//! Hidden controller window, notification-area icon, and single-instance guard.

#![allow(
    unsafe_code,
    reason = "FFI boundary for the controller HWND, Shell_NotifyIcon, menus, mutex, and timer"
)]

use std::cell::RefCell;
use std::os::windows::io::{FromRawHandle, OwnedHandle};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::OnceLock;
use std::sync::mpsc::{Receiver, Sender, channel};

use linerule_core::HotkeyBindings;
use windows::Win32::Foundation::{
    ERROR_ALREADY_EXISTS, GetLastError, HINSTANCE, HWND, LPARAM, LRESULT, POINT, WPARAM,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::Shell::{
    NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW, Shell_NotifyIconW,
};
#[cfg(test)]
use windows::Win32::UI::WindowsAndMessaging::IDI_APPLICATION;
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CREATESTRUCTW, CS_HREDRAW, CS_VREDRAW, CreatePopupMenu, CreateWindowExW,
    DefWindowProcW, DestroyMenu, DestroyWindow, GWLP_USERDATA, GetCursorPos, GetWindowLongPtrW,
    IDC_ARROW, KillTimer, LoadCursorW, LoadIconW, MENU_ITEM_FLAGS, MF_STRING, RegisterClassExW,
    SetForegroundWindow, SetTimer, SetWindowLongPtrW, TPM_LEFTALIGN, TPM_RETURNCMD,
    TPM_RIGHTBUTTON, TrackPopupMenu, WINDOW_EX_STYLE, WM_LBUTTONUP, WM_NCCREATE, WM_NCDESTROY,
    WM_RBUTTONUP, WM_TIMER, WNDCLASSEXW, WS_OVERLAPPED,
};
use windows::core::{PCWSTR, w};

use crate::error::{PlatformError, Result};
use crate::settings_host::SettingsHost;

const CONTROLLER_CLASS: PCWSTR = w!("linerule-controller");
const INSTANCE_NAME: PCWSTR = w!("Local\\P4suta.linerule.controller.v1");
#[cfg(not(test))]
const APP_ICON_RESOURCE: PCWSTR = PCWSTR(core::ptr::without_provenance(1));
const TRAY_CALLBACK: u32 = 0x8003;
const TRAY_ID: u32 = 1;
const MENU_TOGGLE: usize = 1;
const MENU_SETTINGS: usize = 2;
const MENU_EXIT: usize = 3;
const PREFERENCES_TIMER_ID: usize = 4;
const PREFERENCES_DEBOUNCE_MS: u32 = 500;
#[cfg(test)]
const TEST_EXIT_TIMER_ID: usize = 5;
#[cfg(test)]
const TEST_EXIT_DELAY_MS: u32 = 250;

static CONTROLLER_ATOM: OnceLock<u16> = OnceLock::new();

/// User requests emitted by the hidden controller and Fluent settings host.
#[derive(Debug)]
pub enum ShellCommand {
    /// Toggle ruler visibility.
    Toggle,
    /// Atomically replace all shortcuts.
    ApplyBindings(HotkeyBindings),
    /// Surface a recoverable settings-process failure without terminating.
    SettingsFailed(String),
    /// Flush the latest complete preferences snapshot.
    FlushPreferences,
    /// Exit the resident runtime.
    Exit,
}

/// RAII owner for the hidden controller, tray icon, and per-user mutex.
pub struct DesktopShell {
    hwnd: HWND,
    receiver: Receiver<ShellCommand>,
    _mutex: OwnedHandle,
    tray_added: bool,
}

impl DesktopShell {
    /// Create the single controller and notification-area icon.
    pub fn new(bindings: HotkeyBindings) -> Result<Self> {
        let mutex = create_instance_mutex()?;
        ensure_class()?;
        let (sender, receiver) = channel();
        let state = Box::new(ControllerState {
            sender,
            bindings: RefCell::new(bindings),
            settings: SettingsHost::discover(),
        });
        let hwnd = create_controller(state)?;
        if let Err(error) = add_tray(hwnd) {
            // SAFETY: hwnd was created by create_controller on this thread.
            if let Err(cleanup) = unsafe { DestroyWindow(hwnd) } {
                tracing::warn!(%cleanup, "controller cleanup after tray failure failed");
            }
            return Err(error);
        }
        Ok(Self {
            hwnd,
            receiver,
            _mutex: mutex,
            tray_added: true,
        })
    }

    /// Drain controller actions after one dispatched Win32 message.
    pub fn drain(&self) -> Vec<ShellCommand> {
        let mut commands = Vec::new();
        while let Ok(command) = self.receiver.try_recv() {
            commands.push(command);
        }
        commands
    }

    #[cfg(test)]
    pub(crate) fn request_exit_for_test(&self) -> Result<()> {
        set_controller_timer(self.hwnd, TEST_EXIT_TIMER_ID, TEST_EXIT_DELAY_MS)
    }

    #[cfg(test)]
    pub(crate) fn send_for_test(&self, command: ShellCommand) -> Result<()> {
        with_controller(self.hwnd, |state| state.sender.send(command))
            .ok_or(PlatformError::Invariant {
                operation: "DesktopShell::send_for_test controller state",
            })?
            .map_err(|_| PlatformError::Invariant {
                operation: "DesktopShell::send_for_test receiver",
            })
    }

    /// Open the Fluent shortcut editor, keeping the resident shell responsive.
    pub fn open_settings(&self) -> Result<()> {
        with_controller(self.hwnd, |state| open_settings(state, None)).ok_or(
            PlatformError::Invariant {
                operation: "DesktopShell::open_settings controller state",
            },
        )?
    }

    /// Replace the settings editor's source bindings after a successful
    /// platform transaction.
    pub fn set_bindings(&self, bindings: HotkeyBindings) -> Result<()> {
        with_controller(self.hwnd, |state| {
            *state.bindings.borrow_mut() = bindings;
        })
        .ok_or(PlatformError::Invariant {
            operation: "DesktopShell::set_bindings controller state",
        })
    }

    /// Reopen settings with a registration error and highlight its command.
    pub fn show_settings_error(&self, error: &PlatformError) -> Result<()> {
        with_controller(self.hwnd, |state| open_settings(state, Some(error))).ok_or(
            PlatformError::Invariant {
                operation: "DesktopShell::show_settings_error controller state",
            },
        )?
    }

    /// Reset the one-shot 500 ms preferences debounce timer.
    pub fn schedule_preferences_flush(&self) -> Result<()> {
        set_controller_timer(self.hwnd, PREFERENCES_TIMER_ID, PREFERENCES_DEBOUNCE_MS)
    }
}

impl Drop for DesktopShell {
    fn drop(&mut self) {
        if let Err(error) = kill_controller_timer(self.hwnd, PREFERENCES_TIMER_ID) {
            tracing::warn!(%error, "preferences debounce KillTimer failed");
        }
        if self.tray_added
            && let Err(error) = remove_tray(self.hwnd)
        {
            tracing::warn!(%error, "tray icon removal failed");
        }
        // SAFETY: hwnd is owned by this shell and destroyed on its UI thread.
        if let Err(error) = unsafe { DestroyWindow(self.hwnd) } {
            tracing::warn!(%error, "controller DestroyWindow failed");
        }
    }
}

fn set_controller_timer(hwnd: HWND, id: usize, delay_ms: u32) -> Result<()> {
    // SAFETY: the live controller HWND owns this numeric timer ID.
    let timer = unsafe { SetTimer(Some(hwnd), id, delay_ms, None) };
    if timer == 0 {
        Err(last_error("SetTimer(controller)"))
    } else {
        Ok(())
    }
}

fn kill_controller_timer(hwnd: HWND, id: usize) -> Result<()> {
    // SAFETY: cancelling an absent controller-owned timer is harmless.
    unsafe { KillTimer(Some(hwnd), id) }.map_err(hr("KillTimer(controller)"))
}

struct ControllerState {
    sender: Sender<ShellCommand>,
    bindings: RefCell<HotkeyBindings>,
    settings: SettingsHost,
}

struct ControllerCreatePayload {
    state: Option<Box<ControllerState>>,
}

fn create_instance_mutex() -> Result<OwnedHandle> {
    // SAFETY: null security attributes and a static NUL-terminated name.
    let mutex = unsafe { CreateMutexW(None, true, INSTANCE_NAME) }.map_err(hr("CreateMutexW"))?;
    // SAFETY: GetLastError immediately follows CreateMutexW as required.
    let code = unsafe { GetLastError() };
    // SAFETY: CreateMutexW returned a unique live handle; OwnedHandle becomes
    // its sole owner and closes it on every return path.
    let mutex = unsafe { OwnedHandle::from_raw_handle(mutex.0) };
    if code == ERROR_ALREADY_EXISTS {
        return Err(PlatformError::AlreadyRunning);
    }
    Ok(mutex)
}

fn ensure_class() -> Result<()> {
    if CONTROLLER_ATOM.get().is_none() {
        let atom = register_class(CONTROLLER_CLASS, Some(controller_wnd_proc))?;
        if CONTROLLER_ATOM.set(atom).is_err() && CONTROLLER_ATOM.get().is_none() {
            return Err(PlatformError::Invariant {
                operation: "controller class atom initialization",
            });
        }
    }
    Ok(())
}

fn register_class(
    name: PCWSTR,
    procedure: windows::Win32::UI::WindowsAndMessaging::WNDPROC,
) -> Result<u16> {
    let instance = module_handle()?;
    // SAFETY: loading a predefined process-independent cursor resource.
    let cursor = unsafe { LoadCursorW(None, IDC_ARROW) }.map_err(hr("LoadCursorW"))?;
    let class = WNDCLASSEXW {
        cbSize: u32::try_from(size_of::<WNDCLASSEXW>()).unwrap_or(u32::MAX),
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: procedure,
        hInstance: instance,
        hCursor: cursor,
        lpszClassName: name,
        ..Default::default()
    };
    // SAFETY: class points to initialized data and static class-name storage.
    let atom = unsafe { RegisterClassExW(&class) };
    if atom == 0 {
        return Err(last_error("RegisterClassExW(controller)"));
    }
    Ok(atom)
}

fn create_controller(state: Box<ControllerState>) -> Result<HWND> {
    let instance = module_handle()?;
    let mut payload = ControllerCreatePayload { state: Some(state) };
    // SAFETY: the payload lives for the synchronous CreateWindowExW call.
    // WM_NCCREATE takes its Box; otherwise the payload drops it normally.
    unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            CONTROLLER_CLASS,
            w!("linerule controller"),
            WS_OVERLAPPED,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(instance),
            Some((&raw mut payload).cast()),
        )
    }
    .map_err(hr("CreateWindowExW(controller)"))
}

fn add_tray(hwnd: HWND) -> Result<()> {
    #[cfg(not(test))]
    let (module, resource) = (Some(module_handle()?), APP_ICON_RESOURCE);
    #[cfg(test)]
    let (module, resource) = (None, IDI_APPLICATION);
    // SAFETY: production loads the icon resource compiled into linerule.exe;
    // unit-test binaries use the predefined process-independent app icon.
    let icon = unsafe { LoadIconW(module, resource) }.map_err(hr("LoadIconW(app icon)"))?;
    let mut data = NOTIFYICONDATAW {
        cbSize: u32::try_from(size_of::<NOTIFYICONDATAW>()).unwrap_or(u32::MAX),
        hWnd: hwnd,
        uID: TRAY_ID,
        uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
        uCallbackMessage: TRAY_CALLBACK,
        hIcon: icon,
        ..Default::default()
    };
    copy_wide(&mut data.szTip, "linerule");
    // SAFETY: data is initialized for NIM_ADD and remains live for the call.
    if unsafe { Shell_NotifyIconW(NIM_ADD, &raw const data) }.as_bool() {
        Ok(())
    } else {
        Err(last_error("Shell_NotifyIconW(NIM_ADD)"))
    }
}

fn remove_tray(hwnd: HWND) -> Result<()> {
    let data = NOTIFYICONDATAW {
        cbSize: u32::try_from(size_of::<NOTIFYICONDATAW>()).unwrap_or(u32::MAX),
        hWnd: hwnd,
        uID: TRAY_ID,
        ..Default::default()
    };
    // SAFETY: deletion uses only cbSize/hWnd/uID and is idempotent for teardown.
    if unsafe { Shell_NotifyIconW(NIM_DELETE, &raw const data) }.as_bool() {
        Ok(())
    } else {
        Err(last_error("Shell_NotifyIconW(NIM_DELETE)"))
    }
}

/// Controller-window callback installed only on [`CONTROLLER_CLASS`].
///
/// # Safety
/// Win32 supplies a live controller HWND and message-specific parameters. The
/// `WM_NCCREATE` parameter points to the stack payload in [`create_controller`].
unsafe extern "system" fn controller_wnd_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_NCCREATE {
        // SAFETY: Win32 guarantees CREATESTRUCTW for WM_NCCREATE.
        let create = unsafe { &*(lparam.0 as *const CREATESTRUCTW) };
        let payload = create.lpCreateParams.cast::<ControllerCreatePayload>();
        // SAFETY: create_controller passes a live payload synchronously.
        let Some(state) = (unsafe { &mut *payload }).state.take() else {
            return LRESULT(0);
        };
        let raw = Box::into_raw(state);
        // SAFETY: stores the detached Box pointer for this HWND.
        unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, raw as isize) };
        return LRESULT(1);
    }
    if message == WM_NCDESTROY {
        // SAFETY: retrieves and clears the pointer exactly once.
        let raw = unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0) } as *mut ControllerState;
        if !raw.is_null() {
            // SAFETY: pointer originated from Box::into_raw and is detached.
            drop(unsafe { Box::from_raw(raw) });
        }
        // SAFETY: default processing for final destruction.
        return unsafe { DefWindowProcW(hwnd, message, wparam, lparam) };
    }

    let handled = catch_unwind(AssertUnwindSafe(|| {
        with_controller(hwnd, |state| match message {
            TRAY_CALLBACK => {
                let notification = u32::try_from(lparam.0).unwrap_or_default();
                if notification == WM_LBUTTONUP {
                    send(state, ShellCommand::Toggle);
                } else if notification == WM_RBUTTONUP {
                    match tray_menu(hwnd) {
                        Ok(MENU_TOGGLE) => send(state, ShellCommand::Toggle),
                        Ok(MENU_SETTINGS) => {
                            if let Err(error) = open_settings(state, None) {
                                tracing::warn!(%error, "opening shortcut settings failed");
                                send(state, ShellCommand::SettingsFailed(error.to_string()));
                            }
                        },
                        Ok(MENU_EXIT) => send(state, ShellCommand::Exit),
                        Ok(_) => {},
                        Err(error) => tracing::warn!(%error, "tray menu failed"),
                    }
                }
                true
            },
            #[cfg(test)]
            WM_TIMER if wparam.0 == TEST_EXIT_TIMER_ID => {
                if let Err(error) = kill_controller_timer(hwnd, TEST_EXIT_TIMER_ID) {
                    tracing::warn!(%error, "test-exit KillTimer failed");
                }
                send(state, ShellCommand::Exit);
                true
            },
            WM_TIMER if wparam.0 == PREFERENCES_TIMER_ID => {
                if let Err(error) = kill_controller_timer(hwnd, PREFERENCES_TIMER_ID) {
                    tracing::warn!(%error, "preferences debounce KillTimer failed");
                }
                send(state, ShellCommand::FlushPreferences);
                true
            },
            _ => false,
        })
        .unwrap_or(false)
    }));
    let handled = match handled {
        Ok(handled) => handled,
        Err(payload) => {
            let panic_message = payload
                .downcast_ref::<&str>()
                .copied()
                .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
                .unwrap_or("<non-string panic payload>");
            tracing::error!(panic_message, "controller WndProc caught a panic");
            false
        },
    };
    if handled {
        LRESULT(0)
    } else {
        // SAFETY: unhandled controller messages use the platform default.
        unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
    }
}

fn tray_menu(hwnd: HWND) -> Result<usize> {
    // SAFETY: CreatePopupMenu returns an owned HMENU.
    let menu = unsafe { CreatePopupMenu() }.map_err(hr("CreatePopupMenu"))?;
    let selection = (|| {
        for (id, label) in [
            (MENU_TOGGLE, w!("Show/Hide")),
            (MENU_SETTINGS, w!("Shortcut settings...")),
            (MENU_EXIT, w!("Exit")),
        ] {
            // SAFETY: menu is live and label is static.
            unsafe { AppendMenuW(menu, MENU_ITEM_FLAGS(MF_STRING.0), id, label) }
                .map_err(hr("AppendMenuW(tray)"))?;
        }
        let mut point = POINT::default();
        // SAFETY: point is a valid out parameter.
        unsafe { GetCursorPos(&mut point) }.map_err(hr("GetCursorPos(tray)"))?;
        // SAFETY: required before displaying a notification-area context menu.
        if !unsafe { SetForegroundWindow(hwnd) }.as_bool() {
            return Err(last_error("SetForegroundWindow(tray)"));
        }
        // SAFETY: menu and owner are live for the synchronous call. Zero means
        // the user dismissed the menu when TPM_RETURNCMD is set.
        let selected = unsafe {
            TrackPopupMenu(
                menu,
                TPM_LEFTALIGN | TPM_RIGHTBUTTON | TPM_RETURNCMD,
                point.x,
                point.y,
                None,
                hwnd,
                None,
            )
        };
        Ok(usize::try_from(selected.0).unwrap_or_default())
    })();
    // SAFETY: menu is no longer in use after TrackPopupMenu returns.
    let cleanup = unsafe { DestroyMenu(menu) }.map_err(hr("DestroyMenu(tray)"));
    match (selection, cleanup) {
        (Ok(selected), Ok(())) => Ok(selected),
        (Err(error), Ok(())) | (Ok(_), Err(error)) => Err(error),
        (Err(error), Err(cleanup)) => {
            tracing::warn!(%cleanup, "tray menu cleanup also failed");
            Err(error)
        },
    }
}

fn open_settings(state: &ControllerState, error: Option<&PlatformError>) -> Result<()> {
    state
        .settings
        .open(state.sender.clone(), &state.bindings.borrow(), error)
}

fn with_controller<R>(
    hwnd: HWND,
    f: impl for<'state> FnOnce(&'state ControllerState) -> R,
) -> Option<R> {
    // SAFETY: GWLP_USERDATA is read only during a controller UI-thread call.
    let raw = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *const ControllerState;
    if raw.is_null() {
        None
    } else {
        // SAFETY: the HWND owns this Box until WM_NCDESTROY; the reference is
        // scoped to the callback and cannot escape through the closure type.
        Some(f(unsafe { &*raw }))
    }
}

fn send(state: &ControllerState, command: ShellCommand) {
    if state.sender.send(command).is_err() {
        tracing::warn!("desktop runtime command receiver is closed");
    }
}

fn module_handle() -> Result<HINSTANCE> {
    // SAFETY: null module name requests the current executable module.
    let module = unsafe { GetModuleHandleW(PCWSTR::null()) }.map_err(hr("GetModuleHandleW"))?;
    Ok(HINSTANCE(module.0))
}

fn last_error(operation: &'static str) -> PlatformError {
    // SAFETY: caller invokes this immediately after a failed Win32 API.
    let code = unsafe { GetLastError() }.0;
    PlatformError::LastError {
        operation,
        code,
        symbol: crate::error::decode_last_error(code),
    }
}

fn hr(operation: &'static str) -> impl Fn(windows::core::Error) -> PlatformError {
    move |error| PlatformError::BadHr {
        operation,
        hr: error.code().0,
    }
}

fn copy_wide<const N: usize>(target: &mut [u16; N], text: &str) {
    for (slot, value) in target.iter_mut().zip(text.encode_utf16().chain(Some(0))) {
        *slot = value;
    }
}
