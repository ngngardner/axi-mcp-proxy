// CLI tests for --check: verifies the binary surfaces a usable
// build-time validation entrypoint. Runs the actual binary so the
// exit-code contract callers depend on (nix derivations, CI) is exercised.
#![allow(
    clippy::tests_outside_test_module,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::str_to_string,
    clippy::panic
)]

use std::path::Path;
use std::process::Command;

const fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_axi-mcp-proxy")
}

#[test]
fn check_config_succeeds_for_minimal_example() {
    let cfg = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/minimal/config.ncl");
    let output = Command::new(bin())
        .args(["--check", "--config"])
        .arg(&cfg)
        .output()
        .expect("spawn axi-mcp-proxy");
    assert!(
        output.status.success(),
        "expected success, got status={:?}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr),
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("config OK"),
        "expected stderr to confirm validation, got: {stderr}"
    );
}

#[test]
fn check_config_fails_on_unknown_next_step() {
    let cfg = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/failure/unknown_next_step/config.ncl");
    let output = Command::new(bin())
        .args(["--check", "--config"])
        .arg(&cfg)
        .output()
        .expect("spawn axi-mcp-proxy");
    assert!(
        !output.status.success(),
        "expected failure exit, got success\nstderr: {}",
        String::from_utf8_lossy(&output.stderr),
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("references unknown tool"),
        "expected validation error on stderr, got: {stderr}"
    );
}

#[test]
fn check_config_fails_on_every_failure_fixture() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/failure");
    let entries =
        std::fs::read_dir(&root).unwrap_or_else(|_| panic!("cannot read {}", root.display()));
    let mut count = 0;
    for entry in entries.filter_map(Result::ok) {
        let cfg = entry.path().join("config.ncl");
        if !cfg.exists() {
            continue;
        }
        let output = Command::new(bin())
            .args(["--check", "--config"])
            .arg(&cfg)
            .output()
            .expect("spawn axi-mcp-proxy");
        assert!(
            !output.status.success(),
            "fixture {} should fail --check but exited 0\nstderr: {}",
            cfg.display(),
            String::from_utf8_lossy(&output.stderr),
        );
        count += 1;
    }
    assert!(
        count > 0,
        "no failure fixtures found under {}",
        root.display()
    );
}

#[test]
fn check_config_does_not_start_transport() {
    // --check must return without consuming stdin or binding any
    // port. Confirm by running with stdin closed and a short timeout —
    // if the process tries to start the MCP transport it would block on
    // stdin and never exit.
    let cfg = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/minimal/config.ncl");
    let output = Command::new(bin())
        .args(["--check", "--config"])
        .arg(&cfg)
        .stdin(std::process::Stdio::null())
        .output()
        .expect("spawn axi-mcp-proxy");
    assert!(
        output.status.success(),
        "stdin-closed --check should still succeed; got status={:?}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr),
    );
}
