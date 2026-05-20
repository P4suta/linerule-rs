//! ディスプレイのリフレッシュレートを取得する。
//!
//! Pacer は `DwmFlush` で待つので動作自体は Hz を陽に知る必要はないが、
//! HUD telemetry で `144 Hz` 等を表示するために `EnumDisplaySettingsW` 経由で
//! 取得する。

#![forbid(unsafe_code)]
#![cfg(windows)]

use crate::win32_ffi;

/// プライマリディスプレイのリフレッシュレート (Hz) を取得する。失敗時 / OS が
/// `0` または `1` を返す（remote desktop / generic display driver）ときは
/// fallback として 60 Hz を返す。
#[must_use]
pub fn refresh_rate_hz() -> u32 {
    win32_ffi::enum_display_settings_current()
        .map(|dm| dm.dmDisplayFrequency)
        .ok()
        .filter(|&hz| hz > 1)
        .unwrap_or(60)
}
