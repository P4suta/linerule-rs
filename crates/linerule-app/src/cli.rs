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
