//! Win32/COM layer: HWND lifecycle, DComp/D2D/D3D11 rendering, hotkeys, pacing.
//! `unsafe` confined to `win32_ffi`; other modules `forbid(unsafe_code)`. No logic
//! here (delegate to `linerule-core`); `Drop` must release COM/HWND/hooks/threads.

#![cfg(windows)]
#![deny(unsafe_op_in_unsafe_fn)]
#![allow(
    unreachable_pub,
    reason = "implementation modules are private; their public-looking items are visible only inside this crate"
)]
// pedantic/nursery exempted crate-wide for this thin Win32/COM/D3D wrapper layer,
// matching xtask's `clippy-windows-deny-list` (-A on Windows target). `clippy::all`
// and `disallowed_*` stay enforced.
#![allow(
    clippy::pedantic,
    clippy::nursery,
    reason = "pedantic/nursery intentionally exempted for Win32/COM layer (same policy as clippy-windows-deny-list)"
)]

mod cursor_tracker;
mod desktop_runtime;
mod error;
mod ex_style_snapshot;
mod foreground_hook;
mod frame_timing;
mod messages;
mod monitor_info;
mod overlay_state;
mod overlay_window;
mod render_clock;
mod render_timing;
mod renderer_backend;
mod settings_host;
mod win32_ffi;
mod window_class;
mod winrt_composition_renderer;
mod winrt_hud_renderer;
mod wndproc;

pub use desktop_runtime::{DesktopRuntime, LaunchIntent, RuntimeOptions};
pub use error::PlatformError;
