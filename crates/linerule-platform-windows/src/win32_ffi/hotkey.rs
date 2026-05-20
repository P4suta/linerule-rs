//! `RegisterHotKey` / `UnregisterHotKey` の薄い safe wrapper。
//!
//! Phase E (`overlay_window::register_hotkeys`) から呼ばれる。`RegisterHotKey`
//! は `WS_EX_LAYERED + WS_EX_TRANSPARENT + WS_EX_NOACTIVATE` HWND でも
//! `WM_HOTKEY` を受信できる（MSDN 確認）ので、message-only HWND を別途立てる
//! 必要はなく、overlay HWND 自体を target にする。

#![allow(
    unsafe_code,
    reason = "FFI 境界。RegisterHotKey / UnregisterHotKey / GetAsyncKeyState は\
              windows crate でも全部 unsafe。ADR-0003。"
)]

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, HOT_KEY_MODIFIERS, RegisterHotKey, UnregisterHotKey,
};

use crate::error::{PlatformError, Result};

/// `RegisterHotKey(hwnd, id, modifiers, vk)` の薄い safe wrapper。
/// MOD_NOREPEAT を自動付与し OS の auto-repeat を抑止する（hold-to-repeat は
/// HoldFsm 側で扱う将来拡張のため）。
///
/// # Errors
/// `RegisterHotKey` が FALSE を返したとき (重複登録等)。
pub fn register_hotkey(hwnd: HWND, id: i32, modifiers: u32, vk: u32) -> Result<()> {
    // Add MOD_NOREPEAT (0x4000) to suppress OS-level key repeat; HoldFsm のみが repeat を制御する。
    const MOD_NOREPEAT: u32 = 0x4000;
    let m = HOT_KEY_MODIFIERS(modifiers | MOD_NOREPEAT);
    // SAFETY: hwnd は valid (overlay HWND)、id / modifiers / vk は plain int
    unsafe { RegisterHotKey(Some(hwnd), id, m, vk) }.map_err(|e| PlatformError::BadHr {
        operation: "RegisterHotKey",
        hr: e.code().0,
    })
}

/// `UnregisterHotKey(hwnd, id)` の薄い safe wrapper。失敗してもログだけ。
///
/// # Errors
/// `UnregisterHotKey` が FALSE を返したとき。
pub fn unregister_hotkey(hwnd: HWND, id: i32) -> Result<()> {
    // SAFETY: hwnd / id は valid
    unsafe { UnregisterHotKey(Some(hwnd), id) }.map_err(|e| PlatformError::BadHr {
        operation: "UnregisterHotKey",
        hr: e.code().0,
    })
}

/// `GetAsyncKeyState(vk)` の薄い safe wrapper。最上位ビットが立っていれば
/// 押下中。HoldFsm が `still_held` を判定するのに将来使う。
#[allow(
    dead_code,
    reason = "HoldFsm 結線（hold-to-repeat）の将来 PR でこの関数を WM_APP_TICK\
              ハンドラ側から呼ぶ。MOD_NOREPEAT で OS auto-repeat を切ってある\
              ので core 側で押下継続を判定する必要が出る"
)]
pub fn is_key_down(vk: i32) -> bool {
    // SAFETY: GetAsyncKeyState は read-only システムコール
    let state = unsafe { GetAsyncKeyState(vk) };
    (state as u16) & 0x8000 != 0
}
