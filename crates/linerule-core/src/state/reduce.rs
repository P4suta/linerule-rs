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
///
/// # Examples
///
/// `CycleMode` advances through Off → Horizontal → Vertical → Off:
///
/// ```
/// use linerule_core::{Mode, OverlayAction, State, state::reduce};
/// let (next, delta) = reduce::apply(State::DEFAULT, OverlayAction::CycleMode);
/// assert_eq!(next.mode, Mode::Horizontal);
/// assert!(delta.is_any());
/// ```
///
/// `Quit` is a pure no-op at the reducer layer (the tick pipeline turns it
/// into a `TickEffect::Quit`):
///
/// ```
/// use linerule_core::{OverlayAction, State, state::reduce};
/// let (next, delta) = reduce::apply(State::DEFAULT, OverlayAction::Quit);
/// assert_eq!(next, State::DEFAULT);
/// assert!(!delta.is_any());
/// ```
#[must_use]
pub fn apply(state: State, action: OverlayAction) -> (State, StateDelta) {
    use OverlayAction as A;
    match action {
        A::CycleMode => {
            let mode = state.mode.cycle();
            (State { mode, ..state }, StateDelta::mode(mode))
        },
        A::CycleEffect => bump_config(state, |c| OverlayConfig {
            effect: c.effect.cycle(),
            ..c
        }),
        A::ToggleVisible => {
            let visible = !state.visible;
            (State { visible, ..state }, StateDelta::visible(visible))
        },
        A::BumpThickness(delta) => bump_config(state, |c| OverlayConfig {
            thickness: c.thickness.saturating_add(delta),
            ..c
        }),
        // Under the `Blur` effect the opacity hotkeys retarget onto the blur σ
        // amount (the brightness knob was dropped, so opacity is inert there);
        // for the flat effects they tune opacity as before.
        A::BumpOpacity(delta) => bump_config(state, |c| {
            if c.effect.is_blur() {
                OverlayConfig {
                    blur: c.blur.saturating_add(delta),
                    ..c
                }
            } else {
                OverlayConfig {
                    opacity: c.opacity.saturating_add(delta),
                    ..c
                }
            }
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
    a.thickness == b.thickness && a.opacity == b.opacity && a.effect == b.effect && a.blur == b.blur
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

    /// `BumpOpacity` actually mutates the `opacity` field
    /// (`Horizontal` + DEFAULT 0xAA, +8 → 0xB2).
    #[test]
    fn bump_opacity_actually_mutates_opacity_field() {
        let s0 = State {
            mode: Mode::Horizontal,
            ..State::DEFAULT
        };
        let (s1, d) = apply(s0, OverlayAction::BumpOpacity(8));
        assert_eq!(s1.config.opacity, Opacity::DEFAULT.saturating_add(8));
        assert_ne!(
            s1.config.opacity,
            Opacity::DEFAULT,
            "opacity must change from DEFAULT after BumpOpacity(+8)"
        );
        assert!(d.config_changed);
        // thickness and effect stay put (no other field is touched).
        assert_eq!(s1.config.thickness, s0.config.thickness);
        assert_eq!(s1.config.effect, s0.config.effect);
    }

    #[test]
    fn cycle_effect_walks_the_three_state_loop_when_mode_is_on() {
        use crate::state::SurroundEffect;
        let s0 = State {
            mode: Mode::Horizontal,
            ..State::DEFAULT
        };
        assert_eq!(s0.config.effect, SurroundEffect::DimBlack);
        let (s1, d1) = apply(s0, OverlayAction::CycleEffect);
        assert_eq!(s1.config.effect, SurroundEffect::WhiteWash);
        assert!(d1.config_changed);
        let (s2, d2) = apply(s1, OverlayAction::CycleEffect);
        assert_eq!(s2.config.effect, SurroundEffect::Blur);
        assert!(d2.config_changed);
        let (s3, d3) = apply(s2, OverlayAction::CycleEffect);
        assert_eq!(s3.config.effect, SurroundEffect::DimBlack);
        assert!(d3.config_changed);
    }

    /// Under the Blur effect, `BumpOpacity` moves the σ amount (`blur`); `opacity` is inert.
    #[test]
    fn bump_opacity_retargets_to_blur_amount_under_blur_effect() {
        use crate::color::BlurAmount;
        use crate::state::SurroundEffect;
        let s0 = State {
            mode: Mode::Horizontal,
            config: OverlayConfig {
                effect: SurroundEffect::Blur,
                ..OverlayConfig::DEFAULT
            },
            ..State::DEFAULT
        };
        let (s1, d) = apply(s0, OverlayAction::BumpOpacity(8));
        assert_eq!(s1.config.blur, BlurAmount::DEFAULT.saturating_add(8));
        assert_eq!(
            s1.config.opacity, s0.config.opacity,
            "opacity must stay put under the Blur effect"
        );
        assert!(d.config_changed);
    }

    /// Under a flat (non-Blur) effect, `BumpOpacity` moves `opacity`; `blur` is inert.
    #[test]
    fn bump_opacity_tunes_opacity_under_flat_effect() {
        use crate::color::Opacity;
        let s0 = State {
            mode: Mode::Horizontal,
            ..State::DEFAULT // DimBlack
        };
        let (s1, d) = apply(s0, OverlayAction::BumpOpacity(8));
        assert_eq!(s1.config.opacity, Opacity::DEFAULT.saturating_add(8));
        assert_eq!(
            s1.config.blur, s0.config.blur,
            "blur amount must stay put under a flat effect"
        );
        assert!(d.config_changed);
    }

    /// A blur-amount-only change still flags `config_changed`; guards against
    /// `config_unchanged` dropping the `blur` comparison.
    #[test]
    fn blur_amount_only_change_marks_config_changed() {
        use crate::state::SurroundEffect;
        let s0 = State {
            mode: Mode::Horizontal,
            config: OverlayConfig {
                effect: SurroundEffect::Blur,
                ..OverlayConfig::DEFAULT
            },
            ..State::DEFAULT
        };
        let (s1, d) = apply(s0, OverlayAction::BumpOpacity(8));
        assert_ne!(s1.config.blur, s0.config.blur);
        assert!(
            d.config_changed,
            "a blur-amount-only change must still flag config_changed"
        );
    }

    #[test]
    fn cycle_effect_is_a_no_op_when_mode_is_off() {
        let s0 = State::DEFAULT; // mode Off
        let (s1, d) = apply(s0, OverlayAction::CycleEffect);
        assert_eq!(s1, s0);
        assert!(!d.is_any());
    }
}
