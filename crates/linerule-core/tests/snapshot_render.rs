//! `insta` snapshot tests for `render` outputs.
//!
//! Pins the exact `OverlayFrame` structure produced by every Mode at
//! canonical inputs so that any future refactor must EITHER reproduce
//! the same output OR be reviewed against the snapshot diff via
//! `cargo insta review`. Catches geometry / brush / colour drift.
//!
//! Naming convention: `snapshot_<orientation>_<position>` so the file
//! listing reads as the same axis the type encodes.

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
    let frame = render(Mode::Off, cur(960, 540), fhd(), cfg());
    with_settings!({ description => "Off mode emits zero layers" }, {
        assert_yaml_snapshot!(frame);
    });
}

// ---- Horizontal mask ------------------------------------------------------

#[test]
fn snapshot_horizontal_mask_at_centre_fhd() {
    let frame = render(Mode::HORIZONTAL_MASK, cur(960, 540), fhd(), cfg());
    with_settings!({ description => "Horizontal mask centred — top + bottom panels with default mask colour" }, {
        assert_yaml_snapshot!(frame);
    });
}

#[test]
fn snapshot_horizontal_mask_at_top_edge() {
    let frame = render(Mode::HORIZONTAL_MASK, cur(960, 0), fhd(), cfg());
    with_settings!({ description => "Horizontal mask near top — top panel collapses to ~0 height" }, {
        assert_yaml_snapshot!(frame);
    });
}

#[test]
fn snapshot_horizontal_mask_with_max_thickness() {
    let mut cfg = OverlayConfig::default();
    cfg.thickness = Thickness::new(Thickness::MAX_PX).expect("MAX_PX is valid");
    let frame = render(Mode::HORIZONTAL_MASK, cur(960, 540), fhd(), cfg);
    with_settings!({ description => "Horizontal mask with MAX_PX slit thickness — panels shrink to half-monitor minus MAX_PX/2" }, {
        assert_yaml_snapshot!(frame);
    });
}

#[test]
fn snapshot_horizontal_mask_on_offset_monitor() {
    // Real multi-monitor: secondary screen positioned at (1920, 0)
    let monitor = ScreenRect::new(Point::<Logical>::new(1920, 0), 1920, 1080);
    let frame = render(Mode::HORIZONTAL_MASK, cur(2880, 540), monitor, cfg());
    with_settings!({ description => "Horizontal mask on monitor with non-zero origin — bounds offset accordingly" }, {
        assert_yaml_snapshot!(frame);
    });
}

// ---- Vertical mask --------------------------------------------------------

#[test]
fn snapshot_vertical_mask_at_centre_fhd() {
    let frame = render(Mode::VERTICAL_MASK, cur(960, 540), fhd(), cfg());
    with_settings!({ description => "Vertical mask centred — left + right panels with default mask colour, transparent vertical slit at cursor X (縦書き typoscope)" }, {
        assert_yaml_snapshot!(frame);
    });
}

#[test]
fn snapshot_vertical_mask_at_left_edge() {
    let frame = render(Mode::VERTICAL_MASK, cur(0, 540), fhd(), cfg());
    with_settings!({ description => "Vertical mask near left edge — left panel collapses to ~0 width" }, {
        assert_yaml_snapshot!(frame);
    });
}
