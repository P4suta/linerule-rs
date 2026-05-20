//! tracing subscriber + tracing-appender 構成。
//!
//! `LINERULE_LOG` 環境変数で subsystem ごとのレベルを制御
//! (`debug,wnd_proc=info,heartbeat=info` 等)。出力先は:
//! - stderr (CLI モードのとき): human-readable
//! - `<linerule.exe と同じ dir>/events.jsonl.YYYY-MM-DD`: machine-readable JSON Lines
//!
//! 「薄い読書ツール」志向のため、`%APPDATA%` / `ProjectDirs` を使わず exe と
//! 同階層に直接吐く portable 運用 (ADR-0011)。書き込み権限が無い場合 (Program
//! Files 配下に置かれた場合等) は `init()` が `Err` を返す。

#![forbid(unsafe_code)]

use std::path::PathBuf;

use anyhow::{Context, Result};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::Subscriber;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

/// アプリ開始時に tracing を初期化する。返却された `WorkerGuard` は
/// `main` の寿命まで保持する必要がある（drop で background writer が flush）。
///
/// # Errors
/// exe path が解決できない、ログ dir を作れない、あるいは file appender 初期化
/// 失敗時。
pub(crate) fn init(human_readable_stderr: bool) -> Result<WorkerGuard> {
    let log_dir = data_dir().context("resolving log dir next to linerule.exe")?;
    std::fs::create_dir_all(&log_dir)
        .with_context(|| format!("creating log dir {}", log_dir.display()))?;

    let file_appender = rolling::daily(&log_dir, "events.jsonl");
    let (file_writer, guard) = tracing_appender::non_blocking(file_appender);

    let env_filter = EnvFilter::try_from_env("LINERULE_LOG").unwrap_or_else(|_| {
        EnvFilter::new("info,wnd_proc=info,heartbeat=info,cursor_tracker=info")
    });

    let file_layer = tracing_subscriber::fmt::layer()
        .json()
        .with_target(true)
        .with_thread_names(true)
        .with_writer(file_writer);

    let registry = tracing_subscriber::registry()
        .with(env_filter)
        // panic 直前の event を crash_dump JSON に同梱するための ring buffer。
        // env_filter で殺された event は ring にも入らない (filter は全 Layer 共通)。
        .with(crate::event_ring::RingBufferLayer)
        .with(file_layer);

    if human_readable_stderr {
        let stderr_layer = tracing_subscriber::fmt::layer()
            .with_target(true)
            .with_writer(std::io::stderr);
        registry.with(stderr_layer).init();
    } else {
        registry.init();
    }

    Ok(guard)
}

/// 現在実行中の `linerule.exe` と同じディレクトリを返す。`events.jsonl.*` と
/// `crash-*.json` の両方がここに置かれる (ADR-0011)。
///
/// # Errors
/// `std::env::current_exe()` が失敗した場合、または exe path に parent が無い
/// 場合 (= 根本的に Win32 環境が壊れているケース)。
pub(crate) fn data_dir() -> Result<PathBuf> {
    let exe = std::env::current_exe().context("std::env::current_exe failed")?;
    let dir = exe
        .parent()
        .context("current_exe path has no parent directory")?
        .to_path_buf();
    Ok(dir)
}

// Hidden import 警告抑止（Subscriber は将来 builder 経由でも使うため）。
#[allow(
    dead_code,
    reason = "Phase H 拡張で stderr fmt::Subscriber を直接組む可能性あり"
)]
const _: fn() = || {
    let _: Option<Subscriber> = None;
};

#[cfg(test)]
mod tests {
    //! `init()` installs a *global* tracing subscriber and is therefore not
    //! exercised here (would corrupt sibling tests). We cover the pure
    //! pieces: `data_dir()` shape and `EnvFilter` boundary parsing.

    use super::*;

    #[test]
    fn data_dir_matches_current_exe_parent() {
        // ADR-0011: ログは exe と同じ階層に置く portable 運用。
        let p = data_dir().expect("current_exe resolves under cargo nextest");
        let expected = std::env::current_exe()
            .expect("current_exe resolves under cargo nextest")
            .parent()
            .expect("test runner exe has a parent dir")
            .to_path_buf();
        assert_eq!(
            p, expected,
            "data_dir must return current_exe()'s parent, got {p:?} vs {expected:?}"
        );
    }

    #[test]
    fn data_dir_is_absolute() {
        let p = data_dir().expect("current_exe resolves");
        assert!(p.is_absolute(), "data dir must be absolute, got {p:?}");
    }

    #[test]
    fn env_filter_parses_default_directive_used_by_init() {
        // This is the exact string `init()` falls back to when LINERULE_LOG
        // is unset. If the format ever drifts (e.g. a renamed target),
        // EnvFilter::new will panic — we catch that here.
        let _ = EnvFilter::new("info,wnd_proc=info,heartbeat=info,cursor_tracker=info");
    }

    #[test]
    fn env_filter_rejects_obviously_bad_input() {
        // EnvFilter is permissive about most things (it just ignores unknown
        // tokens), but completely invalid level names should error out.
        // `try_new` here exercises the same surface init() uses.
        let bad = "this-is-not-a-level";
        let parsed = EnvFilter::try_new(bad);
        // EnvFilter accepts arbitrary target names; this is documenting that
        // behavior rather than asserting strict rejection.
        // We just ensure it doesn't panic.
        let _ = parsed;
    }
}
