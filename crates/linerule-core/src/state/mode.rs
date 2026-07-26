//! Overlay display mode.

use serde::{Deserialize, Serialize};

/// Overlay display mode. `Off` is reachable only via `ToggleOnOff`; the mode
/// hotkey toggles between the two on-screen axes (`Horizontal ⇄ Vertical`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// Overlay disabled.
    #[default]
    Off,
    /// Horizontal slit follows the cursor's Y.
    Horizontal,
    /// Vertical slit follows the cursor's X.
    Vertical,
}

impl Mode {
    /// `Some(_)` when the mode is on screen, `None` for `Off`.
    #[must_use]
    pub const fn active(self) -> Option<ActiveMode> {
        match self {
            Self::Off => None,
            Self::Horizontal => Some(ActiveMode::Horizontal),
            Self::Vertical => Some(ActiveMode::Vertical),
        }
    }
}

/// The two on-screen modes — [`Mode`] minus `Off`. Stored in
/// [`crate::state::State::last_active`] so the on/off toggle can restore the
/// last-used slit axis.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActiveMode {
    /// Horizontal slit (the restore target before any active mode was used).
    #[default]
    Horizontal,
    /// Vertical slit.
    Vertical,
}

impl ActiveMode {
    /// Flip horizontal ⇄ vertical (the `CycleMode` action while on screen).
    #[must_use]
    pub const fn toggle(self) -> Self {
        match self {
            Self::Horizontal => Self::Vertical,
            Self::Vertical => Self::Horizontal,
        }
    }
}

impl From<ActiveMode> for Mode {
    fn from(m: ActiveMode) -> Self {
        match m {
            ActiveMode::Horizontal => Self::Horizontal,
            ActiveMode::Vertical => Self::Vertical,
        }
    }
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use super::*;

    #[test]
    fn active_mode_toggle_is_involutive() {
        assert_eq!(ActiveMode::Horizontal.toggle(), ActiveMode::Vertical);
        assert_eq!(ActiveMode::Vertical.toggle(), ActiveMode::Horizontal);
        for m in [ActiveMode::Horizontal, ActiveMode::Vertical] {
            assert_eq!(m.toggle().toggle(), m);
        }
    }

    #[test]
    fn active_is_none_exactly_for_off() {
        assert_eq!(Mode::Off.active(), None);
        assert_eq!(Mode::Horizontal.active(), Some(ActiveMode::Horizontal));
        assert_eq!(Mode::Vertical.active(), Some(ActiveMode::Vertical));
    }

    #[test]
    fn active_round_trips_through_mode_for_on_screen_modes() {
        for mode in [Mode::Horizontal, Mode::Vertical] {
            let active = mode.active().expect("on-screen mode");
            assert_eq!(Mode::from(active), mode);
        }
    }
}
