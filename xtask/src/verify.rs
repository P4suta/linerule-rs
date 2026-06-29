//! `cargo xtask verify` — local GUI smoke mirroring the CI release-build verdict.
//! Real work only under `cfg(windows)` (`run` raises a real window); `judge` is
//! platform-independent and unit-tested everywhere to lock the gate shared with CI.

/// Flags for `cargo xtask verify`.
#[derive(Debug, clap::Args)]
pub(crate) struct VerifyArgs {
    /// Which `target/<profile>/linerule.exe` to drive (`debug` | `release`).
    #[arg(long, default_value = "debug")]
    pub(crate) profile: String,
    /// Auto-quit the overlay after this many milliseconds.
    #[arg(long, default_value_t = 3000)]
    pub(crate) duration_ms: u64,
    /// Optional `--initial-mode` passthrough (`off` | `horizontal` | `vertical`).
    #[arg(long)]
    pub(crate) mode: Option<String>,
    /// Optional `--initial-effect` passthrough (`dim` | `white` | `blur`).
    #[arg(long)]
    pub(crate) effect: Option<String>,
    /// Require a clean exit 0 (forbid the headless teardown tolerance).
    #[arg(long)]
    pub(crate) strict: bool,
    /// Keep pre-existing events.jsonl / crash dumps instead of cleaning first.
    #[arg(long)]
    pub(crate) keep_logs: bool,
    /// Drive a scripted Ctrl+Alt chord sequence via `SendInput` and assert the
    /// `state changed` series. Windows + interactive desktop only.
    #[arg(long)]
    pub(crate) scenario: bool,
}

/// Outcome of judging a GUI smoke run. Gated to keep the Linux build (which
/// only references `VerifyArgs`) free of dead-code warnings.
#[cfg(any(target_os = "windows", test))]
#[derive(Debug)]
pub(crate) enum Verdict {
    /// Healthy run, exit 0.
    Pass(String),
    /// Healthy render, non-zero exit during teardown (tolerated unless
    /// `--strict`). Known headless-only `WinRT` blur artifact.
    Tolerated(String),
    /// Failed a health gate or crashed mid-run.
    Fail(String),
}

