//! Win32/COM layer: HWND lifecycle, DComp/D2D/D3D11 rendering, hotkeys, pacing.
//! `unsafe` confined to `win32_ffi`; other modules `forbid(unsafe_code)`. No logic
//! here (delegate to `linerule-core`); `Drop` must release COM/HWND/hooks/threads.

#![cfg(windows)]
#![deny(unsafe_op_in_unsafe_fn)]
// pedantic/nursery exempted crate-wide for this thin Win32/COM/D3D wrapper layer,
// matching xtask's `clippy-windows-deny-list` (-A on Windows target). `clippy::all`
// and `disallowed_*` stay enforced.
#![allow(
    clippy::pedantic,
    clippy::nursery,
    reason = "pedantic/nursery intentionally exempted for Win32/COM layer (same policy as clippy-windows-deny-list)"
)]

pub mod auto_quit;
pub mod cursor_tracker;
pub mod error;
pub mod ex_style_snapshot;
pub mod foreground_hook;
pub mod frame_timing;
pub mod messages;
pub mod monitor_info;
pub mod overlay_state;
pub mod overlay_window;
pub mod render_clock;
pub mod render_timing;
pub mod renderer_backend;
pub mod win32_ffi;
pub mod window_class;
pub mod windows_app;
pub mod winrt_composition_renderer;
pub mod winrt_hud_renderer;
pub mod wndproc;

pub use auto_quit::AutoQuitTimer;
pub use error::{PlatformError, Result};
pub use foreground_hook::ForegroundHook;
pub use overlay_state::{HotkeyConflict, HotkeyFailure, OverlayWndState};
pub use overlay_window::OverlayWindow;
pub use render_clock::RenderClock;
pub use win32_ffi::input::send_chord;
pub use win32_ffi::set_dpi_aware;
pub use windows_app::run_message_pump;
