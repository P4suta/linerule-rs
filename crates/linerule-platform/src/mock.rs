//! In-memory mock implementations of the platform traits.
//!
//! Used by `linerule-core` tests to exercise the trait surface without
//! touching any real OS API. Also acts as the smoke-test target on
//! non-Windows hosts (Linux dev container) where the real Windows impl
//! cannot be linked.

use std::sync::{Arc, Mutex};

use linerule_core::{Action, Logical, OverlayFrame, Point, ScreenRect};

use crate::{
    HotkeyError, HotkeyHost, HotkeyRelease, HotkeySink, HotkeyToken, MouseError, MouseTracker,
    OverlaySurface, SurfaceError,
};

// ---------------------------------------------------------------------------
// Mock surface
// ---------------------------------------------------------------------------

/// In-memory [`OverlaySurface`] that records calls into a shared log.
#[derive(Debug, Clone)]
pub struct MockSurface {
    inner: Arc<Mutex<MockSurfaceState>>,
}

#[derive(Debug)]
struct MockSurfaceState {
    visible: bool,
    monitor: ScreenRect<Logical>,
    dpi: f32,
    frames: Vec<OverlayFrame>,
}

impl MockSurface {
    /// Construct a [`MockSurface`] for `monitor` at the given DPI scale.
    #[must_use]
    pub fn new(monitor: ScreenRect<Logical>, dpi: f32) -> Self {
        Self {
            inner: Arc::new(Mutex::new(MockSurfaceState {
                visible: false,
                monitor,
                dpi,
                frames: Vec::new(),
            })),
        }
    }

    /// Snapshot of frames presented so far.
    ///
    /// # Panics
    /// Panics only if a previous holder of the inner `Mutex` panicked
    /// — a logic bug, not a runtime concern.
    #[must_use]
    pub fn frames(&self) -> Vec<OverlayFrame> {
        self.inner
            .lock()
            .expect("mock surface lock poisoned")
            .frames
            .clone()
    }

    /// Whether the surface is currently shown.
    ///
    /// # Panics
    /// Panics only if a previous holder of the inner `Mutex` panicked.
    #[must_use]
    pub fn is_visible(&self) -> bool {
        self.inner
            .lock()
            .expect("mock surface lock poisoned")
            .visible
    }
}

impl OverlaySurface for MockSurface {
    fn show(&mut self) -> Result<(), SurfaceError> {
        self.inner
            .lock()
            .expect("mock surface lock poisoned")
            .visible = true;
        Ok(())
    }

    fn hide(&mut self) -> Result<(), SurfaceError> {
        self.inner
            .lock()
            .expect("mock surface lock poisoned")
            .visible = false;
        Ok(())
    }

    fn present(&mut self, frame: &OverlayFrame) -> Result<(), SurfaceError> {
        self.inner
            .lock()
            .expect("mock surface lock poisoned")
            .frames
            .push(frame.clone());
        Ok(())
    }

    fn monitor(&self) -> ScreenRect<Logical> {
        self.inner
            .lock()
            .expect("mock surface lock poisoned")
            .monitor
    }

    fn dpi_scale(&self) -> f32 {
        self.inner.lock().expect("mock surface lock poisoned").dpi
    }
}

// ---------------------------------------------------------------------------
// Mock hotkey host
// ---------------------------------------------------------------------------

/// In-memory [`HotkeyHost`] that records every registered chord.
#[derive(Debug, Default)]
pub struct MockHotkeyHost {
    bindings: Vec<(String, Action)>,
}

impl MockHotkeyHost {
    /// Construct an empty mock host.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            bindings: Vec::new(),
        }
    }

    /// Inspect the registered bindings.
    #[must_use]
    pub fn bindings(&self) -> &[(String, Action)] {
        &self.bindings
    }
}

struct MockHotkeyRelease;
impl HotkeyRelease for MockHotkeyRelease {}

impl HotkeyHost for MockHotkeyHost {
    fn register(
        &mut self,
        chord: &str,
        action: Action,
        _sink: HotkeySink,
    ) -> Result<HotkeyToken, HotkeyError> {
        self.bindings.push((chord.to_owned(), action));
        Ok(HotkeyToken::new(Arc::new(MockHotkeyRelease)))
    }
}

// ---------------------------------------------------------------------------
// Mock mouse tracker
// ---------------------------------------------------------------------------

/// In-memory [`MouseTracker`] that returns whatever position the test pins.
#[derive(Debug, Clone)]
pub struct MockMouse {
    inner: Arc<Mutex<Point<Logical>>>,
}

impl MockMouse {
    /// Construct a mouse tracker fixed at `start`.
    #[must_use]
    pub fn new(start: Point<Logical>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(start)),
        }
    }

    /// Move the mouse to `point` (subsequent `position()` returns this).
    ///
    /// # Panics
    /// Panics only if a previous holder of the inner `Mutex` panicked.
    pub fn set(&self, point: Point<Logical>) {
        *self.inner.lock().expect("mock mouse lock poisoned") = point;
    }
}

impl MouseTracker for MockMouse {
    fn position(&self) -> Result<Point<Logical>, MouseError> {
        Ok(*self.inner.lock().expect("mock mouse lock poisoned"))
    }
}
