//! Stable, user-owned preferences persisted by the application.
//!
//! Runtime animation, rendering, and repeat-policy tunables deliberately do
//! not live here. This keeps the on-disk schema small and prevents internal
//! implementation details from becoming compatibility promises.

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    ActiveMode, BlurAmount, ChordError, ChordSpec, Modifiers, Opacity, OverlayAction,
    OverlayConfig, State, StateDelta, SurroundEffect, TapStepConfig, Thickness, parse_chord,
};

/// Current on-disk preferences schema.
pub const PREFERENCES_SCHEMA_VERSION: u32 = 1;

/// A user-configurable action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Command {
    /// Change the ruler axis.
    CycleMode,
    /// Change the surround effect.
    CycleEffect,
    /// Show or hide the ruler.
    ToggleOnOff,
    /// Increase ruler thickness.
    Thicker,
    /// Decrease ruler thickness.
    Thinner,
    /// Increase opacity or blur.
    MoreOpaque,
    /// Decrease opacity or blur.
    LessOpaque,
    /// Show or hide the complete shortcut guide.
    ToggleGuide,
    /// Exit linerule.
    Quit,
}

impl Command {
    /// Every configurable command in stable display order.
    pub const ALL: [Self; 9] = [
        Self::CycleMode,
        Self::CycleEffect,
        Self::ToggleOnOff,
        Self::Thicker,
        Self::Thinner,
        Self::MoreOpaque,
        Self::LessOpaque,
        Self::ToggleGuide,
        Self::Quit,
    ];

    /// Convert this command into the reducer action it represents.
    #[must_use]
    pub const fn action(self, tap_step: TapStepConfig) -> OverlayAction {
        match self {
            Self::CycleMode => OverlayAction::CycleMode,
            Self::CycleEffect => OverlayAction::CycleEffect,
            Self::ToggleOnOff => OverlayAction::ToggleOnOff,
            Self::Thicker => OverlayAction::BumpThickness(tap_step.thickness),
            Self::Thinner => OverlayAction::BumpThickness(-tap_step.thickness),
            Self::MoreOpaque => OverlayAction::BumpOpacity(tap_step.opacity),
            Self::LessOpaque => OverlayAction::BumpOpacity(-tap_step.opacity),
            Self::ToggleGuide => OverlayAction::ToggleHudDetail,
            Self::Quit => OverlayAction::Quit,
        }
    }
}

/// The ruler values that survive restarts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RulerPreferences {
    /// Axis restored the next time the ruler is shown.
    pub last_active: ActiveMode,
    /// Surround effect.
    pub effect: SurroundEffect,
    /// Slit width in logical pixels.
    pub thickness: Thickness,
    /// Mask opacity.
    pub opacity: Opacity,
    /// Backdrop blur level.
    pub blur: BlurAmount,
}

impl RulerPreferences {
    /// Default ruler values.
    pub const DEFAULT: Self = Self {
        last_active: ActiveMode::Horizontal,
        effect: SurroundEffect::DimBlack,
        thickness: Thickness::DEFAULT,
        opacity: Opacity::DEFAULT,
        blur: BlurAmount::DEFAULT,
    };

    /// Build the initial reducer state. Startup visibility is intentionally
    /// always off, regardless of the previous session.
    #[must_use]
    pub const fn initial_state(self) -> State {
        State {
            mode: crate::Mode::Off,
            last_active: self.last_active,
            config: OverlayConfig {
                effect: self.effect,
                thickness: self.thickness,
                opacity: self.opacity,
                blur: self.blur,
            },
        }
    }
}

impl Default for RulerPreferences {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Owned shortcut strings, serialized as a command-name-to-chord map.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct HotkeyBindings(BTreeMap<Command, String>);

impl HotkeyBindings {
    /// Return the chord assigned to a command.
    #[must_use]
    pub fn get(&self, command: Command) -> Option<&str> {
        self.0.get(&command).map(String::as_str)
    }

    /// Iterate in stable command order.
    pub fn iter(&self) -> impl Iterator<Item = (Command, &str)> {
        Command::ALL
            .into_iter()
            .filter_map(|command| self.get(command).map(|chord| (command, chord)))
    }

