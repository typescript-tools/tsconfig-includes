use std::path::PathBuf;

use serde::Deserialize;

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct TypescriptPackage {
    pub scoped_package_name: String,
    pub tsconfig_file: PathBuf,
}

// This gives us a way to look up the typescript-tools PackageManifest
// from the path to a package's tsconfig.json file, but it does incur
// a runtime penalty of reading this information from disk again.
//
// It's a definite hack, but it unblocks today.
#[derive(Debug, Deserialize)]
pub(crate) struct PackageManifest {
    pub name: String,
}
