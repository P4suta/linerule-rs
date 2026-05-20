//! clap derive ベースの CLI。`linerule.exe [run|diagnostics|version]`。

#![forbid(unsafe_code)]

use clap::{Parser, Subcommand};

/// linerule-rs CLI。
#[derive(Debug, Parser)]
#[command(
    name = "linerule",
    about = "Reading-ruler overlay for Windows",
    disable_help_subcommand = true
)]
pub(crate) struct Cli {
    /// 強制的にコンソール出力を attach する（GUI モードでも stderr が見える）。
    #[arg(long, global = true)]
    pub(crate) cli: bool,

    #[command(subcommand)]
    pub(crate) command: Option<Command>,
}

/// 利用可能なサブコマンド。
#[derive(Debug, Subcommand, Clone)]
pub(crate) enum Command {
    /// オーバーレイを起動する（デフォルト）。
    Run,
    /// `%APPDATA%\linerule\` の events.jsonl と crash-*.json を pretty-print する。
    Diagnostics {
        /// 何も書き出さずに pretty-print のみ（exit 0 を確認）。
        #[arg(long)]
        dry_run: bool,
    },
    /// バージョン情報を出力する。
    Version,
}

impl Cli {
    /// CLI 系コマンド（=stderr / stdout に出力する）かどうか。
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
            Some(Command::Diagnostics { dry_run }) => assert!(dry_run),
            other => panic!("got {other:?}"),
        }
    }

    #[test]
    fn parses_diagnostics_without_dry_run_defaults_false() {
        match parse(&["diagnostics"]).command {
            Some(Command::Diagnostics { dry_run }) => assert!(!dry_run),
            other => panic!("got {other:?}"),
        }
    }

    #[test]
    fn parses_run_subcommand() {
        assert!(matches!(parse(&["run"]).command, Some(Command::Run)));
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
}
