use std::{
    collections::{HashMap, HashSet},
    fmt::Display,
    path::{Path, PathBuf},
    process::Command,
    string,
};

use log::{debug, trace};
use rayon::prelude::*;
use typescript_tools::{configuration_file::ConfigurationFile, monorepo_manifest};

use crate::{
    io::read_json_from_file,
    path::{self, is_monorepo_file, remove_relative_path_prefix_from_absolute_path},
    typescript_package::{PackageManifest, TypescriptPackage},
};

#[derive(Debug)]
#[non_exhaustive]
pub struct EnumerateError {
    kind: EnumerateErrorKind,
}

impl Display for EnumerateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.kind {
            EnumerateErrorKind::Command(_) => write!(f, "unable to spawn child process"),
            EnumerateErrorKind::TypescriptCompiler { command, error } => {
                writeln!(
                    f,
                    "tsc exited with non-zero status code for command {:?}:",
                    command
                )?;
                write!(f, "{:?}", error)
            }
            EnumerateErrorKind::InvalidUtf8(_) => {
                write!(f, "command output included invalid UTF-8")
            }
            EnumerateErrorKind::StripPrefix(_) => write!(f, "unable to manipulate file path"),
        }
    }
}

impl std::error::Error for EnumerateError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.kind {
            EnumerateErrorKind::Command(err) => Some(err),
            EnumerateErrorKind::TypescriptCompiler {
                command: _,
                error: _,
            } => None,
            EnumerateErrorKind::InvalidUtf8(err) => Some(err),
            EnumerateErrorKind::StripPrefix(err) => Some(err),
        }
    }
}

#[derive(Debug)]
pub enum EnumerateErrorKind {
    #[non_exhaustive]
    Command(std::io::Error),
    #[non_exhaustive]
    TypescriptCompiler { command: String, error: Vec<u8> },
    #[non_exhaustive]
    InvalidUtf8(string::FromUtf8Error),
    #[non_exhaustive]
    StripPrefix(path::StripPrefixError),
}

impl From<string::FromUtf8Error> for EnumerateErrorKind {
    fn from(err: string::FromUtf8Error) -> Self {
        Self::InvalidUtf8(err)
    }
}

impl From<path::StripPrefixError> for EnumerateErrorKind {
    fn from(err: path::StripPrefixError) -> Self {
        Self::StripPrefix(err)
    }
}

/// Invoke the TypeScript compiler with the [listFilesOnly] flag to enumerate
/// the files included in the compilation process.
fn tsconfig_includes_exact(
    monorepo_root: &Path,
    tsconfig: &Path,
) -> Result<Vec<PathBuf>, EnumerateError> {
    (|| {
        let child = Command::new("tsc")
            .arg("--listFilesOnly")
            .arg("--project")
            .arg(tsconfig)
            .output()
            .map_err(EnumerateErrorKind::Command)?;
        if child.status.code() != Some(0) {
            return Err(EnumerateErrorKind::TypescriptCompiler {
                command: format!("tsc --listFilesOnly --project {:?}", tsconfig),
                error: child.stderr,
            });
        }
        let stdout = String::from_utf8(child.stdout)?;

        let included_files: Vec<PathBuf> = stdout
            .lines()
            // Drop the empty newline at the end of stdout
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .filter(|path| is_monorepo_file(monorepo_root, path))
            .map(|source_file| {
                remove_relative_path_prefix_from_absolute_path(monorepo_root, &source_file)
            })
            .collect::<Result<_, _>>()?;

        Ok(included_files)
    })()
    .map_err(|kind| EnumerateError { kind })
}

