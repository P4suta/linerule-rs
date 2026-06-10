//! Display refresh rate. Not needed for pacing (the pacer waits on `DwmFlush`),
//! only for HUD telemetry; read via `EnumDisplaySettingsW`.

#![forbid(unsafe_code)]
#![cfg(windows)]

use crate::win32_ffi;

/// Primary display refresh rate in Hz. Falls back to 60 on failure or when the
/// OS reports 0/1 (remote desktop / generic display driver).
#[must_use]
pub fn refresh_rate_hz() -> u32 {
    win32_ffi::enum_display_settings_current()
        .map(|dm| dm.dmDisplayFrequency)
        .ok()
        .filter(|&hz| hz > 1)
        .unwrap_or(60)
}
