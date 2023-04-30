use std::collections::HashMap;
use std::path::{Path, PathBuf};

use tsconfig_includes::estimate::tsconfig_includes_by_package_name;

struct PackageIncludes {
    tsconfig_file: String,
    includes: Vec<PathBuf>,
}

impl From<(&str, Vec<&str>)> for PackageIncludes {
    fn from((key, value): (&str, Vec<&str>)) -> Self {
        Self {
            tsconfig_file: key.to_string(),
            includes: value.into_iter().map(PathBuf::from).collect(),
        }
    }
}

fn check<T, E>(tsconfigs: T, expected: E)
where
    T: IntoIterator,
    T::Item: AsRef<Path>,
    E: IntoIterator,
    E::Item: Into<PackageIncludes>,
{
    let tsconfigs: Vec<PathBuf> = tsconfigs
        .into_iter()
        .map(|item| item.as_ref().to_owned())
        .collect();

    match tsconfig_includes_by_package_name(&PathBuf::from("test-data/happy-path"), &tsconfigs) {
        Ok(actual) => {
            let expected: HashMap<String, Vec<PathBuf>> = expected
                .into_iter()
                .map(|item| -> (String, Vec<PathBuf>) {
                    let PackageIncludes {
                        tsconfig_file,
                        includes,
                    } = item.into();
                    (tsconfig_file, includes)
                })
                .collect();

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
        ["packages/bar/tsconfig.json"],
        [
            (
                "@typescript-tools/bar",
                vec![
                    "packages/bar/src/bin.ts",
                    "packages/bar/src/index.ts",
                    "packages/bar/src/legacy.js",
                ],
            ),
            (
                "@typescript-tools/foo",
                vec![
                    "packages/foo/src/data.json",
                    "packages/foo/src/index.ts",
                    "packages/foo/src/lib.ts",
                ],
            ),
        ],
    );
}

#[test]
fn list_grouped_estimate_happy_path_dependencies_foo() {
    check(
        ["packages/foo/tsconfig.json"],
        [(
            "@typescript-tools/foo",
            vec![
                "packages/foo/src/data.json",
                "packages/foo/src/index.ts",
                "packages/foo/src/lib.ts",
            ],
        )],
    );
}

#[test]
fn list_grouped_estimate_happy_path_dependencies_foo_and_bar() {
    check(
        ["packages/foo/tsconfig.json", "packages/bar/tsconfig.json"],
        [
            (
                "@typescript-tools/bar",
                vec![
                    "packages/bar/src/bin.ts",
                    "packages/bar/src/index.ts",
                    "packages/bar/src/legacy.js",
                ],
            ),
            (
                "@typescript-tools/foo",
                vec![
                    "packages/foo/src/data.json",
                    "packages/foo/src/index.ts",
                    "packages/foo/src/lib.ts",
                ],
            ),
        ],
    );
}
