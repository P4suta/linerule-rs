// Pure trait surface forbids unsafe; the per-OS impl modules opt back in
// at narrower granularity (see `windows.rs`).
#![cfg_attr(not(target_os = "windows"), forbid(unsafe_code))]
#![cfg_attr(target_os = "windows", deny(unsafe_op_in_unsafe_fn))]

//! Platform abstraction and OS-specific implementations for linerule.
//!
//! Two surfaces:
//!
//! - [`run`] is the production entry point. The binary calls it once
//!   and the call blocks until the user closes the overlay. On Windows
//!   it spins up the winit event loop, creates a click-through layered
//!   window, registers global hotkeys, and drives the vello renderer.
//!   On every other target it returns [`RunError::Unsupported`] so the
//!   binary still compiles for cross-target smoke tests.
//!
//! - The [`OverlaySurface`] / [`HotkeyHost`] / [`MouseTracker`] traits
//!   are the *mock-side* abstraction used by `linerule-core` tests to
//!   exercise behaviour without an OS surface. They are not used by the
//!   production [`run`] path because winit's `Window` is `!Send` and
//!   the event loop owns everything internally.

use core::fmt;
use std::sync::Arc;

use crossbeam_channel::Sender;
use linerule_core::{Action, Logical, OverlayFrame, Point, ScreenRect, State};
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

    /// Failed to drive the wgpu / vello renderer.
    #[error("renderer error: {0}")]
    Renderer(String),
}

/// Error from [`OverlaySurface`] operations (mock layer).
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum SurfaceError {
    /// Failed to create / configure the underlying OS window.
    #[error("failed to create overlay window: {0}")]
    Create(String),
    /// Failed to drive the GPU surface (`vello` / `wgpu`).
    #[error("renderer error: {0}")]
    Renderer(String),
    /// Failed to make the window click-through (Win32 `SetWindowLongPtr` etc.).
    #[error("failed to apply click-through extended style: {0}")]
    ClickThrough(String),
}

/// Error from [`HotkeyHost`] operations (mock layer).
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum HotkeyError {
    /// Failed to parse a chord string (e.g. `"Ctrl+Alt+R"`).
    #[error("could not parse hotkey chord {chord:?}: {reason}")]
    Parse {
        /// The chord string that failed to parse.
        chord: String,
        /// Reason for the failure.
        reason: String,
    },
    /// OS rejected the registration (chord already bound, etc.).
    #[error("OS rejected hotkey registration: {0}")]
    OsReject(String),
}

/// Error from [`MouseTracker`] operations (mock layer).
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum MouseError {
    /// Underlying OS API returned an error.
    #[error("could not query cursor position: {0}")]
    Query(String),
}

// ===========================================================================
// Production entry point
// ===========================================================================

/// Run the overlay event loop until the user closes it.
///
/// `initial_state` seeds the in-memory [`State`] machine; `hotkeys` is the
/// pre-parsed list of `(chord, action)` bindings the binary built from
/// `linerule_config::HotkeyMap`. Blocks on the calling thread.
///
/// # Errors
/// Returns a [`RunError`] for any platform-level failure: missing OS
/// support, window creation, click-through application, hotkey
/// registration, or renderer init / present.
#[cfg(target_os = "windows")]
pub fn run(initial_state: State, hotkeys: &[(String, Action)]) -> Result<(), RunError> {
    windows::run(initial_state, hotkeys)
}

/// Non-Windows fallback — see [`run`] for the contract.
///
/// # Errors
/// Always returns [`RunError::Unsupported`] in v0.1.
#[cfg(not(target_os = "windows"))]
pub fn run(initial_state: State, hotkeys: &[(String, Action)]) -> Result<(), RunError> {
    let _ = (initial_state, hotkeys);
    Err(RunError::Unsupported(
        "current target_os is not `windows`".into(),
    ))
}

// ===========================================================================
// Mock-layer trait surface (used by linerule-core tests)
// ===========================================================================

/// A live overlay surface. Mock-side abstraction.
///
/// The production [`run`] path does NOT go through this trait — winit's
/// `Window` is `!Send` and the event loop owns the surface concretely.
/// Mock impls (see [`mock::MockSurface`]) implement this for
/// `linerule-core`'s test suite.
pub trait OverlaySurface: 'static {
    /// Show the overlay window.
    ///
    /// # Errors
    /// Surfaces any platform-level failure to display.
    fn show(&mut self) -> Result<(), SurfaceError>;

    /// Hide the overlay window.
    ///
    /// # Errors
    /// Surfaces any platform-level failure to hide.
    fn hide(&mut self) -> Result<(), SurfaceError>;

    /// Render `frame` to the surface.
    ///
    /// # Errors
    /// Surfaces GPU / blit failures.
    fn present(&mut self, frame: &OverlayFrame) -> Result<(), SurfaceError>;

    /// Bounds of the monitor the overlay is currently bound to (logical px).
    fn monitor(&self) -> ScreenRect<Logical>;

    /// HiDPI scale factor of the bound monitor.
    fn dpi_scale(&self) -> f32;
}

/// A live system-wide hotkey host. Mock-side abstraction.
pub trait HotkeyHost: 'static {
    /// Register `chord` so triggering it emits `action` on `sink`.
    ///
    /// The returned [`HotkeyToken`] holds the registration alive; dropping
    /// it releases the OS-level binding (RAII capability).
    ///
    /// # Errors
    /// Returns [`HotkeyError::Parse`] for malformed chords or
    /// [`HotkeyError::OsReject`] if the OS refuses registration.
    fn register(
        &mut self,
        chord: &str,
        action: Action,
        sink: HotkeySink,
    ) -> Result<HotkeyToken, HotkeyError>;
}

/// Pull-mode source of cursor position. Mock-side abstraction.
pub trait MouseTracker: 'static {
    /// Current cursor position in [`Logical`] pixels.
    ///
    /// # Errors
    /// Returns [`MouseError::Query`] if the OS API fails.
    fn position(&self) -> Result<Point<Logical>, MouseError>;
}

/// Sink that fires hotkey actions back into the event loop.
#[derive(Clone, Debug)]
pub struct HotkeySink {
    inner: Sender<Action>,
}

impl HotkeySink {
    /// Wrap a [`crossbeam_channel::Sender`] as a [`HotkeySink`].
    #[must_use]
    pub const fn new(inner: Sender<Action>) -> Self {
        Self { inner }
    }

    /// Send `action`. Returns `false` if the sink has been disconnected
    /// or the bounded channel is full.
    #[must_use = "the boolean reports whether the action was actually queued"]
    pub fn send(&self, action: Action) -> bool {
        self.inner.try_send(action).is_ok()
    }
}

/// Opaque proof of a registered hotkey (RAII capability).
#[must_use = "dropping the token releases the OS hotkey registration"]
pub struct HotkeyToken {
    _release: Arc<dyn HotkeyRelease>,
}

impl HotkeyToken {
    /// Wrap a platform-supplied release handle.
    pub fn new(release: Arc<dyn HotkeyRelease>) -> Self {
        Self { _release: release }
    }
}

impl fmt::Debug for HotkeyToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HotkeyToken").finish_non_exhaustive()
    }
}

/// Erased platform-side release handle.
pub trait HotkeyRelease: Send + Sync + 'static {}

// ===========================================================================
// Per-OS modules
// ===========================================================================

#[cfg(target_os = "windows")]
pub mod windows;

#[cfg(any(feature = "mock", not(target_os = "windows")))]
pub mod mock;
