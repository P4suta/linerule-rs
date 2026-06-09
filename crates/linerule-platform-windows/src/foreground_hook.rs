//! 前景アプリ変更で overlay の z-order を最前面に再 assert する RAII guard。
//!
//! Alt+Tab や `SetForegroundWindow` で他アプリが前面化すると、`WS_EX_TOPMOST`
//! を持つ overlay でも一時的に背後に回るケースが Windows の z-order 競合で
//! 起きうる。本 hook は `SetWinEventHook(EVENT_SYSTEM_FOREGROUND)` で前景変更
//! を監視し、UI thread に `WM_APP_REASSERT_TOPMOST` を投げる。実 `SetWindowPos
//! (HWND_TOPMOST)` は wndproc 側で実行する。
//!
//! `WINEVENT_SKIPOWNPROCESS` で自プロセス前景化は OS が抑制してくれるので、
//! callback 内での HWND 比較は不要。

#![forbid(unsafe_code)]

#[cfg(target_os = "windows")]
use windows::Win32::Foundation::HWND;
#[cfg(target_os = "windows")]
use windows::Win32::UI::Accessibility::HWINEVENTHOOK;

#[cfg(target_os = "windows")]
use crate::error::Result;
#[cfg(target_os = "windows")]
use crate::win32_ffi::accessibility;

/// 前景アプリ変更通知の RAII guard。Drop で必ず `UnhookWinEvent` する。
///
/// `target` HWND は AtomicIsize 経由で callback と共有される (詳細
/// `win32_ffi::accessibility`)。同時に複数 install しても safe だが、その場合
/// 最後に install した HWND だけが通知を受ける (グローバル状態のため)。
/// overlay インスタンスが 1 つの設計なので問題にならない。
#[cfg(target_os = "windows")]
pub struct ForegroundHook {
    hook: HWINEVENTHOOK,
}

#[cfg(target_os = "windows")]
impl ForegroundHook {
    /// `SetWinEventHook` を仕掛け、callback が `target` HWND へ
    /// `WM_APP_REASSERT_TOPMOST` を投げるようにする。
    ///
    /// # Errors
    /// `SetWinEventHook` が null を返したとき (`PlatformError::NullHandle`)。
    pub fn install(target: HWND) -> Result<Self> {
        let hook = accessibility::set_foreground_hook(target)?;
        tracing::info!("ForegroundHook installed for topmost re-assertion");
        Ok(Self { hook })
    }
}

#[cfg(target_os = "windows")]
impl Drop for ForegroundHook {
    fn drop(&mut self) {
        if !self.hook.0.is_null() {
            if let Err(e) = accessibility::unhook_win_event(self.hook) {
                tracing::warn!(error = %e, "UnhookWinEvent failed during ForegroundHook::drop");
            }
        }
    }
}
