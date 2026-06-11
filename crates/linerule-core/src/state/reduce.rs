//! Pure reducer: `(State, OverlayAction) → (State, StateDelta)`.
//!
//! Every state mutation flows through this single function. The return is
//! `(next_state, delta)` because consumers (tick pipeline, HUD) want both
//! the new full state and a cheap "did anything change?" bit.

use crate::{
    config::OverlayConfig,
    state::{ActiveMode, Mode, OverlayAction, RejectReason, State, StateDelta},
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
/// `ToggleOnOff` toggles `Off ⇄ last_active`, so applying it twice is the
/// identity:
///
/// ```
/// use linerule_core::{OverlayAction, State, state::reduce};
/// let (on, _) = reduce::apply(State::DEFAULT, OverlayAction::ToggleOnOff);
/// let (off, _) = reduce::apply(on, OverlayAction::ToggleOnOff);
/// assert_eq!(off, State::DEFAULT);
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
            // Cycling into an active mode records it as the ToggleOnOff
            // restore target; cycling to Off keeps the previous one.
            let last_active = mode.active().unwrap_or(state.last_active);
            (
                State {
                    mode,
                    last_active,
                    ..state
                },
                StateDelta::mode(mode),
            )
        },
        A::ToggleOnOff => {
            let (mode, last_active) = match state.mode {
                // Off → restore the last active mode.
                Mode::Off => (Mode::from(state.last_active), state.last_active),
                // Active → Off, remembering what was on screen. Writing
                // `last_active` here (instead of trusting it) also self-heals
                // a state that violated the mode/last_active invariant.
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
        A::BumpOpacity(delta) => bump_config(state, |c| OverlayConfig {
            opacity: c.opacity.saturating_add(delta),
            ..c
        }),
        A::CycleStyle => bump_config(state, |c| OverlayConfig {
            surround_style: c.surround_style.cycle(),
            ..c
        }),
        // View-layer / process-layer actions: pure no-ops here. The tick
        // pipeline interprets them (HUD tier flip / TickEffect::Quit).
        A::ToggleHudDetail | A::Quit => (state, StateDelta::NONE),
    }
}

/// Apply a config-only mutation while `mode != Off`. While `Off` the action
/// is rejected with [`RejectReason::AdjustWhileOff`] so the user gets HUD
/// feedback instead of a silent nothing. No-op edges *within* an active mode
/// (saturation against bounds, value unchanged) stay silent — the user can
/// see the value is pinned.
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
    a.thickness == b.thickness
        && a.opacity == b.opacity
        && a.mask_color == b.mask_color
        && a.surround_style == b.surround_style
}

// ----- private helpers on StateDelta to keep the reducer terse ----------------

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
mod tests {
    use super::*;
    use crate::color::{Opacity, Thickness};
    use crate::state::ActiveMode;

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
    fn cycle_into_active_updates_last_active() {
        let s0 = State::DEFAULT;
        let (s1, _) = apply(s0, OverlayAction::CycleMode);
        assert_eq!(s1.last_active, ActiveMode::Horizontal);
        let (s2, _) = apply(s1, OverlayAction::CycleMode);
        assert_eq!(s2.last_active, ActiveMode::Vertical);
    }

    #[test]
    fn cycle_to_off_preserves_last_active() {
        let s = State::with_mode(Mode::Vertical);
        let (off, d) = apply(s, OverlayAction::CycleMode);
        assert_eq!(off.mode, Mode::Off);
        assert_eq!(off.last_active, ActiveMode::Vertical);
        assert_eq!(d.mode, Some(Mode::Off));
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

    /// Saturation inside an active mode is a *silent* no-op, not a rejection
    /// — pins the `matches!(mode, Off)` guard against a `true` mutant.
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

    /// `ToggleHudDetail` は view-layer action: reducer では Off 中も含めて
    /// 純粋 no-op (rejection でもない)。tier の反転は tick 側の責務。
    #[test]
    fn toggle_hud_detail_is_a_pure_no_op_even_while_off() {
        for s0 in [State::DEFAULT, State::with_mode(Mode::Horizontal)] {
            let (s1, d) = apply(s0, OverlayAction::ToggleHudDetail);
            assert_eq!(s1, s0);
            assert!(!d.is_any());
            assert_eq!(d.rejected, None);
        }
    }

    #[test]
    fn cycle_style_is_rejected_when_mode_is_off() {
        let s0 = State::DEFAULT;
        let (s1, d) = apply(s0, OverlayAction::CycleStyle);
        assert_eq!(s1, s0);
        assert!(!d.is_any());
        assert_eq!(d.rejected, Some(RejectReason::AdjustWhileOff));
    }

    /// `CycleStyle` must flip the style *and* report `config_changed`. If the
    /// delta were `NONE` (e.g. `surround_style` left out of `config_unchanged`),
    /// the platform layer would never re-render and the style would silently
    /// never change on screen — a test on the field alone would not catch it.
    #[test]
    fn cycle_style_advances_and_reports_config_changed() {
        use crate::config::SurroundStyle;
        let s0 = State::with_mode(Mode::Horizontal);
        assert_eq!(s0.config.surround_style, SurroundStyle::Dim);
        let (s1, d1) = apply(s0, OverlayAction::CycleStyle);
        assert_eq!(s1.config.surround_style, SurroundStyle::Bright);
        assert!(d1.config_changed, "Dim → Bright must report config_changed");
        let (s2, d2) = apply(s1, OverlayAction::CycleStyle);
        assert_eq!(s2.config.surround_style, SurroundStyle::Dim);
        assert!(d2.config_changed, "Bright → Dim must report config_changed");
    }

    /// `BumpOpacity` が `opacity` field を実際に更新することを pin する。
    /// `OverlayConfig` リテラル内で `opacity: ...` を `..c` のみ
    /// (= field 省略) に変えた mutation が、saturating_add(-8) で MIN から MIN
    /// のままになる test では検出できなかった (Phase ε mutation baseline)。
    /// `Horizontal` mode + DEFAULT (= 0xAA) から +8 すると 0xB2 になる、これは
    /// `..c` だと 0xAA のままで失敗する。
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
        // 同時に thickness と mask_color は変化しない (他 field を巻き込まない)
        assert_eq!(s1.config.thickness, s0.config.thickness);
        assert_eq!(s1.config.mask_color, s0.config.mask_color);
    }
}
