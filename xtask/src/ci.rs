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
        ("test-workspace", vec!["cargo", "test", "--workspace"]),
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
