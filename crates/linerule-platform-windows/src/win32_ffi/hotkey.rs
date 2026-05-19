//! `RegisterHotKey` / `UnregisterHotKey` / message-only HWND の薄い safe wrapper。
//!
//! Phase E で `hotkey_host.rs` から呼ばれる。`HWND_MESSAGE = -3` を親 HWND と
//! して `CreateWindowExW` を呼ぶことで「画面に出ない、メッセージ受信専用」
//! window を作る。

#![allow(
    unsafe_code,
    reason = "FFI 境界。RegisterHotKey / CreateWindowExW(HWND_MESSAGE) は\
              windows crate でも全部 unsafe。ADR-0003。"
)]

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, HOT_KEY_MODIFIERS, RegisterHotKey, UnregisterHotKey,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, HWND_MESSAGE, WINDOW_EX_STYLE, WNDPROC,
};
use windows::core::PCWSTR;

use crate::error::{Result, Win32Error};
use crate::overlay_state::OverlayWndState;
use crate::win32_ffi::core::{last_error, module_handle, register_class};

/// Message-only HWND を作成する。`hWndParent = HWND_MESSAGE (-3)` で
/// デスクトップにも表示されず、Alt-Tab にも出ない hidden window。
///
/// # Errors
/// `CreateWindowExW(HWND_MESSAGE, ...)` が失敗したとき。
pub fn create_message_only_window(
    class_name: PCWSTR,
    title: PCWSTR,
    wnd_proc: WNDPROC,
) -> Result<HWND> {
    let _atom = register_class(class_name, wnd_proc)?;
    let h_instance = module_handle()?;
    // SAFETY: HWND_MESSAGE は Windows API 定義の特殊親 HWND (-3)。
    let hwnd = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            class_name,
            title,
            windows::Win32::UI::WindowsAndMessaging::WINDOW_STYLE::default(),
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            Some(h_instance),
            None,
        )
    }
    .map_err(|e| Win32Error::BadHr {
        operation: "CreateWindowExW(HWND_MESSAGE)",
        hr: e.code().0,
    })?;
    if hwnd.0.is_null() {
        return Err(Win32Error::NullHandle {
            operation: "CreateWindowExW(HWND_MESSAGE)",
        });
    }
    Ok(hwnd)
}

/// `RegisterHotKey(hwnd, id, modifiers, vk)` の薄い safe wrapper。
/// MOD_NOREPEAT を自動付与し OS の auto-repeat を抑止する。
///
/// # Errors
/// `RegisterHotKey` が FALSE を返したとき (重複登録等)。
pub fn register_hotkey(hwnd: HWND, id: i32, modifiers: u32, vk: u32) -> Result<()> {
    // Add MOD_NOREPEAT (0x4000) to suppress OS-level key repeat; HoldFsm のみが repeat を制御する。
    const MOD_NOREPEAT: u32 = 0x4000;
    let m = HOT_KEY_MODIFIERS(modifiers | MOD_NOREPEAT);
    // SAFETY: hwnd は valid (message-only HWND)、id / modifiers / vk は plain int
    unsafe { RegisterHotKey(Some(hwnd), id, m, vk) }.map_err(|e| Win32Error::BadHr {
        operation: "RegisterHotKey",
        hr: e.code().0,
    })
}

/// `UnregisterHotKey(hwnd, id)` の薄い safe wrapper。失敗してもログだけ。
pub fn unregister_hotkey(hwnd: HWND, id: i32) -> Result<()> {
    // SAFETY: hwnd / id は valid
    unsafe { UnregisterHotKey(Some(hwnd), id) }.map_err(|e| Win32Error::BadHr {
        operation: "UnregisterHotKey",
        hr: e.code().0,
    })
}

/// `GetAsyncKeyState(vk)` の薄い safe wrapper。最上位ビットが立っていれば
/// 押下中。
pub fn is_key_down(vk: i32) -> bool {
    // SAFETY: GetAsyncKeyState は read-only システムコール
    let state = unsafe { GetAsyncKeyState(vk) };
    (state as u16) & 0x8000 != 0
}

// 警告抑止: OverlayWndState を import しているが現状未使用。Phase E の発展で使う。
#[allow(dead_code, reason = "Phase E 発展用")]
const _: fn() = || {
    let _: Option<&OverlayWndState> = None;
    let _ = last_error;
};
