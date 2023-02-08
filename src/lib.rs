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
//! it makes no attempt to follow the `exclude` or file-type based rules:
//!
//! > If a glob pattern doesn’t include a file extension, then only files with
//! > supported extensions are included (e.g. .ts, .tsx, and .d.ts by default,
//! > with .js and .jsx if allowJs is set to true).
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
#![feature(absolute_path)]
#![deny(warnings, missing_docs)]

use std::{
    collections::HashMap,
    fs::File,
    io::Read,
    path::{self, Path, PathBuf},
    process::Command,
};

use globwalk::{FileType, GlobWalkerBuilder};
use log::{debug, trace};
use rayon::prelude::*;
use serde::Deserialize;
use typescript_tools::{configuration_file::ConfigurationFile, monorepo_manifest};

mod error;
mod find_up;

use crate::error::Error;

/// Method to use to enumerate inputs to the TypeScript compiler.
#[derive(Debug)]
pub enum Calculation {
    /// Estimate the true list of inputs to the TypeScript compiler by listing
    /// the files matching the tsconfig's `include` globs.
    Estimate,
    /// Calculate the exact list of inputs to the TypeScript compiler by
    /// invoking `tsc --listFilesOnly`.
    Exact,
}

#[derive(Debug, Deserialize)]
struct TypescriptConfig {
    // DISCUSS: how should we behave if `include` is not present?
    include: Vec<String>,
}

