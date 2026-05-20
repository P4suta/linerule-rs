//! Microbenchmark: `tick::step` on a few representative inputs.

#![allow(
    missing_docs,
    reason = "criterion_main! / criterion_group! expand to undocumented fns"
)]

use std::time::Duration;

use criterion::{Criterion, criterion_group, criterion_main};
use linerule_core::{
    OverlayAction, Point,
    input::tick::{TickInput, TickWorld, step},
};
use std::hint::black_box;

const REFRESH: Duration = Duration::from_secs(2);

const fn make_input(actions: Vec<OverlayAction>) -> TickInput {
    TickInput {
        now_ms: 1_000,
        polled_cursor: Some(Point::new(960, 540)),
        drained_hotkeys: actions,
    }
}

fn bench_tick(c: &mut Criterion) {
    let mut group = c.benchmark_group("tick::step");
    let world = TickWorld::INITIAL;

    group.bench_function("empty", |b| {
        let input = make_input(Vec::new());
        b.iter(|| step(black_box(world), black_box(&input), black_box(REFRESH)));
    });

    group.bench_function("one_cycle_mode", |b| {
        let input = make_input(vec![OverlayAction::CycleMode]);
        b.iter(|| step(black_box(world), black_box(&input), black_box(REFRESH)));
    });

    group.bench_function("three_actions_chain", |b| {
        let input = make_input(vec![
            OverlayAction::CycleMode,
            OverlayAction::BumpThickness(8),
            OverlayAction::BumpOpacity(-8),
        ]);
        b.iter(|| step(black_box(world), black_box(&input), black_box(REFRESH)));
    });
    group.finish();
}

criterion_group!(benches, bench_tick);
criterion_main!(benches);
