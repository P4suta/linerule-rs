//! Build script for `linerule-app`. Two jobs:
//!
//! 1. Embed the Windows manifest (DPI v2, longPathAware) into the resulting
//!    `linerule.exe` on Windows targets. No-op on other targets so cross-checks
//!    under Linux still build cleanly.
//! 2. Resolve the channel-aware version string and expose it to the crate as
//!    the `LINERULE_VERSION` compile-time env (read by `src/version.rs`).
//!
//! `linerule-app` is the top (leaf) of the one-way dependency graph, so stamping
//! here — rather than in a separate buildstamp crate — keeps `.git`-change
//! rebuilds scoped to this crate alone and adds no workspace member.

use std::process::Command;

fn main() {
    #[cfg(target_os = "windows")]
    {
        let _ = embed_resource::compile("app.manifest", embed_resource::NONE);
    }

    emit_version();
}

/// Resolve and emit `LINERULE_VERSION`. Always emits exactly one value so
/// `env!("LINERULE_VERSION")` never fails to compile, including in source-tarball
/// builds with no `.git`.
fn emit_version() {
    // Rerun when CI's override changes or HEAD moves.
    println!("cargo:rerun-if-env-changed=LINERULE_VERSION");
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../.git/index");

    println!("cargo:rustc-env=LINERULE_VERSION={}", resolve_version());
}

/// Precedence:
/// 1. `LINERULE_VERSION` env override (set by CI for stable/nightly) — verbatim.
/// 2. `{CARGO_PKG_VERSION}-dev+g{short_sha}[.dirty]` when git is available.
/// 3. `{CARGO_PKG_VERSION}-dev` when git / `.git` is absent.
fn resolve_version() -> String {
    if let Ok(forced) = std::env::var("LINERULE_VERSION")
        && !forced.is_empty()
    {
        return forced;
    }

    let base = std::env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.0.0".to_owned());
    git_short_sha().map_or_else(
        || format!("{base}-dev"),
        |sha| {
            let dirty = if git_is_dirty() { ".dirty" } else { "" };
            format!("{base}-dev+g{sha}{dirty}")
        },
    )
}

/// Short git sha (without the `g` prefix). `None` when git / `.git` is absent.
fn git_short_sha() -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--short=7", "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let sha = String::from_utf8(output.stdout).ok()?.trim().to_owned();
    (!sha.is_empty()).then_some(sha)
}

/// Whether the working tree has uncommitted changes (best-effort).
fn git_is_dirty() -> bool {
    Command::new("git")
        .args(["status", "--porcelain"])
        .output()
        .is_ok_and(|o| o.status.success() && !o.stdout.is_empty())
}
