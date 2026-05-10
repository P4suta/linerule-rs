//! Cross-platform parser for hotkey chord strings such as
//! `"Ctrl+Alt+R"`.
//!
//! The parser produces a [`ChordSpec`] — a small ADT with a
//! [`Modifiers`] bitset and a [`KeyCode`] sum type — which is then
//! adapted into the OS-native chord type by each platform module.
//! Keeping the parser OS-independent means tests can exercise every
//! corner of the grammar from the Linux dev container.
//!
//! Grammar (informal):
//!
//! - The chord is a `+`-separated list of *parts*.
//! - Each part is trimmed and ASCII-case-folded.
//! - Modifier parts (`ctrl`, `alt`, `shift`, `meta` / `win` / `cmd`)
//!   may appear in any order, multiple times. Aliases per [`Modifiers`].
//! - Exactly one part must be a non-modifier key; it is the *main key*.
//! - A chord with zero modifiers is allowed (`"R"`).
//! - A chord with zero main keys is rejected with [`ChordError::NoKey`].
//! - Two main keys produce [`ChordError::MultipleKeys`].
//! - An unrecognised part produces [`ChordError::UnknownPart`].

use thiserror::Error;

/// Modifier bitset. Multiple bits may be set simultaneously.
///
/// Four named flags is genuinely the modifier set on every supported
/// OS — promoting to a bitflags struct would be ceremony without
/// payoff at this size.
#[expect(
    clippy::struct_excessive_bools,
    reason = "modifier set is exactly four named flags by design"
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Modifiers {
    /// Control key (Ctrl).
    pub ctrl: bool,
    /// Alt / Option key.
    pub alt: bool,
    /// Shift key.
    pub shift: bool,
    /// Super / Meta / Win / Cmd key.
    pub meta: bool,
}

impl Modifiers {
    /// `true` iff at least one modifier is set.
    #[must_use]
    pub const fn any(self) -> bool {
        self.ctrl || self.alt || self.shift || self.meta
    }
}

/// Main key sum type. Closed at the OS-adapter boundary; new variants
/// land here when the platform impl needs them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum KeyCode {
    /// Letter A..Z. The `u8` is the ASCII code (`b'A'..=b'Z'`).
    Letter(u8),
    /// `[` left square bracket.
    BracketLeft,
    /// `]` right square bracket.
    BracketRight,
    /// `-` minus / hyphen.
    Minus,
    /// `=` equals.
    Equal,
    /// `↑` arrow.
    ArrowUp,
    /// `↓` arrow.
    ArrowDown,
    /// `←` arrow.
    ArrowLeft,
    /// `→` arrow.
    ArrowRight,
}

/// Result of parsing a chord string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChordSpec {
    /// Modifier bitset.
    pub modifiers: Modifiers,
    /// Main key (the non-modifier part).
    pub key: KeyCode,
}

/// Parser failures.
#[derive(Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum ChordError {
    /// The chord string parsed to zero non-modifier parts.
    #[error("chord {0:?} has no main key (only modifiers)")]
    NoKey(String),

    /// The chord string parsed to more than one non-modifier part.
    #[error("chord {0:?} has multiple main keys (only one is allowed)")]
    MultipleKeys(String),

    /// A part was neither a recognised modifier nor a recognised key.
    #[error("chord {chord:?} contains unknown part {part:?}")]
    UnknownPart {
        /// The full chord string for context.
        chord: String,
        /// The offending substring.
        part: String,
    },

    /// The chord string was empty (or whitespace-only).
    #[error("chord string is empty")]
    Empty,
}

/// Parse a chord string such as `"Ctrl+Alt+R"`.
///
/// # Errors
/// See [`ChordError`].
pub fn parse(chord: &str) -> Result<ChordSpec, ChordError> {
    let trimmed = chord.trim();
    if trimmed.is_empty() {
        return Err(ChordError::Empty);
    }

    let mut modifiers = Modifiers::default();
    let mut keys: Vec<KeyCode> = Vec::new();

    for part in trimmed.split('+') {
        let p = part.trim();
        if p.is_empty() {
            continue;
        }
        let folded = p.to_ascii_lowercase();
        match folded.as_str() {
            "ctrl" | "control" => modifiers.ctrl = true,
            "alt" | "option" => modifiers.alt = true,
            "shift" => modifiers.shift = true,
            "super" | "meta" | "win" | "cmd" => modifiers.meta = true,
            _ => match parse_key(p) {
                Some(k) => keys.push(k),
                None => {
                    return Err(ChordError::UnknownPart {
                        chord: chord.to_owned(),
                        part: p.to_owned(),
                    });
                }
            },
        }
    }

    match keys.len() {
        0 => Err(ChordError::NoKey(chord.to_owned())),
        1 => Ok(ChordSpec {
            modifiers,
            key: keys[0],
        }),
        _ => Err(ChordError::MultipleKeys(chord.to_owned())),
    }
}

fn parse_key(part: &str) -> Option<KeyCode> {
    let upper = part.to_ascii_uppercase();
    match upper.as_str() {
        "[" | "BRACKETLEFT" | "LBRACKET" => Some(KeyCode::BracketLeft),
        "]" | "BRACKETRIGHT" | "RBRACKET" => Some(KeyCode::BracketRight),
        "-" | "MINUS" => Some(KeyCode::Minus),
        "=" | "EQUAL" | "EQUALS" => Some(KeyCode::Equal),
        "ARROWUP" | "UP" => Some(KeyCode::ArrowUp),
        "ARROWDOWN" | "DOWN" => Some(KeyCode::ArrowDown),
        "ARROWLEFT" | "LEFT" => Some(KeyCode::ArrowLeft),
        "ARROWRIGHT" | "RIGHT" => Some(KeyCode::ArrowRight),
        s if s.len() == 1 && s.as_bytes()[0].is_ascii_uppercase() => {
            Some(KeyCode::Letter(s.as_bytes()[0]))
        }
        _ => None,
    }
}
