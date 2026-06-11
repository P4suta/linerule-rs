//! clap-derive CLI. `linerule.exe [run|diagnostics|version]`.

#![forbid(unsafe_code)]

use clap::{Parser, Subcommand, ValueEnum};

/// linerule-rs CLI.
#[derive(Debug, Parser)]
#[command(
    name = "linerule",
    about = "Reading-ruler overlay for Windows",
    disable_help_subcommand = true
)]
pub(crate) struct Cli {
    /// Force-attach console output (see stderr even in GUI mode).
    #[arg(long, global = true)]
    pub(crate) cli: bool,

    #[command(subcommand)]
    pub(crate) command: Option<Command>,
}

/// Available subcommands.
#[derive(Debug, Subcommand, Clone)]
pub(crate) enum Command {
    /// Start the overlay (default).
    Run {
        /// Auto-quit after the given milliseconds. Exits gracefully via
        /// `PostQuitMessage`, like a hotkey quit. Omit to run until a hotkey
        /// quit.
        #[arg(long, value_name = "MILLIS")]
        duration_ms: Option<u64>,
        /// Override the initial overlay mode. Default is `Off` (a slit appears
        /// only after pressing Ctrl+Alt+H). Pass `horizontal` to exercise the
        /// slit render path from startup.
        #[arg(long, value_enum, value_name = "MODE")]
        initial_mode: Option<InitialMode>,
        /// Override the initial surround effect. Default is `Dim` (`DimBlack`).
        /// Pass `blur` to exercise the backdrop-blur path from startup.
        #[arg(long, value_enum, value_name = "EFFECT")]
        initial_effect: Option<InitialEffect>,
    },
    /// Pretty-print events.jsonl and crash-*.json from the data dir.
    Diagnostics {
        /// List the data dir only, write nothing (exit-0 check).
        #[arg(long)]
        dry_run: bool,
        /// Pretty-print the latest `crash-*.json`.
        #[arg(long)]
        last_crash: bool,
        /// Show the last `N` events from `events.jsonl.<today>`.
        #[arg(long, value_name = "N")]
        recent_events: Option<usize>,
        /// Print the absolute data-dir path on one line to stdout.
        #[arg(long)]
        data_dir: bool,
    },
    /// Print version info.
    Version,
}

/// Values for `--initial-mode`. Maps to `linerule_core::state::Mode`; defined
/// here as an app-layer boundary type to keep linerule-core off clap.
#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
#[clap(rename_all = "lowercase")]
pub(crate) enum InitialMode {
    /// `Mode::Off` (same as default).
    Off,
    /// `Mode::Horizontal`.
    Horizontal,
    /// `Mode::Vertical`.
    Vertical,
}

#[cfg(target_os = "windows")]
impl From<InitialMode> for linerule_core::Mode {
    fn from(m: InitialMode) -> Self {
        match m {
            InitialMode::Off => Self::Off,
            InitialMode::Horizontal => Self::Horizontal,
            InitialMode::Vertical => Self::Vertical,
        }
    }
}

/// Values for `--initial-effect`. Maps to
/// `linerule_core::state::SurroundEffect`; app-layer boundary type to keep
/// linerule-core off clap.
#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
#[clap(rename_all = "lowercase")]
pub(crate) enum InitialEffect {
    /// `SurroundEffect::DimBlack` (same as default).
    Dim,
    /// `SurroundEffect::WhiteWash`.
    White,
    /// `SurroundEffect::Blur` (backdrop blur).
    Blur,
}

#[cfg(target_os = "windows")]
impl From<InitialEffect> for linerule_core::SurroundEffect {
    fn from(e: InitialEffect) -> Self {
        match e {
            InitialEffect::Dim => Self::DimBlack,
            InitialEffect::White => Self::WhiteWash,
            InitialEffect::Blur => Self::Blur,
        }
    }
}

