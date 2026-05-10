//! Edge cases for `render` — pathological inputs that should still
//! produce a valid `OverlayFrame` (no panic, all geometry inside the
//! supplied monitor). Catches integer overflow / clipping bugs.

use linerule_core::{
    Geometry, Logical, Mode, OverlayConfig, Point, Rgba, ScreenRect, Thickness, render,
};

fn cfg_with_thickness(t: u16) -> OverlayConfig {
    let mut c = OverlayConfig::default();
    c.thickness = Thickness::new(t).expect("thickness in 1..=512");
    c
}

fn assert_layers_inside_monitor(
    frame_layers: &[linerule_core::Layer],
    monitor: ScreenRect<Logical>,
) {
    for layer in frame_layers {
        let Geometry::Rect(bounds) = layer.geometry else {
            panic!("v0.1 emits only Rect geometry")
        };
        assert!(
            monitor.contains_rect(&bounds),
            "layer bounds {bounds:?} must lie inside monitor {monitor:?}",
        );
    }
}

const ALL_MODES: [Mode; 3] = [Mode::Off, Mode::HORIZONTAL_MASK, Mode::VERTICAL_MASK];

const MASK_MODES: [Mode; 2] = [Mode::HORIZONTAL_MASK, Mode::VERTICAL_MASK];

// ---------------------------------------------------------------------------
// cursor far outside the monitor
// ---------------------------------------------------------------------------

#[test]
fn cursor_far_above_monitor_clips_mask_to_top_edge() {
    let monitor = ScreenRect::new(Point::<Logical>::new(0, 0), 1920, 1080);
    let frame = render(
        Mode::HORIZONTAL_MASK,
        Point::<Logical>::new(960, -1_000_000),
        monitor,
        cfg_with_thickness(28),
    );
    assert_layers_inside_monitor(&frame.layers, monitor);
}

#[test]
fn cursor_far_below_monitor_clips_mask_to_bottom_edge() {
    let monitor = ScreenRect::new(Point::<Logical>::new(0, 0), 1920, 1080);
    let frame = render(
        Mode::HORIZONTAL_MASK,
        Point::<Logical>::new(960, i32::MAX),
        monitor,
        cfg_with_thickness(28),
    );
    assert_layers_inside_monitor(&frame.layers, monitor);
}

#[test]
fn cursor_at_int_min_does_not_panic() {
    let monitor = ScreenRect::new(Point::<Logical>::new(0, 0), 1920, 1080);
    for mode in MASK_MODES {
        let frame = render(
            mode,
            Point::<Logical>::new(i32::MIN, i32::MIN),
            monitor,
            OverlayConfig::default(),
        );
        assert_layers_inside_monitor(&frame.layers, monitor);
    }
}

#[test]
fn cursor_at_int_max_does_not_panic() {
    let monitor = ScreenRect::new(Point::<Logical>::new(0, 0), 1920, 1080);
    for mode in MASK_MODES {
        let frame = render(
            mode,
            Point::<Logical>::new(i32::MAX, i32::MAX),
            monitor,
            OverlayConfig::default(),
        );
        assert_layers_inside_monitor(&frame.layers, monitor);
    }
}

// ---------------------------------------------------------------------------
// monitors at non-zero origin (multi-monitor setups)
// ---------------------------------------------------------------------------

#[test]
fn mask_on_secondary_monitor_at_positive_origin() {
    let monitor = ScreenRect::new(Point::<Logical>::new(1920, 0), 1920, 1080);
    let frame = render(
        Mode::HORIZONTAL_MASK,
        Point::<Logical>::new(2880, 540),
        monitor,
        OverlayConfig::default(),
    );
    assert_layers_inside_monitor(&frame.layers, monitor);
}

#[test]
fn mask_on_monitor_at_negative_origin() {
    // Vertical-stacked monitors with the secondary at y = -1080.
    let monitor = ScreenRect::new(Point::<Logical>::new(0, -1080), 1920, 1080);
    let frame = render(
        Mode::HORIZONTAL_MASK,
        Point::<Logical>::new(960, -540),
        monitor,
        OverlayConfig::default(),
    );
    assert_layers_inside_monitor(&frame.layers, monitor);
}

// ---------------------------------------------------------------------------
// thickness boundary values
// ---------------------------------------------------------------------------

