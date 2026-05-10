//! Edge cases for `render` — pathological inputs that should still
//! produce a valid `OverlayFrame` (no panic, all geometry inside the
//! supplied monitor). Catches integer overflow / clipping bugs.

use linerule_core::{
    Brush, Geometry, Logical, Mode, OverlayConfig, Point, Rgba, ScreenRect, Thickness, render,
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

// ---------------------------------------------------------------------------
// cursor far outside the monitor
// ---------------------------------------------------------------------------

#[test]
fn cursor_far_above_monitor_clips_bar_to_top_edge() {
    let monitor = ScreenRect::new(Point::<Logical>::new(0, 0), 1920, 1080);
    let frame = render(
        Mode::BAR,
        Point::<Logical>::new(960, -1_000_000),
        monitor,
        &cfg_with_thickness(28),
    );
    assert_layers_inside_monitor(&frame.layers, monitor);
    // The bar may end up zero-height once fully clipped above the
    // monitor — the assertion is "no panic, no escape", not "non-empty".
}

#[test]
fn cursor_far_below_monitor_clips_bar_to_bottom_edge() {
    let monitor = ScreenRect::new(Point::<Logical>::new(0, 0), 1920, 1080);
    let frame = render(
        Mode::BAR,
        Point::<Logical>::new(960, i32::MAX),
        monitor,
        &cfg_with_thickness(28),
    );
    assert_layers_inside_monitor(&frame.layers, monitor);
}

#[test]
fn cursor_at_int_min_does_not_panic() {
    let monitor = ScreenRect::new(Point::<Logical>::new(0, 0), 1920, 1080);
    for mode in [
        Mode::BAR,
        Mode::MASK,
        Mode::VERTICAL_BAR,
        Mode::VERTICAL_MASK,
    ] {
        let frame = render(
            mode,
            Point::<Logical>::new(i32::MIN, i32::MIN),
            monitor,
            &OverlayConfig::default(),
        );
        assert_layers_inside_monitor(&frame.layers, monitor);
    }
}

#[test]
fn cursor_at_int_max_does_not_panic() {
    let monitor = ScreenRect::new(Point::<Logical>::new(0, 0), 1920, 1080);
    for mode in [
        Mode::BAR,
        Mode::MASK,
        Mode::VERTICAL_BAR,
        Mode::VERTICAL_MASK,
    ] {
        let frame = render(
            mode,
            Point::<Logical>::new(i32::MAX, i32::MAX),
            monitor,
            &OverlayConfig::default(),
        );
        assert_layers_inside_monitor(&frame.layers, monitor);
    }
}

// ---------------------------------------------------------------------------
// monitors at non-zero origin (multi-monitor setups)
// ---------------------------------------------------------------------------

#[test]
fn bar_on_secondary_monitor_at_positive_origin() {
    let monitor = ScreenRect::new(Point::<Logical>::new(1920, 0), 1920, 1080);
    let frame = render(
        Mode::BAR,
        Point::<Logical>::new(2880, 540),
        monitor,
        &OverlayConfig::default(),
    );
    assert_layers_inside_monitor(&frame.layers, monitor);
    let layer = frame.layers.first().expect("Bar mode emits one layer");
    let Geometry::Rect(bounds) = layer.geometry else {
        unreachable!("v0.1 emits only Rect geometry");
    };
    assert_eq!(
        bounds.origin.x, 1920,
        "Bar on offset monitor must start at monitor X"
    );
    assert_eq!(bounds.width, 1920);
}

#[test]
fn bar_on_monitor_at_negative_origin() {
    // Vertical-stacked monitors with the secondary at y = -1080.
    let monitor = ScreenRect::new(Point::<Logical>::new(0, -1080), 1920, 1080);
    let frame = render(
        Mode::BAR,
        Point::<Logical>::new(960, -540),
        monitor,
        &OverlayConfig::default(),
    );
    assert_layers_inside_monitor(&frame.layers, monitor);
}

// ---------------------------------------------------------------------------
// thickness boundary values
// ---------------------------------------------------------------------------

#[test]
fn bar_with_thickness_one_renders_thin_line() {
    let monitor = ScreenRect::new(Point::<Logical>::new(0, 0), 1920, 1080);
    let frame = render(
        Mode::BAR,
        Point::<Logical>::new(960, 540),
        monitor,
        &cfg_with_thickness(1),
    );
    let Geometry::Rect(bounds) = frame.layers.first().unwrap().geometry else {
        unreachable!("v0.1 emits only Rect geometry");
    };
    assert_eq!(
        bounds.height, 1,
        "thickness=1 must produce 1 logical px tall bar"
    );
}

#[test]
fn bar_with_max_thickness_clips_to_monitor() {
    let monitor = ScreenRect::new(Point::<Logical>::new(0, 0), 1920, 1080);
    let frame = render(
        Mode::BAR,
        Point::<Logical>::new(960, 540),
        monitor,
        &cfg_with_thickness(Thickness::MAX_PX),
    );
    assert_layers_inside_monitor(&frame.layers, monitor);
}

#[test]
fn bar_thicker_than_monitor_collapses_safely() {
    // Monitor smaller than maximum thickness — the bar should clip to
    // the monitor height instead of producing a rect outside it.
    let monitor = ScreenRect::new(Point::<Logical>::new(0, 0), 200, 100);
    let frame = render(
        Mode::BAR,
        Point::<Logical>::new(50, 50),
        monitor,
        &cfg_with_thickness(Thickness::MAX_PX),
    );
    assert_layers_inside_monitor(&frame.layers, monitor);
    let Geometry::Rect(bounds) = frame.layers.first().unwrap().geometry else {
        unreachable!("v0.1 emits only Rect geometry");
    };
    assert!(
        bounds.height <= 100,
        "bar height ({}) must not exceed monitor height (100)",
        bounds.height,
    );
}

// ---------------------------------------------------------------------------
// degenerate monitors
// ---------------------------------------------------------------------------

#[test]
fn one_pixel_monitor_does_not_panic() {
    let monitor = ScreenRect::new(Point::<Logical>::new(0, 0), 1, 1);
    for mode in [
        Mode::Off,
        Mode::BAR,
        Mode::MASK,
        Mode::VERTICAL_BAR,
        Mode::VERTICAL_MASK,
    ] {
        let frame = render(
            mode,
            Point::<Logical>::new(0, 0),
            monitor,
            &OverlayConfig::default(),
        );
        assert_layers_inside_monitor(&frame.layers, monitor);
    }
}

#[test]
fn zero_width_monitor_does_not_panic() {
    // u32::width = 0 is degenerate but the API allows it.
    let monitor = ScreenRect::new(Point::<Logical>::new(0, 0), 0, 1080);
    for mode in [
        Mode::Off,
        Mode::BAR,
        Mode::MASK,
        Mode::VERTICAL_BAR,
        Mode::VERTICAL_MASK,
    ] {
        let _frame = render(
            mode,
            Point::<Logical>::new(0, 540),
            monitor,
            &OverlayConfig::default(),
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
    let frame = render(Mode::MASK, Point::<Logical>::new(960, 540), monitor, &cfg);
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
    // top of upper rect == monitor top (0), top of lower rect == slit_bot
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
fn bar_alpha_overrides_to_cfg_opacity() {
    let mut cfg = OverlayConfig::default();
    cfg.bar_color = Rgba::new(10, 20, 30, 99);
    cfg.opacity = linerule_core::Opacity::new(200).unwrap();
    let frame = render(
        Mode::BAR,
        Point::<Logical>::new(960, 540),
        ScreenRect::new(Point::<Logical>::new(0, 0), 1920, 1080),
        &cfg,
    );
    let Brush::Solid(fill) = frame.layers.first().unwrap().brush else {
        unreachable!("v0.1 emits only Solid brush")
    };
    assert_eq!(fill.r, 10);
    assert_eq!(fill.g, 20);
    assert_eq!(fill.b, 30);
    assert_eq!(
        fill.a, 200,
        "render must override bar alpha with cfg.opacity"
    );
}
