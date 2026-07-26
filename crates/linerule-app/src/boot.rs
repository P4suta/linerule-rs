//! Application bootstrap and command dispatch.

#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};

use linerule_core::Preferences;
use uuid::Uuid;

use crate::cli::{Cli, Command, DiagnosticsArgs};
use crate::error::{AppError, Result};
use crate::storage::{DataPaths, Distribution, LoadOutcome, PreferencesStore};
use crate::{crash_dump, logging};

/// Initialize paths, logging, crash reporting, and preferences, then dispatch.
pub(crate) fn boot(cli: Cli) -> Result<()> {
    #[cfg(target_os = "windows")]
    attach_console_if_needed(cli.needs_console())?;
    let paths = DataPaths::discover()?;
    let logging = logging::init(cli.needs_console(), &paths)?;
    let run_id = Uuid::new_v4();
    crash_dump::install_panic_hook(run_id, paths.clone(), logging.event_ring());

    let root = tracing::info_span!("linerule_run", run_id = %run_id);
    let _entered = root.enter();
    tracing::info!(
        run_id = %run_id,
        version = crate::version::VERSION,
        distribution = ?paths.distribution,
        "linerule boot"
    );

    let store = PreferencesStore::new(paths.preferences.clone());
    let loaded = store.load()?;
    let first_gui_launch = matches!(loaded, LoadOutcome::Defaults) && !cli.needs_console();
    let mut notice = load_notice(&loaded);
    log_load_outcome(&loaded);
    let preferences = loaded.preferences().clone();
    if first_gui_launch && loaded.writable() {
        match store.save(&preferences) {
            Ok(()) => tracing::info!("created first-launch preferences document"),
            Err(error) => {
                tracing::warn!(%error, "could not create first-launch preferences document");
                notice = Some(append_notice(
                    notice,
                    format!("Settings could not be saved: {error}"),
                ));
            },
        }
    }
    let writable_store = loaded.writable().then_some(store);
    let _updated = dispatch_command(
        cli,
        &paths,
        preferences,
        notice,
        writable_store,
        first_gui_launch,
    )?;
    Ok(())
}

fn append_notice(current: Option<String>, next: String) -> String {
    match current {
        Some(current) => format!("{current}\n{next}"),
        None => next,
    }
}

fn load_notice(outcome: &LoadOutcome) -> Option<String> {
    match outcome {
        LoadOutcome::Recovered { quarantined, .. } => Some(format!(
            "Invalid settings were reset; backup: {}",
            quarantined.display()
        )),
        LoadOutcome::FutureVersion { found, .. } => Some(format!(
            "Settings schema {found} is newer than this app; changes will not be saved"
        )),
        LoadOutcome::Loaded(_) | LoadOutcome::Defaults => None,
    }
}

fn log_load_outcome(outcome: &LoadOutcome) {
    match outcome {
        LoadOutcome::Loaded(_) => tracing::info!("preferences loaded"),
        LoadOutcome::Defaults => tracing::info!("preferences not found; using defaults"),
        LoadOutcome::Recovered { quarantined, .. } => {
            tracing::warn!(path = %quarantined.display(), "invalid preferences quarantined");
        },
        LoadOutcome::FutureVersion { found, .. } => {
            tracing::warn!(found, "future preferences schema preserved read-only");
        },
    }
}

/// Dispatch without installing process-global hooks, allowing parallel tests.
pub(crate) fn dispatch_command(
    cli: Cli,
    paths: &DataPaths,
    preferences: Preferences,
    startup_notice: Option<String>,
    persistence: Option<PreferencesStore>,
    show_startup_guide: bool,
) -> Result<Preferences> {
    match cli.command {
        None => run_desktop(
            linerule_platform_intent::Launch::Resident,
            preferences,
            startup_notice,
            persistence,
            show_startup_guide,
        ),
        Some(Command::Settings) => run_desktop(
            linerule_platform_intent::Launch::Settings,
            preferences,
            startup_notice,
            persistence,
            show_startup_guide,
        ),
        Some(Command::Diagnostics(args)) => {
            diagnostics(paths, &args)?;
            Ok(preferences)
        },
        Some(Command::Version) => {
            println!("linerule {}", crate::version::VERSION);
            tracing::info!(version = crate::version::VERSION, "linerule version");
            Ok(preferences)
        },
    }
}

