//! Local replication of the CI matrix: build + test + release-build + lint.
//! Useful before `git push` to catch failures without waiting on GitHub.

use std::process::Command;

use anyhow::{Result, anyhow};

pub(crate) fn run() -> Result<()> {
    // Native Windows: event_ring tests share process state and fail under
    // parallel threads. Use nextest (process-per-test) or fall back to serial.
    let test_step: (&str, Vec<&str>) = if crate::mode::is_native() {
        if crate::mode::nextest_available() {
            (
                "test-workspace",
                vec!["cargo", "nextest", "run", "--workspace"],
            )
        } else {
            (
                "test-workspace",
                vec!["cargo", "test", "--workspace", "--", "--test-threads=1"],
            )
        }
    } else {
        ("test-workspace", vec!["cargo", "test", "--workspace"])
    };

    let steps: Vec<(&str, Vec<&str>)> = vec![
        (
            "build-workspace",
            vec!["cargo", "build", "--workspace", "--all-targets"],
        ),
        test_step,
        (
            "release-build-app",
            vec!["cargo", "build", "--release", "-p", "linerule-app"],
        ),
        ("lint", vec!["cargo", "xtask", "lint"]),
    ];

    let mut failed: Vec<&str> = Vec::new();
    for (name, argv) in &steps {
        println!("=== ci: {name} ===");
        let (program, args) = argv.split_first().expect("non-empty argv");
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
