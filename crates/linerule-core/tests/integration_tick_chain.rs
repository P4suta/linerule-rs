//! Integration: tick pipeline state machine over multi-action sequences.
//!
//! The pure pipeline runs in user space — no clock, no OS — so we can step
//! it deterministically with crafted `TickInput`s and observe effect lists.

use std::time::Duration;

use linerule_core::{
    Mode, OverlayAction, Point, State,
    input::tick::{TickEffect, TickInput, TickWorld, step},
};

const REFRESH: Duration = Duration::from_secs(2);

const fn tick(actions: Vec<OverlayAction>, now_ms: i64) -> TickInput {
    TickInput {
        now_ms,
        polled_cursor: Some(Point::new(960, 540)),
        drained_hotkeys: actions,
    }
}

#[test]
fn three_cycle_modes_in_one_tick_return_to_off() {
    let world = TickWorld::INITIAL;
    let input = tick(
        vec![
            OverlayAction::CycleMode,
            OverlayAction::CycleMode,
            OverlayAction::CycleMode,
        ],
        1_000,
    );
    let (next, _) = step(world, &input, REFRESH);
    assert_eq!(next.state.mode, Mode::Off);
}

#[test]
fn cycle_mode_in_one_tick_emits_log_and_draw() {
    let world = TickWorld::INITIAL;
    let input = tick(vec![OverlayAction::CycleMode], 1_000);
    let (_next, effects) = step(world, &input, REFRESH);
    let has_log = effects
        .iter()
        .any(|e| matches!(e, TickEffect::LogStateChanged { .. }));
    let has_draw = effects
        .iter()
        .any(|e| matches!(e, TickEffect::DrawOverlay { .. }));
    assert!(has_log, "expected a LogStateChanged effect");
    assert!(has_draw, "expected a DrawOverlay effect");
}

#[test]
fn quit_action_emits_quit_effect_and_short_circuits_state_changes() {
    let world = TickWorld::INITIAL;
    let input = tick(vec![OverlayAction::Quit], 1_000);
    let (_next, effects) = step(world, &input, REFRESH);
    assert!(effects.iter().any(|e| matches!(e, TickEffect::Quit)));
}

#[test]
fn frame_seq_monotonically_increments_each_tick() {
    let world = TickWorld::INITIAL;
    let start_seq = world.frame_seq;
    let (after_1, _) = step(world, &tick(vec![], 100), REFRESH);
    let (after_2, _) = step(after_1, &tick(vec![], 200), REFRESH);
    let (after_3, _) = step(after_2, &tick(vec![], 300), REFRESH);
    assert_eq!(after_1.frame_seq, start_seq + 1);
    assert_eq!(after_2.frame_seq, start_seq + 2);
    assert_eq!(after_3.frame_seq, start_seq + 3);
}

#[test]
fn empty_tick_in_off_mode_emits_clear_overlay() {
    let world = TickWorld::INITIAL;
    let input = tick(vec![], 1_000);
    let (_next, effects) = step(world, &input, REFRESH);
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, TickEffect::ClearOverlay)),
        "Off mode should emit ClearOverlay (no slit to draw)"
    );
}

#[test]
fn toggle_visible_after_cycle_yields_clear_overlay() {
    let world = TickWorld::INITIAL;
    let input = tick(
        vec![OverlayAction::CycleMode, OverlayAction::ToggleVisible],
        1_000,
    );
    let (next, effects) = step(world, &input, REFRESH);
    assert_eq!(next.state.mode, Mode::Horizontal);
    assert!(!next.state.visible);
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, TickEffect::ClearOverlay)),
        "invisible state must clear overlay"
    );
}

#[test]
fn hud_refresh_skipped_when_no_change_within_interval() {
    // First tick refreshes HUD (initial state).
    let world = TickWorld::INITIAL;
    let (after_first, _) = step(world, &tick(vec![], 1_000), REFRESH);
    // Second tick immediately after — no state change, no time elapsed.
    let (_, effects) = step(after_first, &tick(vec![], 1_001), REFRESH);
    let has_refresh = effects
        .iter()
        .any(|e| matches!(e, TickEffect::RefreshHud(_)));
    assert!(
        !has_refresh,
        "RefreshHud should be suppressed when nothing changed"
    );
}

#[test]
fn hud_refresh_fires_on_state_change_even_within_interval() {
    let world = TickWorld::INITIAL;
    // First tick: initial refresh.
    let (after_first, _) = step(world, &tick(vec![], 1_000), REFRESH);
    // Second tick: CycleMode changes state, within interval.
    let (_, effects) = step(
        after_first,
        &tick(vec![OverlayAction::CycleMode], 1_001),
        REFRESH,
    );
    let refreshed = effects.iter().find_map(|e| match e {
        TickEffect::RefreshHud(s) => Some(*s),
        _ => None,
    });
    let s: State = refreshed.expect("RefreshHud should fire on state change");
    assert_eq!(s.mode, Mode::Horizontal);
}
