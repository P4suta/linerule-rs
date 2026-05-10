// Pure trait surface forbids unsafe; the per-OS impl modules opt back in
// at narrower granularity (see `windows.rs`).
#![cfg_attr(not(target_os = "windows"), forbid(unsafe_code))]
#![cfg_attr(target_os = "windows", deny(unsafe_op_in_unsafe_fn))]

//! Platform-direct event-loop entry for linerule.
//!
//! The crate exposes a single production verb — [`run`] — that the
//! binary calls once and the call blocks until the user closes the
//! overlay. On Windows it spins up the winit event loop, registers
//! global hotkeys, and drives the layered-window renderer. On every
//! other target it returns [`RunError::Unsupported`] so the binary
//! still cross-compiles for smoke tests.
//!
//! There is intentionally no `OverlaySurface` / `HotkeyHost` /
//! `MouseTracker` trait surface (cf. ADR-0010 superseding ADR-0003):
//! winit's `Window` is `!Send` and the event loop owns the surface
//! concretely, so any abstraction had to be a mock-only crutch with no
//! production consumer. v0.2 OS additions land as parallel
//! `mod macos;` / `mod linux_wayland;` modules with their own
//! event-loop entries; the binary dispatches by `cfg(target_os)`.

use linerule_core::{HotkeyEffect, State};
use thiserror::Error;

// ===========================================================================
// Errors
// ===========================================================================

/// Top-level error from [`run`].
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum RunError {
    /// This target OS is not yet supported by the production overlay
    /// path. Today only Windows 10/11 ships; macOS and Linux are
    /// scheduled for v0.2 (see ADR-0004).
    #[error("linerule v0.1 supports Windows only — see ADR-0004 ({0})")]
    Unsupported(String),

    /// Failed to create or drive the winit event loop.
    #[error("event loop error: {0}")]
    EventLoop(String),

    /// Failed to create or configure the overlay window.
    #[error("overlay window error: {0}")]
    Window(String),

    /// Failed to apply the Win32 click-through extended window style.
    #[error("Win32 click-through error: {0}")]
    ClickThrough(String),

    /// Failed to register a system-wide hotkey.
    #[error("hotkey error: {0}")]
    Hotkey(String),

    /// Failed to drive the GDI layered-window renderer.
    #[error("renderer error: {0}")]
    Renderer(String),
}

// ===========================================================================
// Production entry point
// ===========================================================================

/// Run the overlay event loop until the user closes it.
///
/// `initial_state` seeds the in-memory [`State`] machine; `hotkeys` is the
/// pre-parsed list of `(chord, effect)` bindings the binary built from
/// `linerule_config::HotkeyMap`. Blocks on the calling thread.
///
/// # Errors
/// Returns a [`RunError`] for any platform-level failure: missing OS
/// support, window creation, click-through application, hotkey
/// registration, or renderer present.
#[cfg(target_os = "windows")]
pub fn run(initial_state: State, hotkeys: &[(String, HotkeyEffect)]) -> Result<(), RunError> {
    windows::run(initial_state, hotkeys)
}

/// Non-Windows fallback — see [`run`] for the contract.
///
/// # Errors
/// Always returns [`RunError::Unsupported`] in v0.1.
#[cfg(not(target_os = "windows"))]
pub fn run(initial_state: State, hotkeys: &[(String, HotkeyEffect)]) -> Result<(), RunError> {
    let _ = (initial_state, hotkeys);
    Err(RunError::Unsupported(
        "current target_os is not `windows`".into(),
    ))
}

// ===========================================================================
// Per-OS modules
// ===========================================================================

pub mod chord;

#[cfg(target_os = "windows")]
pub mod windows;
