use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::io::{read_json_from_file, FromFileError};

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct TypescriptPackage {
    pub scoped_package_name: String,
    pub tsconfig_file: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub(crate) struct PackageManifestFile(PathBuf);

#[derive(Debug)]
pub(crate) struct PackageInMonorepoRootError(PathBuf);

impl TryFrom<&TypescriptConfigFile> for PackageManifestFile {
    type Error = PackageInMonorepoRootError;

    fn try_from(tsconfig_file: &TypescriptConfigFile) -> Result<Self, Self::Error> {
        let package_directory = tsconfig_file
            .0
            .parent()
            .ok_or_else(|| PackageInMonorepoRootError(tsconfig_file.0.to_owned()))?;
        Ok(Self(package_directory.join("package.json")))
    }
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

impl TryFrom<PackageManifestFile> for PackageManifest {
    type Error = FromFileError;

    fn try_from(manifest_file: PackageManifestFile) -> Result<Self, Self::Error> {
        read_json_from_file(manifest_file.0)
    }
}

#[derive(Debug)]
pub(crate) enum FromTypescriptConfigFileError {
    PackageInMonorepoRoot(PathBuf),
    FromFile(FromFileError),
}

impl From<PackageInMonorepoRootError> for FromTypescriptConfigFileError {
    fn from(err: PackageInMonorepoRootError) -> Self {
        Self::PackageInMonorepoRoot(err.0)
    }
}

impl From<FromFileError> for FromTypescriptConfigFileError {
    fn from(err: FromFileError) -> Self {
        Self::FromFile(err)
    }
}

impl TryFrom<&TypescriptConfigFile> for PackageManifest {
    type Error = FromTypescriptConfigFileError;

    fn try_from(tsconfig_file: &TypescriptConfigFile) -> Result<Self, Self::Error> {
        let package_manifest_file: PackageManifestFile = tsconfig_file.try_into()?;
        let package_manifest: Self = package_manifest_file.try_into()?;
        Ok(package_manifest)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct TypescriptConfigFile(PathBuf);

impl<P> From<P> for TypescriptConfigFile
where
    P: AsRef<Path>,
{
    fn from(value: P) -> Self {
        let path = value.as_ref();
        Self(path.to_owned())
    }
}
