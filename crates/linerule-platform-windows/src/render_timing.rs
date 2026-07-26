//! Display refresh rate. Not needed for pacing (the pacer waits on `DwmFlush`),
//! only for HUD telemetry; read via `EnumDisplaySettingsW`.

#![forbid(unsafe_code)]
#![cfg(windows)]

use crate::win32_ffi;

/// Primary display refresh rate in Hz. Falls back to 60 on failure or when the
/// OS reports 0/1 (remote desktop / generic display driver).
#[must_use]
pub fn refresh_rate_hz() -> u32 {
    match win32_ffi::enum_display_settings_current() {
        Ok(mode) if mode.dmDisplayFrequency > 1 => mode.dmDisplayFrequency,
        Ok(_) => 60,
        Err(error) => {
            tracing::warn!(%error, "display refresh query failed; using 60 Hz");
            60
        },
    }
}
