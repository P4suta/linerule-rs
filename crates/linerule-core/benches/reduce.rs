//! Microbenchmark: `reduce::apply` over the closed `OverlayAction` sum.

#![allow(
    missing_docs,
    reason = "criterion_main! / criterion_group! expand to undocumented fns"
)]

use criterion::{Criterion, criterion_group, criterion_main};
use linerule_core::{Mode, OverlayAction, State, state::reduce};
use std::hint::black_box;

fn bench_reduce(c: &mut Criterion) {
    let mut group = c.benchmark_group("reduce::apply");
    group.bench_function("cycle_mode", |b| {
        b.iter(|| {
            reduce::apply(
                black_box(State::DEFAULT),
                black_box(OverlayAction::CycleMode),
            )
        });
    });
    group.bench_function("toggle_visible", |b| {
        b.iter(|| {
            reduce::apply(
                black_box(State::DEFAULT),
                black_box(OverlayAction::ToggleVisible),
            )
        });
    });
    let active = State {
        mode: Mode::Horizontal,
        ..State::DEFAULT
    };
    group.bench_function("bump_thickness_active", |b| {
        b.iter(|| {
            reduce::apply(
                black_box(active),
                black_box(OverlayAction::BumpThickness(8)),
            )
        });
    });
    group.bench_function("bump_opacity_active", |b| {
        b.iter(|| reduce::apply(black_box(active), black_box(OverlayAction::BumpOpacity(8))));
    });
    group.bench_function("quit", |b| {
        b.iter(|| reduce::apply(black_box(State::DEFAULT), black_box(OverlayAction::Quit)));
    });
    group.finish();
}

criterion_group!(benches, bench_reduce);
criterion_main!(benches);
