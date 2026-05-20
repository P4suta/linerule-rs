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
    Run {
        /// 指定 ms 経過後に自動終了する。CI smoke test 用。`Ctrl+Alt+Q` 等の
        /// hotkey 経由 quit と同じく `PostQuitMessage` で graceful に終わる。
        /// 未指定なら hotkey で終了するまで動作。
        #[arg(long, value_name = "MILLIS")]
        duration_ms: Option<u64>,
    },
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
        assert!(matches!(
            parse(&["run"]).command,
            Some(Command::Run { duration_ms: None })
        ));
    }

    #[test]
    fn parses_run_with_duration_ms() {
        match parse(&["run", "--duration-ms", "2000"]).command {
            Some(Command::Run { duration_ms }) => assert_eq!(duration_ms, Some(2000)),
            other => panic!("expected Run with duration, got {other:?}"),
        }
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

    // ---- error path (Cli::try_parse_from が Err を返すケース) ----------------

    /// 未知の subcommand は `UnknownArgument` 系の error で reject される。
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

    /// 未知の global flag (`--bogus`) は reject される。
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

    /// `--duration-ms` に整数として parse できない値を渡すと reject。
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

    /// `--duration-ms` に負値を渡すと reject (型が u64 のため "-100" は parse 失敗)。
    #[test]
    fn rejects_negative_duration_ms() {
        let err = Cli::try_parse_from(["linerule", "run", "--duration-ms", "-100"])
            .expect_err("negative duration should fail (u64)");
        let msg = err.to_string();
        // clap が `-100` を別の flag と解釈して fail することもあるので寛容に check
        assert!(!msg.is_empty(), "expected non-empty error message");
    }

    /// `--recent-events` に非整数を渡すと reject。
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

    /// subcommand 直後の余計な positional 引数は reject (subcommand は値を取らない)。
    #[test]
    fn rejects_extra_positional_after_subcommand() {
        let err = Cli::try_parse_from(["linerule", "version", "extra-positional"])
            .expect_err("extra positional should fail");
        let msg = err.to_string();
        assert!(!msg.is_empty(), "expected non-empty error message");
    }

    /// `--duration-ms` だけ subcommand なし は reject (Run subcommand の flag)。
    #[test]
    fn rejects_duration_ms_without_run_subcommand() {
        // duration-ms は Run subcommand 側にしかない flag なので、
        // top-level で渡すと unknown global flag として reject される。
        let err = Cli::try_parse_from(["linerule", "--duration-ms", "1000"])
            .expect_err("duration-ms outside Run subcommand should fail");
        let msg = err.to_string();
        assert!(!msg.is_empty(), "expected non-empty error message");
    }

    /// `version` は `--help` flag を持たない (`disable_help_subcommand` のため)。
    /// ただし `--help` global flag は parse 可能 (clap 標準)。これは reject では
    /// なく `DisplayHelp` で正常終了する系統 — error path とは別カテゴリ。
    /// ここでは "help" subcommand が定義されていないことを確認する。
    #[test]
    fn rejects_help_as_subcommand() {
        // `disable_help_subcommand = true` のため `help` という名前の
        // subcommand は存在しない (`linerule help` は Run の bogus arg 相当)。
        let err =
            Cli::try_parse_from(["linerule", "help"]).expect_err("help subcommand is disabled");
        let msg = err.to_string();
        assert!(!msg.is_empty(), "expected non-empty error message");
    }
}
