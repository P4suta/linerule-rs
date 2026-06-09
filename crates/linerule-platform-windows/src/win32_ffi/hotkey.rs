//! `RegisterHotKey` / `UnregisterHotKey` の薄い safe wrapper。
//!
//! `RegisterHotKey` は `WS_EX_LAYERED + WS_EX_TRANSPARENT + WS_EX_NOACTIVATE`
//! HWND でも `WM_HOTKEY` を受信できるので、message-only HWND を別途立てず
//! overlay HWND 自体を target にする。

#![allow(
    unsafe_code,
    reason = "FFI 境界。RegisterHotKey / UnregisterHotKey は windows crate でも unsafe。"
)]

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    HOT_KEY_MODIFIERS, RegisterHotKey, UnregisterHotKey,
};

use crate::error::{PlatformError, Result};

/// `MOD_NOREPEAT` flag value (windows-rs では `HOT_KEY_MODIFIERS` 経由でしか露出
/// していないため定数として再宣言)。`RegisterHotKey` の `fsModifiers` に OR で
/// 付与すると Windows が auto-repeat による `WM_HOTKEY` の連続発火を抑制する。
const MOD_NOREPEAT: u32 = 0x4000;

/// `RegisterHotKey(hwnd, id, modifiers, vk)` の薄い safe wrapper。
///
/// `repeatable = false` のとき `MOD_NOREPEAT` を自動付与し、長押し中の連続発火を
/// 抑止する。CycleMode / ToggleVisible / Quit のような toggle 系 action 向け。
///
/// `repeatable = true` のとき `MOD_NOREPEAT` を付与しないため、Windows のキー
/// リピート速度に従って `WM_HOTKEY` が連続で飛ぶ。BumpThickness / BumpOpacity の
/// ような連続調整 action 向け。
///
/// # Errors
/// `RegisterHotKey` が FALSE を返したとき (重複登録等)。
pub fn register_hotkey(
    hwnd: HWND,
    id: i32,
    modifiers: u32,
    vk: u32,
    repeatable: bool,
) -> Result<()> {
    let mods = if repeatable {
        modifiers
    } else {
        modifiers | MOD_NOREPEAT
    };
    let m = HOT_KEY_MODIFIERS(mods);
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
