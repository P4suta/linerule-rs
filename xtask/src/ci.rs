//! Local replication of the CI matrix: build + test + release-build + lint.
//! Useful before `git push` to catch failures without waiting on GitHub.

use std::process::Command;

use anyhow::{Result, anyhow};

pub(crate) fn run() -> Result<()> {
    let steps: Vec<(&str, Vec<&str>)> = vec![
        (
            "build-workspace",
            vec!["cargo", "build", "--workspace", "--all-targets"],
        ),
        (
            "test-nextest",
            vec![
                "cargo",
                "nextest",
                "run",
                "--workspace",
                "--all-targets",
                "--no-fail-fast",
            ],
        ),
        (
            "test-doctests",
            vec!["cargo", "test", "--doc", "--workspace"],
        ),
        // Keep the stock Cargo runner as an isolation-compatibility gate. This
        // must remain parallel: global test state may not be hidden by
        // nextest's process-per-test execution model.
        (
            "test-cargo-parallel",
            vec!["cargo", "test", "--workspace", "--all-targets"],
        ),
        (
            "release-build-app",
            vec!["cargo", "build", "--release", "-p", "linerule-app"],
        ),
        ("lint", vec!["cargo", "xtask", "lint"]),
    ];

    let mut failed: Vec<&str> = Vec::new();
    for (name, argv) in &steps {
        println!("=== ci: {name} ===");
        let Some((program, args)) = argv.split_first() else {
            return Err(anyhow!("ci: step `{name}` has no command"));
        };
        let status = Command::new(program).args(args).status();
        match status {
            Ok(s) if s.success() => {},
            Ok(s) => {
                eprintln!("[ci] step `{name}` failed with status {s}");
                failed.push(name);
            },
            Err(err) => {
                eprintln!("[ci] step `{name}` could not be spawned: {err}");
                failed.push(name);
            },
        }
    }

    if failed.is_empty() {
        println!("ci: ok");
        Ok(())
    } else {
        Err(anyhow!("ci: failed steps: {}", failed.join(", ")))
    }
}
