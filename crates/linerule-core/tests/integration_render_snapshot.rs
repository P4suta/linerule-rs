//! Integration: golden snapshots of `frame()` output via `insta`.
//!
//! These act as a regression guard against silent behavioral drift in
//! the layer geometry. If anyone changes `split_around`, `band`, indicator
//! placement, or the perceptual byte conversion, the YAML diff is the
//! signal — `cargo insta accept` to confirm intentional changes.

use linerule_core::{Mode, OverlayConfig, Point, ScreenRect, State, SurroundEffect, frame};

const fn monitor() -> ScreenRect<linerule_core::Logical> {
    ScreenRect::new(Point::new(0, 0), 1920, 1080)
}

const fn state(mode: Mode) -> State {
    State {
        mode,
        config: OverlayConfig::DEFAULT,
        ..State::DEFAULT
    }
}

#[test]
fn snapshot_off_mode_empty() {
    let f = frame(state(Mode::Off), Point::new(960, 540), monitor());
    insta::assert_debug_snapshot!(f);
}

#[test]
fn snapshot_horizontal_center() {
    let f = frame(state(Mode::Horizontal), Point::new(960, 540), monitor());
    insta::assert_debug_snapshot!(f);
}

#[test]
fn snapshot_horizontal_top_edge() {
    let f = frame(state(Mode::Horizontal), Point::new(960, 0), monitor());
    insta::assert_debug_snapshot!(f);
}

#[test]
fn snapshot_horizontal_bottom_edge() {
    let f = frame(state(Mode::Horizontal), Point::new(960, 1080), monitor());
    insta::assert_debug_snapshot!(f);
}

#[test]
fn snapshot_vertical_center() {
    let f = frame(state(Mode::Vertical), Point::new(960, 540), monitor());
    insta::assert_debug_snapshot!(f);
}

#[test]
fn snapshot_vertical_left_edge() {
    let f = frame(state(Mode::Vertical), Point::new(0, 540), monitor());
    insta::assert_debug_snapshot!(f);
}

#[test]
fn snapshot_vertical_right_edge() {
    let f = frame(state(Mode::Vertical), Point::new(1920, 540), monitor());
    insta::assert_debug_snapshot!(f);
}

#[test]
fn snapshot_horizontal_negative_cursor() {
    // Cursor sample arrived outside monitor bounds (rare but observed on
    // multi-monitor setups). The frame must still be well-formed.
    let f = frame(state(Mode::Horizontal), Point::new(-50, -50), monitor());
    insta::assert_debug_snapshot!(f);
}

#[test]
fn snapshot_white_wash_horizontal_center() {
    // White-wash surround: dim halves carry white RGB and the indicator flips
    // to black for contrast. Geometry is identical to the dim-black case; only
    // the brush colors differ.
    let s = State {
        mode: Mode::Horizontal,
        config: OverlayConfig {
            effect: SurroundEffect::WhiteWash,
            ..OverlayConfig::DEFAULT
        },
        ..State::DEFAULT
    };
    let f = frame(s, Point::new(960, 540), monitor());
    insta::assert_debug_snapshot!(f);
}

#[test]
fn snapshot_blur_horizontal_center() {
    // Blur surround: the before/after bands carry a `Brush::Blur` (blurred
    // backdrop + tint); the indicator stays solid. Geometry matches dim-black.
    let s = State {
        mode: Mode::Horizontal,
        config: OverlayConfig {
            effect: SurroundEffect::Blur,
            ..OverlayConfig::DEFAULT
        },
        ..State::DEFAULT
    };
    let f = frame(s, Point::new(960, 540), monitor());
    insta::assert_debug_snapshot!(f);
}

#[test]
fn snapshot_hidden_state_emits_empty_even_in_horizontal() {
    let s = State {
        mode: Mode::Horizontal,
        visible: false,
        config: OverlayConfig::DEFAULT,
    };
    let f = frame(s, Point::new(960, 540), monitor());
    insta::assert_debug_snapshot!(f);
}
