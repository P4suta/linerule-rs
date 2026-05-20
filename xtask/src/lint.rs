//! Aggregated lint pipeline: cargo fmt, clippy, cargo-deny, typos, actionlint,
//! `cargo-machete`, `xtask dep-graph`.
//!
//! Each step is run to completion (no early bail) so the operator sees every
//! failure in one pass. The function returns an error if any step failed.

use std::process::Command;

use anyhow::{Result, anyhow};

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