impl Cli {
    /// Whether this is a CLI command that writes to stderr/stdout.
    #[must_use]
    pub(crate) fn needs_console(&self) -> bool {
        if self.cli {
            return true;
        }
        matches!(
            self.command,
            Some(Command::Diagnostics { .. } | Command::Version)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Cli {
        let mut tokens = vec!["linerule"];
        tokens.extend_from_slice(args);
        Cli::try_parse_from(tokens).expect("clap should parse the fixture")
    }

    #[test]
    fn parses_version_subcommand() {
        assert!(matches!(
            parse(&["version"]).command,
            Some(Command::Version)
        ));
    }

    #[test]
    fn parses_diagnostics_with_dry_run() {
        match parse(&["diagnostics", "--dry-run"]).command {
            Some(Command::Diagnostics { dry_run, .. }) => assert!(dry_run),
            other => panic!("got {other:?}"),
        }
    }

    #[test]
    fn parses_diagnostics_without_dry_run_defaults_false() {
        match parse(&["diagnostics"]).command {
            Some(Command::Diagnostics { dry_run, .. }) => assert!(!dry_run),
            other => panic!("got {other:?}"),
        }
    }

    #[test]
    fn parses_diagnostics_last_crash() {
        match parse(&["diagnostics", "--last-crash"]).command {
            Some(Command::Diagnostics { last_crash, .. }) => assert!(last_crash),
            other => panic!("got {other:?}"),
        }
    }

    #[test]
    fn parses_diagnostics_recent_events_with_n() {
        match parse(&["diagnostics", "--recent-events", "20"]).command {
            Some(Command::Diagnostics { recent_events, .. }) => {
                assert_eq!(recent_events, Some(20));
            },
            other => panic!("got {other:?}"),
        }
    }

    #[test]
    fn parses_diagnostics_data_dir() {
        match parse(&["diagnostics", "--data-dir"]).command {
            Some(Command::Diagnostics { data_dir, .. }) => assert!(data_dir),
            other => panic!("got {other:?}"),
        }
    }

    #[test]
    fn parses_diagnostics_all_flags_combined() {
        match parse(&[
            "diagnostics",
            "--data-dir",
            "--last-crash",
            "--recent-events",
            "5",
        ])
        .command
        {
            Some(Command::Diagnostics {
                dry_run,
                last_crash,
                recent_events,
                data_dir,
            }) => {
                assert!(!dry_run);
                assert!(last_crash);
                assert_eq!(recent_events, Some(5));
                assert!(data_dir);
            },
            other => panic!("got {other:?}"),
        }
    }

    #[test]
    fn parses_run_subcommand() {
        assert!(matches!(
            parse(&["run"]).command,
            Some(Command::Run {
                duration_ms: None,
                initial_mode: None,
                initial_effect: None
            })
        ));
    }

    #[test]
    fn parses_run_with_duration_ms() {
        match parse(&["run", "--duration-ms", "2000"]).command {
            Some(Command::Run { duration_ms, .. }) => assert_eq!(duration_ms, Some(2000)),
            other => panic!("expected Run with duration, got {other:?}"),
        }
    }

    #[test]
    fn parses_run_with_initial_mode_horizontal() {
        match parse(&["run", "--initial-mode", "horizontal"]).command {
            Some(Command::Run { initial_mode, .. }) => {
                assert_eq!(initial_mode, Some(InitialMode::Horizontal));
            },
            other => panic!("expected Run with initial_mode horizontal, got {other:?}"),
        }
    }

    #[test]
    fn parses_run_with_initial_mode_vertical_and_duration() {
        match parse(&["run", "--initial-mode", "vertical", "--duration-ms", "1000"]).command {
            Some(Command::Run {
                duration_ms,
                initial_mode,
                ..
            }) => {
                assert_eq!(duration_ms, Some(1000));
                assert_eq!(initial_mode, Some(InitialMode::Vertical));
            },
            other => panic!("expected Run with both flags, got {other:?}"),
        }
    }

    #[test]
    fn parses_run_with_initial_effect_blur() {
        match parse(&["run", "--initial-effect", "blur"]).command {
            Some(Command::Run { initial_effect, .. }) => {
                assert_eq!(initial_effect, Some(InitialEffect::Blur));
            },
            other => panic!("expected Run with initial_effect blur, got {other:?}"),
        }
    }

    #[test]
    fn rejects_unknown_initial_mode_value() {
        let err = Cli::try_parse_from(["linerule", "run", "--initial-mode", "diagonal"])
            .expect_err("unknown initial-mode value should fail");
        let msg = err.to_string();
        assert!(!msg.is_empty(), "expected non-empty error message");
    }

    #[test]
    fn no_args_yields_none_command() {
        assert!(parse(&[]).command.is_none());
    }

    #[test]
    fn cli_flag_is_global_and_independent_of_subcommand() {
        let cli = parse(&["--cli", "version"]);
        assert!(cli.cli);
        assert!(matches!(cli.command, Some(Command::Version)));
    }

    #[test]
    fn needs_console_for_version() {
        assert!(parse(&["version"]).needs_console());
    }

    #[test]
    fn needs_console_for_diagnostics() {
        assert!(parse(&["diagnostics"]).needs_console());
        assert!(parse(&["diagnostics", "--dry-run"]).needs_console());
    }

    #[test]
    fn needs_console_for_run_only_with_cli_flag() {
        assert!(!parse(&["run"]).needs_console());
        assert!(parse(&["--cli", "run"]).needs_console());
    }

    #[test]
    fn needs_console_for_no_args_only_with_cli_flag() {
        assert!(!parse(&[]).needs_console());
        assert!(parse(&["--cli"]).needs_console());
    }

    // ---- error path (Cli::try_parse_from returns Err) -----------------------

    /// An unknown subcommand is rejected.
    #[test]
    fn rejects_unknown_subcommand() {
        let err = Cli::try_parse_from(["linerule", "bogus-subcommand"])
            .expect_err("unknown subcommand should fail");
        let msg = err.to_string();
        assert!(
            msg.contains("bogus-subcommand") || msg.contains("unrecognized"),
            "expected unknown-subcommand error, got: {msg}"
        );
    }

    /// An unknown global flag is rejected.
    #[test]
    fn rejects_unknown_global_flag() {
        let err = Cli::try_parse_from(["linerule", "--bogus-flag"])
            .expect_err("unknown global flag should fail");
        let msg = err.to_string();
        assert!(
            msg.contains("bogus-flag") || msg.contains("unrecognized") || msg.contains("--bogus"),
            "expected unknown-flag error, got: {msg}"
        );
    }

    /// A non-integer `--duration-ms` value is rejected.
    #[test]
    fn rejects_non_numeric_duration_ms() {
        let err = Cli::try_parse_from(["linerule", "run", "--duration-ms", "abc"])
            .expect_err("non-numeric duration should fail");
        let msg = err.to_string();
        assert!(
            msg.contains("duration-ms") || msg.contains("invalid value"),
            "expected duration parse error, got: {msg}"
        );
    }

    /// A negative `--duration-ms` is rejected (the type is u64).
    #[test]
    fn rejects_negative_duration_ms() {
        let err = Cli::try_parse_from(["linerule", "run", "--duration-ms", "-100"])
            .expect_err("negative duration should fail (u64)");
        let msg = err.to_string();
        // clap may instead read `-100` as a flag; check leniently.
        assert!(!msg.is_empty(), "expected non-empty error message");
    }

    /// A non-integer `--recent-events` value is rejected.
    #[test]
    fn rejects_non_numeric_recent_events() {
        let err = Cli::try_parse_from(["linerule", "diagnostics", "--recent-events", "abc"])
            .expect_err("non-numeric recent-events should fail");
        let msg = err.to_string();
        assert!(
            msg.contains("recent-events") || msg.contains("invalid value"),
            "expected recent-events parse error, got: {msg}"
        );
    }

    /// An extra positional after a subcommand is rejected (none take a value).
    #[test]
    fn rejects_extra_positional_after_subcommand() {
        let err = Cli::try_parse_from(["linerule", "version", "extra-positional"])
            .expect_err("extra positional should fail");
        let msg = err.to_string();
        assert!(!msg.is_empty(), "expected non-empty error message");
    }

    /// `--duration-ms` without the Run subcommand is rejected.
    #[test]
    fn rejects_duration_ms_without_run_subcommand() {
        // duration-ms exists only on Run, so at top level it is an unknown
        // global flag.
        let err = Cli::try_parse_from(["linerule", "--duration-ms", "1000"])
            .expect_err("duration-ms outside Run subcommand should fail");
        let msg = err.to_string();
        assert!(!msg.is_empty(), "expected non-empty error message");
    }

    /// No `help` subcommand exists (`disable_help_subcommand = true`).
    #[test]
    fn rejects_help_as_subcommand() {
        let err =
            Cli::try_parse_from(["linerule", "help"]).expect_err("help subcommand is disabled");
        let msg = err.to_string();
        assert!(!msg.is_empty(), "expected non-empty error message");
    }
}