/// Judge a GUI smoke run. Single source of truth for the pass/fail criteria;
/// substring patterns match `.github/workflows/ci.yml` exactly.
#[cfg(any(target_os = "windows", test))]
pub(crate) fn judge(
    events_body: Option<&str>,
    crash_dump_present: bool,
    exit_code: Option<i32>,
    strict: bool,
) -> Verdict {
    let Some(body) = events_body else {
        return Verdict::Fail(
            "events.jsonl not produced — the run never reached the render path".to_owned(),
        );
    };

    // Hard health gates, independent of exit code (same order as CI).
    let errors = body
        .lines()
        .filter(|l| l.contains(r#""level":"ERROR""#))
        .count();
    if errors > 0 {
        return Verdict::Fail(format!("{errors} ERROR event(s) during the run"));
    }
    let tick_failed = body
        .lines()
        .filter(|l| l.contains(r#""message":"tick processing failed""#))
        .count();
    if tick_failed > 0 {
        return Verdict::Fail(format!("{tick_failed} `tick processing failed` event(s)"));
    }
    if crash_dump_present {
        return Verdict::Fail("a crash dump was written during the run".to_owned());
    }

    // Exit-code evaluation.
    let loop_exited = body.contains(r#""message":"Win32 message loop exited""#);
    match exit_code {
        Some(0) => Verdict::Pass(
            "clean events.jsonl (no ERROR / tick-failed / crash dump), exit 0".to_owned(),
        ),
        other if strict => Verdict::Fail(format!(
            "non-zero/unknown exit ({other:?}); --strict forbids the teardown tolerance"
        )),
        other if loop_exited => Verdict::Tolerated(format!(
            "clean render but non-zero exit ({other:?}) during teardown — known headless WinRT \
             blur artifact; real hardware exits 0"
        )),
        other => Verdict::Fail(format!(
            "exit {other:?} before a clean message-loop exit (crash during run, not teardown)"
        )),
    }
}

/// `HotkeyMap` field names injected by `--scenario`, terminated by `quit` so the
/// overlay quits gracefully instead of waiting out `--duration-ms`.
#[cfg(target_os = "windows")]
const SCENARIO_ACTIONS: &str = "toggle_on_off,cycle_mode,cycle_effect,quit";

/// Expected `state changed` action values, in order, for `SCENARIO_ACTIONS`
/// (`quit` produces no state change). Prefix-matched, so `BumpThickness(8)`
/// matches a `BumpThickness` entry.
#[cfg(any(target_os = "windows", test))]
const SCENARIO_EXPECTED: &[&str] = &["ToggleOnOff", "CycleMode", "CycleEffect"];

/// The ordered `action` values of every `state changed` line in an events body.
#[cfg(any(target_os = "windows", test))]
fn state_changed_actions(events_body: &str) -> Vec<String> {
    events_body
        .lines()
        .filter(|l| l.contains(r#""message":"state changed""#))
        .filter_map(extract_field_action)
        .collect()
}

/// Pull the `"action":"…"` value out of a JSON line by substring (no JSON dep).
#[cfg(any(target_os = "windows", test))]
fn extract_field_action(line: &str) -> Option<String> {
    let key = r#""action":""#;
    let start = line.find(key)? + key.len();
    let rest = &line[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_owned())
}

/// Whether `expected` appears as an ordered subsequence of `observed`, matching
/// each expected entry as a prefix (tolerates extra interleaved transitions).
#[cfg(any(target_os = "windows", test))]
fn actions_contain_subsequence(
    observed: &[String],
    expected: &[&str],
) -> std::result::Result<(), String> {
    let mut it = observed.iter();
    for &want in expected {
        if !it.by_ref().any(|got| got.starts_with(want)) {
            return Err(format!(
                "expected `{want}` not found (in order) within observed {observed:?}"
            ));
        }
    }
    Ok(())
}

#[cfg(target_os = "windows")]
pub(crate) fn run(args: VerifyArgs) -> anyhow::Result<()> {
    win::run(args)
}

#[cfg(not(target_os = "windows"))]
#[allow(
    clippy::needless_pass_by_value,
    clippy::unnecessary_wraps,
    reason = "signature must match the Windows `run` (main.rs dispatches identically on every \
              target); this variant is a no-op stub that owns and drops the args and never errors"
)]
pub(crate) fn run(args: VerifyArgs) -> anyhow::Result<()> {
    let _ = args;
    println!(
        "verify: skipped — `linerule run` raises a real window, so this GUI smoke runs only on a \
         Windows host (the Linux dev container can build / cross-check but not render)."
    );
    Ok(())
}

#[cfg(target_os = "windows")]
mod win {
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::thread::sleep;
    use std::time::{Duration, Instant};

    use anyhow::{Context, Result, anyhow};

    use super::{Verdict, VerifyArgs, judge};

    pub(super) fn run(args: VerifyArgs) -> Result<()> {
        let VerifyArgs {
            profile,
            duration_ms,
            mode,
            effect,
            strict,
            keep_logs,
            scenario,
        } = args;

        if profile != "debug" && profile != "release" {
            anyhow::bail!("--profile must be `debug` or `release`, got `{profile}`");
        }
        let profile_dir = Path::new("target").join(&profile);
        let exe = profile_dir.join("linerule.exe");
        if !exe.exists() {
            anyhow::bail!(
                "{} not found — build it first: cargo build{} -p linerule-app",
                exe.display(),
                if profile == "release" {
                    " --release"
                } else {
                    ""
                }
            );
        }

        // Pre-clean stale events / crash dumps so a previous run can't produce a
        // false verdict (matches CI before the smoke step).
        if !keep_logs {
            clean_artifacts(&profile_dir);
        }

        if scenario {
            return run_scenario(&profile_dir, &exe, duration_ms, strict);
        }

        let mut run_args: Vec<String> = vec!["run".to_owned()];
        if let Some(m) = &mode {
            run_args.push("--initial-mode".to_owned());
            run_args.push(m.clone());
        }
        if let Some(e) = &effect {
            run_args.push("--initial-effect".to_owned());
            run_args.push(e.clone());
        }
        run_args.push("--duration-ms".to_owned());
        run_args.push(duration_ms.to_string());

        println!("=== verify: {} {} ===", exe.display(), run_args.join(" "));

        // `Command::status()` returns the true Windows exit code, sidestepping
        // the MSYS2 "exit 127" illusion for GUI-subsystem binaries. Blocks until
        // the AutoQuitTimer fires the graceful quit.
        let status = Command::new(&exe)
            .args(&run_args)
            .status()
            .with_context(|| format!("spawning {}", exe.display()))?;
        let exit_code = status.code();

        // Latest `events.jsonl.<date>` by mtime (matches `diagnostics --recent-events`).
        let events_path = latest_events_file(&profile_dir);
        let events_body = match &events_path {
            Some(p) => Some(
                std::fs::read_to_string(p).with_context(|| format!("reading {}", p.display()))?,
            ),
            None => None,
        };
        let crash_present = has_crash_dump(&profile_dir);

        match judge(events_body.as_deref(), crash_present, exit_code, strict) {
            Verdict::Pass(reason) => {
                println!("verify: PASS — {reason}");
                Ok(())
            },
            Verdict::Tolerated(reason) => {
                println!("verify: PASS (tolerated) — {reason}");
                dump_diagnostics(&exe, events_path.as_deref());
                Ok(())
            },
            Verdict::Fail(reason) => {
                eprintln!("[verify] FAIL — {reason} (exit={exit_code:?})");
                dump_diagnostics(&exe, events_path.as_deref());
                Err(anyhow!("verify failed: {reason}"))
            },
        }
    }

    /// Tier 2: launch the overlay, drive a scripted Ctrl+Alt chord sequence via
    /// the `inject_chords` example, then assert the `state changed` series on top
    /// of the Tier 0/1 health gates. Synthetic input needs a real interactive
    /// desktop, so Docker/Linux can't run this.
    fn run_scenario(profile_dir: &Path, exe: &Path, duration_ms: u64, strict: bool) -> Result<()> {
        let injector = profile_dir.join("examples").join("inject_chords.exe");
        if !injector.exists() {
            anyhow::bail!(
                "{} not found — build it first: cargo build --example inject_chords -p \
                 linerule-platform-windows",
                injector.display()
            );
        }

        // Generous safety-net duration; the injected `quit` ends the run sooner.
        let safety_ms = duration_ms.max(15_000).to_string();
        println!(
            "=== verify --scenario: {} run --duration-ms {safety_ms} (Off start) ===",
            exe.display()
        );
        let mut child = Command::new(exe)
            .args(["run", "--duration-ms", &safety_ms])
            .spawn()
            .with_context(|| format!("spawning {}", exe.display()))?;

        // Don't inject until the message loop is up: RegisterHotKey runs just
        // before the pump, so earlier chords are lost.
        if let Err(e) = wait_for_message_loop(profile_dir, Duration::from_secs(15)) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(e);
        }

        println!(
            "=== verify --scenario: inject [{}] ===",
            super::SCENARIO_ACTIONS
        );
        let injected = Command::new(&injector)
            .args([super::SCENARIO_ACTIONS, "600"])
            .status()
            .with_context(|| format!("spawning {}", injector.display()))?;
        if !injected.success() {
            let _ = child.wait();
            anyhow::bail!("injector exited with {:?}", injected.code());
        }

        // Injected Ctrl+Alt+Q quits the overlay; the duration-ms safety net
        // bounds this if a chord was dropped.
        let exit_code = child.wait().context("waiting for overlay exit")?.code();

        let events_path = latest_events_file(profile_dir);
        let events_body = match &events_path {
            Some(p) => Some(
                std::fs::read_to_string(p).with_context(|| format!("reading {}", p.display()))?,
            ),
            None => None,
        };
        let crash_present = has_crash_dump(profile_dir);

        // Health gates first (reuse the Tier 0/1 verdict).
        if let Verdict::Fail(reason) =
            judge(events_body.as_deref(), crash_present, exit_code, strict)
        {
            dump_diagnostics(exe, events_path.as_deref());
            anyhow::bail!("verify --scenario health gate failed: {reason}");
        }

        // Then the dynamic transition assertion.
        let observed = super::state_changed_actions(events_body.as_deref().unwrap_or_default());
        match super::actions_contain_subsequence(&observed, super::SCENARIO_EXPECTED) {
            Ok(()) => {
                println!("verify --scenario: PASS — observed transitions {observed:?}");
                Ok(())
            },
            Err(e) => {
                dump_diagnostics(exe, events_path.as_deref());
                Err(anyhow!(
                    "verify --scenario transition assertion failed: {e}"
                ))
            },
        }
    }

    /// Poll the latest events file until the overlay logs `entering Win32
    /// message loop`, or `timeout` elapses.
    fn wait_for_message_loop(dir: &Path, timeout: Duration) -> Result<()> {
        let start = Instant::now();
        loop {
            if let Some(path) = latest_events_file(dir)
                && let Ok(body) = std::fs::read_to_string(&path)
                && body.contains(r#""message":"entering Win32 message loop""#)
            {
                return Ok(());
            }
            if start.elapsed() > timeout {
                anyhow::bail!("overlay did not reach the Win32 message loop within {timeout:?}");
            }
            sleep(Duration::from_millis(100));
        }
    }

    fn latest_events_file(dir: &Path) -> Option<PathBuf> {
        std::fs::read_dir(dir)
            .ok()?
            .filter_map(std::result::Result::ok)
            .filter(|e| e.file_name().to_string_lossy().starts_with("events.jsonl"))
            .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
            .max_by_key(|(modified, _)| *modified)
            .map(|(_, path)| path)
    }

    fn has_crash_dump(dir: &Path) -> bool {
        let Ok(rd) = std::fs::read_dir(dir) else {
            return false;
        };
        rd.filter_map(std::result::Result::ok).any(|e| {
            let name = e.file_name();
            let name = name.to_string_lossy();
            name.starts_with("crash-") && name.ends_with(".json")
        })
    }

    fn clean_artifacts(dir: &Path) {
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        for e in rd.filter_map(std::result::Result::ok) {
            let name = e.file_name();
            let name = name.to_string_lossy();
            if name.starts_with("events.jsonl")
                || (name.starts_with("crash-") && name.ends_with(".json"))
            {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }

    fn dump_diagnostics(exe: &Path, events_path: Option<&Path>) {
        if let Some(p) = events_path {
            println!("--- {} (tail 80) ---", p.display());
            if let Ok(body) = std::fs::read_to_string(p) {
                let lines: Vec<&str> = body.lines().collect();
                let start = lines.len().saturating_sub(80);
                for line in &lines[start..] {
                    println!("{line}");
                }
            }
        }
        // Best-effort: let the binary pretty-print the latest crash dump.
        let _ = Command::new(exe)
            .args(["diagnostics", "--last-crash"])
            .status();
    }
}

#[cfg(test)]
mod tests {
    use super::{Verdict, judge};

    const CLEAN: &str = concat!(
        r#"{"level":"INFO","fields":{"message":"linerule boot"}}"#,
        "\n",
        r#"{"level":"INFO","fields":{"message":"entering Win32 message loop"}}"#,
        "\n",
        r#"{"level":"INFO","fields":{"message":"Win32 message loop exited"}}"#,
    );

    #[test]
    fn missing_events_is_fail() {
        assert!(matches!(
            judge(None, false, Some(0), false),
            Verdict::Fail(_)
        ));
    }

    #[test]
    fn clean_exit_zero_passes() {
        assert!(matches!(
            judge(Some(CLEAN), false, Some(0), false),
            Verdict::Pass(_)
        ));
    }

    #[test]
    fn error_line_fails_even_on_exit_zero() {
        let body = format!("{CLEAN}\n{{\"level\":\"ERROR\",\"fields\":{{\"message\":\"boom\"}}}}");
        assert!(matches!(
            judge(Some(&body), false, Some(0), false),
            Verdict::Fail(_)
        ));
    }

    #[test]
    fn tick_failed_fails() {
        // WARN-level line so this specifically exercises the tick-failed gate,
        // not the ERROR gate ahead of it.
        let body = format!(
            "{CLEAN}\n{{\"level\":\"WARN\",\"fields\":{{\"message\":\"tick processing failed\"}}}}"
        );
        assert!(matches!(
            judge(Some(&body), false, Some(0), false),
            Verdict::Fail(_)
        ));
    }

    #[test]
    fn crash_dump_fails() {
        assert!(matches!(
            judge(Some(CLEAN), true, Some(0), false),
            Verdict::Fail(_)
        ));
    }

    #[test]
    fn nonzero_with_clean_loop_exit_is_tolerated() {
        assert!(matches!(
            judge(Some(CLEAN), false, Some(1), false),
            Verdict::Tolerated(_)
        ));
    }

    #[test]
    fn strict_forbids_tolerance() {
        assert!(matches!(
            judge(Some(CLEAN), false, Some(1), true),
            Verdict::Fail(_)
        ));
    }

    #[test]
    fn nonzero_without_loop_exit_is_fail() {
        let body = r#"{"level":"INFO","fields":{"message":"entering Win32 message loop"}}"#;
        assert!(matches!(
            judge(Some(body), false, Some(1), false),
            Verdict::Fail(_)
        ));
    }

    #[test]
    fn every_verdict_carries_a_nonempty_reason() {
        // Reads every variant's String payload: on non-Windows the production
        // reader (`win::run`) is cfg'd out, so without this the fields are
        // dead_code. Also pins that every verdict explains itself.
        for v in [
            judge(None, false, Some(0), false),
            judge(Some(CLEAN), false, Some(0), false),
            judge(Some(CLEAN), false, Some(1), false),
        ] {
            let reason = match &v {
                Verdict::Pass(r) | Verdict::Tolerated(r) | Verdict::Fail(r) => r,
            };
            assert!(!reason.is_empty(), "verdict should explain itself: {v:?}");
        }
    }

    // ---- scenario transition assertions ---------------------------------

    #[test]
    fn extracts_state_changed_actions_in_order() {
        let body = concat!(
            r#"{"fields":{"message":"state changed","action":"ToggleOnOff","mode":"Horizontal"}}"#,
            "\n",
            r#"{"fields":{"message":"overlay running","action":"ignored"}}"#,
            "\n",
            r#"{"fields":{"message":"state changed","action":"CycleMode","mode":"Vertical"}}"#,
        );
        assert_eq!(
            super::state_changed_actions(body),
            vec!["ToggleOnOff".to_owned(), "CycleMode".to_owned()],
        );
    }

    #[test]
    fn subsequence_matches_in_order_with_extras_and_prefix() {
        let observed = vec![
            "ToggleOnOff".to_owned(),
            "CycleMode".to_owned(),
            "CycleEffect".to_owned(),
            "BumpThickness(8)".to_owned(),
        ];
        assert!(
            super::actions_contain_subsequence(&observed, &["ToggleOnOff", "CycleEffect"]).is_ok()
        );
        // Prefix match covers the delta-carrying bump variants.
        assert!(super::actions_contain_subsequence(&observed, &["BumpThickness"]).is_ok());
        assert!(super::actions_contain_subsequence(&observed, super::SCENARIO_EXPECTED).is_ok());
    }

    #[test]
    fn subsequence_rejects_out_of_order_or_missing() {
        let observed = vec!["CycleMode".to_owned(), "ToggleOnOff".to_owned()];
        // ToggleOnOff→CycleMode is not an ordered subsequence of the above.
        assert!(
            super::actions_contain_subsequence(&observed, &["ToggleOnOff", "CycleMode"]).is_err()
        );
        assert!(super::actions_contain_subsequence(&observed, &["Quit"]).is_err());
    }
}
