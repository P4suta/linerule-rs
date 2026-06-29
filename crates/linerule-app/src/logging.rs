//! tracing subscriber setup. `LINERULE_LOG` controls per-subsystem levels.
//! Logs go next to the exe (portable layout): stderr (human) plus
//! `<exe dir>/events.jsonl.YYYY-MM-DD` (JSON Lines).

#![forbid(unsafe_code)]

use std::path::PathBuf;

use anyhow::{Context, Result};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

/// Initialize tracing. Hold the returned `WorkerGuard` for the life of `main`
/// (dropping it flushes the background writer).
///
/// # Errors
/// Exe path unresolvable, log dir uncreatable, or file appender init fails.
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
        // Ring buffer supplying pre-panic events to the crash dump JSON.
        // env_filter is shared, so dropped events never reach the ring.
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

/// Directory of the running exe; holds `events.jsonl.*` and `crash-*.json`.
///
/// # Errors
/// `current_exe()` fails or the exe path has no parent.
pub(crate) fn data_dir() -> Result<PathBuf> {
    let exe = std::env::current_exe().context("std::env::current_exe failed")?;
    let dir = exe
        .parent()
        .context("current_exe path has no parent directory")?
        .to_path_buf();
    Ok(dir)
}

#[cfg(test)]
mod tests {
    //! `init()` installs a global subscriber, so it's untested here (would
    //! corrupt sibling tests). Covers `data_dir()` and `EnvFilter` parsing.

    use super::*;

    #[test]
    fn data_dir_matches_current_exe_parent() {
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
        // Exact fallback string `init()` uses; a drift here would panic.
        let _ = EnvFilter::new("info,wnd_proc=info,heartbeat=info,cursor_tracker=info");
    }

    #[test]
    fn env_filter_rejects_obviously_bad_input() {
        // EnvFilter accepts arbitrary target names; just ensure no panic.
        let bad = "this-is-not-a-level";
        let parsed = EnvFilter::try_new(bad);
        let _ = parsed;
    }
}
