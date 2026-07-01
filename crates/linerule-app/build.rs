//! Build script: embed the Windows resources (manifest + app icon; no-op off
//! Windows) and emit `LINERULE_VERSION` for `src/version.rs`.

use std::process::Command;

fn main() {
    #[cfg(target_os = "windows")]
    {
        // Two separate compiles on purpose. The manifest must stay a plain
        // `.manifest` compile — routing it through an explicit `1 24` line in a
        // .rc makes its PerMonitorV2 DPI awareness take effect at load time and
        // collide with the app's runtime SetProcessDpiAwarenessContext call
        // (E_ACCESSDENIED). app.rc carries only the icon.
        let _ = embed_resource::compile("app.manifest", embed_resource::NONE);
        let _ = embed_resource::compile("app.rc", embed_resource::NONE);
        println!("cargo:rerun-if-changed=app.rc");
        println!("cargo:rerun-if-changed=app.manifest");
        println!("cargo:rerun-if-changed=assets/linerule.ico");
    }

    emit_version();
}

/// Emit `LINERULE_VERSION`. Always emits one value so `env!` never fails, even
/// in source-tarball builds with no `.git`.
fn emit_version() {
    println!("cargo:rerun-if-env-changed=LINERULE_VERSION");
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../.git/index");

    println!("cargo:rustc-env=LINERULE_VERSION={}", resolve_version());
}

/// Resolve version: CI `LINERULE_VERSION` override verbatim, else
/// `{CARGO_PKG_VERSION}-dev+g{sha}[.dirty]`, else `{CARGO_PKG_VERSION}-dev`.
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
