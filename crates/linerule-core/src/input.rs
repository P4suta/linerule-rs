//! Input subsystem: chord parsing, hold-to-repeat FSM, tick pipeline,
//! HUD fade kernel, and hotkey assignments.

mod chord;
mod hud_fade;
mod tick;
mod win32_vk;

pub use chord::{ChordError, ChordSpec, Direction, KeyCode, Letter, Modifiers, parse};
pub use hud_fade::{apply_envelope, compute_opacity};
pub use tick::{ActionBatch, TickEffect, TickEffects, TickInput, TickWorld, step};
pub use win32_vk::{
    MOD_ALT, MOD_CONTROL, MOD_SHIFT, MOD_WIN, chord_from_win32, chord_to_win32, key_to_vk,
};
