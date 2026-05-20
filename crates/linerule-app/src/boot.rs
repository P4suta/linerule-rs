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
/// `run_id` は panic hook と tracing root span の両方に渡し、`events.jsonl` と
/// `crash-<run_id>-*.json` を機械的に紐付けられるようにする。span の lifetime
/// が boot 全体を覆うので、全 subcommand (`Run` / `Diagnostics` / `Version`) の
/// `tracing` event に `run_id` field が自動付与される。
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

    // subscriber init 後に root span を enter。`tracing-subscriber` の JSON layer
    // は親 span の field を `span` キーに自動付与するので、本 span 内で発火する
    // 全 event の `events.jsonl` 行に `"run_id":"<UUID>"` が乗る。
    let root = tracing::info_span!("linerule_run", run_id = %run_id);
    let _entered = root.enter();
    tracing::info!(run_id = %run_id, version = env!("CARGO_PKG_VERSION"), "linerule boot");

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
        Command::Diagnostics {
            dry_run,
            last_crash,
            recent_events,
            data_dir,
        } => diagnostics(DiagnosticsArgs {
            dry_run,
            last_crash,
            recent_events,
            data_dir,
        }),
        Command::Version => {
            println!("linerule {}", env!("CARGO_PKG_VERSION"));
            tracing::info!(version = env!("CARGO_PKG_VERSION"), "linerule version");
            Ok(())
        },
    }
}

/// `diagnostics` サブコマンドの flag をまとめた struct (`clap` の構造を本体
/// 関数のシグネチャに直接持ち込まないため)。
#[derive(Debug, Clone, Copy, Default)]
struct DiagnosticsArgs {
    dry_run: bool,
    last_crash: bool,
    recent_events: Option<usize>,
    data_dir: bool,
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

fn diagnostics(args: DiagnosticsArgs) -> Result<()> {
    let data_dir = logging::data_dir()?;

    // `--data-dir`: path だけ stdout に書いて exit。script pipe 用 ergonomic。
    if args.data_dir {
        println!("{}", data_dir.display());
        tracing::info!(data_dir = %data_dir.display(), "linerule --data-dir");
        return Ok(());
    }

    // `--last-crash`: 最新の crash-*.json を pretty-print。
    if args.last_crash {
        return print_last_crash(&data_dir);
    }

    // `--recent-events N`: events.jsonl.<today> の末尾 N 行を JSON pretty-print。
    if let Some(n) = args.recent_events {
        return print_recent_events(&data_dir, n);
    }

    // Default (or `--dry-run`): data dir 列挙のみ。
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
    let _ = args.dry_run; // 既存挙動の明示 (file 列挙以外の I/O はもともと無い)
    Ok(())
}

/// `%APPDATA%\linerule\crash-*.json` を mtime で並べ最新を pretty-print する。
fn print_last_crash(data_dir: &std::path::Path) -> Result<()> {
    if !data_dir.exists() {
        println!("(no crash dumps — data dir does not exist)");
        return Ok(());
    }
    let latest = std::fs::read_dir(data_dir)?
        .filter_map(std::result::Result::ok)
        .filter(|e| e.file_name().to_string_lossy().starts_with("crash-"))
        .filter(|e| e.file_name().to_string_lossy().ends_with(".json"))
        .filter_map(|e| {
            let modified = e.metadata().ok()?.modified().ok()?;
            Some((modified, e.path()))
        })
        .max_by_key(|(modified, _)| *modified);
    let Some((_, path)) = latest else {
        println!("(no crash dumps in {})", data_dir.display());
        return Ok(());
    };
    println!("# {}", path.display());
    let raw = std::fs::read_to_string(&path)?;
    let value: serde_json::Value = serde_json::from_str(&raw)?;
    println!("{}", serde_json::to_string_pretty(&value)?);
    tracing::info!(crash_path = %path.display(), "diagnostics --last-crash");
    Ok(())
}

/// `events.jsonl.<today>` の末尾 N 行を 1 行ずつ pretty-print する。jq -C 風の
/// 表示を Rust 内で完結させる。
fn print_recent_events(data_dir: &std::path::Path, n: usize) -> Result<()> {
    use std::io::{BufRead, BufReader};
    if !data_dir.exists() {
        println!("(no events — data dir does not exist)");
        return Ok(());
    }
    // 最新の `events.jsonl.YYYY-MM-DD` を mtime で選ぶ。
    let latest_log = std::fs::read_dir(data_dir)?
        .filter_map(std::result::Result::ok)
        .filter(|e| e.file_name().to_string_lossy().starts_with("events.jsonl"))
        .filter_map(|e| {
            let modified = e.metadata().ok()?.modified().ok()?;
            Some((modified, e.path()))
        })
        .max_by_key(|(modified, _)| *modified);
    let Some((_, path)) = latest_log else {
        println!("(no events.jsonl in {})", data_dir.display());
        return Ok(());
    };
    println!("# {} (tail {n})", path.display());
    let file = std::fs::File::open(&path)?;
    let reader = BufReader::new(file);
    let lines: Vec<String> = reader.lines().map_while(std::result::Result::ok).collect();
    let start = lines.len().saturating_sub(n);
    for line in &lines[start..] {
        match serde_json::from_str::<serde_json::Value>(line) {
            Ok(value) => println!("{}", serde_json::to_string_pretty(&value)?),
            Err(_) => println!("{line}"),
        }
    }
    tracing::info!(events_path = %path.display(), n, "diagnostics --recent-events");
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

    /// PR-B: `boot()` の `info_span!("linerule_run", run_id = ...)` 経由で
    /// log line に `run_id` が乗ることを確認する。`boot()` 全体を叩くと global
    /// subscriber + panic hook を install してしまうので、span だけ手で構築
    /// して `dispatch_command` を呼び、`traced_test` subscriber が span field
    /// を含めて log line を render することを assert する。
    #[traced_test]
    #[test]
    fn root_span_propagates_run_id_into_log_lines() {
        let run_id = Uuid::new_v4();
        let id_str = run_id.to_string();
        let root = tracing::info_span!("linerule_run", run_id = %run_id);
        let _entered = root.enter();
        tracing::info!(run_id = %run_id, "test run started");
        dispatch_command(parse(&["version"])).expect("version subcommand");
        assert!(
            logs_contain(&id_str),
            "events under linerule_run span should carry run_id={id_str}"
        );
    }

    /// `Diagnostics` 経路でも `run_id` span field が log line に乗ることを確認。
    #[traced_test]
    #[test]
    fn root_span_propagates_run_id_in_diagnostics_path() {
        let run_id = Uuid::new_v4();
        let id_str = run_id.to_string();
        let root = tracing::info_span!("linerule_run", run_id = %run_id);
        let _entered = root.enter();
        let _ = dispatch_command(parse(&["diagnostics", "--dry-run"]));
        assert!(
            logs_contain(&id_str),
            "diagnostics events should carry run_id={id_str}"
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
