//! Asserts the one-way internal dependency graph:
//! `linerule-app → linerule-platform-windows → linerule-core`.
//!
//! Any internal (path) dependency outside this DAG is a violation. Reverse
//! edges, peer-to-peer edges between leaves, and `linerule-core` pulling on a
//! sibling are all caught here.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result, anyhow};
use cargo_metadata::MetadataCommand;

const INTERNAL_PREFIX: &str = "linerule-";

pub(crate) fn run() -> Result<()> {
    let metadata = MetadataCommand::new()
        .no_deps() // we only care about declared deps, not the resolved graph
        .exec()
        .context("running `cargo metadata`")?;

    let allowed: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::from([
        ("linerule-core", BTreeSet::new()),
        (
            "linerule-platform-windows",
            BTreeSet::from(["linerule-core"]),
        ),
        (
            "linerule-app",
            BTreeSet::from(["linerule-core", "linerule-platform-windows"]),
        ),
    ]);

    let mut violations: Vec<String> = Vec::new();

    for pkg in &metadata.packages {
        if !pkg.name.starts_with(INTERNAL_PREFIX) {
            continue;
        }
        let Some(permitted) = allowed.get(pkg.name.as_str()) else {
            violations.push(format!(
                "unknown internal crate `{}` — extend `dep_graph::run` allow-list",
                pkg.name
            ));
            continue;
        };
        for dep in &pkg.dependencies {
            if !dep.name.starts_with(INTERNAL_PREFIX) {
                continue;
            }
            if dep.name == pkg.name {
                violations.push(format!("crate `{}` depends on itself", pkg.name));
                continue;
            }
            if !permitted.contains(dep.name.as_str()) {
                violations.push(format!(
                    "crate `{}` depends on `{}` — not permitted by one-way DAG",
                    pkg.name, dep.name
                ));
            }
        }
    }

    if violations.is_empty() {
        println!("dep-graph: ok (linerule-app → linerule-platform-windows → linerule-core)");
        Ok(())
    } else {
        for v in &violations {
            eprintln!("[dep-graph] {v}");
        }
        Err(anyhow!(
            "dep-graph: {} violation(s) found",
            violations.len()
        ))
    }
}
