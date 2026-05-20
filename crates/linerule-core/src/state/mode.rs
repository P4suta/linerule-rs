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
}
