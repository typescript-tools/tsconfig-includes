use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::io::{read_json_from_file, FromFileError};

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct TypescriptPackage {
    pub scoped_package_name: String,
    pub tsconfig_file: TypescriptConfigFile,
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub(crate) struct PackageManifestFile(PathBuf);

impl From<PathBuf> for PackageManifestFile {
    fn from(path: PathBuf) -> Self {
        Self(path)
    }
}

#[derive(Debug)]
pub(crate) struct PackageInMonorepoRootError(pub PathBuf);

impl TryFrom<TypescriptConfigFile> for PackageManifestFile {
    type Error = PackageInMonorepoRootError;

    fn try_from(tsconfig_file: TypescriptConfigFile) -> Result<Self, Self::Error> {
        let package_directory = tsconfig_file
            .0
            .parent()
            .ok_or_else(|| PackageInMonorepoRootError(tsconfig_file.0.clone()))?;
        Ok(Self(package_directory.join("package.json")))
    }
}

impl TryFrom<&TypescriptConfigFile> for PackageManifestFile {
    type Error = PackageInMonorepoRootError;

    fn try_from(value: &TypescriptConfigFile) -> Result<Self, Self::Error> {
        Self::try_from(value.to_owned())
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

impl TryFrom<TypescriptConfigFile> for PackageManifest {
    type Error = FromTypescriptConfigFileError;

    fn try_from(tsconfig_file: TypescriptConfigFile) -> Result<Self, Self::Error> {
        let package_manifest_file: PackageManifestFile = tsconfig_file.try_into()?;
        let package_manifest: Self = package_manifest_file.try_into()?;
        Ok(package_manifest)
    }
}

impl TryFrom<&TypescriptConfigFile> for PackageManifest {
    type Error = FromTypescriptConfigFileError;

    fn try_from(value: &TypescriptConfigFile) -> Result<Self, Self::Error> {
        Self::try_from(value.to_owned())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct TypescriptConfigFile(PathBuf);

impl TypescriptConfigFile {
    pub fn as_path(&self) -> &Path {
        &self.0
    }

    pub fn package_directory<P: AsRef<Path>>(
        &self,
        monorepo_root: P,
    ) -> Result<PathBuf, PackageInMonorepoRootError> {
        let tsconfig_path = monorepo_root.as_ref().join(&self.0);
        tsconfig_path
            .parent()
            .map(ToOwned::to_owned)
            .ok_or_else(|| PackageInMonorepoRootError(tsconfig_path))
    }
}

impl<P> From<P> for TypescriptConfigFile
where
    P: AsRef<Path>,
{
    fn from(value: P) -> Self {
        let path = value.as_ref();
        Self(path.to_owned())
    }
}

impl TryFrom<PackageManifestFile> for TypescriptConfigFile {
    type Error = PackageInMonorepoRootError;

    fn try_from(package_manifest_file: PackageManifestFile) -> Result<Self, Self::Error> {
        let package_directory = package_manifest_file
            .0
            .parent()
            .ok_or_else(|| PackageInMonorepoRootError(package_manifest_file.0.clone()))?;
        Ok(Self(package_directory.join("tsconfig.json")))
    }
}

impl TryFrom<&PackageManifestFile> for TypescriptConfigFile {
    type Error = PackageInMonorepoRootError;

    fn try_from(value: &PackageManifestFile) -> Result<Self, Self::Error> {
        Self::try_from(value.to_owned())
    }
}
