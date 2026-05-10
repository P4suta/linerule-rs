//! Defensive grep gate, in Rust. Replaces the previous bash script.
//!
//! Each rule encodes a pattern we have decided is a bug-source
//! (`feedback_defensive_gates_upfront`). Adding a new rule requires
//! a matching ADR; relaxing one requires removing the rule entirely
//! (no in-source carve-outs). Output is human-readable and CI-friendly.

use std::fs;
use std::path::{Path, PathBuf};

use regex::Regex;
use walkdir::{DirEntry, WalkDir};

/// Run every check; return `Ok(())` on a clean pass or `Err(count)`
/// when violations were found. Caller maps to the exit code.
pub(crate) fn run() -> Result<(), usize> {
    let crates_dir = PathBuf::from("crates");
    let workflows_dir = PathBuf::from(".github/workflows");

    // xtask is dev automation, not production code — and the rule
    // definitions themselves reference forbidden tokens by design
    // (e.g. the "TODO|FIXME|XXX" regex literal). Scope the gate to
    // production crates only.
    let crate_files: Vec<PathBuf> = collect_rs(&crates_dir)
        .into_iter()
        .filter(|p| !p.to_string_lossy().contains("crates/xtask/"))
        .collect();
    let pure_files: Vec<PathBuf> = crate_files
        .iter()
        .filter(|p| in_pure_crate(p))
        .cloned()
        .collect();
    let lib_files: Vec<PathBuf> = crate_files
        .iter()
        .filter(|p| in_library_crate(p) && !in_test_or_example(p))
        .cloned()
        .collect();
    let workflow_files: Vec<PathBuf> = collect_yml(&workflows_dir);

    let mut violations = 0usize;
    macro_rules! check {
        ($result:expr) => {
            violations += $result;
        };
    }

    check!(no_allow_attribute(&crate_files));
    check!(no_nightly_feature_gate(&crate_files));
    check!(no_unsafe_in_pure_crates(&pure_files));
    check!(safety_comment_required(&crate_files));
    check!(forbid_unsafe_directive(&pure_files));
    check!(toolchain_is_stable());
    check!(no_bare_todo(&crate_files));
    check!(no_println_in_libraries(&lib_files));
    check!(no_continue_on_error(&workflow_files));
    check!(no_workflow_schedule(&workflow_files));

    if violations == 0 {
        println!("strict-code: clean");
        Ok(())
    } else {
        Err(violations)
    }
}

// ===========================================================================
// File discovery helpers
// ===========================================================================

fn collect_rs(root: &Path) -> Vec<PathBuf> {
    WalkDir::new(root)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| !e.file_type().is_dir())
        .map(DirEntry::into_path)
        .filter(|p| p.extension().is_some_and(|x| x == "rs"))
        .collect()
}

fn collect_yml(root: &Path) -> Vec<PathBuf> {
    WalkDir::new(root)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| !e.file_type().is_dir())
        .map(DirEntry::into_path)
        .filter(|p| p.extension().is_some_and(|x| x == "yml" || x == "yaml"))
        .collect()
}

fn in_pure_crate(path: &Path) -> bool {
    let s = path.to_string_lossy();
    s.contains("crates/linerule-core/") || s.contains("crates/linerule-config/")
}

fn in_library_crate(path: &Path) -> bool {
    let s = path.to_string_lossy();
    s.contains("crates/linerule-core/")
        || s.contains("crates/linerule-config/")
        || s.contains("crates/linerule-platform/")
}

fn in_test_or_example(path: &Path) -> bool {
    let s = path.to_string_lossy();
    s.contains("/tests/") || s.contains("/benches/") || s.contains("/examples/")
}

fn read_lines(path: &Path) -> Vec<(usize, String)> {
    fs::read_to_string(path)
        .map(|s| {
            s.lines()
                .enumerate()
                .map(|(i, l)| (i + 1, l.to_owned()))
                .collect()
        })
        .unwrap_or_default()
}

fn report(rule: &str, hits: &[(PathBuf, usize, String)]) -> usize {
    if hits.is_empty() {
        return 0;
    }
    eprintln!("==> forbidden: {rule}");
    for (path, line, content) in hits {
        eprintln!("  {}:{}: {}", path.display(), line, content.trim());
    }
    hits.len()
}

// ===========================================================================
// Rules
// ===========================================================================

fn match_lines(files: &[PathBuf], pattern: &Regex) -> Vec<(PathBuf, usize, String)> {
    let mut hits = Vec::new();
    for path in files {
        for (line, content) in read_lines(path) {
            if pattern.is_match(&content) {
                hits.push((path.clone(), line, content));
            }
        }
    }
    hits
}

