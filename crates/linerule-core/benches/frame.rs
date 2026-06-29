//! Microbenchmark: `frame()` on a 1080p monitor for each Mode.
//! Guards the per-tick hot path against redundant allocation/iteration.

#![allow(
    missing_docs,
    reason = "criterion_main! / criterion_group! expand to undocumented fns"
)]

use criterion::{Criterion, criterion_group, criterion_main};
use linerule_core::{Mode, OverlayConfig, OverlaySample, Point, ScreenRect, frame};
use std::hint::black_box;

const fn monitor() -> ScreenRect<linerule_core::Logical> {
    ScreenRect::new(Point::new(0, 0), 1920, 1080)
}

fn bench_frame(c: &mut Criterion) {
    let m = monitor();
    let cursor = Point::new(960, 540);
    let config = OverlayConfig::DEFAULT;
    let settled = OverlaySample::settled(config);
    let mut group = c.benchmark_group("frame");
    group.bench_function("off", |b| {
        b.iter(|| {
            frame(
                black_box(Mode::Off),
                black_box(config),
                black_box(cursor),
                black_box(m),
                black_box(settled),
            )
        });
    });
    group.bench_function("horizontal", |b| {
        b.iter(|| {
            frame(
                black_box(Mode::Horizontal),
                black_box(config),
                black_box(cursor),
                black_box(m),
                black_box(settled),
            )
        });
    });
    group.bench_function("vertical", |b| {
        b.iter(|| {
            frame(
                black_box(Mode::Vertical),
                black_box(config),
                black_box(cursor),
                black_box(m),
                black_box(settled),
            )
        });
    });
    // Mid-transition (non-settled sample) hot path.
    group.bench_function("horizontal_mid_fade", |b| {
        let mid = OverlaySample {
            master: 128,
            thickness_px: 64,
            mask_alpha: 0x90,
            style_mix: 128,
        };
        b.iter(|| {
            frame(
                black_box(Mode::Horizontal),
                black_box(config),
                black_box(cursor),
                black_box(m),
                black_box(mid),
            )
        });
    });
    group.finish();
}

criterion_group!(benches, bench_frame);
criterion_main!(benches);
