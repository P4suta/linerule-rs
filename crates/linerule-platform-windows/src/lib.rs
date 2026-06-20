//! linerule-platform-windows
//!
//! Win32 / COM layer: HWND lifecycle, DirectComposition + Direct2D + D3D11
//! rendering, hotkeys, `DwmFlush` pacing, structured `tracing` events. No logic.
//!
//! Gated to Windows via `#![cfg(windows)]`; compiles as an empty crate elsewhere.
//!
//! `unsafe` is confined to `win32_ffi.rs`. Every other module is
//! `#![forbid(unsafe_code)]` and calls Win32 / COM only through its safe wrappers.
//!
//! Invariants:
//! - No logic here; call `linerule-core` reducers / render.
//! - `Drop` must release COM objects, HWNDs, hooks, and `JoinHandle`s.

#![cfg(windows)]
#![deny(unsafe_op_in_unsafe_fn)]

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
