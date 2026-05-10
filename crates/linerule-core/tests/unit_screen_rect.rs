//! `ScreenRect` / `Point` geometry helpers — `contains` and `contains_rect`
//! are bug-prone; pin them with explicit corner-and-edge cases.

use linerule_core::{Logical, Point, ScreenRect};

fn rect(x: i32, y: i32, w: u32, h: u32) -> ScreenRect<Logical> {
    ScreenRect::new(Point::<Logical>::new(x, y), w, h)
}

fn point(x: i32, y: i32) -> Point<Logical> {
    Point::<Logical>::new(x, y)
}

#[test]
fn contains_includes_origin() {
    assert!(rect(0, 0, 10, 10).contains(point(0, 0)));
}

#[test]
fn contains_excludes_far_corner() {
    // half-open: (10, 10) is just past width=10, height=10 from origin (0, 0).
    assert!(!rect(0, 0, 10, 10).contains(point(10, 10)));
}

#[test]
fn contains_includes_just_before_far_corner() {
    assert!(rect(0, 0, 10, 10).contains(point(9, 9)));
}

#[test]
fn contains_excludes_negative_offset() {
    assert!(!rect(0, 0, 10, 10).contains(point(-1, 5)));
    assert!(!rect(0, 0, 10, 10).contains(point(5, -1)));
}

#[test]
fn contains_works_with_offset_origin() {
    let r = rect(100, 200, 50, 75);
    assert!(r.contains(point(100, 200)));
    assert!(r.contains(point(149, 274)));
    assert!(!r.contains(point(150, 275)));
    assert!(!r.contains(point(99, 200)));
}

#[test]
fn contains_rect_includes_self() {
    let r = rect(0, 0, 100, 100);
    assert!(r.contains_rect(&r));
}

#[test]
fn contains_rect_excludes_partial_overlap() {
    let outer = rect(0, 0, 100, 100);
    let inner = rect(50, 50, 100, 100); // extends past outer
    assert!(!outer.contains_rect(&inner));
}

#[test]
fn contains_rect_includes_inner() {
    let outer = rect(0, 0, 100, 100);
    let inner = rect(10, 20, 30, 40);
    assert!(outer.contains_rect(&inner));
}

#[test]
fn contains_rect_includes_zero_sized_corner() {
    let outer = rect(0, 0, 100, 100);
    let inner = rect(100, 100, 0, 0); // zero-size at far corner — fits
    assert!(outer.contains_rect(&inner));
}

// ---- Manual Clone impls (Point<S> / ScreenRect<S> are generic over a
// phantom marker, so derive(Clone) cannot infer the bound — they ship
// hand-written impls that delegate to Copy. Reach the impls explicitly
// via `Clone::clone(&_)` so clippy's clone_on_copy doesn't bypass us
// onto the Copy path.) ----

#[test]
fn point_clone_round_trips() {
    let p = point(42, -7);
    let q = Clone::clone(&p);
    assert_eq!(p, q);
}

#[test]
fn screen_rect_clone_round_trips() {
    let r = rect(1, 2, 3, 4);
    let s = Clone::clone(&r);
    assert_eq!(r, s);
}
