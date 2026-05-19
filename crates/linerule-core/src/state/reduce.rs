//! Pure reducer: `(State, OverlayAction) → (State, StateDelta)`.
//!
//! Every state mutation flows through this single function. The return is
//! `(next_state, delta)` because consumers (tick pipeline, HUD) want both
//! the new full state and a cheap "did anything change?" bit.

use crate::{
    config::OverlayConfig,
    state::{Mode, OverlayAction, State, StateDelta},
};

/// Apply `action` to `state`, returning the new state and a delta describing
/// which fields changed.
#[must_use]
pub fn apply(state: State, action: OverlayAction) -> (State, StateDelta) {
    use OverlayAction as A;
    match action {
        A::CycleMode => {
            let mode = state.mode.cycle();
            (State { mode, ..state }, StateDelta::mode(mode))
        },
        A::ToggleVisible => {
            let visible = !state.visible;
            (State { visible, ..state }, StateDelta::visible(visible))
        },
        A::BumpThickness(delta) => bump_config(state, |c| OverlayConfig {
            thickness: c.thickness.saturating_add(delta),
            ..c
        }),
        A::BumpOpacity(delta) => bump_config(state, |c| OverlayConfig {
            opacity: c.opacity.saturating_add(delta),
            ..c
        }),
        A::Quit => (state, StateDelta::NONE),
    }
}

/// Apply a config-only mutation while `mode != Off`, suppressing no-op edges
/// (saturation against bounds, mode is off, value unchanged) into a clean
/// `(state, StateDelta::NONE)`.
fn bump_config(
    state: State,
    mutate: impl FnOnce(OverlayConfig) -> OverlayConfig,
) -> (State, StateDelta) {
    if matches!(state.mode, Mode::Off) {
        return (state, StateDelta::NONE);
    }
    let next = mutate(state.config);
    if config_unchanged(state.config, next) {
        return (state, StateDelta::NONE);
    }
    (
        State {
            config: next,
            ..state
        },
        StateDelta::config_changed(),
    )
}

fn config_unchanged(a: OverlayConfig, b: OverlayConfig) -> bool {
    a.thickness == b.thickness && a.opacity == b.opacity && a.mask_color == b.mask_color
}

// ----- private helpers on StateDelta to keep the reducer terse ----------------

impl StateDelta {
    pub(crate) const fn mode(m: Mode) -> Self {
        Self {
            mode: Some(m),
            visible: None,
            config_changed: false,
        }
    }

    pub(crate) const fn visible(v: bool) -> Self {
        Self {
            mode: None,
            visible: Some(v),
            config_changed: false,
        }
    }

    pub(crate) const fn config_changed() -> Self {
        Self {
            mode: None,
            visible: None,
            config_changed: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::{Opacity, Thickness};

    #[test]
    fn cycle_mode_walks_the_three_state_loop() {
        let s0 = State::DEFAULT;
        let (s1, _) = apply(s0, OverlayAction::CycleMode);
        let (s2, _) = apply(s1, OverlayAction::CycleMode);
        let (s3, _) = apply(s2, OverlayAction::CycleMode);
        assert_eq!(
            [s1.mode, s2.mode, s3.mode],
            [Mode::Horizontal, Mode::Vertical, Mode::Off]
        );
    }

    #[test]
    fn toggle_visible_flips() {
        let s0 = State::DEFAULT;
        let (s1, d1) = apply(s0, OverlayAction::ToggleVisible);
        assert!(!s1.visible);
        assert_eq!(d1.visible, Some(false));
    }

    #[test]
    fn bump_thickness_is_a_no_op_when_mode_is_off() {
        let s0 = State::DEFAULT;
        let (s1, d) = apply(s0, OverlayAction::BumpThickness(8));
        assert_eq!(s1, s0);
        assert!(!d.is_any());
    }

    #[test]
    fn bump_thickness_changes_config_when_mode_is_on() {
        let s0 = State {
            mode: Mode::Horizontal,
            ..State::DEFAULT
        };
        let (s1, d) = apply(s0, OverlayAction::BumpThickness(8));
        assert_eq!(s1.config.thickness, Thickness::DEFAULT.saturating_add(8));
        assert!(d.config_changed);
    }

    #[test]
    fn bump_at_saturation_yields_no_delta() {
        let s0 = State {
            mode: Mode::Vertical,
            config: OverlayConfig {
                opacity: Opacity::MIN,
                ..OverlayConfig::DEFAULT
            },
            ..State::DEFAULT
        };
        let (s1, d) = apply(s0, OverlayAction::BumpOpacity(-8));
        assert_eq!(s1, s0);
        assert!(!d.is_any());
    }

    #[test]
    fn quit_is_a_pure_no_op() {
        let s0 = State::DEFAULT;
        let (s1, d) = apply(s0, OverlayAction::Quit);
        assert_eq!(s1, s0);
        assert!(!d.is_any());
    }
}
