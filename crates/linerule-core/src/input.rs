//! Input subsystem: chord parsing, hold-to-repeat FSM, tick pipeline,
//! HUD fade kernel, and hotkey assignments.
//!
//! Modules:
//! - [`chord`]: parse `"Ctrl+Alt+R"` style strings into [`chord::ChordSpec`].
//! - [`hold`]: hold-to-repeat FSM ([`hold::step`]).
//! - [`tick`]: per-tick coordination pipeline ([`tick::step`]).
//! - [`hud_fade`]: distance-driven HUD opacity ([`hud_fade::compute_opacity`]).
//! - [`hotkey_map`]: default chord-to-action assignments.

pub mod chord;
pub mod hold;
pub mod hotkey_map;
pub mod hud_fade;
pub mod tick;

pub use chord::{ChordError, ChordSpec, Direction, KeyCode, Letter, Modifiers};
pub use hotkey_map::HotkeyMap;
