//! Aggregated lint pipeline (fmt, clippy, deny, typos, actionlint, machete, dep-graph).
//! All steps run to completion (no early bail); errors if any failed.

use std::fs;
use std::path::Path;
use std::process::Command;

use anyhow::{Result, anyhow};

#[allow(
    clippy::too_many_lines,
    reason = "pipeline definition is an inherently long linear sequence"
)]
pub(crate) fn run() -> Result<()> {
    // Native Windows host already targets msvc, so run clippy directly; from
    // Linux the Windows-target step needs cargo-xwin. Flags identical either way.
    let mut win_clippy: Vec<&str> = if crate::mode::is_native() {
        vec!["cargo", "clippy"]
    } else {
        vec!["cargo", "xwin", "clippy"]
    };
    win_clippy.extend_from_slice(&[
        "--target",
        "x86_64-pc-windows-msvc",
        "--workspace",
        "--all-targets",
        "--",
        "-A",
        "warnings",
        "-A",
        "clippy::all",
        "-A",
        "clippy::pedantic",
        "-A",
        "clippy::nursery",
        "-A",
        "clippy::cargo",
        "-A",
        "clippy::wildcard_imports",
        "-A",
        "clippy::mod_module_files",
        "-A",
        "clippy::or_fun_call",
        "-A",
        "clippy::unwrap_used",
        "-A",
        "clippy::dbg_macro",
        "-A",
        "clippy::allow_attributes_without_reason",
        "-A",
        "unsafe_op_in_unsafe_fn",
        "-A",
        "static_mut_refs",
        "-D",
        "clippy::disallowed_methods",
        "-D",
        "clippy::disallowed_types",
        "-D",
        "clippy::disallowed_macros",
    ]);

    let yaml_files = yaml_files()?;
    let mut yamlfmt = vec!["yamlfmt", "--lint"];
    yamlfmt.extend(yaml_files.iter().map(String::as_str));

    let steps: Vec<(&str, Vec<&str>)> = vec![
        ("rustfmt", vec!["cargo", "fmt", "--all", "--", "--check"]),
        (
            "cargo-sort",
            vec!["cargo", "sort", "--workspace", "--check"],
        ),
        ("taplo", vec!["taplo", "fmt", "--check"]),
        ("biome", vec!["biome", "format", "."]),
        ("yamlfmt", yamlfmt),
        (
            "clippy",
            vec![
                "cargo",
                "clippy",
                "--workspace",
                "--all-targets",
                "--",
                "-D",
                "warnings",
            ],
        ),
        // `disallowed_*`-only clippy against the Windows target. Linux native
        // clippy gates out `#![cfg(windows)]` code, so its deny lists never fire;
        // run the Windows target to reject calls like `IDCompositionSurface::BeginDraw`.
        // All other lints are `-A`'d; only `disallowed_methods`/`disallowed_types`/
        // `disallowed_macros` are `-D`.
        ("clippy-windows-deny-list", win_clippy),
        (
            "cargo-deny",
            vec![
                "cargo",
                "deny",
                "check",
                "advisories",
                "bans",
                "licenses",
                "sources",
            ],
        ),
        ("typos", vec!["typos"]),
        ("actionlint", vec!["actionlint"]),
        // Call cargo-machete directly: `cargo machete` passes "machete" as argv[1],
        // which older versions misinterpret as a target path.
        ("cargo-machete", vec!["cargo-machete"]),
        ("dep-graph", vec!["cargo", "xtask", "dep-graph"]),
        ("policy", vec!["cargo", "xtask", "policy"]),
        ("reuse", vec!["reuse", "lint"]),
    ];

    let mut failed: Vec<&str> = Vec::new();
    for (name, argv) in &steps {
        println!("=== lint: {name} ===");
        let Some((program, args)) = argv.split_first() else {
            return Err(anyhow!("lint: step `{name}` has no command"));
        };
        let status = Command::new(program).args(args).status();
        match status {
            Ok(s) if s.success() => {},
            Ok(s) => {
                eprintln!("[lint] step `{name}` failed with status {s}");
                failed.push(name);
            },
            Err(err) => {
                eprintln!("[lint] step `{name}` could not be spawned: {err}");
                failed.push(name);
            },
        }
    }

    if failed.is_empty() {
        println!("lint: ok");
        Ok(())
    } else {
        Err(anyhow!("lint: failed steps: {}", failed.join(", ")))
    }
}

fn yaml_files() -> Result<Vec<String>> {
    let mut files = Vec::new();
    collect_yaml_files(Path::new("."), &mut files)?;
    files.sort_unstable();
    Ok(files)
}

fn collect_yaml_files(directory: &Path, output: &mut Vec<String>) -> Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            let excluded = path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| {
                    matches!(
                        name,
                        ".git"
                            | ".sccache"
                            | ".xwin-cache"
                            | "artifacts"
                            | "bin"
                            | "node_modules"
                            | "obj"
                            | "target"
                    )
                });
            if !excluded {
                collect_yaml_files(&path, output)?;
            }
        } else if path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                extension.eq_ignore_ascii_case("yml") || extension.eq_ignore_ascii_case("yaml")
            })
        {
            output.push(path.to_string_lossy().into_owned());
        }
    }
    Ok(())
}
