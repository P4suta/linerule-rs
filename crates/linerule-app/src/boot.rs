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

    dispatch_command(cli)
}

/// `boot()` から global subscriber 初期化と panic hook 設置を取り除いた本体。
/// テストで `#[traced_test]` を当てる用。
///
/// # Errors
/// 各サブコマンドが失敗したとき。
pub(crate) fn dispatch_command(cli: Cli) -> Result<()> {
    match cli.command.unwrap_or(Command::Run) {
        Command::Run => run_overlay(),
        Command::Diagnostics { dry_run } => diagnostics(dry_run),
        Command::Version => {
            println!("linerule {}", env!("CARGO_PKG_VERSION"));
            tracing::info!(version = env!("CARGO_PKG_VERSION"), "linerule version");
            Ok(())
        },
    }
}

#[cfg(target_os = "windows")]
fn run_overlay() -> Result<()> {
    use linerule_core::UserConfig;
    use linerule_platform_windows::{
        OverlayWindow, RenderClock, monitor_info, run_message_pump, set_dpi_aware,
    };

    // 最初に DPI awareness を Per-Monitor V2 に設定する。Window 作成前に呼ぶ
    // 必要があるため `OverlayWindow::new` より前に置く。失敗しても fatal には
    // せず log のみ（既に dpi awareness が manifested 等のケース）。
    if let Err(e) = set_dpi_aware() {
        tracing::warn!(error = %e, "SetProcessDpiAwarenessContext failed; continuing with default awareness");
    }

    let config = UserConfig::DEFAULT;
    // virtual screen bounds (全 monitor を覆う矩形) を使い、multi-monitor 環境で
    // overlay HWND がモニタ境界を跨いで slit を引けるようにする。
    let monitor = monitor_info::virtual_screen_bounds()?;

    // Drop order が重要: pacer (`_clock`) は overlay HWND に `PostMessageW` を
    // 投げ続けるので、overlay HWND が破棄される前に pacer を止める必要がある。
    // Rust の逆順 Drop（後に宣言した変数が先に Drop される）を活かすため、
    // overlay → _clock の順に変数を宣言する。
    let mut overlay = OverlayWindow::new(monitor, config.hud)?;
    overlay.attach_dcomp()?;
    overlay.register_hotkeys(&config.hotkeys, config.input.tap_step)?;
    let _clock = RenderClock::spawn(overlay.hwnd())?;

    tracing::info!(
        cycle_mode = config.hotkeys.cycle_mode,
        toggle_visible = config.hotkeys.toggle_visible,
        quit = config.hotkeys.quit,
        "overlay running; press Ctrl+Alt+R to cycle modes, Ctrl+Alt+Q to quit"
    );
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
    tracing::info!(data_dir = %data_dir.display(), "linerule data dir");
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

#[cfg(test)]
mod tests {
    //! Log-assertion tests for `dispatch_command`. These exercise the post-
    //! initialization code path under a `#[traced_test]`-installed subscriber,
    //! letting us assert that key user-visible events are actually emitted.

    use super::*;
    use clap::Parser;
    use tracing_test::traced_test;

    fn parse(args: &[&str]) -> Cli {
        let mut tokens = vec!["linerule"];
        tokens.extend_from_slice(args);
        Cli::try_parse_from(tokens).expect("clap should parse the fixture")
    }

    #[traced_test]
    #[test]
    fn version_dispatch_emits_info_with_package_version() {
        dispatch_command(parse(&["version"])).expect("version subcommand");
        let pkg = env!("CARGO_PKG_VERSION");
        assert!(
            logs_contain(pkg),
            "info event should include CARGO_PKG_VERSION"
        );
        assert!(
            logs_contain("linerule version"),
            "version event should carry the `linerule version` message"
        );
    }

    #[traced_test]
    #[test]
    fn diagnostics_dispatch_emits_data_dir_event() {
        // diagnostics() touches the OS data dir but tolerates missing
        // directories. We just check that the data-dir info event fired
        // before any I/O failure.
        let _ = dispatch_command(parse(&["diagnostics", "--dry-run"]));
        assert!(
            logs_contain("linerule data dir"),
            "diagnostics should log the data dir"
        );
    }
}