    /// Assign a chord. Call [`validate`](Self::validate) before committing the
    /// complete set.
    pub fn set(&mut self, command: Command, chord: impl Into<String>) {
        self.0.insert(command, chord.into());
    }

    /// Parse and validate all assignments as one transaction.
    ///
    /// # Errors
    /// Returns every missing, malformed, modifierless, or duplicate binding.
    pub fn validate(&self) -> Result<Vec<(Command, ChordSpec)>, BindingErrors> {
        let mut parsed = Vec::with_capacity(Command::ALL.len());
        let mut failures = Vec::new();
        let mut occupied: HashMap<ChordSpec, Command> = HashMap::new();

        for command in Command::ALL {
            let Some(raw) = self.get(command) else {
                failures.push(BindingError::Missing { command });
                continue;
            };
            match parse_chord(raw) {
                Ok(spec) => {
                    if spec.modifiers == Modifiers::empty() {
                        failures.push(BindingError::ModifierRequired { command });
                    } else if let Some(first) = occupied.insert(spec, command) {
                        failures.push(BindingError::Duplicate {
                            first,
                            second: command,
                            chord: spec.display(),
                        });
                    } else {
                        parsed.push((command, spec));
                    }
                },
                Err(source) => failures.push(BindingError::Invalid { command, source }),
            }
        }

        if failures.is_empty() {
            Ok(parsed)
        } else {
            Err(BindingErrors(failures))
        }
    }
}

impl Default for HotkeyBindings {
    fn default() -> Self {
        Self(BTreeMap::from([
            (Command::CycleMode, "Ctrl+Alt+R".to_owned()),
            (Command::CycleEffect, "Ctrl+Alt+E".to_owned()),
            (Command::ToggleOnOff, "Ctrl+Alt+H".to_owned()),
            (Command::Thicker, "Ctrl+Alt+Up".to_owned()),
            (Command::Thinner, "Ctrl+Alt+Down".to_owned()),
            (Command::MoreOpaque, "Ctrl+Alt+Right".to_owned()),
            (Command::LessOpaque, "Ctrl+Alt+Left".to_owned()),
            (Command::ToggleGuide, "Ctrl+Alt+K".to_owned()),
            (Command::Quit, "Ctrl+Alt+Q".to_owned()),
        ]))
    }
}

/// Stable versioned preferences document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preferences {
    /// Schema discriminator. Version 1 is the only writable version.
    pub schema_version: u32,
    /// Persisted ruler values.
    pub ruler: RulerPreferences,
    /// Persisted shortcut assignments.
    pub hotkeys: HotkeyBindings,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            schema_version: PREFERENCES_SCHEMA_VERSION,
            ruler: RulerPreferences::DEFAULT,
            hotkeys: HotkeyBindings::default(),
        }
    }
}

impl Preferences {
    /// Validate the version and every shortcut assignment.
    ///
    /// # Errors
    /// A future schema is preserved by callers and never silently rewritten.
    pub fn validate(&self) -> Result<(), PreferencesError> {
        if self.schema_version != PREFERENCES_SCHEMA_VERSION {
            return Err(PreferencesError::UnsupportedSchema {
                found: self.schema_version,
                supported: PREFERENCES_SCHEMA_VERSION,
            });
        }
        self.hotkeys.validate().map(|_| ()).map_err(Into::into)
    }
}

/// Pure reducer facade exposed at the workspace boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Engine {
    state: State,
}

impl Engine {
    /// Start from preferences while keeping startup visibility off.
    #[must_use]
    pub const fn from_preferences(preferences: &Preferences) -> Self {
        Self {
            state: preferences.ruler.initial_state(),
        }
    }

    /// Current user-visible reducer state.
    #[must_use]
    pub const fn state(self) -> State {
        self.state
    }

    /// Apply a command and return its semantic delta.
    pub fn apply(&mut self, command: Command, tap_step: TapStepConfig) -> StateDelta {
        let (next, delta) = crate::reduce(self.state, command.action(tap_step));
        self.state = next;
        delta
    }
}

