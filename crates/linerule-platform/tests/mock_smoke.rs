//! Smoke tests for the in-memory mock platform impls.
//!
//! Exercises the trait surface without any OS API. These run on every
//! target so the trait shape stays portable across v0.2 OS additions.
//!
//! Gated by `feature = "mock"` (always-on for non-Windows targets) so
//! the Windows native build does not pull in the mock module.

#![cfg(any(feature = "mock", not(target_os = "windows")))]

use linerule_core::{Action, Logical, OverlayFrame, Point, ScreenRect};
use linerule_platform::{
    HotkeyHost, HotkeySink, MouseTracker, OverlaySurface,
    mock::{MockHotkeyHost, MockMouse, MockSurface},
};

fn monitor() -> ScreenRect<Logical> {
    ScreenRect::new(Point::<Logical>::new(0, 0), 1920, 1080)
}

#[test]
fn surface_starts_hidden() {
    let s = MockSurface::new(monitor(), 1.0);
    assert!(!s.is_visible());
}

#[test]
fn surface_show_then_hide_round_trip() {
    let mut s = MockSurface::new(monitor(), 1.0);
    s.show().expect("show");
    assert!(s.is_visible());
    s.hide().expect("hide");
    assert!(!s.is_visible());
}

#[test]
fn surface_present_records_frames() {
    let mut s = MockSurface::new(monitor(), 1.0);
    s.present(&OverlayFrame::empty()).expect("present empty");
    assert_eq!(s.frames().len(), 1, "one present must record one frame");
}

#[test]
fn surface_dpi_scale_is_passed_through() {
    let s = MockSurface::new(monitor(), 1.5);
    assert!((s.dpi_scale() - 1.5).abs() < f32::EPSILON);
}

#[test]
fn hotkey_host_records_registration() {
    let (tx, _rx) = crossbeam_channel::bounded(8);
    let sink = HotkeySink::new(tx);
    let mut host = MockHotkeyHost::new();
    let _token = host
        .register("Ctrl+Alt+R", Action::CycleMode, sink)
        .expect("register");
    assert_eq!(host.bindings().len(), 1);
    assert_eq!(host.bindings()[0].0, "Ctrl+Alt+R");
    assert_eq!(host.bindings()[0].1, Action::CycleMode);
}

#[test]
fn mouse_tracker_returns_pinned_position() {
    let m = MockMouse::new(Point::<Logical>::new(100, 200));
    let p = m.position().expect("query position");
    assert_eq!(p.x, 100);
    assert_eq!(p.y, 200);

    m.set(Point::<Logical>::new(300, 400));
    let p = m.position().expect("query updated position");
    assert_eq!(p.x, 300);
    assert_eq!(p.y, 400);
}
