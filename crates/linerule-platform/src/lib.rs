// Note: pure trait surface forbids unsafe; the per-OS impl modules opt back
// in at narrower granularity (see `windows.rs`).
#![cfg_attr(not(any(target_os = "windows")), forbid(unsafe_code))]
#![cfg_attr(target_os = "windows", deny(unsafe_op_in_unsafe_fn))]

//! Platform abstraction and OS-specific implementations for linerule.
//!
//! Defines three traits — [`OverlaySurface`], [`HotkeyHost`],
//! [`MouseTracker`] — that the binary wires together. Concrete impls live
//! in target-gated submodules (`windows.rs`, future `macos.rs`,
//! `linux_x11.rs`, `linux_wayland.rs`).
//!
//! Hotkey registration is RAII-flavoured: [`HotkeyHost::register`] returns
//! a [`HotkeyToken`] that, when dropped, releases the OS-level binding.

use core::fmt;
use std::sync::Arc;

use crossbeam_channel::Sender;
use linerule_core::{Action, Logical, OverlayFrame, Point, ScreenRect};
use thiserror::Error;

// ===========================================================================
// Errors
// ===========================================================================

/// Error from [`OverlaySurface`] operations.
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

/// Error from [`HotkeyHost`] operations.
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

/// Error from [`MouseTracker`] operations.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum MouseError {
    /// Underlying OS API returned an error.
    #[error("could not query cursor position: {0}")]
    Query(String),
}

// ===========================================================================
// Traits
// ===========================================================================

/// A live overlay surface — owns the OS window, GPU context, and presenter.
///
/// Drop tears down the surface; no global state leaks. Implementations
/// must be `Send + 'static` so the binary can move them onto the event
/// loop's thread.
pub trait OverlaySurface: Send + 'static {
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

/// A live system-wide hotkey host.
pub trait HotkeyHost: Send + 'static {
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

/// Sink that fires hotkey actions back into the event loop.
///
/// Bounded under the hood; on overflow the platform impl logs via
/// `tracing::warn!` and drops (the user's recent action just lost a
/// frame — they'll repeat it).
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
    /// (the receiver dropped) or if the bounded channel is full.
    #[must_use = "the boolean reports whether the action was actually queued"]
    pub fn send(&self, action: Action) -> bool {
        self.inner.try_send(action).is_ok()
    }
}

/// Pull-mode source of cursor position.
///
/// Polled from the event loop tick (winit drives the cadence) — adding a
/// push channel would be a wasted thread-context per redraw.
pub trait MouseTracker: Send + 'static {
    /// Current cursor position in [`Logical`] pixels.
    ///
    /// # Errors
    /// Returns [`MouseError::Query`] if the OS API fails.
    fn position(&self) -> Result<Point<Logical>, MouseError>;
}

// ===========================================================================
// HotkeyToken — RAII capability
// ===========================================================================

/// Opaque proof of a registered hotkey.
///
/// Cannot be cloned. Dropping unregisters the chord through the
/// `Drop`-bearing inner closure provided by the platform impl.
#[must_use = "dropping the token releases the OS hotkey registration"]
pub struct HotkeyToken {
    /// Owned drop-fn supplied by the platform impl.
    /// `Arc` wrapper is for type-erased ownership; not for sharing.
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
///
/// Implementations call into the OS to unregister on Drop.
pub trait HotkeyRelease: Send + Sync + 'static {}

// ===========================================================================
// Per-OS impls (gated)
// ===========================================================================

#[cfg(target_os = "windows")]
pub mod windows;

#[cfg(target_os = "windows")]
pub use windows::{hotkeys as open_hotkeys, mouse as open_mouse, open as open_overlay};

// ===========================================================================
// Non-Windows fallbacks — symbols exist so the binary compiles on the
// Linux dev container (used for cross-build smoke tests). They return a
// clear error at runtime so `linerule run` doesn't pretend.
//
// v0.2 will replace these with real macOS / Linux X11 / Wayland impls,
// at which point the cfg gating tightens.
// ===========================================================================

/// Open the overlay surface bound to the monitor containing the cursor.
///
/// # Errors
/// Returns a [`SurfaceError`] explaining that this OS is unsupported in v0.1.
#[cfg(not(target_os = "windows"))]
pub fn open_overlay() -> Result<Box<dyn OverlaySurface>, SurfaceError> {
    Err(SurfaceError::Create(
        "linerule v0.1 supports Windows only — see ADR-0004".into(),
    ))
}

/// Open a system-wide hotkey host.
///
/// # Errors
/// Returns a [`HotkeyError`] explaining that this OS is unsupported in v0.1.
#[cfg(not(target_os = "windows"))]
pub fn open_hotkeys() -> Result<Box<dyn HotkeyHost>, HotkeyError> {
    Err(HotkeyError::OsReject(
        "linerule v0.1 supports Windows only — see ADR-0004".into(),
    ))
}

/// Open the cursor-position tracker.
///
/// # Errors
/// Returns a [`MouseError`] explaining that this OS is unsupported in v0.1.
#[cfg(not(target_os = "windows"))]
pub fn open_mouse() -> Result<Box<dyn MouseTracker>, MouseError> {
    Err(MouseError::Query(
        "linerule v0.1 supports Windows only — see ADR-0004".into(),
    ))
}

// ===========================================================================
// Cross-platform mock — used by core tests and on non-Windows targets
// for trait-shape verification.
// ===========================================================================

#[cfg(any(feature = "mock", not(target_os = "windows")))]
pub mod mock;
