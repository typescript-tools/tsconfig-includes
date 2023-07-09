use std::collections::HashMap;
use std::path::PathBuf;

use tsconfig_includes::{tsconfig_includes_by_package_name, Calculation};

fn check(tsconfig: &[&str], expected: &[(&str, &str)]) {
    match tsconfig_includes_by_package_name(
        &PathBuf::from("test-data/happy-path"),
        tsconfig
            .into_iter()
            .map(|s| PathBuf::from(s))
            .collect::<Vec<_>>()
            .as_ref(),
        Calculation::Estimate,
    ) {
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
fn list_grouped_estimate_happy_path_dependencies_bar() {
    check(
        &["packages/bar/tsconfig.json"],
        &[
            ("@typescript-tools/bar", "packages/bar/src/bin.ts"),
            ("@typescript-tools/bar", "packages/bar/src/index.ts"),
            ("@typescript-tools/bar", "packages/bar/src/legacy.js"),
            ("@typescript-tools/foo", "packages/foo/src/data.json"),
            ("@typescript-tools/foo", "packages/foo/src/index.ts"),
            ("@typescript-tools/foo", "packages/foo/src/lib.ts"),
        ],
    );
}

#[test]
fn list_grouped_estimate_happy_path_dependencies_foo() {
    check(
        &["packages/foo/tsconfig.json"],
        &[
            ("@typescript-tools/foo", "packages/foo/src/data.json"),
            ("@typescript-tools/foo", "packages/foo/src/index.ts"),
            ("@typescript-tools/foo", "packages/foo/src/lib.ts"),
        ],
    );
}

#[test]
fn list_grouped_estimate_happy_path_dependencies_foo_and_bar() {
    check(
        &["packages/foo/tsconfig.json", "packages/bar/tsconfig.json"],
        &[
            ("@typescript-tools/bar", "packages/bar/src/bin.ts"),
            ("@typescript-tools/bar", "packages/bar/src/index.ts"),
            ("@typescript-tools/bar", "packages/bar/src/legacy.js"),
            ("@typescript-tools/foo", "packages/foo/src/data.json"),
            ("@typescript-tools/foo", "packages/foo/src/index.ts"),
            ("@typescript-tools/foo", "packages/foo/src/lib.ts"),
        ],
    );
}
