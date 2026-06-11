//! Overlay display mode.

use serde::{Deserialize, Serialize};

/// Overlay display mode. The 3-state cycle is `Off → Horizontal → Vertical → Off`.
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
    /// Advance to the next mode in the canonical cycle.
    #[must_use]
    pub const fn cycle(self) -> Self {
        match self {
            Self::Off => Self::Horizontal,
            Self::Horizontal => Self::Vertical,
            Self::Vertical => Self::Off,
        }
    }

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

impl From<ActiveMode> for Mode {
    fn from(m: ActiveMode) -> Self {
        match m {
            ActiveMode::Horizontal => Self::Horizontal,
            ActiveMode::Vertical => Self::Vertical,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cycle_visits_each_state_once_before_returning() {
        let m0 = Mode::Off;
        let m1 = m0.cycle();
        let m2 = m1.cycle();
        let m3 = m2.cycle();
        assert_eq!(m1, Mode::Horizontal);
        assert_eq!(m2, Mode::Vertical);
        assert_eq!(m3, Mode::Off);
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
