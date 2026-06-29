//! Golden `insta` snapshots of `frame()` output; guards layer-geometry drift.
//! All use the settled sample, pinning that transitions don't move a settled frame.

use linerule_core::{
    Mode, OverlayConfig, OverlayFrame, OverlaySample, Point, ScreenRect, SurroundEffect, frame,
};

const fn monitor() -> ScreenRect<linerule_core::Logical> {
    ScreenRect::new(Point::new(0, 0), 1920, 1080)
}

fn settled_frame(mode: Mode, cursor: Point<linerule_core::Logical>) -> OverlayFrame {
    settled_frame_with(OverlayConfig::DEFAULT, mode, cursor)
}

fn settled_frame_with(
    config: OverlayConfig,
    mode: Mode,
    cursor: Point<linerule_core::Logical>,
) -> OverlayFrame {
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
    // Cursor outside monitor bounds (seen on multi-monitor); frame must stay well-formed.
    let f = settled_frame(Mode::Horizontal, Point::new(-50, -50));
    insta::assert_debug_snapshot!(f);
}

#[test]
fn snapshot_white_wash_horizontal_center() {
    // White-wash: dim halves carry white RGB; geometry same as dim-black.
    let config = OverlayConfig {
        effect: SurroundEffect::WhiteWash,
        ..OverlayConfig::DEFAULT
    };
    let f = settled_frame_with(config, Mode::Horizontal, Point::new(960, 540));
    insta::assert_debug_snapshot!(f);
}

#[test]
fn snapshot_blur_horizontal_center() {
    // Blur: bands carry `Brush::Blur` (no veil); geometry matches dim-black.
    let config = OverlayConfig {
        effect: SurroundEffect::Blur,
        ..OverlayConfig::DEFAULT
    };
    let f = settled_frame_with(config, Mode::Horizontal, Point::new(960, 540));
    insta::assert_debug_snapshot!(f);
}
