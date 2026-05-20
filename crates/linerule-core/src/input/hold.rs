//! Hold-to-repeat FSM.
//!
//! The platform layer fires this on every hotkey press and every ~16 ms tick
//! while a chord is held. The FSM is a pure function from
//! `(HoldState, HoldInput, RepeatConfig, oracle) → (HoldState, Vec<HoldEffect>)`:
//! it never touches the OS, never queries state, never wakes a timer. The
//! platform layer is responsible for delivering the effects (Enqueue,
//! Schedule, Halt) to the right subsystem.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::{config::RepeatConfig, input::chord::ChordSpec, state::OverlayAction};

/// Repeat-tick pacing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepeatCadence {
    /// Frequency ramps up over hold time (used for thickness / opacity bumps).
    Accelerating,
    /// Steady slow interval (used for mode cycling).
    Slow,
}

/// FSM state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "state")]
pub enum HoldState {
    /// No chord is currently being tracked.
    Idle,
    /// Chord is held; we're emitting repeat ticks.
    Repeating {
        /// Chord being held.
        chord: ChordSpec,
        /// Action emitted on each repeat (magnitude scaled by cadence).
        unit_step: OverlayAction,
        /// Repeat pacing.
        cadence: RepeatCadence,
        /// Hold start time (millisecond timestamp).
        started_at_ms: i64,
    },
    /// Chord is held but we're waiting for release before emitting anything.
    AwaitingRelease {
        /// Chord being held.
        chord: ChordSpec,
        /// Action to enqueue if the release counts as a long-press.
        undo_on_long_press: OverlayAction,
        /// Hold start time (millisecond timestamp).
        started_at_ms: i64,
    },
}

/// FSM input. `Fired` is a fresh hotkey press; `Tick` is a periodic poll
/// while the platform is tracking the hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HoldInput {
    /// A new chord was pressed.
    Fired {
        /// The chord that fired.
        chord: ChordSpec,
        /// Action bound to the chord.
        action: OverlayAction,
        /// Press timestamp (millisecond).
        now_ms: i64,
    },
    /// Periodic poll from the platform layer.
    Tick {
        /// Current timestamp.
        now_ms: i64,
        /// Whether the chord's keys are still held.
        still_held: bool,
    },
}

/// FSM output. Effects are produced in order and applied by the caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "effect")]
pub enum HoldEffect {
    /// Submit `action` to the reducer queue.
    Enqueue(OverlayAction),
    /// Wake the FSM again after `delay`.
    Schedule(Duration),
    /// Stop tracking this hold; return the FSM to `Idle`.
    Halt,
}

/// Pure FSM transition.
///
/// `can_progress` is consulted on each repeat tick to suppress further bumps
/// once saturation is reached (e.g. thickness already at max).
pub fn step(
    state: HoldState,
    input: HoldInput,
    config: RepeatConfig,
    can_progress: impl Fn(OverlayAction) -> bool,
) -> (HoldState, Vec<HoldEffect>) {
    match input {
        HoldInput::Fired {
            chord,
            action,
            now_ms,
        } => on_fired(chord, action, now_ms, config),
        HoldInput::Tick { now_ms, still_held } => {
            on_tick(state, now_ms, still_held, config, can_progress)
        },
    }
}

fn on_fired(
    chord: ChordSpec,
    action: OverlayAction,
    now_ms: i64,
    config: RepeatConfig,
) -> (HoldState, Vec<HoldEffect>) {
    match classify(action) {
        Classification::AccelRepeat => (
            HoldState::Repeating {
                chord,
                unit_step: action,
                cadence: RepeatCadence::Accelerating,
                started_at_ms: now_ms,
            },
            vec![HoldEffect::Schedule(config.initial_delay)],
        ),
        Classification::SlowRepeat => (
            HoldState::Repeating {
                chord,
                unit_step: action,
                cadence: RepeatCadence::Slow,
                started_at_ms: now_ms,
            },
            vec![HoldEffect::Schedule(config.initial_delay)],
        ),
        Classification::AwaitRelease { undo_on_long_press } => (
            HoldState::AwaitingRelease {
                chord,
                undo_on_long_press,
                started_at_ms: now_ms,
            },
            vec![HoldEffect::Schedule(config.release_poll)],
        ),
        Classification::OneShot => (HoldState::Idle, vec![HoldEffect::Halt]),
    }
}

