use std::path::PathBuf;

use tsconfig_includes::{tsconfig_includes, Calculation};

fn check(tsconfig: &str, expected: &[&str]) {
    match tsconfig_includes(&PathBuf::from(tsconfig), Calculation::Exact) {
        Ok(actual) => {
            assert_eq!(
                actual,
                expected
                    .iter()
                    .map(|s| PathBuf::from(s))
                    .collect::<Vec<_>>()
            );
        }
        // Don't care what went wrong for now
        Err(err) => {
            panic!("Unexpected error: {:?}", err);
        }
    };
}

#[test]
fn list_happy_path_dependencies_bar() {
    check(
        "test-data/happy-path/packages/bar/tsconfig.json",
        &[
            "packages/bar/src/bin.ts",
            "packages/bar/src/index.ts",
            "packages/foo/src/index.ts",
            "packages/foo/src/lib.ts",
        ],
    );
}

#[test]
fn list_happy_path_dependencies_foo() {
    check(
        "test-data/happy-path/packages/foo/tsconfig.json",
        &["packages/foo/src/index.ts", "packages/foo/src/lib.ts"],
    );
}
