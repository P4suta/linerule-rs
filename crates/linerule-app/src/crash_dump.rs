//! Panic hook がトリガーされたとき、`%APPDATA%\linerule\crash-<runid>-<ts>.json`
//! にクラッシュレポートを同期書き出す。

#![forbid(unsafe_code)]

use std::panic::PanicHookInfo;
use std::path::PathBuf;

use serde::Serialize;
use uuid::Uuid;

use crate::logging;

/// アプリ開始時に panic hook をインストールする。
pub(crate) fn install_panic_hook(run_id: Uuid) {
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if let Err(e) = write_crash_dump(info, run_id) {
            // crash dump 自体が失敗した場合は stderr に流すしかない。
            eprintln!("crash_dump write failed: {e}");
        }
        // 標準 hook も呼んで通常通り backtrace を表示
        prev(info);
    }));
}

#[derive(Debug, Serialize)]
struct CrashRecord<'a> {
    run_id: Uuid,
    unix_ms: i128,
    message: String,
    location: Option<CrashLocation<'a>>,
    backtrace: String,
}

#[derive(Debug, Serialize)]
struct CrashLocation<'a> {
    file: &'a str,
    line: u32,
    col: u32,
}

fn write_crash_dump(info: &PanicHookInfo<'_>, run_id: Uuid) -> anyhow::Result<()> {
    let unix_ms: i128 = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i128::try_from(d.as_millis()).unwrap_or(i128::MAX));

    let message = info
        .payload()
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| info.payload().downcast_ref::<String>().map(String::as_str))
        .unwrap_or("<non-string payload>")
        .to_string();

    let location = info.location().map(|loc| CrashLocation {
        file: loc.file(),
        line: loc.line(),
        col: loc.column(),
    });

    let backtrace = std::backtrace::Backtrace::force_capture().to_string();

    let record = CrashRecord {
        run_id,
        unix_ms,
        message,
        location,
        backtrace,
    };

    let path = crash_path(run_id, unix_ms)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = std::fs::File::create(&path)?;
    serde_json::to_writer_pretty(&mut file, &record)?;
    Ok(())
}

fn crash_path(run_id: Uuid, unix_ms: i128) -> anyhow::Result<PathBuf> {
    let dir = logging::data_dir()?;
    Ok(dir.join(format!("crash-{run_id}-{unix_ms}.json")))
}

#[cfg(test)]
mod tests {
    //! Unit tests for crash filename construction and JSON schema.
    //!
    //! `install_panic_hook` cannot be exercised inside the test harness
    //! without poisoning subsequent tests (it replaces the global hook).
    //! We test the file-name pattern and the JSON shape via the struct.

    use super::*;
    use serde::Deserialize;

    /// Test-only counterpart of `CrashRecord` for deserialization.
    #[derive(Debug, Deserialize)]
    struct ReadCrashRecord {
        run_id: Uuid,
        unix_ms: i128,
        message: String,
        location: Option<ReadCrashLocation>,
        backtrace: String,
    }

    #[derive(Debug, Deserialize)]
    struct ReadCrashLocation {
        file: String,
        line: u32,
        col: u32,
    }

    #[test]
    fn crash_record_serializes_round_trip_via_serde_json() {
        let r = CrashRecord {
            run_id: Uuid::nil(),
            unix_ms: 1_700_000_000_000,
            message: "boom".to_string(),
            location: Some(CrashLocation {
                file: "src/main.rs",
                line: 42,
                col: 7,
            }),
            backtrace: "<stack frames>".to_string(),
        };
        let json = serde_json::to_string(&r).expect("serialize");
        let parsed: ReadCrashRecord = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.run_id, Uuid::nil());
        assert_eq!(parsed.unix_ms, 1_700_000_000_000);
        assert_eq!(parsed.message, "boom");
        assert_eq!(parsed.backtrace, "<stack frames>");
        let loc = parsed.location.expect("location set");
        assert_eq!(loc.file, "src/main.rs");
        assert_eq!(loc.line, 42);
        assert_eq!(loc.col, 7);
    }

    #[test]
    fn crash_record_with_none_location_serializes_as_null() {
        let r = CrashRecord {
            run_id: Uuid::nil(),
            unix_ms: 0,
            message: "no-location".to_string(),
            location: None,
            backtrace: String::new(),
        };
        let json = serde_json::to_string(&r).expect("serialize");
        assert!(
            json.contains("\"location\":null"),
            "expected null location in JSON, got: {json}"
        );
    }

    #[test]
    fn crash_path_filename_has_expected_shape() {
        // `crash_path` resolves the data-dir from the OS; we can't isolate
        // that without env-mocking, so we assert via the returned PathBuf's
        // final component pattern.
        let run = Uuid::nil();
        let ts: i128 = 1_700_000_000_000;
        if let Ok(p) = crash_path(run, ts) {
            let name = p
                .file_name()
                .and_then(|n| n.to_str())
                .expect("file_name UTF-8");
            assert!(
                name.starts_with("crash-"),
                "filename should start with `crash-`, got `{name}`"
            );
            assert!(
                std::path::Path::new(name)
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("json")),
                "filename should end with `.json`, got `{name}`"
            );
            assert!(
                name.contains(&run.to_string()),
                "filename should include run_id `{run}`, got `{name}`"
            );
            assert!(
                name.contains(&ts.to_string()),
                "filename should include unix_ms `{ts}`, got `{name}`"
            );
        }
    }
}
