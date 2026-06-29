//! Overlay smoke test: transparent click-through overlay over the primary
//! monitor, blocking in the message pump until `WM_QUIT`.
//!
//! Checks `WndProc` survivability (panic in `WndProc` is contained by
//! `catch_unwind`) with minimal setup; hotkey/tick wiring is covered by
//! `linerule.exe run`. Invisible window of class `linerule-rs-overlay`,
//! click-through, hidden from Alt+Tab (`WS_EX_TOOLWINDOW`).

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
