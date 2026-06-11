//! Overlay smoke test.
//!
//! `cargo run --example overlay_smoke` (Windows host) raises a transparent
//! click-through overlay covering the primary monitor and blocks in the
//! message pump until `WM_QUIT`. Exit cleanly by killing
//! `linerule-platform-windows.exe` from Task Manager, or by triggering
//! `DestroyWindow` so `PostQuitMessage` flows.
//!
//! Expected behavior:
//! - Nothing is visible, but Spy++ shows an HWND of class `linerule-rs-overlay`
//! - Clicks on other windows pass through the overlay
//! - The overlay does not appear in Alt+Tab (`WS_EX_TOOLWINDOW`)
//! - A deliberate panic planted in `WndProc` does not kill the overlay
//!   (`catch_unwind`)
//!
//! Hotkey + tick wiring is verified by `linerule.exe run`; this example checks
//! `WndProc` survivability with the minimal setup (HWND + compositor attach).

#![forbid(unsafe_code)]
#![allow(
    clippy::print_stderr,
    reason = "guidance output when the smoke example runs on a non-windows target"
)]

#[cfg(windows)]
fn main() -> anyhow::Result<()> {
    use linerule_core::HudConfig;
    use linerule_platform_windows::{OverlayWindow, monitor_info, run_message_pump};

    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_target(true)
        .init();

    let monitor = monitor_info::primary_bounds()?;
    tracing::info!(
        width = monitor.width,
        height = monitor.height,
        "creating overlay"
    );

    let _overlay = OverlayWindow::new(
        monitor,
        HudConfig::DEFAULT,
        linerule_core::AnimConfig::DEFAULT,
    )?;
    run_message_pump()?;
    Ok(())
}

#[cfg(not(windows))]
fn main() {
    eprintln!("overlay_smoke is Windows-only; build with --target x86_64-pc-windows-msvc");
}
