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
