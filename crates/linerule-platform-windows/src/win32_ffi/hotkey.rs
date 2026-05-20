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
