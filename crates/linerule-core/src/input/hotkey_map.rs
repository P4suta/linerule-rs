//! User-facing chord assignments. Strings are parsed by
//! [`crate::input::chord::parse`]; the names below are the canonical defaults.

use serde::Serialize;

/// One chord string per `OverlayAction` variant the user can trigger.
//
// `Deserialize` is omitted — the fields are `&'static str`, which cannot
// satisfy `Deserialize<'de>` for arbitrary `'de`. Compile-time const only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub struct HotkeyMap {
    /// Chord that triggers `OverlayAction::CycleMode`.
    pub cycle_mode: &'static str,
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
    pub const DEFAULT: Self = Self {
        cycle_mode: "Ctrl+Alt+R",
        toggle_visible: "Ctrl+Alt+H",
        thicker: "Ctrl+Alt+]",
        thinner: "Ctrl+Alt+[",
        more_opaque: "Ctrl+Alt+=",
        less_opaque: "Ctrl+Alt+-",
        quit: "Ctrl+Alt+Q",
    };
}

impl Default for HotkeyMap {
    fn default() -> Self {
        Self::DEFAULT
    }
}
