//! tracing subscriber setup. `LINERULE_LOG` controls per-subsystem levels.
//! Logs go to the selected data layout: stderr (human) plus daily JSON Lines.

#![forbid(unsafe_code)]

use std::time::Duration;
use std::{env, ffi::OsString};

use thiserror::Error;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use crate::event_ring::{EventRing, RingBufferLayer};
use crate::storage::StorageError;
use crate::storage::{DataPaths, prune_files};

/// Typed failures while installing the process logging pipeline.
#[derive(Debug, Error)]
pub(crate) enum LoggingError {
    #[error(transparent)]
    Storage(#[from] StorageError),
    #[error("cannot open the bounded daily JSON log: {0}")]
    OpenAppender(#[from] tracing_appender::rolling::InitError),
    #[error("cannot install the tracing subscriber: {0}")]
    InstallSubscriber(#[from] tracing_subscriber::util::TryInitError),
    #[error("LINERULE_LOG is not valid Unicode")]
    NonUnicodeFilter,
    #[error("invalid LINERULE_LOG value `{value}`: {source}")]
    InvalidFilter {
        value: String,
        source: tracing_subscriber::filter::ParseError,
    },
}

/// Owned logging resources. Dropping this value flushes the file writer.
pub(crate) struct LoggingSession {
    _guard: WorkerGuard,
    ring: EventRing,
}

impl LoggingSession {
    pub(crate) fn event_ring(&self) -> EventRing {
        self.ring.clone()
    }
}

/// Initialize tracing for one application session.
///
/// # Errors
/// Data-directory creation, retention, or global subscriber installation fails.
pub(crate) fn init(
    human_readable_stderr: bool,
    paths: &DataPaths,
) -> Result<LoggingSession, LoggingError> {
    paths.ensure_directories()?;
    prune_files(
        &paths.logs,
        "events.jsonl",
        Some(Duration::from_hours(168)),
        usize::MAX,
    )?;

    let file_appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix("events.jsonl")
        .max_log_files(7)
        .build(&paths.logs)?;
    let (file_writer, guard) = tracing_appender::non_blocking(file_appender);
    let ring = EventRing::new();

    let env_filter = configured_filter()?;

    let file_layer = tracing_subscriber::fmt::layer()
        .json()
        .with_target(true)
        .with_thread_names(true)
        .with_writer(file_writer);

    let registry = tracing_subscriber::registry()
        .with(env_filter)
        // Ring buffer supplying pre-panic events to the crash dump JSON.
        // env_filter is shared, so dropped events never reach the ring.
        .with(RingBufferLayer::new(ring.clone()))
        .with(file_layer);

    if human_readable_stderr {
        let stderr_layer = tracing_subscriber::fmt::layer()
            .with_target(true)
            .with_writer(std::io::stderr);
        registry.with(stderr_layer).try_init()?;
    } else {
        registry.try_init()?;
    }

    Ok(LoggingSession {
        _guard: guard,
        ring,
    })
}

fn configured_filter() -> Result<EnvFilter, LoggingError> {
    parse_filter(env::var_os("LINERULE_LOG"))
}

fn parse_filter(value: Option<OsString>) -> Result<EnvFilter, LoggingError> {
    let Some(value) = value else {
        return Ok(EnvFilter::new("info,wnd_proc=info,cursor_tracker=info"));
    };
    let value = value
        .into_string()
        .map_err(|_| LoggingError::NonUnicodeFilter)?;
    EnvFilter::try_new(&value).map_err(|source| LoggingError::InvalidFilter { value, source })
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    //! `init()` installs a global subscriber, so unit tests cover only the
    //! filter construction. Path and retention behavior lives in `storage`.

    use super::*;

    #[test]
    fn env_filter_parses_default_directive_used_by_init() {
        parse_filter(None).expect("default filter");
    }

    #[test]
    fn env_filter_accepts_an_injected_directive() {
        parse_filter(Some("linerule_core=trace".into())).expect("valid filter");
    }

    #[test]
    fn env_filter_rejects_invalid_input() {
        assert!(matches!(
            parse_filter(Some("linerule_core=not-a-level".into())),
            Err(LoggingError::InvalidFilter { .. })
        ));
    }
}
