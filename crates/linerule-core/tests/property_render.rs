//! `render` invariants. Once the Bar/Mask/Vertical impls land, every
//! assertion below should hold without exception. Until then the test
//! suite is in the canonical TDD red state — `just test` fails and
//! `just coverage` will not reach 100% on render.rs.

use linerule_core::{
    Brush, DimLevel, Geometry, Layer, Logical, Mode, Opacity, OverlayConfig, OverlayFrame, Point,
    Rgba, ScreenRect, Thickness, render,
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

// ---- Mode::Off ------------------------------------------------------------

#[test]
fn render_off_produces_no_layers() {
    let frame = render(Mode::Off, cursor(960, 540), monitor(), &cfg());
    assert_eq!(frame, OverlayFrame::empty(), "Off must produce zero layers");
}

// ---- Mode::Bar ------------------------------------------------------------

#[test]
fn render_bar_produces_exactly_one_layer() {
    let frame = render(Mode::Bar, cursor(960, 540), monitor(), &cfg());
    assert_eq!(
        frame.layers.len(),
        1,
        "Bar mode must emit exactly one layer"
    );
}

#[test]
fn render_bar_layer_spans_full_screen_width() {
    let frame = render(Mode::Bar, cursor(960, 540), monitor(), &cfg());
    let (bounds, _) = solid_layer_bounds(frame.layers.first().expect("Bar layer missing"));
    assert_eq!(bounds.width, 1920, "Bar must span the full monitor width");
}

#[test]
fn render_bar_layer_centered_on_cursor_y() {
    let cur_y = 540;
    let frame = render(Mode::Bar, cursor(960, cur_y), monitor(), &cfg());
    let (bounds, _) = solid_layer_bounds(frame.layers.first().expect("Bar layer missing"));
    let half_thickness = i32::from(Thickness::DEFAULT.get()) / 2;
    assert_eq!(
        bounds.origin.y,
        cur_y - half_thickness,
        "Bar must be centered on cursor Y",
    );
}

#[test]
fn render_bar_uses_configured_color() {
    let frame = render(Mode::Bar, cursor(960, 540), monitor(), &cfg());
    let (_, fill) = solid_layer_bounds(frame.layers.first().expect("Bar layer missing"));
    let mut expected = Rgba::DEFAULT_BAR;
    expected.a = Opacity::DEFAULT.get();
    assert_eq!(
        fill, expected,
        "bar fill must derive from cfg.bar_color + cfg.opacity"
    );
}

// ---- Mode::Mask -----------------------------------------------------------

#[test]
fn render_mask_produces_exactly_two_layers() {
    let frame = render(Mode::Mask, cursor(960, 540), monitor(), &cfg());
    assert_eq!(
        frame.layers.len(),
        2,
        "Mask mode must emit two layers (above + below the slit)",
    );
}

#[test]
fn render_mask_layers_use_configured_mask_color() {
    let frame = render(Mode::Mask, cursor(960, 540), monitor(), &cfg());
    for layer in &frame.layers {
        let (_, fill) = solid_layer_bounds(layer);
        assert_eq!(fill, Rgba::DEFAULT_MASK, "mask layers must use mask_color");
    }
    let _ = DimLevel::DEFAULT; // referenced for forward-compat split-channel work
}

#[test]
fn render_mask_slit_is_thickness_pixels_high() {
    let cur_y = 540;
    let frame = render(Mode::Mask, cursor(960, cur_y), monitor(), &cfg());
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
        "slit height must match Thickness::DEFAULT",
    );
}

// ---- Mode::Vertical -------------------------------------------------------

#[test]
fn render_vertical_produces_exactly_one_layer() {
    let frame = render(Mode::Vertical, cursor(960, 540), monitor(), &cfg());
    assert_eq!(
        frame.layers.len(),
        1,
        "Vertical mode must emit exactly one layer"
    );
}

#[test]
fn render_vertical_layer_spans_full_screen_height() {
    let frame = render(Mode::Vertical, cursor(960, 540), monitor(), &cfg());
    let (bounds, _) = solid_layer_bounds(frame.layers.first().expect("Vertical layer missing"));
    assert_eq!(
        bounds.height, 1080,
        "Vertical must span the full monitor height"
    );
}

#[test]
fn render_vertical_layer_centered_on_cursor_x() {
    let cur_x = 960;
    let frame = render(Mode::Vertical, cursor(cur_x, 540), monitor(), &cfg());
    let (bounds, _) = solid_layer_bounds(frame.layers.first().expect("Vertical layer missing"));
    let half_thickness = i32::from(Thickness::DEFAULT.get()) / 2;
    assert_eq!(
        bounds.origin.x,
        cur_x - half_thickness,
        "Vertical must be centered on cursor X",
    );
}

// ---- Mode::VerticalMask ---------------------------------------------------

#[test]
fn render_vertical_mask_produces_exactly_two_layers() {
    let frame = render(Mode::VerticalMask, cursor(960, 540), monitor(), &cfg());
    assert_eq!(
        frame.layers.len(),
        2,
        "VerticalMask must emit two layers (left + right of the slit)",
    );
}

#[test]
fn render_vertical_mask_layers_use_configured_mask_color() {
    let frame = render(Mode::VerticalMask, cursor(960, 540), monitor(), &cfg());
    for layer in &frame.layers {
        let (_, fill) = solid_layer_bounds(layer);
        assert_eq!(
            fill,
            Rgba::DEFAULT_MASK,
            "vertical-mask layers use mask_color"
        );
    }
}

#[test]
fn render_vertical_mask_slit_is_thickness_pixels_wide() {
    let cur_x = 960;
    let frame = render(Mode::VerticalMask, cursor(cur_x, 540), monitor(), &cfg());
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

// ---- Cross-mode invariants ------------------------------------------------

#[test]
fn render_layers_always_lie_within_monitor() {
    let m = monitor();
    for mode in [
        Mode::Off,
        Mode::Bar,
        Mode::Mask,
        Mode::Vertical,
        Mode::VerticalMask,
    ] {
        for &(x, y) in &[(0, 0), (1919, 1079), (960, 540), (1, 1080), (1920, 0)] {
            let frame = render(mode, cursor(x, y), m, &cfg());
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
fn render_uses_thickness_and_opacity_from_cfg() {
    let mut cfg = OverlayConfig::default();
    cfg.thickness = Thickness::new(40).expect("40 is in range");
    cfg.opacity = Opacity::new(200).expect("200 is in range");
    let frame = render(Mode::Bar, cursor(960, 540), monitor(), &cfg);
    let (bounds, fill) = solid_layer_bounds(frame.layers.first().expect("Bar layer missing"));
    assert_eq!(bounds.height, 40, "thickness must drive bar height");
    assert_eq!(fill.a, 200, "render must replace alpha with cfg.opacity");
}
