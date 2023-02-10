//! Enumerate source code files used by the TypeScript compiler during
//! compilation. The return value is a list of relative paths from the monorepo
//! root, sorted in alphabetical order.
//!
//! There are two methods of calculating this list of files: the exact
//! way, and using an estimation.
//!
//! The **exact** method uses the TypeScript compiler's [listFilesOnly] flag as the
//! source of truth. We do not try to reimplement this algorithm independently
//! because this list requires following `import` statements in JavaScript and
//! TypeScript code. From the [tsconfig exclude] documentation:
//!
//! > Important: `exclude` *only* changes which files are included as a result
//! > of the `include` setting. A file specified by exclude can still become
//! > part of your codebase due to an import statement in your code, a types
//! > inclusion, a `/// <reference` directive, or being specified in the
//! > `files` list.
//!
//! The TypeScript compiler is a project where the implementation is the spec,
//! so this method of enumeration trades the runtime penalty of invoking the
//! TypeScript compiler for accuracy of output as defined by the "spec".
//!
//! The **estimation** method uses the list of globs from the `include`
//! property in a package's tsconfig.json file to calculate the list of source
//! files.
//!
//! This estimation is currently imprecise (and likely to stay that way) --
//! it makes a best attempt to follow the `exclude` or file-type based rules:
//!
//! > If a glob pattern doesn’t include a file extension, then only files with
//! > supported extensions are included (e.g. .ts, .tsx, and .d.ts by default,
//! > with .js and .jsx if allowJs is set to true).
//!
//! without any guarantee of exhaustive compatibility.
//!
//! Additionally, this method performs no source-code analysis to follow
//! imported files.
//!
//! You might want to use the estimation method if speed is a concern, because it
//! is several orders of magnitude faster than the exact method.
//!
//! [listfilesonly]: https://www.typescriptlang.org/docs/handbook/compiler-options.html#compiler-options
//! [tsconfig exclude]: https://www.typescriptlang.org/tsconfig#exclude

#![forbid(unsafe_code)]
#![deny(warnings, missing_docs)]

use std::{
    collections::{HashMap, HashSet},
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    process::Command,
};

use globwalk::{FileType, GlobWalkerBuilder};
use log::{debug, trace};
use rayon::prelude::*;
use serde::Deserialize;
use typescript_tools::{configuration_file::ConfigurationFile, monorepo_manifest};

mod error;

use crate::error::Error;

/// Method to use to enumerate inputs to the TypeScript compiler.
#[derive(Copy, Clone, Debug)]
pub enum Calculation {
    /// Estimate the true list of inputs to the TypeScript compiler by listing
    /// the files matching the tsconfig's `include` globs.
    Estimate,
    /// Calculate the exact list of inputs to the TypeScript compiler by
    /// invoking `tsc --listFilesOnly`.
    Exact,
}

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