fn on_tick(
    state: HoldState,
    now_ms: i64,
    still_held: bool,
    config: RepeatConfig,
    can_progress: impl Fn(OverlayAction) -> bool,
) -> (HoldState, Vec<HoldEffect>) {
    match state {
        HoldState::Idle => (HoldState::Idle, Vec::new()),
        HoldState::Repeating {
            chord,
            unit_step,
            cadence,
            started_at_ms,
        } => {
            if !still_held {
                return (HoldState::Idle, vec![HoldEffect::Halt]);
            }
            on_repeat_tick(
                chord,
                unit_step,
                cadence,
                started_at_ms,
                now_ms,
                config,
                can_progress,
            )
        },
        HoldState::AwaitingRelease {
            chord,
            undo_on_long_press,
            started_at_ms,
        } => {
            if still_held {
                return (
                    HoldState::AwaitingRelease {
                        chord,
                        undo_on_long_press,
                        started_at_ms,
                    },
                    vec![HoldEffect::Schedule(config.release_poll)],
                );
            }
            let held = duration_between(started_at_ms, now_ms);
            if held >= config.long_press_threshold {
                (
                    HoldState::Idle,
                    vec![HoldEffect::Enqueue(undo_on_long_press), HoldEffect::Halt],
                )
            } else {
                (HoldState::Idle, vec![HoldEffect::Halt])
            }
        },
    }
}

fn on_repeat_tick(
    chord: ChordSpec,
    unit_step: OverlayAction,
    cadence: RepeatCadence,
    started_at_ms: i64,
    now_ms: i64,
    config: RepeatConfig,
    can_progress: impl Fn(OverlayAction) -> bool,
) -> (HoldState, Vec<HoldEffect>) {
    let held = duration_between(started_at_ms, now_ms);
    let (interval, magnitude) = compute_next_step(held, cadence, config.slow_repeat_interval);
    let action = with_magnitude(unit_step, magnitude);

    if !can_progress(action) {
        return (HoldState::Idle, vec![HoldEffect::Halt]);
    }
    (
        HoldState::Repeating {
            chord,
            unit_step,
            cadence,
            started_at_ms,
        },
        vec![HoldEffect::Enqueue(action), HoldEffect::Schedule(interval)],
    )
}

