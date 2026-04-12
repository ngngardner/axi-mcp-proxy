// Integration tests — unwrap/expect for brevity, test_module lint inapplicable
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::str_to_string,
    clippy::tests_outside_test_module,
    clippy::panic
)]

use std::path::Path;

/// Collect all `config.ncl` files under a directory (one level deep: `dir/*/config.ncl`).
fn collect_configs(dir: &Path) -> Vec<std::path::PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        panic!("cannot read directory: {}", dir.display());
    };
    let mut paths: Vec<std::path::PathBuf> = entries
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|ft| ft.is_dir()))
        .map(|e| e.path().join("config.ncl"))
        .filter(|p| p.exists())
        .collect();
    paths.sort();
    paths
}

#[test]
fn examples_all_pass() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples");
    let configs = collect_configs(&root);
    assert!(
        !configs.is_empty(),
        "no example configs found under {}",
        root.display()
    );
    for path in &configs {
        let result = axi_mcp_proxy::config::load(path);
        assert!(
            result.is_ok(),
            "example {} should load successfully: {}",
            path.display(),
            result.unwrap_err()
        );
    }
}

#[test]
fn fixtures_success_all_pass() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/success");
    let configs = collect_configs(&root);
    assert!(
        !configs.is_empty(),
        "no success fixtures found under {}",
        root.display()
    );
    for path in &configs {
        let result = axi_mcp_proxy::config::load(path);
        assert!(
            result.is_ok(),
            "fixture {} should load successfully: {}",
            path.display(),
            result.unwrap_err()
        );
    }
}

#[test]
fn fixtures_failure_all_fail() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/failure");
    let configs = collect_configs(&root);
    assert!(
        !configs.is_empty(),
        "no failure fixtures found under {}",
        root.display()
    );
    for path in &configs {
        let result = axi_mcp_proxy::config::load(path);
        assert!(
            result.is_err(),
            "fixture {} should fail to load but passed",
            path.display()
        );
    }
}
