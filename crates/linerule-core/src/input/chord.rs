//! Keyboard chord (modifiers + key) parsed from `"Ctrl+Alt+R"`-style strings.
//!
//! Parsing is total and reversible: every accepted form round-trips through
//! [`ChordSpec::display`], and unknown tokens produce structured [`ChordError`]
//! values with the original input surfaced for diagnostics.

use std::fmt;

use bitflags::bitflags;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Fully-resolved chord: modifier set + one key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChordSpec {
    /// Modifier bitset (Ctrl / Alt / Shift / Meta).
    pub modifiers: Modifiers,
    /// The non-modifier key bound to this chord.
    pub key: KeyCode,
}

impl ChordSpec {
    /// Construct a [`ChordSpec`] from modifiers and a key.
    #[must_use]
    pub const fn new(modifiers: Modifiers, key: KeyCode) -> Self {
        Self { modifiers, key }
    }

    /// Canonical text form, suitable for round-tripping and user display.
    #[must_use]
    pub fn display(self) -> String {
        let mut parts = Vec::with_capacity(5);
        if self.modifiers.contains(Modifiers::CTRL) {
            parts.push("Ctrl");
        }
        if self.modifiers.contains(Modifiers::ALT) {
            parts.push("Alt");
        }
        if self.modifiers.contains(Modifiers::SHIFT) {
            parts.push("Shift");
        }
        if self.modifiers.contains(Modifiers::META) {
            parts.push("Meta");
        }
        let key = self.key.display();
        parts.push(key.as_str());
        parts.join("+")
    }
}

impl fmt::Display for ChordSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&(*self).display())
    }
}

bitflags! {
    /// Modifier set as a bitflag-style newtype. Combine with `|`, query with
    /// [`Modifiers::contains`] / [`Modifiers::is_empty`].
    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub struct Modifiers: u8 {
        /// Control key.
        const CTRL = 1 << 0;
        /// Alt / Option key.
        const ALT = 1 << 1;
        /// Shift key.
        const SHIFT = 1 << 2;
        /// Meta / Win / Super / Cmd key.
        const META = 1 << 3;
    }
}

/// Keys linerule recognizes. A small closed set — every new chord-able key
/// needs an explicit variant here so the parser stays exhaustive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyCode {
    /// ASCII letter `A`..=`Z` (case-folded by the parser).
    Letter(Letter),
    /// `[` key.
    BracketLeft,
    /// `]` key.
    BracketRight,
    /// `-` key.
    Minus,
    /// `=` key.
    Equal,
    /// Arrow key (Up / Down / Left / Right).
    Arrow(Direction),
}

impl KeyCode {
    fn display(self) -> String {
        match self {
            Self::Letter(l) => char::from(l.as_u8()).to_string(),
            Self::BracketLeft => "[".into(),
            Self::BracketRight => "]".into(),
            Self::Minus => "-".into(),
            Self::Equal => "=".into(),
            Self::Arrow(d) => match d {
                Direction::Up => "Up".into(),
                Direction::Down => "Down".into(),
                Direction::Left => "Left".into(),
                Direction::Right => "Right".into(),
            },
        }
    }
}

/// Arrow direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    /// Up arrow.
    Up,
    /// Down arrow.
    Down,
    /// Left arrow.
    Left,
    /// Right arrow.
    Right,
}

/// `Letter` is a newtype guaranteeing the byte is uppercase ASCII `A..=Z`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Letter(u8);

impl Letter {
    /// Construct from an ASCII byte. Case is folded to upper.
    ///
    /// Returns `None` for bytes outside `A..=Z` / `a..=z`.
    #[must_use]
    pub const fn from_ascii(b: u8) -> Option<Self> {
        match b {
            b'A'..=b'Z' => Some(Self(b)),
            b'a'..=b'z' => Some(Self(b - 32)),
            _ => None,
        }
    }

    /// Inner ASCII byte (uppercase, `A..=Z`).
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        self.0
    }
}

/// Errors produced by [`parse`].
#[derive(Debug, Clone, PartialEq, Eq, Hash, Error)]
pub enum ChordError {
    /// Input string was empty or contained only whitespace.
    #[error("chord string is empty")]
    Empty,
    /// `+` separator with no content on either side.
    #[error("empty token at position {position} (consecutive `+` or trailing separator)")]
    EmptyToken {
        /// Index of the offending token (0-based).
        position: usize,
    },
    /// Token didn't match any known modifier or key.
    #[error("unknown chord part `{part}`")]
    UnknownPart {
        /// The unrecognized token.
        part: String,
    },
    /// More than one key token in the chord.
    #[error("multiple keys in chord (`{first}` then `{second}`)")]
    MultipleKeys {
        /// First key token seen.
        first: String,
        /// Second key token seen.
        second: String,
    },
    /// Chord contained modifiers but no key.
    #[error("chord has no key, only modifiers")]
    NoKey,
}

