//! Pipeline that turns a tick's worth of inputs (drained hotkeys, polled
//! cursor, timestamp) into the next [`TickWorld`] and a list of [`TickEffect`]
//! the platform layer should carry out.
//!
//! This is the single coordination point for "what changed this tick?" — the
//! reducer is invoked here, log lines are scheduled here, draw / clear /
//! HUD-refresh decisions are made here. All of it pure.

use std::time::Duration;

use serde::Serialize;

use crate::{
    config::OverlayConfig,
    geometry::{Logical, Point},
    state::{Mode, OverlayAction, State, reduce},
};

/// Per-tick input from the platform.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TickInput {
    /// Current timestamp (millisecond).
    pub now_ms: i64,
    /// Latest cursor sample from the OS (`None` if not yet known).
    pub polled_cursor: Option<Point<Logical>>,
    /// Hotkey actions drained from the platform channel this tick.
    pub drained_hotkeys: Vec<OverlayAction>,
}

/// Tick pipeline's persistent state.
//
// `Deserialize` is omitted — `Point<S>` carries `PhantomData<fn() -> S>` which
// blocks the standard `Deserialize` derive. Pure runtime state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub struct TickWorld {
    /// Last applied overlay state.
    pub state: State,
    /// Previous tick's cursor sample.
    pub last_cursor: Option<Point<Logical>>,
    /// Monotonically incrementing frame counter.
    pub frame_seq: u64,
    /// Last timestamp at which the HUD was refreshed.
    pub last_hud_refresh_at_ms: i64,
}

impl TickWorld {
    /// Initial state. `last_hud_refresh_at_ms = i64::MIN` so that the very
    /// first tick is treated as "interval has elapsed" and refreshes the HUD
    /// regardless of the clock origin.
    pub const INITIAL: Self = Self {
        state: State::DEFAULT,
        last_cursor: None,
        frame_seq: 0,
        last_hud_refresh_at_ms: i64::MIN,
    };

    /// Initial state with a caller-supplied [`State`]. Useful for booting
    /// the overlay directly into a non-default mode (e.g. `--initial-mode
    /// horizontal` for CI smoke tests that need to exercise the slit
    /// render path without sending a synthetic `Ctrl+Alt+R`).
    #[must_use]
    pub const fn with_initial_state(state: State) -> Self {
        Self {
            state,
            last_cursor: None,
            frame_seq: 0,
            last_hud_refresh_at_ms: i64::MIN,
        }
    }
}

impl Default for TickWorld {
    fn default() -> Self {
        Self::INITIAL
    }
}

/// Effects emitted by [`step`]. Order is significant — the platform applies
/// them sequentially.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case", tag = "effect")]
pub enum TickEffect {
    /// Stop the application.
    Quit,
    /// Draw or update the overlay at the given cursor position.
    DrawOverlay {
        /// Active overlay mode.
        mode: Mode,
        /// Cursor position for slit anchoring.
        cursor: Point<Logical>,
        /// Current overlay config.
        config: OverlayConfig,
    },
    /// Hide the overlay (mode off, not visible, or no cursor yet).
    ClearOverlay,
    /// Refresh the full HUD with the supplied state snapshot.
    RefreshHud(State),
    /// Update HUD opacity for the current cursor distance.
    SetHudOpacity {
        /// Current state (for `visible` / `mode` checks).
        state: State,
        /// Cursor position used for the distance calculation.
        cursor: Point<Logical>,
    },
    /// Log a `LogStateChanged` event after a successful reduce.
    LogStateChanged {
        /// Action that caused the change.
        action: OverlayAction,
        /// New mode.
        mode: Mode,
        /// New visibility.
        visible: bool,
    },
}