#[derive(Debug)]
#[non_exhaustive]
pub struct Error {
    kind: ErrorKind,
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.kind {
            ErrorKind::PackageInMonorepoRoot(path) => {
                write!(f, "unexpected package in monorepo root: {:?}", path)
            }
            _ => write!(f, "unable to enumerate exact tsconfig includes"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.kind {
            ErrorKind::MonorepoManifest(err) => Some(err),
            ErrorKind::EnumeratePackageManifestsError(err) => Some(err),
            ErrorKind::PackageInMonorepoRoot(_) => None,
            ErrorKind::FromFile(err) => Some(err),
            ErrorKind::Enumerate(err) => Some(err),
        }
    }
}

impl From<ErrorKind> for Error {
    fn from(kind: ErrorKind) -> Self {
        Self { kind }
    }
}

impl From<typescript_tools::io::FromFileError> for Error {
    fn from(err: typescript_tools::io::FromFileError) -> Self {
        Self {
            kind: ErrorKind::MonorepoManifest(err),
        }
    }
}

impl From<typescript_tools::monorepo_manifest::EnumeratePackageManifestsError> for Error {
    fn from(err: typescript_tools::monorepo_manifest::EnumeratePackageManifestsError) -> Self {
        Self {
            kind: ErrorKind::EnumeratePackageManifestsError(err),
        }
    }
}

impl From<crate::io::FromFileError> for Error {
    fn from(err: crate::io::FromFileError) -> Self {
        Self {
            kind: ErrorKind::FromFile(err),
        }
    }
}

impl From<EnumerateError> for Error {
    fn from(err: EnumerateError) -> Self {
        Self {
            kind: ErrorKind::Enumerate(err),
        }
    }
}

#[derive(Debug)]
pub enum ErrorKind {
    #[non_exhaustive]
    MonorepoManifest(typescript_tools::io::FromFileError),
    #[non_exhaustive]
    EnumeratePackageManifestsError(
        typescript_tools::monorepo_manifest::EnumeratePackageManifestsError,
    ),
    #[non_exhaustive]
    PackageInMonorepoRoot(PathBuf),
    #[non_exhaustive]
    FromFile(crate::io::FromFileError),
    #[non_exhaustive]
    Enumerate(EnumerateError),
}

/// Enumerate source code files used by the TypeScript compiler during
/// compilation. The return value is a list of alphabetically-sorted relative
/// paths from the monorepo root, grouped by scoped package name.
///
/// - `monorepo_root` may be an absolute path
/// - `tsconfig_files` should be relative paths from the monorepo root
pub fn tsconfig_includes_by_package_name<P, Q>(
    monorepo_root: P,
    tsconfig_files: &[Q],
) -> Result<HashMap<String, Vec<PathBuf>>, Error>
where
    P: AsRef<Path> + Sync,
    Q: AsRef<Path>,
{
    let lerna_manifest =
        monorepo_manifest::MonorepoManifest::from_directory(monorepo_root.as_ref())
            .map_err(|thing| thing)?;
    let package_manifests_by_package_name = lerna_manifest.package_manifests_by_package_name()?;
    trace!("{:?}", lerna_manifest);

    // As relative path from monorepo root
    let transitive_internal_dependency_tsconfigs_inclusive_to_enumerate: HashSet<
        TypescriptPackage,
    > = tsconfig_files
        .iter()
        .map(|tsconfig_file| -> Result<Vec<TypescriptPackage>, Error> {
            let package_manifest_file = tsconfig_file
                .as_ref()
                .parent()
                .ok_or_else(|| ErrorKind::PackageInMonorepoRoot(tsconfig_file.as_ref().to_owned()))?
                .join("package.json");
            let PackageManifest {
                name: package_manifest_name,
            } = read_json_from_file(&monorepo_root.as_ref().join(package_manifest_file))?;
            let package_manifest = package_manifests_by_package_name
                .get(&package_manifest_name)
                .expect(&format!(
                    "tsconfig {:?} should belong to a package in the lerna monorepo",
                    tsconfig_file.as_ref()
                ));

            let transitive_internal_dependencies_inclusive = {
                // Enumerate internal dependencies (exclusive)
                let mut packages = package_manifest
                    .transitive_internal_dependency_package_names_exclusive(
                        &package_manifests_by_package_name,
                    );
                // Make this list inclusive of the target package
                packages.push(&package_manifest);
                packages
            };

            Ok(transitive_internal_dependencies_inclusive
                .iter()
                .map(|package_manifest| {
                    let path = package_manifest.path();
                    TypescriptPackage {
                        scoped_package_name: package_manifest.contents.name.clone(),
                        tsconfig_file: path
                            .parent()
                            .ok_or_else(|| ErrorKind::PackageInMonorepoRoot(path.to_owned()))
                            // REFACTOR: avoid unwrap
                            .expect("No package should exist in the monorepo root")
                            .join("tsconfig.json"),
                    }
                })
                .collect())
        })
        // REFACTOR: avoid intermediate allocations
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .flatten()
        .collect();

    debug!(
        "transitive_internal_dependency_tsconfigs_inclusive_to_enumerate: {:?}",
        transitive_internal_dependency_tsconfigs_inclusive_to_enumerate
    );

    let included_files: HashMap<String, Vec<PathBuf>> =
        transitive_internal_dependency_tsconfigs_inclusive_to_enumerate
            .into_par_iter()
            .map(|package| -> Result<(_, _), Error> {
                // This relies on the assumption that tsconfig.json is always the name of the tsconfig file
                let tsconfig = &monorepo_root.as_ref().join(package.tsconfig_file);
                let mut included_files = tsconfig_includes_exact(monorepo_root.as_ref(), tsconfig)?;
                included_files.sort_unstable();
                Ok((package.scoped_package_name, included_files))
            })
            .collect::<Result<HashMap<_, _>, _>>()?;

    debug!("tsconfig_includes: {:?}", included_files);
    Ok(included_files)
}
