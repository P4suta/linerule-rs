//! Input subsystem: chord parsing, hold-to-repeat FSM, tick pipeline,
//! HUD fade kernel, and hotkey assignments.

pub mod chord;
pub mod hold;
pub mod hotkey_map;
pub mod hud_fade;
pub mod tick;
pub mod win32_vk;

pub use chord::{ChordError, ChordSpec, Direction, KeyCode, Letter, Modifiers};
pub use hotkey_map::HotkeyMap;