mod linerule_platform_intent {
    #[derive(Clone, Copy)]
    pub(super) enum Launch {
        Resident,
        Settings,
    }
}

#[cfg(target_os = "windows")]
fn run_desktop(
    launch: linerule_platform_intent::Launch,
    preferences: Preferences,
    startup_notice: Option<String>,
    persistence: Option<PreferencesStore>,
    show_startup_guide: bool,
) -> Result<Preferences> {
    use linerule_platform_windows::{DesktopRuntime, LaunchIntent, RuntimeOptions};

    let intent = match launch {
        linerule_platform_intent::Launch::Resident => LaunchIntent::Resident,
        linerule_platform_intent::Launch::Settings => LaunchIntent::Settings,
    };
    let mut options = RuntimeOptions::new(preferences)
        .with_startup_notice(startup_notice)
        .with_startup_guide(show_startup_guide);
    if let Some(store) = persistence {
        options = options.with_persistence(move |preferences| {
            store.save(preferences).map_err(|error| error.to_string())
        });
    }
    DesktopRuntime::run(intent, options).map_err(Into::into)
}

#[cfg(not(target_os = "windows"))]
fn run_desktop(
    _launch: linerule_platform_intent::Launch,
    _preferences: Preferences,
    _startup_notice: Option<String>,
    _persistence: Option<PreferencesStore>,
    _show_startup_guide: bool,
) -> Result<Preferences> {
    Err(AppError::UnsupportedPlatform)
}

fn diagnostics(paths: &DataPaths, args: &DiagnosticsArgs) -> Result<()> {
    if args.data_dir {
        println!("{}", paths.root.display());
        tracing::info!(data_dir = %paths.root.display(), "diagnostics data directory");
        return Ok(());
    }
    if args.last_crash {
        print_last_crash(&paths.crashes)?;
        return Ok(());
    }
    if let Some(count) = args.recent_events {
        print_recent_events(&paths.logs, count)?;
        return Ok(());
    }

    let distribution = match paths.distribution {
        Distribution::Installed => "installed",
        Distribution::Portable => "portable",
    };
    println!("version: {}", crate::version::VERSION);
    println!("distribution: {distribution}");
    println!("data_dir: {}", paths.root.display());
    println!("settings: {}", presence(&paths.preferences));
    println!(
        "logs: {}",
        matching_file_count(&paths.logs, "events.jsonl")?
    );
    println!(
        "crashes: {}",
        matching_file_count(&paths.crashes, "crash-")?
    );
    tracing::info!(distribution, data_dir = %paths.root.display(), "diagnostics summary");
    Ok(())
}

fn presence(path: &Path) -> &'static str {
    if path.is_file() { "present" } else { "missing" }
}

fn matching_file_count(directory: &Path, prefix: &str) -> Result<usize> {
    if !directory.exists() {
        return Ok(0);
    }
    let entries = std::fs::read_dir(directory).map_err(|source| AppError::DiagnosticIo {
        operation: "read directory",
        path: directory.to_path_buf(),
        source,
    })?;
    let mut count = 0;
    for entry in entries {
        let entry = entry.map_err(|source| AppError::DiagnosticIo {
            operation: "read directory entry in",
            path: directory.to_path_buf(),
            source,
        })?;
        count += usize::from(entry.file_name().to_string_lossy().starts_with(prefix));
    }
    Ok(count)
}