// This gives us a way to look up the typescript-tools PackageManifest
// from the path to a package's tsconfig.json file, but it does incur
// a runtime penalty of reading this information from disk again.
//
// It's a definite hack, but it unblocks today.
#[derive(Debug, Deserialize)]
struct PackageManifest {
    name: String,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct TypescriptPackage {
    scoped_package_name: String,
    tsconfig_file: PathBuf,
}

fn is_glob(string: &str) -> bool {
    string.contains('*')
}

fn glob_file_extension(glob: &str) -> Option<String> {
    if glob.ends_with('*') {
        return None;
    }
    Some(
        glob.rsplit('*')
            .next()
            .expect("Expected glob to contain an asterisk")
            .to_owned(),
    )
}

fn is_monorepo_file(monorepo_root: &Path, file: &Path) -> bool {
    for ancestor in file.ancestors() {
        if ancestor.ends_with(monorepo_root) {
            return true;
        }
    }
    false
}

fn read_json_from_file<T>(filename: &Path) -> Result<T, Error>
where
    for<'de> T: Deserialize<'de>,
{
    // Reading a file into a string before invoking Serde is faster than
    // invoking Serde from a BufReader, see
    // https://github.com/serde-rs/json/issues/160
    let mut string = String::new();
    File::open(filename)?.read_to_string(&mut string)?;
    Ok(
        serde_json::from_str(&string).map_err(|err| Error::TypescriptConfigParseError {
            source: err,
            filename: filename.to_owned(),
        })?,
    )
}

fn remove_relative_path_prefix_from_absolute_path(
    prefix: &Path,
    absolute_path: &Path,
) -> Result<PathBuf, Error> {
    for ancestor in absolute_path.ancestors() {
        if ancestor.ends_with(prefix) {
            return Ok(absolute_path
                .strip_prefix(ancestor)
                .map(|path| path.to_owned())
                .map_err(|err| Error::RelativePathStripError {
                    source: err,
                    absolute_path: absolute_path.to_path_buf(),
                })?);
        }
    }

    eprintln!(
        "Absolute path {:?} did not contain relative-path prefix {:?}",
        absolute_path, prefix,
    );
    panic!();
}

/// Invoke the TypeScript compiler with the [listFilesOnly] flag to enumerate
/// the files included in the compilation process.
fn tsconfig_includes_exact(monorepo_root: &Path, tsconfig: &Path) -> Result<Vec<PathBuf>, Error> {
    let string = String::from_utf8(
        Command::new("tsc")
            .arg("--listFilesOnly")
            .arg("--project")
            .arg(tsconfig)
            .output()?
            .stdout,
    )?;

    let included_files: Vec<PathBuf> = string
        .lines()
        .filter_map(|s| match s.is_empty() {
            true => None,
            false => Some(PathBuf::from(s)),
        })
        .filter_map(|source_file| {
            if is_monorepo_file(monorepo_root, &source_file) {
                let relative_path =
                    remove_relative_path_prefix_from_absolute_path(monorepo_root, &source_file);
                Some(relative_path.unwrap())
            } else {
                None
            }
        })
        .collect();

    Ok(included_files)
}

/// Use the `tsconfig_file`'s `include` configuration to enumerate the list of files
/// matching include globs.
fn tsconfig_includes_estimate(
    monorepo_root: &Path,
    tsconfig_file: &Path,
) -> Result<Vec<PathBuf>, Error> {
    let package_directory = tsconfig_file
        .parent()
        .expect("No package should exist in the monorepo root");
    let tsconfig: TypescriptConfig = read_json_from_file(tsconfig_file)?;

    // LIMITATION: The TypeScript compiler docs state:
    //
    // > If a glob pattern doesn’t include a file extension, then only files
    // > with supported extensions are included (e.g. .ts, .tsx, and .d.ts by
    // > default, with .js and .jsx if allowJs is set to true).
    //
    // This implementation does not examine if globs contain extensions.

    let whitelisted_file_extensions: HashSet<String> = {
        let mut whitelist = vec![".ts", ".tsx", ".d.ts"];
        if tsconfig.compiler_options.allow_js {
            whitelist.append(&mut vec![".js", ".jsx"]);
        }
        let mut whitelist: Vec<String> = whitelist.into_iter().map(|s| s.to_owned()).collect();

        // add extensions from any glob that specifies one
        let mut glob_extensions: Vec<String> = tsconfig
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
                tsconfig.compiler_options.resolve_json_module
            })
            .collect()
    };

    let is_whitelisted_file_extension = |path: &Path| -> bool {
        // Can't use path::extension here because some globs specify more than
        // just a single extension (like .d.ts).
        whitelisted_file_extensions.iter().any(|extension| {
            path.to_str()
                .expect("Path should contain only valid UTF-8")
                .ends_with(extension)
        })
    };

    let included_files: Vec<PathBuf> =
        GlobWalkerBuilder::from_patterns(package_directory, &tsconfig.include)
            .file_type(FileType::FILE)
            .min_depth(0)
            .build()
            .expect("Should be able to create glob walker")
            .into_iter()
            .filter(|maybe_dir_entry| {
                if let Ok(dir_entry) = maybe_dir_entry {
                    is_monorepo_file(monorepo_root, dir_entry.path())
                        && is_whitelisted_file_extension(dir_entry.path())
                } else {
                    true
                }
            })
            .filter_map(|maybe_dir_entry| match maybe_dir_entry {
                Ok(dir_entry) => Some(Ok(dir_entry
                    .path()
                    .strip_prefix(monorepo_root)
                    .map(|path| path.to_owned())
                    .expect(&format!(
                        "Should be able to strip monorepo-root prefix from path in monorepo: {:?}",
                        dir_entry.path()
                    )))),
                Err(err) => Some(Err(err.into())),
            })
            .collect::<Result<Vec<PathBuf>, Error>>()?;

    Ok(included_files)
}

/// Enumerate source code files used by the TypeScript compiler during
/// compilation. The return value is a list of alphabetically-sorted relative
/// paths from the monorepo root, grouped by scoped package name.
///
/// - `monorepo_root` may be an absolute path
/// - `tsconfig_files` should be relative paths from the monorepo root
pub fn tsconfig_includes_by_package_name<P, Q, C>(
    monorepo_root: P,
    tsconfig_files: &[Q],
    calculation_type: C,
) -> Result<HashMap<String, Vec<PathBuf>>, Error>
where
    P: AsRef<Path> + Sync,
    Q: AsRef<Path>,
    C: Into<Calculation> + Copy + Sync + Send,
{
    let lerna_manifest =
        monorepo_manifest::MonorepoManifest::from_directory(monorepo_root.as_ref())?;
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
                .map(|package_manifest| TypescriptPackage {
                    scoped_package_name: package_manifest.contents.name.clone(),
                    tsconfig_file: package_manifest
                        .path()
                        .parent()
                        .expect("No package should exist in the monorepo root")
                        .join("tsconfig.json"),
                })
                .collect())
        })
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
                let mut included_files = match calculation_type.into() {
                    Calculation::Estimate => {
                        tsconfig_includes_estimate(monorepo_root.as_ref(), tsconfig)
                    }
                    Calculation::Exact => tsconfig_includes_exact(monorepo_root.as_ref(), tsconfig),
                }?;
                included_files.sort_unstable();
                Ok((package.scoped_package_name, included_files))
            })
            .collect::<Result<HashMap<_, _>, _>>()?;

    debug!("tsconfig_includes: {:?}", included_files);
    Ok(included_files)
}
