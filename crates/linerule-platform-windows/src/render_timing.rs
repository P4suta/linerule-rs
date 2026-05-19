//! ディスプレイのリフレッシュレートを取得する。
//!
//! Phase F では現状未使用（pacer は `DwmFlush` で待つので Hz を陽に知る必要は
//! ない）。Phase G の HUD telemetry で `120 Hz` 等を表示するためのフックを
//! 用意しておく。

#![forbid(unsafe_code)]
#![cfg(windows)]

/// プライマリディスプレイのリフレッシュレート (Hz) を取得する。失敗時は
/// fallback として 60 Hz を返す。
#[must_use]
pub fn refresh_rate_hz() -> u32 {
    // TODO Phase G: EnumDisplaySettingsW(NULL, ENUM_CURRENT_SETTINGS) → DEVMODEW.dmDisplayFrequency
    60
}
