use std::{
    collections::{HashMap, HashSet},
    fmt::Display,
    iter,
    path::{Path, PathBuf},
};

use globwalk::{FileType, GlobWalkerBuilder};
use log::{debug, trace};
use rayon::prelude::*;
use serde::Deserialize;
use typescript_tools::{configuration_file::ConfigurationFile, monorepo_manifest};

use crate::{
    io::read_json_from_file,
    path::{self, *},
    typescript_package::{PackageManifest, TypescriptPackage},
};

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CompilerOptions {
    #[serde(default)]
    allow_js: bool,
    #[serde(default)]
    resolve_json_module: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TypescriptConfig {
    #[serde(default)]
    compiler_options: CompilerOptions,
    // DISCUSS: how should we behave if `include` is not present?
    include: Vec<String>,
}

impl TypescriptConfig {
    /// LIMITATION: The TypeScript compiler docs state:
    ///
    /// > If a glob pattern doesn’t include a file extension, then only files
    /// > with supported extensions are included (e.g. .ts, .tsx, and .d.ts by
    /// > default, with .js and .jsx if allowJs is set to true).
    ///
    /// This implementation does not examine if globs contain extensions.
    fn whitelisted_file_extensions(&self) -> HashSet<String> {
        let mut whitelist: Vec<String> = vec![
            String::from(".ts"),
            String::from(".tsx"),
            String::from(".d.ts"),
        ];
        if self.compiler_options.allow_js {
            whitelist.append(&mut vec![String::from(".js"), String::from(".jsx")]);
        }

        // add extensions from any glob that specifies one
        let mut glob_extensions: Vec<String> = self
            .include
            .iter()
            .filter(|pattern| is_glob(pattern))
            .filter_map(|glob| glob_file_extension(glob))
            .collect();

        // FIXME: glob extensions apply to a specific glob, not every glob
        whitelist.append(&mut glob_extensions);
        whitelist
            .into_iter()
            .filter(|extension| {
                if !extension.ends_with(".json") {
                    return true;
                }
                // For JSON modules, the presence of a "src/**/*.json" include glob
                // is not enough, JSON imports are still gated by this compiler option.
                self.compiler_options.resolve_json_module
            })
            .collect()
    }
}

#[derive(Debug)]
#[non_exhaustive]
pub struct BuildWalkerError {
    kind: BuildWalkerErrorKind,
}

impl Display for BuildWalkerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.kind {
            BuildWalkerErrorKind::PackageInMonorepoRoot(path) => {
                write!(f, "unexpected package in monorepo root: {:?}", path)
            }
            BuildWalkerErrorKind::IO(_) => write!(f, "unable to estimate tsconfig includes"),
        }
    }
}

impl std::error::Error for BuildWalkerError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.kind {
            BuildWalkerErrorKind::IO(err) => Some(err),
            BuildWalkerErrorKind::PackageInMonorepoRoot(_) => None,
        }
    }
}

#[derive(Debug)]
pub enum BuildWalkerErrorKind {
    #[non_exhaustive]
    IO(crate::io::FromFileError),
    #[non_exhaustive]
    PackageInMonorepoRoot(PathBuf),
}

impl From<crate::io::FromFileError> for BuildWalkerErrorKind {
    fn from(err: crate::io::FromFileError) -> Self {
        Self::IO(err)
    }
}

#[derive(Debug)]
#[non_exhaustive]
pub struct WalkError {
    kind: WalkErrorKind,
}

impl Display for WalkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.kind {
            WalkErrorKind::WalkError(_) => write!(f, "unable to walk directory tree"),
            // DISCUSS: is this something we can fully test for at compile time?
            // If so, we can use `expect` instead of exposing this possibility to the user.
            WalkErrorKind::Path(_) => write!(f, "unable to strip path prefix"),
        }
    }
}

impl std::error::Error for WalkError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.kind {
            WalkErrorKind::Path(err) => Some(err),
            WalkErrorKind::WalkError(err) => Some(err),
        }
    }
}

impl From<globwalk::WalkError> for WalkError {
    fn from(err: globwalk::WalkError) -> Self {
        Self {
            kind: WalkErrorKind::WalkError(err),
        }
    }
}

#[derive(Debug)]
pub enum WalkErrorKind {
    #[non_exhaustive]
    Path(path::StripPrefixError),
    #[non_exhaustive]
    WalkError(globwalk::WalkError),
}

