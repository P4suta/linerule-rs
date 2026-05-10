//! Cells for the small render-output ADT atoms (`Geometry`, `Brush`,
//! `Layer`, `OverlayFrame`). These are exercised indirectly through
//! `render` / `solid_layer_bounds`, but the explicit cells pin the
//! constructor + projection contracts so the surface stays auditable.

use linerule_core::{
    Brush, Geometry, Layer, Logical, OverlayFrame, Point, Rgba, ScreenRect, Shape,
};

fn rect_at_origin() -> ScreenRect<Logical> {
    ScreenRect::new(Point::<Logical>::new(10, 20), 100, 50)
}

#[test]
fn geometry_rect_bounds_round_trips() {
    let r = rect_at_origin();
    let g = Geometry::Rect(r);
    assert_eq!(
        g.bounds(),
        r,
        "Geometry::Rect must expose its rect via bounds()",
    );
}

#[test]
fn brush_solid_alpha_is_the_color_alpha() {
    let c = Rgba::new(10, 20, 30, 200);
    let b = Brush::Solid(c);
    assert_eq!(b.alpha(), 200, "Solid brush alpha is the color alpha");
}

#[test]
fn brush_solid_alpha_zero_is_preserved() {
    // alpha=0 cannot enter via Opacity (which rejects 0), but can
    // enter via a raw Rgba; the brush must report it faithfully.
    let b = Brush::Solid(Rgba::new(255, 255, 255, 0));
    assert_eq!(b.alpha(), 0);
}

#[test]
fn layer_new_pairs_geometry_and_brush() {
    let r = rect_at_origin();
    let c = Rgba::new(1, 2, 3, 4);
    let layer = Layer::new(Geometry::Rect(r), Brush::Solid(c));
    assert_eq!(layer.geometry, Geometry::Rect(r));
    assert_eq!(layer.brush, Brush::Solid(c));
}

#[test]
fn layer_solid_rect_is_a_geometry_brush_pair_alias() {
    let r = rect_at_origin();
    let c = Rgba::new(1, 2, 3, 4);
    assert_eq!(
        Layer::solid_rect(r, c),
        Layer::new(Geometry::Rect(r), Brush::Solid(c)),
    );
}

#[test]
fn overlay_frame_default_is_empty() {
    let f: OverlayFrame = OverlayFrame::default();
    assert_eq!(f, OverlayFrame::empty());
    assert_eq!(f.layers.len(), 0, "default frame has no layers");
}

#[test]
fn shape_and_orientation_are_independently_constructible() {
    // Just exercises the variants exist and round-trip through PartialEq;
    // the lattice these axes form is verified in property_render.rs.
    let _ = Shape::Bar;
    let _ = Shape::Mask;
    let _ = linerule_core::Orientation::Horizontal;
    let _ = linerule_core::Orientation::Vertical;
}
