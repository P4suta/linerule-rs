//! `boot()` — clap dispatch から呼ばれるブートストラップ。
//!
//! 流れ:
//! 1. tracing + crash dump を初期化（最優先、panic 時のレポートが残せるように）
//! 2. CLI 系コマンドなら console attach
//! 3. サブコマンド分岐:
//!    - `Run`: Windows のみ。`linerule-platform-windows` に委譲。
//!    - `Diagnostics`: `%APPDATA%\linerule\` の中身を pretty-print。
//!    - `Version`: バージョン情報。

#![forbid(unsafe_code)]

use anyhow::Result;
use uuid::Uuid;

use crate::cli::{Cli, Command};
use crate::{console, crash_dump, logging};

/// 実 main。
///
/// # Errors
/// 各サブコマンドが失敗したとき。
pub(crate) fn boot(cli: Cli) -> Result<()> {
    let run_id = Uuid::new_v4();
    crash_dump::install_panic_hook(run_id);
    let _guard = logging::init(cli.needs_console())?;

    if cli.needs_console() {
        console::ensure_console_attached();
    }

    match cli.command.unwrap_or(Command::Run) {
        Command::Run => run_overlay(),
        Command::Diagnostics { dry_run } => diagnostics(dry_run),
        Command::Version => {
            println!("linerule {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        },
    }
}

#[cfg(target_os = "windows")]
fn run_overlay() -> Result<()> {
    use linerule_core::UserConfig;
    use linerule_platform_windows::{OverlayWindow, monitor_info, run_message_pump};

    let _config = UserConfig::DEFAULT;
    let monitor = monitor_info::primary_bounds()?;
    let mut overlay = OverlayWindow::new(monitor)?;
    overlay.attach_dcomp()?;
    // TODO Phase E/F: HotkeyHost + RenderClock + TickPipeline を結線
    run_message_pump()?;
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn run_overlay() -> Result<()> {
    anyhow::bail!("`linerule run` is Windows-only");
}

fn diagnostics(_dry_run: bool) -> Result<()> {
    let data_dir = logging::data_dir()?;
    println!("linerule data dir: {}", data_dir.display());
    if data_dir.exists() {
        for entry in std::fs::read_dir(&data_dir)? {
            let entry = entry?;
            println!("  {}", entry.file_name().to_string_lossy());
        }
    } else {
        println!("  (directory does not exist yet — no events / crashes)");
    }
    Ok(())
}
