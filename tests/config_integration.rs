// Integration tests — unwrap/expect/to_string for brevity, test_module lint
// inapplicable to standalone test files
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::str_to_string,
    clippy::tests_outside_test_module
)]

use std::path::Path;

// Import the crate (it's a binary crate, so we need to reference it as a library)
// For now, test via the example config file

#[test]
fn test_example_repo_context_loads() {
    use nickel_lang_core::error::NullReporter;
    use nickel_lang_core::eval::cache::CacheImpl;
    use nickel_lang_core::program::Program;

    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("lib/example_repo_context.ncl");

    let mut prog = Program::<CacheImpl>::new_from_file(&path, std::io::stderr(), NullReporter {})
        .expect("should load program");

    prog.add_import_paths(std::iter::once(path.parent().unwrap().to_path_buf()));

    let result = prog.eval_full_for_export();
    assert!(result.is_ok(), "example config should evaluate: {result:?}");
}