/// One shortcut validation failure.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Error)]
pub enum BindingError {
    /// A required command has no map entry.
    #[error("missing shortcut for {command:?}")]
    Missing {
        /// Command without a shortcut.
        command: Command,
    },
    /// A chord string could not be parsed.
    #[error("invalid shortcut for {command:?}: {source}")]
    Invalid {
        /// Command with the invalid shortcut.
        command: Command,
        /// Parser error.
        source: ChordError,
    },
    /// A global shortcut without modifiers is unsafe to register.
    #[error("shortcut for {command:?} requires Ctrl, Alt, Shift, or Meta")]
    ModifierRequired {
        /// Command with the unsafe shortcut.
        command: Command,
    },
    /// Two commands resolve to the same chord.
    #[error("duplicate shortcut {chord}: {first:?} and {second:?}")]
    Duplicate {
        /// First command using the chord.
        first: Command,
        /// Second command using the chord.
        second: Command,
        /// Canonical chord text.
        chord: String,
    },
}

/// Complete set of shortcut validation failures.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Error)]
#[error("invalid shortcut assignments: {0:?}")]
pub struct BindingErrors(pub Vec<BindingError>);

/// Preferences validation error.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Error)]
pub enum PreferencesError {
    /// The file uses a schema this binary must not overwrite.
    #[error("unsupported preferences schema {found}; this build supports {supported}")]
    UnsupportedSchema {
        /// Version found on disk.
        found: u32,
        /// Version supported by this build.
        supported: u32,
    },
    /// Shortcut assignments are invalid.
    #[error(transparent)]
    Bindings(#[from] BindingErrors),
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use super::*;

    #[test]
    fn default_preferences_validate_and_start_off() {
        let preferences = Preferences::default();
        assert!(preferences.validate().is_ok());
        let state = preferences.ruler.initial_state();
        assert_eq!(state.mode, crate::Mode::Off);
        assert_eq!(state.last_active, ActiveMode::Horizontal);
    }

    #[test]
    fn command_actions_preserve_signed_decrements() {
        let step = TapStepConfig {
            thickness: 7,
            opacity: 9,
        };
        assert_eq!(
            Command::Thinner.action(step),
            OverlayAction::BumpThickness(-7)
        );
        assert_eq!(
            Command::LessOpaque.action(step),
            OverlayAction::BumpOpacity(-9)
        );
    }

    #[test]
    fn hotkey_iteration_is_complete_and_stably_ordered() {
        let bindings = HotkeyBindings::default();
        let commands: Vec<Command> = bindings.iter().map(|(command, _)| command).collect();
        assert_eq!(commands, Command::ALL);
    }

    #[test]
    fn serialized_schema_has_hotkey_map() {
        let json = serde_json::to_value(Preferences::default()).expect("serialize preferences");
        assert_eq!(json["schema_version"], 1);
        assert_eq!(json["hotkeys"]["toggle_on_off"], "Ctrl+Alt+H");
        assert!(json.get("anim").is_none());
        assert!(json.get("render").is_none());
    }

    #[test]
    fn duplicate_and_modifierless_bindings_are_rejected_together() {
        let mut bindings = HotkeyBindings::default();
        bindings.set(Command::CycleMode, "Ctrl+Alt+H");
        bindings.set(Command::Quit, "Q");
        let errors = bindings.validate().expect_err("two invalid assignments");
        assert!(errors.0.iter().any(|error| matches!(
            error,
            BindingError::Duplicate {
                first: Command::CycleMode | Command::ToggleOnOff,
                second: Command::CycleMode | Command::ToggleOnOff,
                ..
            }
        )));
        assert!(errors.0.iter().any(|error| matches!(
            error,
            BindingError::ModifierRequired {
                command: Command::Quit
            }
        )));
    }

    #[test]
    fn future_schema_is_rejected_without_mutation() {
        let preferences = Preferences {
            schema_version: 2,
            ..Preferences::default()
        };
        assert!(matches!(
            preferences.validate(),
            Err(PreferencesError::UnsupportedSchema {
                found: 2,
                supported: 1
            })
        ));
    }
}
