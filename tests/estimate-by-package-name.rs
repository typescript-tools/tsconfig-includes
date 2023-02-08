use std::collections::HashMap;
use std::path::PathBuf;

use tsconfig_includes::{tsconfig_includes_by_package_name, Calculation};

fn check(tsconfig: &str, expected: &[(&str, &str)]) {
    match tsconfig_includes_by_package_name(&PathBuf::from(tsconfig), Calculation::Estimate) {
        Ok(actual) => {
            let expected = expected.iter().fold(
                HashMap::new(),
                |mut acc, (package_name, included_file)| -> HashMap<String, Vec<PathBuf>> {
                    let included_files = acc.entry(package_name.to_owned().to_owned()).or_default();
                    included_files.push(PathBuf::from(included_file));
                    acc
                },
            );

            assert_eq!(actual, expected);
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
            ("bar", "packages/bar/src/bin.ts"),
            ("bar", "packages/bar/src/index.ts"),
            ("foo", "packages/foo/src/index.ts"),
            ("foo", "packages/foo/src/lib.ts"),
        ],
    );
}

#[test]
fn list_happy_path_dependencies_foo() {
    check(
        "test-data/happy-path/packages/foo/tsconfig.json",
        &[
            ("foo", "packages/foo/src/index.ts"),
            ("foo", "packages/foo/src/lib.ts"),
        ],
    );
}
