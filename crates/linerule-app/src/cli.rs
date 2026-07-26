//! Public command-line contract.

#![forbid(unsafe_code)]

use clap::{ArgGroup, Args, Parser, Subcommand};

/// linerule command line.
#[derive(Debug, Parser)]
#[command(
    name = "linerule",
    about = "Reading-ruler overlay for Windows",
    version = crate::version::VERSION,
    disable_help_subcommand = true
)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Option<Command>,
}

/// Supported launch commands. No subcommand starts the resident shell.
#[derive(Debug, Subcommand, Clone)]
pub(crate) enum Command {
    /// Open the shortcut settings window.
    Settings,
    /// Inspect logs, crashes, or the selected data directory.
    Diagnostics(DiagnosticsArgs),
    /// Print version information.
    Version,
}

/// Mutually exclusive diagnostics selectors.
#[derive(Debug, Args, Clone, Default)]
#[command(group(
    ArgGroup::new("selector")
        .args(["last_crash", "recent_events", "data_dir"])
        .multiple(false)
))]
pub(crate) struct DiagnosticsArgs {
    /// Pretty-print the latest crash report.
    #[arg(long)]
    pub(crate) last_crash: bool,
    /// Show the last `N` structured events.
    #[arg(long, value_name = "N")]
    pub(crate) recent_events: Option<usize>,
    /// Print the absolute data directory.
    #[arg(long)]
    pub(crate) data_dir: bool,
}

impl Cli {
    /// CLI commands need a console; resident/settings launches do not.
    #[must_use]
    pub(crate) const fn needs_console(&self) -> bool {
        matches!(
            self.command,
            Some(Command::Diagnostics(_) | Command::Version)
        )
    }
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Cli {
        let mut tokens = vec!["linerule"];
        tokens.extend_from_slice(args);
        Cli::try_parse_from(tokens).expect("fixture parses")
    }

    #[test]
    fn no_arguments_launches_resident_shell() {
        assert!(parse(&[]).command.is_none());
        assert!(!parse(&[]).needs_console());
    }

    #[test]
    fn settings_is_gui_command() {
        assert!(matches!(
            parse(&["settings"]).command,
            Some(Command::Settings)
        ));
        assert!(!parse(&["settings"]).needs_console());
    }

    #[test]
    fn diagnostics_and_version_need_console() {
        assert!(parse(&["diagnostics"]).needs_console());
        assert!(parse(&["version"]).needs_console());
    }

    #[test]
    fn diagnostics_selectors_are_mutually_exclusive() {
        let error = Cli::try_parse_from(["linerule", "diagnostics", "--data-dir", "--last-crash"])
            .expect_err("selectors conflict");
        assert!(error.to_string().contains("cannot be used with"));
    }

    #[test]
    fn removed_test_flags_are_rejected() {
        for arguments in [
            &["run"][..],
            &["--cli"][..],
            &["diagnostics", "--dry-run"][..],
            &["--duration-ms", "10"][..],
        ] {
            assert!(
                Cli::try_parse_from(std::iter::once("linerule").chain(arguments.iter().copied()))
                    .is_err(),
                "removed arguments unexpectedly accepted: {arguments:?}"
            );
        }
    }
}
