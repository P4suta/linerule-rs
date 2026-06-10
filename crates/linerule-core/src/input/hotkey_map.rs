//! User-facing chord assignments. Strings are parsed by
//! [`crate::input::chord::parse`]; the names below are the canonical defaults.

use serde::Serialize;

/// One chord string per `OverlayAction` variant the user can trigger.
//
// `Deserialize` omitted: fields are `&'static str`, which cannot satisfy
// `Deserialize<'de>` for arbitrary `'de`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub struct HotkeyMap {
    /// Chord that triggers `OverlayAction::CycleMode`.
    pub cycle_mode: &'static str,
    /// Chord that triggers `OverlayAction::CycleEffect`.
    pub cycle_effect: &'static str,
    /// Chord that triggers `OverlayAction::ToggleVisible`.
    pub toggle_visible: &'static str,
    /// Chord that bumps thickness up.
    pub thicker: &'static str,
    /// Chord that bumps thickness down.
    pub thinner: &'static str,
    /// Chord that bumps opacity up.
    pub more_opaque: &'static str,
    /// Chord that bumps opacity down.
    pub less_opaque: &'static str,
    /// Chord that triggers `OverlayAction::Quit`.
    pub quit: &'static str,
}

impl HotkeyMap {
    /// Default chord assignments (`Ctrl+Alt+...`).
    ///
    /// Bumps use arrow keys, not OEM keys (`]`/`[`/`=`/`-`): OEM virtual-key
    /// codes vary by keyboard layout/IME on Windows, so `RegisterHotKey` can
    /// silently miss. Arrow keys are layout-independent.
    pub const DEFAULT: Self = Self {
        cycle_mode: "Ctrl+Alt+R",
        cycle_effect: "Ctrl+Alt+E",
        toggle_visible: "Ctrl+Alt+H",
        thicker: "Ctrl+Alt+Up",
        thinner: "Ctrl+Alt+Down",
        more_opaque: "Ctrl+Alt+Right",
        less_opaque: "Ctrl+Alt+Left",
        quit: "Ctrl+Alt+Q",
    };
}

impl Default for HotkeyMap {
    fn default() -> Self {
        Self::DEFAULT
    }
}