#[test]
fn mask_with_thickness_one_renders_thin_slit() {
    let monitor = ScreenRect::new(Point::<Logical>::new(0, 0), 1920, 1080);
    let frame = render(
        Mode::HORIZONTAL_MASK,
        Point::<Logical>::new(960, 540),
        monitor,
        cfg_with_thickness(1),
    );
    assert_layers_inside_monitor(&frame.layers, monitor);
    // top + bottom panels should sum to 1080 - 1 = 1079.
    let total_mask_height: u32 = frame
        .layers
        .iter()
        .map(|l| {
            let Geometry::Rect(r) = l.geometry else {
                unreachable!("v0.1 emits only Rect geometry")
            };
            r.height
        })
        .sum();
    assert_eq!(
        total_mask_height + 1,
        1080,
        "thickness=1 leaves a 1px slit in the otherwise full-height mask",
    );
}

#[test]
fn mask_with_max_thickness_clips_to_monitor() {
    let monitor = ScreenRect::new(Point::<Logical>::new(0, 0), 1920, 1080);
    let frame = render(
        Mode::HORIZONTAL_MASK,
        Point::<Logical>::new(960, 540),
        monitor,
        cfg_with_thickness(Thickness::MAX_PX),
    );
    assert_layers_inside_monitor(&frame.layers, monitor);
}

#[test]
fn mask_thicker_than_monitor_collapses_safely() {
    // Monitor smaller than maximum thickness — the mask should clip to
    // the monitor height instead of producing a rect outside it.
    let monitor = ScreenRect::new(Point::<Logical>::new(0, 0), 200, 100);
    let frame = render(
        Mode::HORIZONTAL_MASK,
        Point::<Logical>::new(50, 50),
        monitor,
        cfg_with_thickness(Thickness::MAX_PX),
    );
    assert_layers_inside_monitor(&frame.layers, monitor);
}

// ---------------------------------------------------------------------------
// degenerate monitors
// ---------------------------------------------------------------------------

#[test]
fn one_pixel_monitor_does_not_panic() {
    let monitor = ScreenRect::new(Point::<Logical>::new(0, 0), 1, 1);
    for mode in ALL_MODES {
        let frame = render(
            mode,
            Point::<Logical>::new(0, 0),
            monitor,
            OverlayConfig::default(),
        );
        assert_layers_inside_monitor(&frame.layers, monitor);
    }
}

#[test]
fn zero_width_monitor_does_not_panic() {
    // u32::width = 0 is degenerate but the API allows it.
    let monitor = ScreenRect::new(Point::<Logical>::new(0, 0), 0, 1080);
    for mode in ALL_MODES {
        let _frame = render(
            mode,
            Point::<Logical>::new(0, 540),
            monitor,
            OverlayConfig::default(),
        );
        // Output may be empty or zero-area; the contract is just "no panic".
    }
}

// ---------------------------------------------------------------------------
// mask invariants
// ---------------------------------------------------------------------------

#[test]
fn mask_two_layers_plus_slit_cover_monitor_height() {
    let monitor = ScreenRect::new(Point::<Logical>::new(0, 0), 1920, 1080);
    let cfg = cfg_with_thickness(50);
    let frame = render(
        Mode::HORIZONTAL_MASK,
        Point::<Logical>::new(960, 540),
        monitor,
        cfg,
    );
    assert_eq!(frame.layers.len(), 2);

    let mut tops: Vec<i32> = frame
        .layers
        .iter()
        .map(|l| match l.geometry {
            Geometry::Rect(r) => r.origin.y,
            _ => unreachable!(),
        })
        .collect();
    tops.sort_unstable();
    assert_eq!(tops[0], 0, "upper mask must start at monitor top");

    let total_mask_height: u32 = frame
        .layers
        .iter()
        .map(|l| {
            let Geometry::Rect(r) = l.geometry else {
                unreachable!("v0.1 emits only Rect geometry")
            };
            r.height
        })
        .sum();
    assert_eq!(
        total_mask_height + 50,
        1080,
        "two mask panels + slit thickness must equal monitor height",
    );
}

// ---------------------------------------------------------------------------
// colour fidelity
// ---------------------------------------------------------------------------

#[test]
fn mask_alpha_passes_through_from_cfg() {
    let mut cfg = OverlayConfig::default();
    cfg.mask_color = Rgba::new(10, 20, 30, 99);
    let frame = render(
        Mode::HORIZONTAL_MASK,
        Point::<Logical>::new(960, 540),
        ScreenRect::new(Point::<Logical>::new(0, 0), 1920, 1080),
        cfg,
    );
    for layer in &frame.layers {
        let linerule_core::Brush::Solid(fill) = layer.brush else {
            unreachable!("v0.1 emits only Solid brush")
        };
        assert_eq!(
            fill,
            Rgba::new(10, 20, 30, 99),
            "mask layer fill must match cfg.mask_color verbatim",
        );
    }
}
