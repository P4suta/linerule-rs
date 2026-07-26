//! Pure reducer: `(State, OverlayAction) → (State, StateDelta)`.
//! The delta is a cheap "what changed?" bit for consumers (tick pipeline, HUD).

use crate::{
    config::OverlayConfig,
    state::{ActiveMode, Mode, OverlayAction, RejectReason, State, StateDelta},
};

/// Apply `action` to `state`, returning the new state and a change delta.
///
/// # Examples
///
/// `CycleMode` flips the axis (`Horizontal ⇄ Vertical`); on/off is `ToggleOnOff`:
///
/// ```
/// use linerule_core::{Mode, OverlayAction, State, reduce};
/// let on = State::with_mode(Mode::Horizontal);
/// let (next, delta) = reduce(on, OverlayAction::CycleMode);
/// assert_eq!(next.mode, Mode::Vertical);
/// assert!(delta.is_any());
/// ```
///
/// `ToggleOnOff` toggles `Off ⇄ last_active`, so twice is the identity:
///
/// ```
/// use linerule_core::{OverlayAction, State, reduce};
/// let (on, _) = reduce(State::DEFAULT, OverlayAction::ToggleOnOff);
/// let (off, _) = reduce(on, OverlayAction::ToggleOnOff);
/// assert_eq!(off, State::DEFAULT);
/// ```
///
/// `Quit` is a pure no-op here (the tick pipeline turns it into `TickEffect::Quit`):
///
/// ```
/// use linerule_core::{OverlayAction, State, reduce};
/// let (next, delta) = reduce(State::DEFAULT, OverlayAction::Quit);
/// assert_eq!(next, State::DEFAULT);
/// assert!(!delta.is_any());
/// ```
#[must_use]
#[allow(
    clippy::inline_always,
    reason = "the reducer is a steady-state hot path and cross-crate calls regressed the pinned Criterion baseline"
)]
#[inline(always)]
pub fn apply(state: State, action: OverlayAction) -> (State, StateDelta) {
    use OverlayAction as A;
    match action {
        // Deriving from `state.mode` (not `last_active`) self-heals a broken
        // mode/last_active invariant. While Off the flip is rejected — nothing
        // visible would change.
        A::CycleMode => state.mode.active().map_or_else(
            || (state, StateDelta::rejected(RejectReason::AdjustWhileOff)),
            |axis| {
                let last_active = axis.toggle();
                let mode = Mode::from(last_active);
                (
                    State {
                        mode,
                        last_active,
                        ..state
                    },
                    StateDelta::mode(mode),
                )
            },
        ),
        // Via `bump_config`: cycling while `Off` is rejected with HUD feedback.
        A::CycleEffect => bump_config(state, |c| OverlayConfig {
            effect: c.effect.cycle(),
            ..c
        }),
        A::ToggleOnOff => {
            let (mode, last_active) = match state.mode {
                // Off → restore last active mode.
                Mode::Off => (Mode::from(state.last_active), state.last_active),
                // Active → Off; rewriting `last_active` self-heals a broken
                // mode/last_active invariant.
                Mode::Horizontal => (Mode::Off, ActiveMode::Horizontal),
                Mode::Vertical => (Mode::Off, ActiveMode::Vertical),
            };
            (
                State {
                    mode,
                    last_active,
                    ..state
                },
                StateDelta::mode(mode),
            )
        },
        A::BumpThickness(delta) => bump_config(state, |c| OverlayConfig {
            thickness: c.thickness.saturating_add(delta),
            ..c
        }),
        // Under `Blur`, opacity hotkeys retarget onto the blur σ amount (opacity
        // is inert there); flat effects tune opacity.
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
        // View/process-layer actions: no-ops here; the tick pipeline interprets
        // them (HUD tier flip / TickEffect::Quit).
        A::ToggleHudDetail | A::Quit => (state, StateDelta::NONE),
    }
}

