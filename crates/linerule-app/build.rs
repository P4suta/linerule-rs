//! Build script: embed the Windows resources (manifest + app icon; no-op off
//! Windows) and emit `LINERULE_VERSION` for `src/version.rs`.

use std::process::Command;

#[cfg(target_os = "windows")]
use std::{env, fs, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(target_os = "windows")]
    embed_windows_resources()?;

    emit_version()?;
    Ok(())
}

#[cfg(target_os = "windows")]
fn embed_windows_resources() -> Result<(), Box<dyn std::error::Error>> {
    let package_version = env::var("CARGO_PKG_VERSION")?;
    let numeric = numeric_version(&package_version)?;
    let file_version = format!(
        "{}.{}.{}.{}",
        numeric[0], numeric[1], numeric[2], numeric[3]
    );
    let comma_version = numeric.map(|part| part.to_string()).join(",");
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").ok_or("OUT_DIR is not set")?);

    let manifest = fs::read_to_string("app.manifest.in")?.replace("@FILE_VERSION@", &file_version);
    let manifest_path = out_dir.join("linerule.manifest");
    fs::write(&manifest_path, manifest)?;
    let manifest_resource_path = out_dir.join("linerule-manifest.rc");
    let manifest_resource = format!(
        "1 24 \"{}\"\n",
        manifest_path.to_string_lossy().replace('\\', "/")
    );
    fs::write(&manifest_resource_path, manifest_resource)?;

    let icon_path = fs::canonicalize("assets/linerule.ico")?
        .to_string_lossy()
        .replace('\\', "/");
    let resource = fs::read_to_string("app.rc.in")?
        .replace("@FILE_VERSION@", &file_version)
        .replace("@VERSION_COMMAS@", &comma_version)
        .replace("@ICON_PATH@", &icon_path);
    let resource_path = out_dir.join("linerule.rc");
    fs::write(&resource_path, resource)?;

    // Both resources are product requirements. A missing compiler, malformed
    // manifest, icon failure, or VERSIONINFO failure must abort the build.
    embed_resource::compile(&manifest_resource_path, embed_resource::NONE).manifest_required()?;
    embed_resource::compile(&resource_path, embed_resource::NONE).manifest_required()?;

    println!("cargo:rerun-if-changed=app.manifest.in");
    println!("cargo:rerun-if-changed=app.rc.in");
    println!("cargo:rerun-if-changed=assets/linerule.ico");
    Ok(())
}

#[cfg(target_os = "windows")]
fn numeric_version(version: &str) -> Result<[u16; 4], Box<dyn std::error::Error>> {
    let stable = version
        .split_once('-')
        .map_or(version, |(stable, _)| stable);
    let mut parts = stable.split('.');
    let major = parts
        .next()
        .ok_or("version has no major component")?
        .parse()?;
    let minor = parts
        .next()
        .ok_or("version has no minor component")?
        .parse()?;
    let patch = parts
        .next()
        .ok_or("version has no patch component")?
        .parse()?;
    if parts.next().is_some() {
        return Err("version has more than three numeric components".into());
    }
    Ok([major, minor, patch, 0])
}

/// Emit `LINERULE_VERSION`. Always emits one value so `env!` never fails, even
/// in source-tarball builds with no `.git`.
fn emit_version() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-env-changed=LINERULE_VERSION");
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../.git/index");

    println!("cargo:rustc-env=LINERULE_VERSION={}", resolve_version()?);
    Ok(())
}

/// Resolve version: CI `LINERULE_VERSION` override verbatim, else
/// `{CARGO_PKG_VERSION}-dev+g{sha}[.dirty]`, else `{CARGO_PKG_VERSION}-dev`.
fn resolve_version() -> Result<String, Box<dyn std::error::Error>> {
    if let Ok(forced) = std::env::var("LINERULE_VERSION")
        && !forced.is_empty()
    {
        return Ok(forced);
    }

    let base = std::env::var("CARGO_PKG_VERSION")?;
    Ok(git_short_sha().map_or_else(
        || format!("{base}-dev"),
        |sha| {
            let dirty = if git_is_dirty() { ".dirty" } else { "" };
            format!("{base}-dev+g{sha}{dirty}")
        },
    ))
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
