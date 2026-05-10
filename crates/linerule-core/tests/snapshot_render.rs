//! `insta` snapshot tests for `render` outputs.
//!
//! Pins the exact `OverlayFrame` structure produced by every Mode at
//! canonical inputs so that any future refactor must EITHER reproduce
//! the same output OR be reviewed against the snapshot diff via
//! `cargo insta review`. Catches geometry / brush / colour drift.

use insta::{assert_yaml_snapshot, with_settings};
use linerule_core::{Logical, Mode, OverlayConfig, Point, ScreenRect, Thickness, render};

fn fhd() -> ScreenRect<Logical> {
    ScreenRect::new(Point::<Logical>::new(0, 0), 1920, 1080)
}

fn cfg() -> OverlayConfig {
    OverlayConfig::default()
}

fn cur(x: i32, y: i32) -> Point<Logical> {
    Point::<Logical>::new(x, y)
}

#[test]
fn snapshot_off_at_centre() {
    let frame = render(Mode::Off, cur(960, 540), fhd(), &cfg());
    with_settings!({ description => "Off mode emits zero layers" }, {
        assert_yaml_snapshot!(frame);
    });
}

#[test]
fn snapshot_bar_at_centre_fhd() {
    let frame = render(Mode::Bar, cur(960, 540), fhd(), &cfg());
    with_settings!({ description => "Bar at FHD centre with default thickness/colour" }, {
        assert_yaml_snapshot!(frame);
    });
}

#[test]
fn snapshot_bar_at_top_left() {
    let frame = render(Mode::Bar, cur(0, 0), fhd(), &cfg());
    with_settings!({ description => "Bar at (0,0) — clip prevents top of bar going negative" }, {
        assert_yaml_snapshot!(frame);
    });
}

#[test]
fn snapshot_bar_at_bottom_right() {
    let frame = render(Mode::Bar, cur(1919, 1079), fhd(), &cfg());
    with_settings!({ description => "Bar at FHD far corner — clip prevents bar overshooting bottom" }, {
        assert_yaml_snapshot!(frame);
    });
}

#[test]
fn snapshot_mask_at_centre_fhd() {
    let frame = render(Mode::Mask, cur(960, 540), fhd(), &cfg());
    with_settings!({ description => "Mask centred — top + bottom panels with default mask colour" }, {
        assert_yaml_snapshot!(frame);
    });
}

#[test]
fn snapshot_mask_at_top() {
    let frame = render(Mode::Mask, cur(960, 0), fhd(), &cfg());
    with_settings!({ description => "Mask near top — top panel collapses to ~0 height" }, {
        assert_yaml_snapshot!(frame);
    });
}

#[test]
fn snapshot_vertical_at_centre_fhd() {
    let frame = render(Mode::Vertical, cur(960, 540), fhd(), &cfg());
    with_settings!({ description => "Vertical at FHD centre — full-height bar at cursor X" }, {
        assert_yaml_snapshot!(frame);
    });
}

#[test]
fn snapshot_vertical_mask_at_centre_fhd() {
    let frame = render(Mode::VerticalMask, cur(960, 540), fhd(), &cfg());
    with_settings!({ description => "VerticalMask centred — left + right panels with default mask colour, transparent vertical slit at cursor X (縦書き typoscope)" }, {
        assert_yaml_snapshot!(frame);
    });
}

#[test]
fn snapshot_vertical_mask_at_left_edge() {
    let frame = render(Mode::VerticalMask, cur(0, 540), fhd(), &cfg());
    with_settings!({ description => "VerticalMask near left edge — left panel collapses to ~0 width" }, {
        assert_yaml_snapshot!(frame);
    });
}

#[test]
fn snapshot_bar_with_max_thickness() {
    let mut cfg = OverlayConfig::default();
    cfg.thickness = Thickness::new(Thickness::MAX_PX).expect("MAX_PX is valid");
    let frame = render(Mode::Bar, cur(960, 540), fhd(), &cfg);
    with_settings!({ description => "Bar at MAX_PX thickness — bounds clipped within monitor" }, {
        assert_yaml_snapshot!(frame);
    });
}

#[test]
fn snapshot_bar_on_offset_monitor() {
    // Real multi-monitor: secondary screen positioned at (1920, 0)
    let monitor = ScreenRect::new(Point::<Logical>::new(1920, 0), 1920, 1080);
    let frame = render(Mode::Bar, cur(2880, 540), monitor, &cfg());
    with_settings!({ description => "Bar on monitor with non-zero origin — bounds offset accordingly" }, {
        assert_yaml_snapshot!(frame);
    });
}