/// Pure tick step.
#[must_use]
pub fn step(
    world: TickWorld,
    input: &TickInput,
    telemetry_refresh: Duration,
) -> (TickWorld, Vec<TickEffect>) {
    let mut effects = Vec::with_capacity(4);

    let prev_state = world.state;
    let mut state = world.state;

    let mut quit_requested = false;
    for action in &input.drained_hotkeys {
        if matches!(action, OverlayAction::Quit) {
            quit_requested = true;
        }
        let (next, delta) = reduce::apply(state, *action);
        if delta.is_any() {
            effects.push(TickEffect::LogStateChanged {
                action: *action,
                mode: next.mode,
                visible: next.visible,
            });
        }
        state = next;
    }

    if quit_requested {
        effects.push(TickEffect::Quit);
    }

    let cursor_moved = input.polled_cursor != world.last_cursor;
    let next_cursor = input.polled_cursor;

    match (state.visible, state.mode, next_cursor) {
        (true, Mode::Horizontal | Mode::Vertical, Some(cursor)) => {
            effects.push(TickEffect::DrawOverlay {
                mode: state.mode,
                cursor,
                config: state.config,
            });
        },
        _ => effects.push(TickEffect::ClearOverlay),
    }

    if cursor_moved && let Some(cursor) = next_cursor {
        effects.push(TickEffect::SetHudOpacity { state, cursor });
    }

    let state_changed = state != prev_state;
    let interval_ms = i64::try_from(telemetry_refresh.as_millis()).unwrap_or(i64::MAX);
    let interval_elapsed = input.now_ms.saturating_sub(world.last_hud_refresh_at_ms) >= interval_ms;
    let next_last_hud_refresh = if state_changed || interval_elapsed {
        effects.push(TickEffect::RefreshHud(state));
        input.now_ms
    } else {
        world.last_hud_refresh_at_ms
    };

    let next_world = TickWorld {
        state,
        last_cursor: next_cursor,
        frame_seq: world.frame_seq.wrapping_add(1),
        last_hud_refresh_at_ms: next_last_hud_refresh,
    };

    // Debug build 限定の invariant check。`frame_seq` は `wrapping_add(1)` で常に
    // +1 されるため u64::MAX 越えの wrap (294 兆 tick = 60Hz で 1500 万年) を
    // 除いて単調増加する。誤って 0 や減少値を入れた場合 debug_assert! が即捕捉。
    debug_assert!(
        next_world.frame_seq == world.frame_seq.wrapping_add(1),
        "frame_seq must be wrapping_add(1) of previous: prev={}, next={}",
        world.frame_seq,
        next_world.frame_seq
    );
    debug_assert!(
        next_world.last_hud_refresh_at_ms >= world.last_hud_refresh_at_ms
            || world.last_hud_refresh_at_ms == i64::MIN,
        "last_hud_refresh_at_ms must be monotonic: prev={}, next={}",
        world.last_hud_refresh_at_ms,
        next_world.last_hud_refresh_at_ms
    );

    (next_world, effects)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TELEMETRY: Duration = Duration::from_millis(200);

    fn world() -> TickWorld {
        TickWorld::INITIAL
    }

    fn input(now_ms: i64) -> TickInput {
        TickInput {
            now_ms,
            polled_cursor: None,
            drained_hotkeys: Vec::new(),
        }
    }

    #[test]
    fn empty_tick_clears_and_refreshes_hud() {
        let (next, fx) = step(world(), &input(0), TELEMETRY);
        assert_eq!(fx[0], TickEffect::ClearOverlay);
        assert!(matches!(fx.last(), Some(TickEffect::RefreshHud(_))));
        assert_eq!(next.frame_seq, 1);
    }

    #[test]
    fn cycle_mode_emits_log_and_draws_overlay() {
        let mut input = input(0);
        input.drained_hotkeys.push(OverlayAction::CycleMode);
        input.polled_cursor = Some(Point::new(100, 100));
        let (next, fx) = step(world(), &input, TELEMETRY);
        assert!(matches!(fx[0], TickEffect::LogStateChanged { .. }));
        assert!(
            fx.iter()
                .any(|e| matches!(e, TickEffect::DrawOverlay { .. }))
        );
        assert_eq!(next.state.mode, Mode::Horizontal);
    }

    #[test]
    fn quit_action_emits_quit_effect() {
        let mut input = input(0);
        input.drained_hotkeys.push(OverlayAction::Quit);
        let (_, fx) = step(world(), &input, TELEMETRY);
        assert!(fx.contains(&TickEffect::Quit));
    }

    #[test]
    fn hud_refresh_is_skipped_when_neither_state_changed_nor_interval_elapsed() {
        let (w1, _) = step(world(), &input(0), TELEMETRY);
        let (_, fx2) = step(w1, &input(100), TELEMETRY);
        assert!(!fx2.iter().any(|e| matches!(e, TickEffect::RefreshHud(_))));
    }
}
