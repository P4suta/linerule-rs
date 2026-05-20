//! Aggregated lint pipeline: cargo fmt, clippy, cargo-deny, typos, actionlint,
//! `cargo-machete`, `xtask dep-graph`.
//!
//! Each step is run to completion (no early bail) so the operator sees every
//! failure in one pass. The function returns an error if any step failed.

use std::process::Command;

use anyhow::{Result, anyhow};

#[allow(
    clippy::too_many_lines,
    reason = "lint パイプライン定義は本質的に長い線形列。helper 抽出は可読性を損なう。"
)]
pub(crate) fn run() -> Result<()> {
    let steps: Vec<(&str, Vec<&str>)> = vec![
        ("rustfmt", vec!["cargo", "fmt", "--all", "--", "--check"]),
        (
            "cargo-sort",
            vec!["cargo", "sort", "--workspace", "--check"],
        ),
        ("taplo", vec!["taplo", "fmt", "--check"]),
        ("biome", vec!["biome", "format", "."]),
        // Don't pass "." — that bypasses the include/exclude in .yamlfmt and
        // makes yamlfmt walk node_modules/. Letting it pick up files from the
        // include patterns gives the same coverage minus the noise.
        ("yamlfmt", vec!["yamlfmt", "--lint"]),
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
        // Windows target に対する `disallowed_*` 限定 clippy。
        //
        // `linerule-platform-windows` は `#![cfg(windows)]` で Linux 上の native
        // clippy では gate out されるため、`disallowed_methods` 等の deny list が
        // 機能しない。`cargo xwin clippy` で Windows target を走らせ、本ステップで
        // `IDCompositionSurface::BeginDraw` 等の直叩きが PR レベルで reject される
        // (Phase I E_NOINTERFACE 事故再発防止、ADR-0009 系)。
        //
        // 本ステップでは Windows 専用コードの他 lint (pedantic, style, unwrap_used
        // 等) を `-A` で抑え、`disallowed_methods` / `disallowed_types` /
        // `disallowed_macros` のみを `-D` で発火させる。pre-existing な warning を
        // この PR で一掃しないと CI が回らない、という連鎖修正を避けるための
        // 設計判断 (deny list 系の事故防止が主目的、他 lint clean up は別 PR)。
        (
            "clippy-windows-deny-list",
            vec![
                "cargo",
                "xwin",
                "clippy",
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
            ],
        ),
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
        // Call cargo-machete directly, not via `cargo machete`. The cargo
        // subcommand path passes "machete" as argv[1] to the binary which
        // older cargo-machete versions misinterpret as a target path.
        ("cargo-machete", vec!["cargo-machete"]),
        ("dep-graph", vec!["cargo", "xtask", "dep-graph"]),
    ];

    let mut failed: Vec<&str> = Vec::new();
    for (name, argv) in &steps {
        println!("=== lint: {name} ===");
        let (program, args) = argv.split_first().expect("non-empty argv");
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
