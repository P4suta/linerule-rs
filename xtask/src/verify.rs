//! `cargo xtask verify` — local GUI smoke that mirrors the CI release-build
//! verdict so the maintainer can run the exact same check a Windows runner does.
//!
//! It spawns `linerule.exe run [--initial-mode ..] [--initial-effect ..]
//! --duration-ms N`, waits for the time-boxed graceful quit, then judges the
//! resulting `events.jsonl` exactly as `.github/workflows/ci.yml` does:
//! no `level":"ERROR"`, no `tick processing failed`, no `crash-*.json`, and
//! either exit 0 or a clean `Win32 message loop exited` (the known headless
//! `WinRT`-blur teardown artifact `0xE0464645`).
//!
//! This is the one check Docker/Linux cannot run — `run` raises a real window —
//! so it does real work only under `cfg(windows)` and no-ops elsewhere. The
//! pure verdict (`judge`) is platform-independent and unit-tested on every
//! runner, locking the gate semantics shared with CI.

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
}

/// Outcome of judging a GUI smoke run. Used by the Windows `run` and the unit
/// tests; the non-Windows `run` is a no-op, so gate it to keep a Linux build
/// (where only `VerifyArgs` is referenced) free of dead-code warnings.
#[cfg(any(target_os = "windows", test))]
#[derive(Debug)]
pub(crate) enum Verdict {
    /// Healthy run, exit 0.
    Pass(String),
    /// Healthy render but a non-zero exit during teardown (tolerated unless
    /// `--strict`). Known headless-only `WinRT` blur artifact.
    Tolerated(String),
    /// Failed a health gate or crashed mid-run.
    Fail(String),
}

/// Judge a GUI smoke run from its `events.jsonl` body, whether a crash dump was
/// left behind, the process exit code, and whether `--strict` was set. This is
/// the single source of truth for the pass/fail criteria shared with CI; the
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
        // false verdict (CI does the same before the smoke step).
        if !keep_logs {
            clean_artifacts(&profile_dir);
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

        // Spawn from Rust: `Command::status()` returns the true Windows exit
        // code, sidestepping the MSYS2 "exit 127" illusion for GUI-subsystem
        // binaries. Blocks until the AutoQuitTimer fires the graceful quit.
        let status = Command::new(&exe)
            .args(&run_args)
            .status()
            .with_context(|| format!("spawning {}", exe.display()))?;
        let exit_code = status.code();

        // Pick the latest events.jsonl.<date> by mtime (same selection as
        // `diagnostics --recent-events`).
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
        // Reads the String payload of all three variants. On a non-Windows test
        // build the production reader (`win::run`) is cfg'd out, so without this
        // the fields are "never read" (dead_code); it also pins the contract
        // that every verdict explains itself.
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
}