fn is_monorepo_file(monorepo_root: &Path, file: &Path) -> bool {
    for ancestor in file.ancestors() {
        if ancestor == monorepo_root {
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
                let relative_path = pathdiff::diff_paths(&source_file, monorepo_root);
                relative_path
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
    let package_directory = tsconfig_file.parent().unwrap();
    let tsconfig: TypescriptConfig = read_json_from_file(tsconfig_file)?;

    let included_files: Vec<PathBuf> =
        GlobWalkerBuilder::from_patterns(package_directory, &tsconfig.include)
            .file_type(FileType::FILE)
            .min_depth(0)
            .build()
            .expect("Should be able to create glob walker")
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .map(|dir_entry| {
                let relative_path = pathdiff::diff_paths(dir_entry.path(), monorepo_root);
                relative_path.ok_or_else(|| Error::RelativePathError {
                    filename: dir_entry.path().to_owned(),
                })
            })
            .collect::<Result<Vec<_>, Error>>()?;

    Ok(included_files)
}

/// Enumerate source code files used by the TypeScript compiler during
/// compilation. The return value is a list of relative paths from the monorepo
/// root, sorted in alphabetical order.
pub fn tsconfig_includes(
    tsconfig: &Path,
    calculation_type: Calculation,
) -> Result<Vec<PathBuf>, Error> {
    let tsconfig = path::absolute(tsconfig).expect(&format!(
        "Should be able to convert parameter `tsconfig` ({:?}) into an absolute path",
        tsconfig,
    ));
    debug!("tsconfig absolute path is {:?}", tsconfig);

    let monorepo_root = find_up::find_file(&tsconfig, "lerna.json").ok_or_else(|| {
        Error::TypescriptProjectNotInMonorepo {
            filename: tsconfig.to_string_lossy().into_owned(),
        }
    })?;
    debug!("monorepo_root: {:?}", monorepo_root);

    // This relies on an assumption that the package's package.json and tsconfig.json
    // live in the same directory (the package root).
    let target_package_manifest = tsconfig.parent().unwrap().join("package.json");
    debug!("target package manifest: {:?}", target_package_manifest);

    let lerna_manifest = monorepo_manifest::MonorepoManifest::from_directory(&monorepo_root)?;
    let package_manifests_by_package_name = lerna_manifest.package_manifests_by_package_name()?;
    trace!("{:?}", lerna_manifest);

    let package_manifest = lerna_manifest
        .internal_package_manifests()?
        .into_iter()
        .filter(|manifest| &target_package_manifest == &monorepo_root.join(manifest.path()))
        .take(1)
        .next()
        .expect("Expected project to reside in monorepo");

    debug!("package_manifest: {:?}", package_manifest);

    // Enumerate internal dependencies (exclusive)
    let transitive_internal_dependencies_inclusive = {
        let mut packages = package_manifest.transitive_internal_dependency_package_names_exclusive(
            &package_manifests_by_package_name,
        );
        // Make this list inclusive of the target package
        packages.push(&package_manifest);
        packages
    };

    debug!(
        "transitive_internal_dependencies_inclusive: {:?}",
        transitive_internal_dependencies_inclusive
            .iter()
            .map(|manifest| manifest.contents.name.clone())
            .collect::<Vec<_>>()
    );

    let mut included_files: Vec<PathBuf> = transitive_internal_dependencies_inclusive
        .into_par_iter()
        .map(|manifest| {
            // This relies on the assumption that tsconfig.json is always the name of the tsconfig file
            let tsconfig = &monorepo_root
                .join(manifest.path())
                .parent()
                .unwrap()
                .join("tsconfig.json");
            match calculation_type {
                Calculation::Estimate => tsconfig_includes_estimate(&monorepo_root, tsconfig),
                Calculation::Exact => tsconfig_includes_exact(&monorepo_root, tsconfig),
            }
        })
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .flatten()
        .collect();

    included_files.sort_unstable();

    debug!("tsconfig_includes: {:?}", included_files);
    Ok(included_files)
}

/// Enumerate source code files used by the TypeScript compiler during
/// compilation. The return value is a list of alphabetically-sorted relative
/// paths from the monorepo root, grouped by scoped package name.
pub fn tsconfig_includes_by_package_name(
    tsconfig: &Path,
    calculation_type: Calculation,
) -> Result<HashMap<String, Vec<PathBuf>>, Error> {
    let tsconfig = path::absolute(tsconfig).expect(&format!(
        "Should be able to convert parameter `tsconfig` ({:?}) into an absolute path",
        tsconfig,
    ));
    debug!("tsconfig absolute path is {:?}", tsconfig);

    let monorepo_root = find_up::find_file(&tsconfig, "lerna.json").ok_or_else(|| {
        Error::TypescriptProjectNotInMonorepo {
            filename: tsconfig.to_string_lossy().into_owned(),
        }
    })?;
    debug!("monorepo_root: {:?}", monorepo_root);

    // This relies on an assumption that the package's package.json and tsconfig.json
    // live in the same directory (the package root).
    let target_package_manifest = tsconfig.parent().unwrap().join("package.json");
    debug!("target package manifest: {:?}", target_package_manifest);

    let lerna_manifest = monorepo_manifest::MonorepoManifest::from_directory(&monorepo_root)?;
    let package_manifests_by_package_name = lerna_manifest.package_manifests_by_package_name()?;
    trace!("{:?}", lerna_manifest);

    let package_manifest = lerna_manifest
        .internal_package_manifests()?
        .into_iter()
        .filter(|manifest| &target_package_manifest == &monorepo_root.join(manifest.path()))
        .take(1)
        .next()
        .expect("Expected project to reside in monorepo");

    debug!("package_manifest: {:?}", package_manifest);

    // Enumerate internal dependencies (exclusive)
    let transitive_internal_dependencies_inclusive = {
        let mut packages = package_manifest.transitive_internal_dependency_package_names_exclusive(
            &package_manifests_by_package_name,
        );
        // Make this list inclusive of the target package
        packages.push(&package_manifest);
        packages
    };

    debug!(
        "transitive_internal_dependencies_inclusive: {:?}",
        transitive_internal_dependencies_inclusive
            .iter()
            .map(|manifest| manifest.contents.name.clone())
            .collect::<Vec<_>>()
    );

    let included_files: HashMap<String, Vec<PathBuf>> = transitive_internal_dependencies_inclusive
        .into_par_iter()
        .map(|manifest| -> Result<(_, _), Error> {
            // This relies on the assumption that tsconfig.json is always the name of the tsconfig file
            let tsconfig = &monorepo_root
                .join(manifest.path())
                .parent()
                .unwrap()
                .join("tsconfig.json");
            let mut included_files = match calculation_type {
                Calculation::Estimate => tsconfig_includes_estimate(&monorepo_root, tsconfig),
                Calculation::Exact => tsconfig_includes_exact(&monorepo_root, tsconfig),
            }?;
            included_files.sort_unstable();
            Ok((manifest.contents.name.clone(), included_files))
        })
        .collect::<Result<HashMap<_, _>, _>>()?;

    debug!("tsconfig_includes: {:?}", included_files);
    Ok(included_files)
}
