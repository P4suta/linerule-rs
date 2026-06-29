//! Bootstrap called from clap dispatch.
//! Inits tracing + crash dump, attaches a console, dispatches subcommands.

#![forbid(unsafe_code)]

use anyhow::Result;
use uuid::Uuid;

use crate::cli::{Cli, Command};
use crate::{console, crash_dump, logging};

/// Real main.
///
/// `run_id` goes to both the panic hook and the tracing root span so
/// `events.jsonl` and `crash-<run_id>-*.json` correlate.
///
/// # Errors
/// When a subcommand fails.
pub(crate) fn boot(cli: Cli) -> Result<()> {
    let run_id = Uuid::new_v4();
    crash_dump::install_panic_hook(run_id);
    let _guard = logging::init(cli.needs_console())?;

    if cli.needs_console() {
        console::ensure_console_attached();
    }

    // Root span after subscriber init so in-scope events carry run_id.
    let root = tracing::info_span!("linerule_run", run_id = %run_id);
    let _entered = root.enter();
    tracing::info!(run_id = %run_id, version = crate::version::VERSION, "linerule boot");

    dispatch_command(cli)
}

/// `boot()` body without subscriber/panic-hook install, so tests can drive it
/// under `#[traced_test]`.
///
/// # Errors
/// When a subcommand fails.
pub(crate) fn dispatch_command(cli: Cli) -> Result<()> {
    match cli.command.unwrap_or(Command::Run {
        duration_ms: None,
        initial_mode: None,
        initial_effect: None,
    }) {
        Command::Run {
            duration_ms,
            initial_mode,
            initial_effect,
        } => run_overlay(duration_ms, initial_mode, initial_effect),
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
            println!("linerule {}", crate::version::VERSION);
            tracing::info!(version = crate::version::VERSION, "linerule version");
            Ok(())
        },
    }
}

/// Flags for the `diagnostics` subcommand.
#[derive(Debug, Clone, Copy, Default)]
struct DiagnosticsArgs {
    dry_run: bool,
    last_crash: bool,
    recent_events: Option<usize>,
    data_dir: bool,
}