/// Parse `"Ctrl+Alt+R"`-style chord strings.
///
/// Recognized tokens (case-insensitive):
/// - Modifiers: `ctrl`/`control`, `alt`/`option`/`opt`, `shift`, `meta`/`win`/`super`/`cmd`
/// - Letters: a single `A`..`Z`
/// - Punctuation keys: `[`, `]`, `-`, `=`
/// - Arrows: `up`, `down`, `left`, `right`
///
/// # Errors
/// See [`ChordError`].
pub fn parse(input: &str) -> Result<ChordSpec, ChordError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(ChordError::Empty);
    }

    let mut modifiers = Modifiers::empty();
    let mut key: Option<(KeyCode, String)> = None;

    for (idx, raw) in trimmed.split('+').enumerate() {
        let part = raw.trim();
        if part.is_empty() {
            return Err(ChordError::EmptyToken { position: idx });
        }

        if let Some(flag) = match_modifier(part) {
            modifiers |= flag;
            continue;
        }

        let parsed_key = parse_key(part).ok_or_else(|| ChordError::UnknownPart {
            part: part.to_owned(),
        })?;

        if let Some((_, first)) = &key {
            return Err(ChordError::MultipleKeys {
                first: first.clone(),
                second: part.to_owned(),
            });
        }
        key = Some((parsed_key, part.to_owned()));
    }

    key.map(|(k, _)| ChordSpec::new(modifiers, k))
        .ok_or(ChordError::NoKey)
}

fn match_modifier(part: &str) -> Option<Modifiers> {
    match part.to_ascii_lowercase().as_str() {
        "ctrl" | "control" => Some(Modifiers::CTRL),
        "alt" | "option" | "opt" => Some(Modifiers::ALT),
        "shift" => Some(Modifiers::SHIFT),
        "meta" | "win" | "super" | "cmd" => Some(Modifiers::META),
        _ => None,
    }
}

fn parse_key(part: &str) -> Option<KeyCode> {
    if let [single] = part.as_bytes() {
        match single {
            b'[' => return Some(KeyCode::BracketLeft),
            b']' => return Some(KeyCode::BracketRight),
            b'-' => return Some(KeyCode::Minus),
            b'=' => return Some(KeyCode::Equal),
            _ => {
                if let Some(letter) = Letter::from_ascii(*single) {
                    return Some(KeyCode::Letter(letter));
                }
            },
        }
    }
    match part.to_ascii_lowercase().as_str() {
        "up" => Some(KeyCode::Arrow(Direction::Up)),
        "down" => Some(KeyCode::Arrow(Direction::Down)),
        "left" => Some(KeyCode::Arrow(Direction::Left)),
        "right" => Some(KeyCode::Arrow(Direction::Right)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_basic_chord() {
        let c = parse("Ctrl+Alt+R").expect("valid chord");
        assert!(c.modifiers.contains(Modifiers::CTRL));
        assert!(c.modifiers.contains(Modifiers::ALT));
        assert!(!c.modifiers.contains(Modifiers::SHIFT));
        assert!(!c.modifiers.contains(Modifiers::META));
        assert_eq!(c.key, KeyCode::Letter(Letter::from_ascii(b'R').unwrap()));
    }

    #[test]
    fn parser_is_case_insensitive_for_modifiers_and_keys() {
        let a = parse("ctrl+alt+r").unwrap();
        let b = parse("CTRL+ALT+r").unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn modifier_aliases_accepted() {
        assert!(parse("control+alt+q").is_ok());
        assert!(parse("opt+option+a").is_ok());
        assert!(parse("cmd+a").unwrap().modifiers.contains(Modifiers::META));
        assert!(
            parse("super+a")
                .unwrap()
                .modifiers
                .contains(Modifiers::META)
        );
        assert!(parse("win+a").unwrap().modifiers.contains(Modifiers::META));
    }

    #[test]
    fn punctuation_keys() {
        assert_eq!(parse("Ctrl+[").unwrap().key, KeyCode::BracketLeft);
        assert_eq!(parse("Ctrl+]").unwrap().key, KeyCode::BracketRight);
        assert_eq!(parse("Ctrl+-").unwrap().key, KeyCode::Minus);
        assert_eq!(parse("Ctrl+=").unwrap().key, KeyCode::Equal);
    }

    #[test]
    fn arrow_keys() {
        assert_eq!(parse("Up").unwrap().key, KeyCode::Arrow(Direction::Up));
        assert_eq!(parse("down").unwrap().key, KeyCode::Arrow(Direction::Down));
    }

    #[test]
    fn empty_input_errors() {
        assert_eq!(parse(""), Err(ChordError::Empty));
        assert_eq!(parse("   "), Err(ChordError::Empty));
    }

    #[test]
    fn empty_token_errors_at_correct_position() {
        let err = parse("Ctrl++R").unwrap_err();
        assert_eq!(err, ChordError::EmptyToken { position: 1 });
    }

    #[test]
    fn unknown_part_errors() {
        let err = parse("Ctrl+Funkey").unwrap_err();
        assert!(matches!(err, ChordError::UnknownPart { ref part } if part == "Funkey"));
    }

    #[test]
    fn multiple_keys_errors() {
        let err = parse("Ctrl+R+S").unwrap_err();
        assert_eq!(
            err,
            ChordError::MultipleKeys {
                first: "R".into(),
                second: "S".into(),
            }
        );
    }

    #[test]
    fn no_key_errors() {
        assert_eq!(parse("Ctrl+Alt").unwrap_err(), ChordError::NoKey);
    }

    #[test]
    fn display_round_trip_through_parse() {
        for input in &["Ctrl+Alt+R", "Shift+Up", "Ctrl+=", "Meta+Q"] {
            let parsed = parse(input).expect("parse");
            let printed = parsed.display();
            let reparsed = parse(&printed).expect("reparse");
            assert_eq!(parsed, reparsed, "round-trip failed for {input}");
        }
    }
}
