//! `insta` snapshot tests for `render` outputs.
//!
//! Pins the exact `OverlayFrame` structure produced by every Mode at
//! canonical inputs so that any future refactor must EITHER reproduce
//! the same output OR be reviewed against the snapshot diff via
//! `cargo insta review`. Catches geometry / brush / colour drift.
//!
//! Naming convention: `snapshot_<orientation>_<shape>_<position>` so
//! the file listing reads as a 2-axis grid (cf. ADR-0012).

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

// ---- Active(Bar, Horizontal) ----------------------------------------------

#[test]
fn snapshot_horizontal_bar_at_centre_fhd() {
    let frame = render(Mode::BAR, cur(960, 540), fhd(), &cfg());
    with_settings!({ description => "Horizontal bar at FHD centre with default thickness/colour" }, {
        assert_yaml_snapshot!(frame);
    });
}

#[test]
fn snapshot_horizontal_bar_at_top_left() {
    let frame = render(Mode::BAR, cur(0, 0), fhd(), &cfg());
    with_settings!({ description => "Horizontal bar at (0,0) — clip prevents top of bar going negative" }, {
        assert_yaml_snapshot!(frame);
    });
}

#[test]
fn snapshot_horizontal_bar_at_bottom_right() {
    let frame = render(Mode::BAR, cur(1919, 1079), fhd(), &cfg());
    with_settings!({ description => "Horizontal bar at FHD far corner — clip prevents bar overshooting bottom" }, {
        assert_yaml_snapshot!(frame);
    });
}

#[test]
fn snapshot_horizontal_bar_with_max_thickness() {
    let mut cfg = OverlayConfig::default();
    cfg.thickness = Thickness::new(Thickness::MAX_PX).expect("MAX_PX is valid");
    let frame = render(Mode::BAR, cur(960, 540), fhd(), &cfg);
    with_settings!({ description => "Horizontal bar at MAX_PX thickness — bounds clipped within monitor" }, {
        assert_yaml_snapshot!(frame);
    });
}

#[test]
fn snapshot_horizontal_bar_on_offset_monitor() {
    // Real multi-monitor: secondary screen positioned at (1920, 0)
    let monitor = ScreenRect::new(Point::<Logical>::new(1920, 0), 1920, 1080);
    let frame = render(Mode::BAR, cur(2880, 540), monitor, &cfg());
    with_settings!({ description => "Horizontal bar on monitor with non-zero origin — bounds offset accordingly" }, {
        assert_yaml_snapshot!(frame);
    });
}

// ---- Active(Mask, Horizontal) ---------------------------------------------

#[test]
fn snapshot_horizontal_mask_at_centre_fhd() {
    let frame = render(Mode::MASK, cur(960, 540), fhd(), &cfg());
    with_settings!({ description => "Horizontal mask centred — top + bottom panels with default mask colour" }, {
        assert_yaml_snapshot!(frame);
    });
}

#[test]
fn snapshot_horizontal_mask_at_top_edge() {
    let frame = render(Mode::MASK, cur(960, 0), fhd(), &cfg());
    with_settings!({ description => "Horizontal mask near top — top panel collapses to ~0 height" }, {
        assert_yaml_snapshot!(frame);
    });
}

// ---- Active(Bar, Vertical) ------------------------------------------------

#[test]
fn snapshot_vertical_bar_at_centre_fhd() {
    let frame = render(Mode::VERTICAL_BAR, cur(960, 540), fhd(), &cfg());
    with_settings!({ description => "Vertical bar at FHD centre — full-height bar at cursor X" }, {
        assert_yaml_snapshot!(frame);
    });
}

// ---- Active(Mask, Vertical) -----------------------------------------------

#[test]
fn snapshot_vertical_mask_at_centre_fhd() {
    let frame = render(Mode::VERTICAL_MASK, cur(960, 540), fhd(), &cfg());
    with_settings!({ description => "Vertical mask centred — left + right panels with default mask colour, transparent vertical slit at cursor X (縦書き typoscope)" }, {
        assert_yaml_snapshot!(frame);
    });
}

#[test]
fn snapshot_vertical_mask_at_left_edge() {
    let frame = render(Mode::VERTICAL_MASK, cur(0, 540), fhd(), &cfg());
    with_settings!({ description => "Vertical mask near left edge — left panel collapses to ~0 width" }, {
        assert_yaml_snapshot!(frame);
    });
}
