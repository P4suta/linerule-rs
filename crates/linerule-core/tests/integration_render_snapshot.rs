//! Integration: golden snapshots of `frame()` output via `insta`.
//!
//! These act as a regression guard against silent behavioral drift in
//! the layer geometry. If anyone changes `split_around`, `band`, indicator
//! placement, or the perceptual byte conversion, the YAML diff is the
//! signal — `cargo insta accept` to confirm intentional changes.
//!
//! All snapshots use the settled sample (`OverlaySample::settled`), pinning
//! the invariant that transitions never change where a settled frame lands.

use linerule_core::{Mode, OverlayConfig, OverlayFrame, OverlaySample, Point, ScreenRect, frame};

const fn monitor() -> ScreenRect<linerule_core::Logical> {
    ScreenRect::new(Point::new(0, 0), 1920, 1080)
}

fn settled_frame(mode: Mode, cursor: Point<linerule_core::Logical>) -> OverlayFrame {
    let config = OverlayConfig::DEFAULT;
    frame(
        mode,
        config,
        cursor,
        monitor(),
        OverlaySample::settled(config),
    )
}

#[test]
fn snapshot_off_mode_empty() {
    let f = settled_frame(Mode::Off, Point::new(960, 540));
    insta::assert_debug_snapshot!(f);
}

#[test]
fn snapshot_horizontal_center() {
    let f = settled_frame(Mode::Horizontal, Point::new(960, 540));
    insta::assert_debug_snapshot!(f);
}

#[test]
fn snapshot_horizontal_top_edge() {
    let f = settled_frame(Mode::Horizontal, Point::new(960, 0));
    insta::assert_debug_snapshot!(f);
}

#[test]
fn snapshot_horizontal_bottom_edge() {
    let f = settled_frame(Mode::Horizontal, Point::new(960, 1080));
    insta::assert_debug_snapshot!(f);
}

#[test]
fn snapshot_vertical_center() {
    let f = settled_frame(Mode::Vertical, Point::new(960, 540));
    insta::assert_debug_snapshot!(f);
}

#[test]
fn snapshot_vertical_left_edge() {
    let f = settled_frame(Mode::Vertical, Point::new(0, 540));
    insta::assert_debug_snapshot!(f);
}

#[test]
fn snapshot_vertical_right_edge() {
    let f = settled_frame(Mode::Vertical, Point::new(1920, 540));
    insta::assert_debug_snapshot!(f);
}

#[test]
fn snapshot_horizontal_negative_cursor() {
    // Cursor sample arrived outside monitor bounds (rare but observed on
    // multi-monitor setups). The frame must still be well-formed.
    let f = settled_frame(Mode::Horizontal, Point::new(-50, -50));
    insta::assert_debug_snapshot!(f);
}