fn no_allow_attribute(files: &[PathBuf]) -> usize {
    let pat = Regex::new(r"^\s*(#!?\[allow\(|#!?\[cfg_attr\([^)]*allow\()").expect("regex");
    let hits = match_lines(files, &pat);
    report(
        "warning suppression (`#[allow(...)]` / `cfg_attr(..., allow(...))`)",
        &hits,
    )
}

fn no_nightly_feature_gate(files: &[PathBuf]) -> usize {
    let pat = Regex::new(r"^\s*#!?\[feature\(").expect("regex");
    report(
        "nightly feature gate (`#[feature(...)]` / `#![feature(...)]`)",
        &match_lines(files, &pat),
    )
}

fn no_unsafe_in_pure_crates(files: &[PathBuf]) -> usize {
    let pat = Regex::new(r"(^|[^a-zA-Z_#])unsafe\s+(fn|impl|trait|\{)").expect("regex");
    report(
        "unsafe code in pure crate (linerule-core / linerule-config)",
        &match_lines(files, &pat),
    )
}

fn safety_comment_required(files: &[PathBuf]) -> usize {
    // Only check files in `linerule-platform` — that is the one crate
    // allowed to use unsafe at the FFI boundary.
    let plat: Vec<&PathBuf> = files
        .iter()
        .filter(|p| p.to_string_lossy().contains("crates/linerule-platform/"))
        .collect();
    let unsafe_pat = Regex::new(r"^\s*unsafe\s+(fn|\{|impl|trait)").expect("regex");
    let safety_pat = Regex::new(r"//\s*SAFETY:").expect("regex");
    let mut hits = Vec::new();
    for path in plat {
        let lines = read_lines(path);
        let mut prev = String::new();
        for (line, content) in &lines {
            if unsafe_pat.is_match(content) && !safety_pat.is_match(&prev) {
                hits.push((path.clone(), *line, content.clone()));
            }
            prev.clone_from(content);
        }
    }
    report(
        "unsafe block without preceding `// SAFETY:` justification",
        &hits,
    )
}

fn forbid_unsafe_directive(files: &[PathBuf]) -> usize {
    // Each pure crate root must declare `#![forbid(unsafe_code)]`.
    let mut hits = Vec::new();
    for path in files {
        let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if name != "lib.rs" && name != "main.rs" {
            continue;
        }
        if !path.to_string_lossy().contains("/src/") {
            continue;
        }
        let body = fs::read_to_string(path).unwrap_or_default();
        if !body.contains("#![forbid(unsafe_code)]") {
            hits.push((path.clone(), 1, "missing #![forbid(unsafe_code)]".into()));
        }
    }
    report("pure crate root missing `#![forbid(unsafe_code)]`", &hits)
}

fn toolchain_is_stable() -> usize {
    let body = fs::read_to_string("rust-toolchain.toml").unwrap_or_default();
    let pat = Regex::new(r#"(?m)^\s*channel\s*=\s*"(nightly|beta)"#).expect("regex");
    if pat.is_match(&body) {
        eprintln!("==> forbidden: rust-toolchain.toml pins a pre-stable channel");
        eprintln!("  {body}");
        1
    } else {
        0
    }
}

fn no_bare_todo(files: &[PathBuf]) -> usize {
    let pat = Regex::new(r"(^|[^A-Za-z0-9_])(TODO|FIXME|XXX)([^A-Za-z0-9_]|$)").expect("regex");
    let ref_pat = Regex::new(r"(#[0-9]+|M[0-9]|issue|ADR-[0-9]+)").expect("regex");
    let mut hits = Vec::new();
    for path in files {
        for (line, content) in read_lines(path) {
            if pat.is_match(&content) && !ref_pat.is_match(&content) {
                hits.push((path.clone(), line, content));
            }
        }
    }
    report(
        "bare TODO/FIXME/XXX without issue / milestone / ADR reference",
        &hits,
    )
}

fn no_println_in_libraries(files: &[PathBuf]) -> usize {
    let pat = Regex::new(r"(^|[^A-Za-z0-9_])e?print(ln)?!\s*\(").expect("regex");
    report(
        "println! / eprintln! in library crate (use tracing)",
        &match_lines(files, &pat),
    )
}

fn no_continue_on_error(files: &[PathBuf]) -> usize {
    let pat = Regex::new(r"continue-on-error\s*:\s*true").expect("regex");
    report(
        "continue-on-error: true in CI workflow",
        &match_lines(files, &pat),
    )
}

fn no_workflow_schedule(files: &[PathBuf]) -> usize {
    let pat = Regex::new(r"(?m)^\s*schedule\s*:").expect("regex");
    report(
        "on.schedule: trigger in workflow (Dependabot weekly only — feedback_no_cron_in_repos)",
        &match_lines(files, &pat),
    )
}