fn print_last_crash(directory: &Path) -> Result<()> {
    let Some(path) = latest_matching_file(directory, "crash-")? else {
        println!("(no crash reports)");
        return Ok(());
    };
    println!("# {}", path.display());
    let raw = std::fs::read_to_string(&path).map_err(|source| AppError::DiagnosticIo {
        operation: "read",
        path: path.clone(),
        source,
    })?;
    let value: serde_json::Value =
        serde_json::from_str(&raw).map_err(|source| AppError::DiagnosticJson {
            path: path.clone(),
            source,
        })?;
    println!(
        "{}",
        serde_json::to_string_pretty(&value).map_err(AppError::EncodeDiagnosticJson)?
    );
    tracing::info!(crash_path = %path.display(), "diagnostics latest crash");
    Ok(())
}

fn print_recent_events(directory: &Path, count: usize) -> Result<()> {
    use std::io::{BufRead, BufReader};

    let Some(path) = latest_matching_file(directory, "events.jsonl")? else {
        println!("(no event logs)");
        return Ok(());
    };
    println!("# {} (tail {count})", path.display());
    let file = std::fs::File::open(&path).map_err(|source| AppError::DiagnosticIo {
        operation: "open",
        path: path.clone(),
        source,
    })?;
    let lines: Vec<String> = BufReader::new(file)
        .lines()
        .collect::<std::io::Result<_>>()
        .map_err(|source| AppError::DiagnosticIo {
            operation: "read lines from",
            path: path.clone(),
            source,
        })?;
    for line in lines.iter().skip(lines.len().saturating_sub(count)) {
        match serde_json::from_str::<serde_json::Value>(line) {
            Ok(value) => println!(
                "{}",
                serde_json::to_string_pretty(&value).map_err(AppError::EncodeDiagnosticJson)?
            ),
            Err(_) => println!("{line}"),
        }
    }
    tracing::info!(events_path = %path.display(), count, "diagnostics recent events");
    Ok(())
}

fn latest_matching_file(directory: &Path, prefix: &str) -> Result<Option<PathBuf>> {
    if !directory.exists() {
        return Ok(None);
    }
    let entries = std::fs::read_dir(directory).map_err(|source| AppError::DiagnosticIo {
        operation: "read directory",
        path: directory.to_path_buf(),
        source,
    })?;
    let mut latest = None;
    for entry in entries {
        let entry = entry.map_err(|source| AppError::DiagnosticIo {
            operation: "read directory entry in",
            path: directory.to_path_buf(),
            source,
        })?;
        if !entry.file_name().to_string_lossy().starts_with(prefix) {
            continue;
        }
        let path = entry.path();
        let modified = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .map_err(|source| AppError::DiagnosticIo {
                operation: "read modification time for",
                path: path.clone(),
                source,
            })?;
        if latest
            .as_ref()
            .is_none_or(|(current, _)| modified > *current)
        {
            latest = Some((modified, path));
        }
    }
    Ok(latest.map(|(_, path)| path))
}