/// Apply a config-only mutation while `mode != Off`. While `Off` the action is
/// rejected with [`RejectReason::AdjustWhileOff`] for HUD feedback. A no-op edge
/// within an active mode (saturation, value unchanged) stays silent.
fn bump_config(
    state: State,
    mutate: impl FnOnce(OverlayConfig) -> OverlayConfig,
) -> (State, StateDelta) {
    if matches!(state.mode, Mode::Off) {
        return (state, StateDelta::rejected(RejectReason::AdjustWhileOff));
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

// Private StateDelta constructors.
impl StateDelta {
    pub(crate) const fn mode(m: Mode) -> Self {
        Self {
            mode: Some(m),
            config_changed: false,
            rejected: None,
        }
    }

    pub(crate) const fn config_changed() -> Self {
        Self {
            mode: None,
            config_changed: true,
            rejected: None,
        }
    }

    pub(crate) const fn rejected(reason: RejectReason) -> Self {
        Self {
            mode: None,
            config_changed: false,
            rejected: Some(reason),
        }
    }
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use super::*;
    use crate::color::{Opacity, Thickness};
    use crate::state::ActiveMode;

    #[test]
    fn cycle_mode_toggles_between_the_two_axes() {
        let s0 = State::with_mode(Mode::Horizontal);
        let (s1, d1) = apply(s0, OverlayAction::CycleMode);
        assert_eq!(s1.mode, Mode::Vertical);
        assert_eq!(d1.mode, Some(Mode::Vertical));
        let (s2, d2) = apply(s1, OverlayAction::CycleMode);
        assert_eq!(s2.mode, Mode::Horizontal);
        assert_eq!(d2.mode, Some(Mode::Horizontal));
    }

    #[test]
    fn cycle_mode_updates_last_active_with_the_axis() {
        let s0 = State::with_mode(Mode::Horizontal);
        let (s1, _) = apply(s0, OverlayAction::CycleMode);
        assert_eq!(s1.last_active, ActiveMode::Vertical);
        let (s2, _) = apply(s1, OverlayAction::CycleMode);
        assert_eq!(s2.last_active, ActiveMode::Horizontal);
    }

    /// While Off, `CycleMode` is rejected (turning on is `ToggleOnOff`'s job).
    #[test]
    fn cycle_mode_is_rejected_when_mode_is_off() {
        let s = State::DEFAULT;
        let (next, d) = apply(s, OverlayAction::CycleMode);
        assert_eq!(next, s);
        assert!(!d.is_any());
        assert_eq!(d.rejected, Some(RejectReason::AdjustWhileOff));
    }

    #[test]
    fn toggle_from_off_restores_last_active() {
        for last in [ActiveMode::Horizontal, ActiveMode::Vertical] {
            let s = State {
                mode: Mode::Off,
                last_active: last,
                ..State::DEFAULT
            };
            let (next, d) = apply(s, OverlayAction::ToggleOnOff);
            assert_eq!(next.mode, Mode::from(last));
            assert_eq!(next.last_active, last);
            assert_eq!(d.mode, Some(Mode::from(last)));
        }
    }

    #[test]
    fn toggle_from_active_goes_off_and_records_last_active() {
        let s = State::with_mode(Mode::Vertical);
        let (next, d) = apply(s, OverlayAction::ToggleOnOff);
        assert_eq!(next.mode, Mode::Off);
        assert_eq!(next.last_active, ActiveMode::Vertical);
        assert_eq!(d.mode, Some(Mode::Off));
    }

    #[test]
    fn bump_thickness_is_rejected_when_mode_is_off() {
        let s0 = State::DEFAULT;
        let (s1, d) = apply(s0, OverlayAction::BumpThickness(8));
        assert_eq!(s1, s0);
        assert!(!d.is_any());
        assert_eq!(d.rejected, Some(RejectReason::AdjustWhileOff));
    }

    #[test]
    fn bump_thickness_changes_config_when_mode_is_on() {
        let s0 = State::with_mode(Mode::Horizontal);
        let (s1, d) = apply(s0, OverlayAction::BumpThickness(8));
        assert_eq!(s1.config.thickness, Thickness::DEFAULT.saturating_add(8));
        assert!(d.config_changed);
        assert_eq!(d.rejected, None);
    }

    /// Saturation inside an active mode is a silent no-op, not a rejection.
    #[test]
    fn bump_at_saturation_yields_no_delta_and_no_rejection() {
        let s0 = State {
            config: OverlayConfig {
                opacity: Opacity::MIN,
                ..OverlayConfig::DEFAULT
            },
            ..State::with_mode(Mode::Vertical)
        };
        let (s1, d) = apply(s0, OverlayAction::BumpOpacity(-8));
        assert_eq!(s1, s0);
        assert!(!d.is_any());
        assert_eq!(d.rejected, None);
    }

    #[test]
    fn quit_is_a_pure_no_op() {
        let s0 = State::DEFAULT;
        let (s1, d) = apply(s0, OverlayAction::Quit);
        assert_eq!(s1, s0);
        assert!(!d.is_any());
        assert_eq!(d.rejected, None);
    }

    /// `ToggleHudDetail` is a pure no-op in the reducer, even while off.
    #[test]
    fn toggle_hud_detail_is_a_pure_no_op_even_while_off() {
        for s0 in [State::DEFAULT, State::with_mode(Mode::Horizontal)] {
            let (s1, d) = apply(s0, OverlayAction::ToggleHudDetail);
            assert_eq!(s1, s0);
            assert!(!d.is_any());
            assert_eq!(d.rejected, None);
        }
    }

    /// Cycling the effect while `Off` is rejected with HUD feedback.
    #[test]
    fn cycle_effect_is_rejected_when_mode_is_off() {
        let s0 = State::DEFAULT;
        let (s1, d) = apply(s0, OverlayAction::CycleEffect);
        assert_eq!(s1, s0);
        assert!(!d.is_any());
        assert_eq!(d.rejected, Some(RejectReason::AdjustWhileOff));
    }

    /// `BumpOpacity` actually mutates the `opacity` field
    /// (`Horizontal` + DEFAULT 0xAA, +8 → 0xB2).
    #[test]
    fn bump_opacity_actually_mutates_opacity_field() {
        let s0 = State::with_mode(Mode::Horizontal);
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

    /// A blur-amount-only change still flags `config_changed` (`config_unchanged`
    /// must compare `blur`).
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
#[test]
fn toggling_off_repairs_stale_last_active_axis() {
    let inconsistent = State {
        mode: Mode::Horizontal,
        last_active: ActiveMode::Vertical,
        ..State::DEFAULT
    };
    let (off, delta) = apply(inconsistent, OverlayAction::ToggleOnOff);
    assert_eq!(off.mode, Mode::Off);
    assert_eq!(off.last_active, ActiveMode::Horizontal);
    assert!(delta.is_any());
}