#[cfg(target_os = "windows")]
fn run_overlay(
    duration_ms: Option<u64>,
    initial_mode: Option<crate::cli::InitialMode>,
    initial_effect: Option<crate::cli::InitialEffect>,
) -> Result<()> {
    use std::time::Duration;

    use linerule_core::input::tick::TickWorld;
    use linerule_core::{State, UserConfig};
    use linerule_platform_windows::{
        AutoQuitTimer, ForegroundHook, OverlayWindow, RenderClock, monitor_info, run_message_pump,
        set_dpi_aware,
    };

    // Set DPI awareness Per-Monitor V2 before creating any window. Non-fatal:
    // defer the HUD notification until the overlay handle exists.
    let mut early_recoverable: Vec<String> = Vec::new();
    if let Err(e) = set_dpi_aware() {
        let app_err: crate::error::AppError = e.into();
        if crate::error::classify_and_log(&app_err) == crate::error::RunDecision::Continue {
            early_recoverable.push(format!("DPI awareness: {app_err}"));
        }
    }

    let config = UserConfig::DEFAULT;
    // Virtual screen bounds (all monitors) so the overlay can draw slits across
    // monitor boundaries.
    let monitor = monitor_info::virtual_screen_bounds()?;

    // Override initial TickWorld state when initial_mode/initial_effect given
    // (CI smoke). `State::with_mode` keeps the mode/last_active invariant.
    let initial_world = if initial_mode.is_some() || initial_effect.is_some() {
        let mut state = initial_mode.map_or(State::DEFAULT, |m| State::with_mode(m.into()));
        if let Some(e) = initial_effect {
            state.config.effect = e.into();
        }
        TickWorld::with_initial_state(state)
    } else {
        TickWorld::INITIAL
    };

    // Drop order matters: `_clock`/`_auto_quit` threads and `_foreground_hook`
    // may `PostMessageW` to the overlay HWND, so they must drop before it.
    // Declare overlay first so reverse-order Drop tears it down last.
    let mut overlay =
        OverlayWindow::new_with_initial_world(monitor, config.hud, config.anim, initial_world)?;
    overlay.attach_compositor()?;
    overlay.register_hotkeys(&config.hotkeys, config.input.tap_step)?;
    // Keep the overlay topmost after Alt+Tab etc. Non-fatal: WS_EX_TOPMOST
    // already covers most cases.
    let _foreground_hook = match ForegroundHook::install(overlay.hwnd()) {
        Ok(h) => Some(h),
        Err(e) => {
            tracing::warn!(error = %e, "ForegroundHook install failed; continuing without z-order re-assertion");
            None
        },
    };

    // Surface boot-time recoverable errors as 10s HUD notifications.
    for message in early_recoverable {
        overlay
            .state()
            .push_notification(linerule_core::NotificationClass::Warn, message, 10_000);
    }

    let _clock = RenderClock::spawn(overlay.hwnd())?;
    let _auto_quit = duration_ms
        .map(|ms| AutoQuitTimer::spawn(overlay.hwnd(), Duration::from_millis(ms)))
        .transpose()?;

    tracing::info!(
        cycle_mode = config.hotkeys.cycle_mode,
        cycle_effect = config.hotkeys.cycle_effect,
        toggle_on_off = config.hotkeys.toggle_on_off,
        quit = config.hotkeys.quit,
        duration_ms = duration_ms.unwrap_or(0),
        initial_mode = ?initial_mode,
        initial_effect = ?initial_effect,
        "overlay running; press Ctrl+Alt+H to show, Ctrl+Alt+R to flip the axis, Ctrl+Alt+E to cycle effects, Ctrl+Alt+Q to quit"
    );
    run_message_pump()?;
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn run_overlay(
    _duration_ms: Option<u64>,
    _initial_mode: Option<crate::cli::InitialMode>,
    _initial_effect: Option<crate::cli::InitialEffect>,
) -> Result<()> {
    anyhow::bail!("`linerule run` is Windows-only");
}

fn diagnostics(args: DiagnosticsArgs) -> Result<()> {
    let data_dir = logging::data_dir()?;

    if args.data_dir {
        println!("{}", data_dir.display());
        tracing::info!(data_dir = %data_dir.display(), "linerule --data-dir");
        return Ok(());
    }

    if args.last_crash {
        return print_last_crash(&data_dir);
    }

    if let Some(n) = args.recent_events {
        return print_recent_events(&data_dir, n);
    }

    // Default / `--dry-run`: list the data dir.
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
    let _ = args.dry_run; // listing-only, so dry_run is a no-op here
    Ok(())
}

/// Pretty-print the most recent `crash-*.json` (by mtime) in the data dir.
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

/// Pretty-print the last N lines of `events.jsonl.<today>`, one entry at a time.
fn print_recent_events(data_dir: &std::path::Path, n: usize) -> Result<()> {
    use std::io::{BufRead, BufReader};
    if !data_dir.exists() {
        println!("(no events — data dir does not exist)");
        return Ok(());
    }
    // Most recent `events.jsonl.YYYY-MM-DD` by mtime.
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
    //! Log-assertion tests for `dispatch_command` under a `#[traced_test]`
    //! subscriber: assert key user-visible events are emitted.

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
    fn version_dispatch_emits_info_with_stamped_version() {
        dispatch_command(parse(&["version"])).expect("version subcommand");
        assert!(
            logs_contain(crate::version::VERSION),
            "info event should include the stamped LINERULE_VERSION"
        );
        // Stamped string always carries the base X.Y.Z, even with a dev suffix.
        assert!(
            logs_contain(env!("CARGO_PKG_VERSION")),
            "stamped version should contain the base CARGO_PKG_VERSION"
        );
        assert!(
            logs_contain("linerule version"),
            "version event should carry the `linerule version` message"
        );
    }

    /// Log lines carry `run_id` via boot's `linerule_run` span. Build the span
    /// by hand since `boot()` would install the global subscriber + panic hook.
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

    /// Assert the `run_id` span field reaches log lines on the `Diagnostics` path.
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
        // diagnostics() tolerates a missing data dir; check the data-dir event
        // fires before any I/O failure.
        let _ = dispatch_command(parse(&["diagnostics", "--dry-run"]));
        assert!(
            logs_contain("linerule data dir"),
            "diagnostics should log the data dir"
        );
    }
}
