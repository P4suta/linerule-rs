//! `render` invariants. Both `Mask(_)` orientations share a single
//! axis-symmetric pipeline (project → slit → lift → paint), so most
//! invariants are stated *generically* over `Orientation`.

use linerule_core::{
    Brush, Geometry, Layer, Logical, Mode, OverlayConfig, OverlayFrame, Point, Rgba, ScreenRect,
    Thickness, render,
};

fn monitor() -> ScreenRect<Logical> {
    ScreenRect::new(Point::<Logical>::new(0, 0), 1920, 1080)
}

fn cursor(x: i32, y: i32) -> Point<Logical> {
    Point::<Logical>::new(x, y)
}

fn cfg() -> OverlayConfig {
    OverlayConfig::default()
}

fn solid_layer_bounds(layer: &Layer) -> (ScreenRect<Logical>, Rgba) {
    match (layer.geometry, layer.brush) {
        (Geometry::Rect(b), Brush::Solid(c)) => (b, c),
        // Geometry / Brush are `#[non_exhaustive]`; future variants
        // (paths, gradients) cannot be flattened to (rect, color).
        _ => panic!("v0.1 render path emits only solid-fill rectangles"),
    }
}

const ALL_MODES: [Mode; 3] = [Mode::Off, Mode::HORIZONTAL_MASK, Mode::VERTICAL_MASK];

const MASK_MODES: [Mode; 2] = [Mode::HORIZONTAL_MASK, Mode::VERTICAL_MASK];

// ---- Mode::Off ------------------------------------------------------------

#[test]
fn render_off_produces_no_layers() {
    let frame = render(Mode::Off, cursor(960, 540), monitor(), cfg());
    assert_eq!(frame, OverlayFrame::empty(), "Off must produce zero layers");
}

// ---- Mask(_) — symmetric across orientations ------------------------------

#[test]
fn render_mask_produces_exactly_two_layers_in_either_orientation() {
    for mode in MASK_MODES {
        let frame = render(mode, cursor(960, 540), monitor(), cfg());
        assert_eq!(
            frame.layers.len(),
            2,
            "{mode:?} must emit two layers (slit complement)",
        );
    }
}

#[test]
fn render_mask_layers_use_configured_mask_color_in_either_orientation() {
    for mode in MASK_MODES {
        let frame = render(mode, cursor(960, 540), monitor(), cfg());
        for layer in &frame.layers {
            let (_, fill) = solid_layer_bounds(layer);
            assert_eq!(
                fill,
                Rgba::DEFAULT_MASK,
                "{mode:?}: mask layers must use mask_color",
            );
        }
    }
}

#[test]
fn render_horizontal_mask_slit_is_thickness_pixels_high() {
    let cur_y = 540;
    let frame = render(Mode::HORIZONTAL_MASK, cursor(960, cur_y), monitor(), cfg());
    let mut layers = frame.layers.iter();
    let first = layers.next().expect("mask top layer missing");
    let second = layers.next().expect("mask bottom layer missing");
    let (b1, _) = solid_layer_bounds(first);
    let (b2, _) = solid_layer_bounds(second);
    let (top, bot) = if b1.origin.y < b2.origin.y {
        (b1, b2)
    } else {
        (b2, b1)
    };
    let slit_top = top.origin.y.saturating_add_unsigned(top.height);
    let slit_bot = bot.origin.y;
    assert_eq!(
        slit_bot - slit_top,
        i32::from(Thickness::DEFAULT.get()),
        "horizontal slit height must match Thickness::DEFAULT",
    );
}

#[test]
fn render_vertical_mask_slit_is_thickness_pixels_wide() {
    let cur_x = 960;
    let frame = render(Mode::VERTICAL_MASK, cursor(cur_x, 540), monitor(), cfg());
    let mut layers = frame.layers.iter();
    let first = layers.next().expect("vertical-mask left layer missing");
    let second = layers.next().expect("vertical-mask right layer missing");
    let (b1, _) = solid_layer_bounds(first);
    let (b2, _) = solid_layer_bounds(second);
    let (left, right) = if b1.origin.x < b2.origin.x {
        (b1, b2)
    } else {
        (b2, b1)
    };
    let slit_left = left.origin.x.saturating_add_unsigned(left.width);
    let slit_right = right.origin.x;
    assert_eq!(
        slit_right - slit_left,
        i32::from(Thickness::DEFAULT.get()),
        "vertical slit width must match Thickness::DEFAULT",
    );
}

#[test]
fn render_horizontal_mask_layers_span_full_screen_width() {
    let frame = render(Mode::HORIZONTAL_MASK, cursor(960, 540), monitor(), cfg());
    for layer in &frame.layers {
        let (b, _) = solid_layer_bounds(layer);
        assert_eq!(
            b.width, 1920,
            "horizontal mask panel must span monitor width"
        );
    }
}

#[test]
fn render_vertical_mask_layers_span_full_screen_height() {
    let frame = render(Mode::VERTICAL_MASK, cursor(960, 540), monitor(), cfg());
    for layer in &frame.layers {
        let (b, _) = solid_layer_bounds(layer);
        assert_eq!(
            b.height, 1080,
            "vertical mask panel must span monitor height",
        );
    }
}

// ---- Cross-mode invariants ------------------------------------------------

#[test]
fn render_layers_always_lie_within_monitor() {
    let m = monitor();
    for mode in ALL_MODES {
        for &(x, y) in &[(0, 0), (1919, 1079), (960, 540), (1, 1080), (1920, 0)] {
            let frame = render(mode, cursor(x, y), m, cfg());
            for layer in &frame.layers {
                let (bounds, _) = solid_layer_bounds(layer);
                assert!(
                    m.contains_rect(&bounds),
                    "{mode:?} at cursor ({x},{y}): bounds {bounds:?} escape monitor {m:?}",
                );
            }
        }
    }
}

#[test]
fn render_uses_thickness_from_cfg() {
    let mut cfg = OverlayConfig::default();
    cfg.thickness = Thickness::new(40).expect("40 is in range");
    let frame = render(Mode::HORIZONTAL_MASK, cursor(960, 540), monitor(), cfg);
    let mut layers = frame.layers.iter();
    let first = layers.next().expect("mask top layer missing");
    let second = layers.next().expect("mask bottom layer missing");
    let (b1, _) = solid_layer_bounds(first);
    let (b2, _) = solid_layer_bounds(second);
    let (top, bot) = if b1.origin.y < b2.origin.y {
        (b1, b2)
    } else {
        (b2, b1)
    };
    let slit_top = top.origin.y.saturating_add_unsigned(top.height);
    let slit_bot = bot.origin.y;
    assert_eq!(slit_bot - slit_top, 40, "thickness must drive slit height");
}

#[test]
fn render_uses_mask_color_from_cfg() {
    let mut cfg = OverlayConfig::default();
    cfg.mask_color = Rgba::new(10, 20, 30, 200);
    let frame = render(Mode::HORIZONTAL_MASK, cursor(960, 540), monitor(), cfg);
    for layer in &frame.layers {
        let (_, fill) = solid_layer_bounds(layer);
        assert_eq!(fill, cfg.mask_color);
    }
}