/// Return `(next_interval, step_magnitude)` for the next repeat tick.
#[must_use]
pub const fn compute_next_step(
    held: Duration,
    cadence: RepeatCadence,
    slow_interval: Duration,
) -> (Duration, i32) {
    match cadence {
        RepeatCadence::Slow => (slow_interval, 1),
        RepeatCadence::Accelerating => match held.as_millis() {
            ..1000 => (Duration::from_millis(50), 1),
            1000..2000 => (Duration::from_millis(25), 1),
            2000..3000 => (Duration::from_millis(16), 1),
            3000.. => (Duration::from_millis(16), 4),
        },
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Classification {
    AccelRepeat,
    SlowRepeat,
    AwaitRelease { undo_on_long_press: OverlayAction },
    OneShot,
}

pub(crate) const fn classify(action: OverlayAction) -> Classification {
    use OverlayAction as A;
    match action {
        A::BumpThickness(_) | A::BumpOpacity(_) => Classification::AccelRepeat,
        A::CycleMode => Classification::SlowRepeat,
        A::ToggleVisible => Classification::AwaitRelease {
            undo_on_long_press: A::ToggleVisible,
        },
        A::Quit => Classification::OneShot,
    }
}

pub(crate) const fn with_magnitude(action: OverlayAction, magnitude: i32) -> OverlayAction {
    use OverlayAction as A;
    match action {
        A::BumpThickness(d) => A::BumpThickness(d.saturating_mul(magnitude)),
        A::BumpOpacity(d) => A::BumpOpacity(d.saturating_mul(magnitude)),
        A::CycleMode | A::ToggleVisible | A::Quit => action,
    }
}

pub(crate) fn duration_between(started_at_ms: i64, now_ms: i64) -> Duration {
    let diff = now_ms.saturating_sub(started_at_ms).max(0);
    Duration::from_millis(u64::try_from(diff).unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::chord::{KeyCode, Letter, Modifiers};

    fn chord() -> ChordSpec {
        ChordSpec::new(
            Modifiers::CTRL | Modifiers::ALT,
            KeyCode::Letter(Letter::from_ascii(b'R').unwrap()),
        )
    }

    fn always_progress() -> impl Fn(OverlayAction) -> bool {
        |_| true
    }

    #[test]
    fn fired_bump_enters_accelerating_repeat() {
        let cfg = RepeatConfig::DEFAULT;
        let (next, effects) = step(
            HoldState::Idle,
            HoldInput::Fired {
                chord: chord(),
                action: OverlayAction::BumpThickness(8),
                now_ms: 0,
            },
            cfg,
            always_progress(),
        );
        assert!(matches!(
            next,
            HoldState::Repeating {
                cadence: RepeatCadence::Accelerating,
                ..
            }
        ));
        assert_eq!(effects.len(), 1);
        assert!(matches!(effects[0], HoldEffect::Schedule(_)));
    }

    #[test]
    fn fired_cycle_mode_enters_slow_repeat() {
        let (next, _) = step(
            HoldState::Idle,
            HoldInput::Fired {
                chord: chord(),
                action: OverlayAction::CycleMode,
                now_ms: 0,
            },
            RepeatConfig::DEFAULT,
            always_progress(),
        );
        assert!(matches!(
            next,
            HoldState::Repeating {
                cadence: RepeatCadence::Slow,
                ..
            }
        ));
    }

    #[test]
    fn fired_toggle_enters_awaiting_release() {
        let (next, _) = step(
            HoldState::Idle,
            HoldInput::Fired {
                chord: chord(),
                action: OverlayAction::ToggleVisible,
                now_ms: 0,
            },
            RepeatConfig::DEFAULT,
            always_progress(),
        );
        assert!(matches!(next, HoldState::AwaitingRelease { .. }));
    }

    #[test]
    fn repeat_tick_with_release_halts() {
        let state = HoldState::Repeating {
            chord: chord(),
            unit_step: OverlayAction::BumpThickness(8),
            cadence: RepeatCadence::Accelerating,
            started_at_ms: 0,
        };
        let (next, effects) = step(
            state,
            HoldInput::Tick {
                now_ms: 200,
                still_held: false,
            },
            RepeatConfig::DEFAULT,
            always_progress(),
        );
        assert_eq!(next, HoldState::Idle);
        assert_eq!(effects, vec![HoldEffect::Halt]);
    }

    #[test]
    fn repeat_tick_enqueues_and_reschedules_while_held() {
        let state = HoldState::Repeating {
            chord: chord(),
            unit_step: OverlayAction::BumpThickness(8),
            cadence: RepeatCadence::Accelerating,
            started_at_ms: 0,
        };
        let (next, effects) = step(
            state,
            HoldInput::Tick {
                now_ms: 500,
                still_held: true,
            },
            RepeatConfig::DEFAULT,
            always_progress(),
        );
        assert_eq!(next, state);
        assert!(matches!(
            effects[0],
            HoldEffect::Enqueue(OverlayAction::BumpThickness(8))
        ));
        assert!(matches!(effects[1], HoldEffect::Schedule(_)));
    }

    #[test]
    fn saturation_oracle_halts_repeat() {
        let state = HoldState::Repeating {
            chord: chord(),
            unit_step: OverlayAction::BumpThickness(8),
            cadence: RepeatCadence::Accelerating,
            started_at_ms: 0,
        };
        let (next, effects) = step(
            state,
            HoldInput::Tick {
                now_ms: 500,
                still_held: true,
            },
            RepeatConfig::DEFAULT,
            |_| false,
        );
        assert_eq!(next, HoldState::Idle);
        assert_eq!(effects, vec![HoldEffect::Halt]);
    }

    #[test]
    fn long_press_release_emits_undo() {
        let state = HoldState::AwaitingRelease {
            chord: chord(),
            undo_on_long_press: OverlayAction::ToggleVisible,
            started_at_ms: 0,
        };
        let (next, effects) = step(
            state,
            HoldInput::Tick {
                now_ms: 400,
                still_held: false,
            },
            RepeatConfig::DEFAULT,
            always_progress(),
        );
        assert_eq!(next, HoldState::Idle);
        assert_eq!(
            effects,
            vec![
                HoldEffect::Enqueue(OverlayAction::ToggleVisible),
                HoldEffect::Halt,
            ]
        );
    }

    #[test]
    fn short_press_release_just_halts() {
        let state = HoldState::AwaitingRelease {
            chord: chord(),
            undo_on_long_press: OverlayAction::ToggleVisible,
            started_at_ms: 0,
        };
        let (next, effects) = step(
            state,
            HoldInput::Tick {
                now_ms: 100,
                still_held: false,
            },
            RepeatConfig::DEFAULT,
            always_progress(),
        );
        assert_eq!(next, HoldState::Idle);
        assert_eq!(effects, vec![HoldEffect::Halt]);
    }

    #[test]
    fn compute_next_step_brackets() {
        let slow = Duration::from_millis(400);
        assert_eq!(
            compute_next_step(Duration::from_millis(50), RepeatCadence::Accelerating, slow),
            (Duration::from_millis(50), 1)
        );
        assert_eq!(
            compute_next_step(
                Duration::from_millis(1500),
                RepeatCadence::Accelerating,
                slow
            ),
            (Duration::from_millis(25), 1)
        );
        assert_eq!(
            compute_next_step(
                Duration::from_millis(2500),
                RepeatCadence::Accelerating,
                slow
            ),
            (Duration::from_millis(16), 1)
        );
        assert_eq!(
            compute_next_step(Duration::from_secs(5), RepeatCadence::Accelerating, slow),
            (Duration::from_millis(16), 4)
        );
        assert_eq!(
            compute_next_step(Duration::ZERO, RepeatCadence::Slow, slow),
            (Duration::from_millis(400), 1)
        );
    }

    // ---- classify ---------------------------------------------------------

    #[test]
    fn classify_bump_actions_are_acceleration_repeat() {
        assert_eq!(
            classify(OverlayAction::BumpThickness(8)),
            Classification::AccelRepeat
        );
        assert_eq!(
            classify(OverlayAction::BumpOpacity(-4)),
            Classification::AccelRepeat
        );
    }

    #[test]
    fn classify_cycle_mode_is_slow_repeat() {
        assert_eq!(
            classify(OverlayAction::CycleMode),
            Classification::SlowRepeat
        );
    }

    #[test]
    fn classify_toggle_visible_awaits_release_and_undoes_with_self() {
        match classify(OverlayAction::ToggleVisible) {
            Classification::AwaitRelease { undo_on_long_press } => {
                assert_eq!(undo_on_long_press, OverlayAction::ToggleVisible);
            },
            other => panic!("expected AwaitRelease, got {other:?}"),
        }
    }

    #[test]
    fn classify_quit_is_one_shot() {
        assert_eq!(classify(OverlayAction::Quit), Classification::OneShot);
    }

    // ---- with_magnitude --------------------------------------------------

    #[test]
    fn with_magnitude_scales_bump_thickness() {
        assert_eq!(
            with_magnitude(OverlayAction::BumpThickness(8), 4),
            OverlayAction::BumpThickness(32)
        );
    }

    #[test]
    fn with_magnitude_scales_bump_opacity() {
        assert_eq!(
            with_magnitude(OverlayAction::BumpOpacity(-2), 5),
            OverlayAction::BumpOpacity(-10)
        );
    }

    #[test]
    fn with_magnitude_saturates_on_overflow() {
        assert_eq!(
            with_magnitude(OverlayAction::BumpThickness(i32::MAX), 2),
            OverlayAction::BumpThickness(i32::MAX)
        );
        assert_eq!(
            with_magnitude(OverlayAction::BumpThickness(i32::MIN), 2),
            OverlayAction::BumpThickness(i32::MIN)
        );
    }

    #[test]
    fn with_magnitude_leaves_non_bump_actions_unchanged() {
        for a in [
            OverlayAction::CycleMode,
            OverlayAction::ToggleVisible,
            OverlayAction::Quit,
        ] {
            assert_eq!(with_magnitude(a, 99), a);
        }
    }

    // ---- duration_between ------------------------------------------------

    #[test]
    fn duration_between_positive_diff() {
        assert_eq!(duration_between(100, 250), Duration::from_millis(150));
    }

    #[test]
    fn duration_between_zero_diff() {
        assert_eq!(duration_between(500, 500), Duration::ZERO);
    }

    #[test]
    fn duration_between_negative_diff_saturates_to_zero() {
        // now < started — clock went backwards; we still return a non-negative Duration.
        assert_eq!(duration_between(500, 100), Duration::ZERO);
    }

    #[test]
    fn duration_between_handles_i64_extremes_without_panic() {
        // started_at = i64::MIN, now = i64::MAX: saturating_sub avoids overflow.
        let d = duration_between(i64::MIN, i64::MAX);
        assert!(d > Duration::ZERO, "expected positive duration, got {d:?}");
    }
}
