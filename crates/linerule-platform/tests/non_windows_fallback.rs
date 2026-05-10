//! On non-Windows targets the production `run` fn must short-circuit
//! to `RunError::Unsupported` so the binary still cross-compiles for
//! smoke tests (cf. ADR-0004 / ADR-0010).

#![cfg(not(target_os = "windows"))]

use linerule_core::State;
use linerule_platform::{RunError, run};

#[test]
fn run_returns_unsupported_on_non_windows_targets() {
    let result = run(State::default(), &[]);
    let err = result.expect_err("non-Windows targets must reject run()");
    match err {
        RunError::Unsupported(reason) => {
            assert!(
                reason.contains("windows") || reason.contains("Windows"),
                "Unsupported reason should mention Windows: {reason:?}",
            );
        }
        other => panic!("expected RunError::Unsupported, got {other:?}"),
    }
}
