//! Microbenchmark: `frame()` on a 1080p monitor for each Mode.
//!
//! The frame builder is on the per-tick hot path; this guards against
//! a refactor that accidentally allocates a Vec twice or walks the
//! geometry list redundantly.

#![allow(
    missing_docs,
    reason = "criterion_main! / criterion_group! expand to undocumented fns"
)]

use criterion::{Criterion, criterion_group, criterion_main};
use linerule_core::{Mode, Point, ScreenRect, State, frame};
use std::hint::black_box;

const fn monitor() -> ScreenRect<linerule_core::Logical> {
    ScreenRect::new(Point::new(0, 0), 1920, 1080)
}

fn bench_frame(c: &mut Criterion) {
    let m = monitor();
    let cursor = Point::new(960, 540);
    let mut group = c.benchmark_group("frame");
    group.bench_function("off", |b| {
        b.iter(|| frame(black_box(State::DEFAULT), black_box(cursor), black_box(m)));
    });
    group.bench_function("horizontal", |b| {
        let s = State {
            mode: Mode::Horizontal,
            ..State::DEFAULT
        };
        b.iter(|| frame(black_box(s), black_box(cursor), black_box(m)));
    });
    group.bench_function("vertical", |b| {
        let s = State {
            mode: Mode::Vertical,
            ..State::DEFAULT
        };
        b.iter(|| frame(black_box(s), black_box(cursor), black_box(m)));
    });
    group.finish();
}

criterion_group!(benches, bench_frame);
criterion_main!(benches);
