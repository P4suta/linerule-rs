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
    ///
    /// Bump / opacity adjustments are bound to arrow keys instead of OEM keys
    /// (`]`/`[`/`=`/`-`) because the latter map to different virtual-key codes
    /// depending on the active keyboard layout / IME on Windows — e.g. a JIS
    /// keyboard with the English IME does *not* deliver `VK_OEM_4` for the
    /// physical `[` key, so `RegisterHotKey(VK_OEM_4, ...)` silently misses.
    /// Arrow keys (`VK_UP/DOWN/LEFT/RIGHT`) are layout-independent.
    pub const DEFAULT: Self = Self {
        cycle_mode: "Ctrl+Alt+R",
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