#[cfg(target_os = "windows")]
fn attach_console_if_needed(needed: bool) -> Result<()> {
    if needed {
        linerule_platform_windows::DesktopRuntime::attach_console()?;
    }
    Ok(())
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use super::*;
    use clap::Parser;
    use tracing_test::traced_test;

    fn parse(arguments: &[&str]) -> Cli {
        Cli::try_parse_from(std::iter::once("linerule").chain(arguments.iter().copied()))
            .expect("fixture parses")
    }

    fn paths(root: &Path) -> DataPaths {
        DataPaths {
            distribution: Distribution::Portable,
            root: root.to_path_buf(),
            preferences: root.join("settings.json"),
            logs: root.join("logs"),
            crashes: root.join("crashes"),
        }
    }

    #[traced_test]
    #[test]
    fn version_dispatch_emits_stamped_version() {
        let temp = tempfile::tempdir().expect("tempdir");
        dispatch_command(
            parse(&["version"]),
            &paths(temp.path()),
            Preferences::default(),
            None,
            None,
            false,
        )
        .expect("version");
        assert!(logs_contain(crate::version::VERSION));
        assert!(logs_contain("linerule version"));
    }

    #[traced_test]
    #[test]
    fn diagnostics_summary_is_side_effect_free() {
        let temp = tempfile::tempdir().expect("tempdir");
        let selected = paths(temp.path());
        dispatch_command(
            parse(&["diagnostics"]),
            &selected,
            Preferences::default(),
            None,
            None,
            false,
        )
        .expect("diagnostics");
        assert!(!selected.preferences.exists());
        assert!(logs_contain("diagnostics summary"));
    }

    #[test]
    fn future_schema_notice_is_explicit() {
        let outcome = LoadOutcome::FutureVersion {
            preferences: Preferences::default(),
            found: 7,
        };
        assert!(
            load_notice(&outcome)
                .expect("notice")
                .contains("will not be saved")
        );
    }

    #[test]
    fn load_notices_cover_normal_and_recovered_preferences() {
        assert!(load_notice(&LoadOutcome::Defaults).is_none());
        assert!(load_notice(&LoadOutcome::Loaded(Preferences::default())).is_none());

        let recovered = LoadOutcome::Recovered {
            preferences: Preferences::default(),
            quarantined: std::path::PathBuf::from("settings.invalid-20260726.json"),
        };
        assert!(
            load_notice(&recovered)
                .expect("recovery notice")
                .contains("settings.invalid-20260726.json")
        );

        log_load_outcome(&LoadOutcome::Defaults);
        log_load_outcome(&LoadOutcome::Loaded(Preferences::default()));
        log_load_outcome(&recovered);
        log_load_outcome(&LoadOutcome::FutureVersion {
            preferences: Preferences::default(),
            found: 7,
        });
    }

    #[test]
    fn appending_a_notice_preserves_existing_context() {
        assert_eq!(append_notice(None, "next".to_owned()), "next");
        assert_eq!(
            append_notice(Some("first".to_owned()), "next".to_owned()),
            "first\nnext"
        );
    }

    #[test]
    fn diagnostic_file_helpers_cover_missing_and_present_data() {
        let temp = tempfile::tempdir().expect("tempdir");
        let selected = paths(temp.path());
        assert_eq!(presence(&selected.preferences), "missing");
        std::fs::write(&selected.preferences, b"{}").expect("settings fixture");
        assert_eq!(presence(&selected.preferences), "present");

        assert_eq!(
            matching_file_count(&selected.logs, "events.jsonl").expect("missing logs"),
            0
        );
        std::fs::create_dir_all(&selected.logs).expect("logs directory");
        std::fs::write(
            selected.logs.join("events.jsonl.2026-07-25"),
            b"{\"message\":\"first\"}\nnot-json\n",
        )
        .expect("events fixture");
        std::fs::write(selected.logs.join("unrelated.txt"), []).expect("unrelated fixture");
        assert_eq!(
            matching_file_count(&selected.logs, "events.jsonl").expect("count logs"),
            1
        );
        print_recent_events(&selected.logs, 2).expect("print events");
        print_recent_events(&temp.path().join("missing-logs"), 2).expect("no events");

        std::fs::create_dir_all(&selected.crashes).expect("crash directory");
        std::fs::write(selected.crashes.join("crash-1.json"), b"{}").expect("crash fixture");
        print_last_crash(&selected.crashes).expect("print crash");
        print_last_crash(&temp.path().join("missing-crashes")).expect("no crash");
        assert!(
            latest_matching_file(&selected.crashes, "crash-")
                .expect("latest crash")
                .is_some()
        );
        assert!(
            latest_matching_file(&temp.path().join("absent"), "crash-")
                .expect("absent directory")
                .is_none()
        );

        #[cfg(target_os = "windows")]
        attach_console_if_needed(false).expect("console not requested");
    }

    #[test]
    fn diagnostics_selectors_cover_crash_and_event_paths() {
        let temp = tempfile::tempdir().expect("tempdir");
        let selected = paths(temp.path());
        diagnostics(
            &selected,
            &DiagnosticsArgs {
                last_crash: true,
                ..DiagnosticsArgs::default()
            },
        )
        .expect("last crash selector");
        diagnostics(
            &selected,
            &DiagnosticsArgs {
                recent_events: Some(3),
                ..DiagnosticsArgs::default()
            },
        )
        .expect("recent events selector");
    }
}
