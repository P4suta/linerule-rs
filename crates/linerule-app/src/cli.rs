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
        /// data dir 列挙のみで何も書き出さない（exit 0 確認用）。
        #[arg(long)]
        dry_run: bool,
        /// 最新の `crash-*.json` を pretty-print する。
        #[arg(long)]
        last_crash: bool,
        /// 直近 `N` 件の event を `events.jsonl.<today>` の末尾から表示する。
        #[arg(long, value_name = "N")]
        recent_events: Option<usize>,
        /// data dir の絶対 path を 1 行だけ stdout に出す (script から `xargs ls`
        /// などで使う用)。
        #[arg(long)]
        data_dir: bool,
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
