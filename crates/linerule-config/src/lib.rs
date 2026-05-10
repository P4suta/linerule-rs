#![forbid(unsafe_code)]

//! TOML configuration schema and loader for linerule.
//!
//! Wraps [`linerule_core::OverlayConfig`] in a fuller user-facing schema
//! that adds hotkey bindings and persistence concerns. All errors carry
//! [`miette`]-friendly diagnostics with source spans so the
//! `linerule config` subcommand can render actionable messages at the
//! TTY boundary.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use linerule_core::OverlayConfig;
use miette::{Diagnostic, NamedSource, SourceSpan};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::instrument;

/// Top-level user-facing configuration loaded from `config.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields, default)]
#[non_exhaustive]
pub struct Config {
    /// Visual overlay parameters.
    pub overlay: OverlayConfig,
    /// User-overridable hotkey bindings.
    pub hotkeys: HotkeyMap,
}

/// Map of [`Action`](linerule_core::Action) names to chord strings.
///
/// Chord parsing happens at the platform layer; this struct keeps the raw
/// strings so the config is decoupled from any specific hotkey parser.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct HotkeyMap {
    /// Combo for `Action::CycleMode`.
    pub cycle_mode: String,
    /// Combo for `Action::ToggleVisible`.
    pub toggle_visible: String,
    /// Combo for `Action::BumpThickness(+1)`.
    pub thicker: String,
    /// Combo for `Action::BumpThickness(-1)`.
    pub thinner: String,
    /// Combo for `Action::BumpOpacity(+5)`.
    pub more_opaque: String,
    /// Combo for `Action::BumpOpacity(-5)`.
    pub less_opaque: String,
    /// Combo for `Action::TogglePause`. While paused the overlay
    /// freezes at its current position — useful for stopping the
    /// follow-the-cursor behaviour while you read a specific line.
    /// Defaults to `Ctrl+Alt+P`.
    pub pause: String,
    /// Combo for `Action::Quit`. The user's emergency-exit path when
    /// the always-on-top overlay covers the whole screen and another
    /// hotkey collision wedges them out. Defaults to `Ctrl+Alt+Q`,
    /// chosen NOT to clash with common system bindings.
    pub quit: String,
}

impl Default for HotkeyMap {
    fn default() -> Self {
        Self {
            cycle_mode: "Ctrl+Alt+R".into(),
            toggle_visible: "Ctrl+Alt+H".into(),
            thicker: "Ctrl+Alt+]".into(),
            thinner: "Ctrl+Alt+[".into(),
            more_opaque: "Ctrl+Alt+=".into(),
            less_opaque: "Ctrl+Alt+-".into(),
            pause: "Ctrl+Alt+P".into(),
            quit: "Ctrl+Alt+Q".into(),
        }
    }
}

// ===========================================================================
// Errors
// ===========================================================================

/// Errors produced by the config loader, carrying [`miette`] diagnostics.
#[derive(Debug, Error, Diagnostic)]
#[non_exhaustive]
pub enum ConfigError {
    /// IO failed (file not found, permission denied, etc.).
    #[error("could not read config file at {path}")]
    #[diagnostic(code(linerule::config::io))]
    Io {
        /// Path that was attempted.
        path: PathBuf,
        /// Underlying IO error.
        #[source]
        source: io::Error,
    },

    /// TOML parse failed; the diagnostic carries a labelled span into the
    /// source so `miette` can render an actionable error at the TTY.
    #[error("malformed TOML in config file")]
    #[diagnostic(code(linerule::config::parse))]
    Parse {
        /// Original source for diagnostic rendering.
        #[source_code]
        src: NamedSource<String>,
        /// Span pointing at the offending region.
        #[label("here")]
        span: SourceSpan,
        /// Human-readable parser message.
        #[help]
        help: String,
    },

    /// Could not determine the platform-default config directory.
    #[error("could not determine the platform config directory")]
    #[diagnostic(code(linerule::config::default_path))]
    NoDefaultPath,
}

// ===========================================================================
// Loaders — implementation lands in task #7
// ===========================================================================

/// Resolve the platform-default location of `config.toml`.
///
/// On Windows: `%APPDATA%\linerule\config.toml`. On other platforms: the
/// equivalent of `dirs::config_dir() / linerule / config.toml`.
///
/// # Errors
/// Returns [`ConfigError::NoDefaultPath`] if no platform config dir resolves.
pub fn default_path() -> Result<PathBuf, ConfigError> {
    dirs::config_dir()
        .map(|d| d.join("linerule").join("config.toml"))
        .ok_or(ConfigError::NoDefaultPath)
}

/// Load and parse the config file at `path`.
///
/// # Errors
/// Returns [`ConfigError::Io`] on IO failure or [`ConfigError::Parse`] if
/// the TOML is malformed (the diagnostic carries a span pointing at the
/// offending location).
#[instrument(skip_all, fields(path = %path.display()))]
pub fn load(path: &Path) -> Result<Config, ConfigError> {
    let body = fs::read_to_string(path).map_err(|source| ConfigError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    parse_str(path, &body)
}

/// Parse a TOML string into a [`Config`].
///
/// `path` is used solely for error reporting (the [`NamedSource`] label).
///
/// # Errors
/// Returns [`ConfigError::Parse`] with a labelled span if the TOML is malformed.
#[instrument(skip_all)]
pub fn parse_str(path: &Path, body: &str) -> Result<Config, ConfigError> {
    toml::from_str::<Config>(body).map_err(|err| {
        let span = err.span().map_or_else(
            || SourceSpan::from((0, body.len().min(1))),
            |r| SourceSpan::from((r.start, r.end.saturating_sub(r.start).max(1))),
        );
        ConfigError::Parse {
            src: NamedSource::new(path.display().to_string(), body.to_owned()),
            span,
            help: err.message().to_owned(),
        }
    })
}