/// Use the `tsconfig_file`'s `include` configuration to enumerate the list of files
/// matching include globs.
fn tsconfig_includes_estimate<'a, 'b>(
    monorepo_root: &'a Path,
    tsconfig_file: &'b Path,
) -> Result<impl Iterator<Item = Result<PathBuf, WalkError>>, BuildWalkerError> {
    let monorepo_root = monorepo_root.to_owned();
    let tsconfig_file = tsconfig_file.to_owned();
    let package_directory = tsconfig_file.parent().ok_or_else(|| BuildWalkerError {
        kind: BuildWalkerErrorKind::PackageInMonorepoRoot(tsconfig_file.to_owned()),
    })?;
    let tsconfig: TypescriptConfig =
        read_json_from_file(&tsconfig_file).map_err(|err| BuildWalkerError {
            kind: BuildWalkerErrorKind::IO(err),
        })?;

    let whitelisted_file_extensions = tsconfig.whitelisted_file_extensions();

    let is_whitelisted_file_extension = move |path: &Path| -> bool {
        // Can't use path::extension here because some globs specify more than
        // just a single extension (like .d.ts).
        whitelisted_file_extensions.iter().any(|extension| {
            path.to_str()
                .expect("Path should contain only valid UTF-8")
                .ends_with(extension)
        })
    };

    let monorepo_root_two = monorepo_root.clone();
    let included_files = GlobWalkerBuilder::from_patterns(package_directory, &tsconfig.include)
        .file_type(FileType::FILE)
        .min_depth(0)
        .build()
        .expect("should be able to create glob walker")
        .filter(move |maybe_dir_entry| match maybe_dir_entry {
            Ok(dir_entry) => {
                is_monorepo_file(&monorepo_root_two, dir_entry.path())
                    && is_whitelisted_file_extension(dir_entry.path())
            }
            Err(_) => true,
        })
        .map(move |maybe_dir_entry| -> Result<PathBuf, WalkError> {
            let dir_entry = maybe_dir_entry?;
            let path = dir_entry
                .path()
                .strip_prefix(&monorepo_root)
                .map(ToOwned::to_owned)
                .expect(&format!(
                    "Should be able to strip monorepo-root prefix from path in monorepo: {:?}",
                    dir_entry.path()
                ));
            Ok(path)
        });

    Ok(included_files)
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
            _ => write!(f, "unable to estimate tsconfig includes"),
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
            ErrorKind::BuildWalker(err) => Some(err),
            ErrorKind::Walk(err) => Some(err),
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

impl From<BuildWalkerError> for Error {
    fn from(err: BuildWalkerError) -> Self {
        match err.kind {
            // avoid nesting this error to present a cleaner backtrace
            BuildWalkerErrorKind::PackageInMonorepoRoot(path) => Self {
                kind: ErrorKind::PackageInMonorepoRoot(path),
            },
            _ => Self {
                kind: ErrorKind::BuildWalker(err),
            },
        }
    }
}

impl From<WalkError> for Error {
    fn from(err: WalkError) -> Self {
        Self {
            kind: ErrorKind::Walk(err),
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
    BuildWalker(BuildWalkerError),
    #[non_exhaustive]
    Walk(WalkError),
}

/// Enumerate source code files used by the TypeScript compiler during
/// compilation. The return value is a list of alphabetically-sorted relative
/// paths from the monorepo root, grouped by scoped package name.
///
/// - `monorepo_root` may be an absolute path
/// - `tsconfig_files` should be relative paths from the monorepo root
pub fn tsconfig_includes_by_package_name<P, Q>(
    monorepo_root: P,
    tsconfig_files: Q,
) -> Result<HashMap<String, Vec<PathBuf>>, Error>
where
    P: AsRef<Path> + Sync,
    Q: IntoIterator,
    Q::Item: AsRef<Path>,
{
    // REFACTOR: avoid duplicated code in estimate.rs and exact.rs
    let lerna_manifest =
        monorepo_manifest::MonorepoManifest::from_directory(monorepo_root.as_ref())?;
    let package_manifests_by_package_name = lerna_manifest.package_manifests_by_package_name()?;
    trace!("{:?}", lerna_manifest);

    // As relative path from monorepo root
    let transitive_internal_dependency_tsconfigs_inclusive_to_enumerate: HashSet<
        TypescriptPackage,
    > = tsconfig_files
        .into_iter()
        .map(|tsconfig_file| -> Result<Vec<TypescriptPackage>, Error> {
            let package_manifest_file = tsconfig_file
                .as_ref()
                .parent()
                .expect("No package should exist in the monorepo root")
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

            // DISCUSS: what's the deal with transitive deps if enumerate is point and shoot?
            let transitive_internal_dependencies_inclusive = {
                // Enumerate internal dependencies (exclusive)
                package_manifest
                    .transitive_internal_dependency_package_names_exclusive(
                        &package_manifests_by_package_name,
                    )
                    // Make this list inclusive of the target package
                    .chain(iter::once(package_manifest))
            };

            Ok(transitive_internal_dependencies_inclusive
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
                let mut included_files: Vec<_> =
                    tsconfig_includes_estimate(monorepo_root.as_ref(), tsconfig)?
                        .collect::<Result<_, _>>()?;
                included_files.sort_unstable();
                Ok((package.scoped_package_name, included_files))
            })
            .collect::<Result<HashMap<_, _>, _>>()?;

    debug!("tsconfig_includes: {:?}", included_files);
    Ok(included_files)
}
