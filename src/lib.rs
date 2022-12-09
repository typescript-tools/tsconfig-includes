use std::{
    path::{Path, PathBuf},
    process::Command,
};

use log::{debug, trace};
use rayon::prelude::*;
use typescript_tools::{configuration_file::ConfigurationFile, monorepo_manifest};

mod error;
mod find_up;

use crate::error::Error;

fn list_included_files(tsconfig: &Path) -> Result<Vec<PathBuf>, Error> {
    let string = String::from_utf8(
        Command::new("tsc")
            .arg("--listFilesOnly")
            .arg("--project")
            .arg(tsconfig)
            .output()?
            .stdout,
    )?;
    Ok(string
        .split('\n')
        .filter(|s| !s.is_empty())
        .filter(|s| !s.starts_with("/nix/store/"))
        .map(|s| PathBuf::from(s))
        .collect())
}

/// Invoke the TypeScript compiler with the [listFilesOnly] flag to enumerate
/// the files included in the compilation process.
///
/// This function leans on the TypeScript compiler to determine this list
/// of files used in the compilation process (instead of trying to calculate
/// the list independently) because this list requires following `import`
/// statements in JavaScript and TypeScript code. From the [tsconfig exclude]
/// documentation:
///
/// > Important: `exclude` *only* changes which files are included as a result
/// > of the `include` setting. A file specified by exclude can still become
/// > part of your codebase due to an import statement in your code, a types
/// > inclusion, a `/// <reference` directive, or being specified in the
/// > `files` list.
///
/// The TypeScript compiler is a project where the implementation is the spec,
/// so this project trades the runtime penalty of invoking the TypeScript
/// compiler for accuracy of output as defined by the "spec".
///
/// [listfilesonly]: https://www.typescriptlang.org/docs/handbook/compiler-options.html#compiler-options
/// [tsconfig exclude]: https://www.typescriptlang.org/tsconfig#exclude
pub fn tsconfig_includes(tsconfig: &Path) -> Result<Vec<PathBuf>, Error> {
    let monorepo_root = find_up::find_file(tsconfig, "lerna.json").ok_or_else(|| {
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
        // TODO: list the assumption here
        .expect("Expected project to reside in monorepo");

    debug!("package_manifest: {:?}", package_manifest);

    // Enumerate internal dependencies (exclusive)
    let mut transitive_internal_dependencies_inclusive = package_manifest
        .transitive_internal_dependency_package_names_exclusive(&package_manifests_by_package_name);
    // Make this list inclusive of the target package
    transitive_internal_dependencies_inclusive.push(&package_manifest);

    debug!(
        "transitive_internal_dependencies_inclusive: {:?}",
        transitive_internal_dependencies_inclusive
            .iter()
            .map(|manifest| manifest.contents.name.clone())
            .collect::<Vec<_>>()
    );

    let included_files: Vec<PathBuf> = transitive_internal_dependencies_inclusive
        .into_par_iter()
        .map(|manifest| {
            // This relies on the assumption that tsconfig.json is always the name of the tsconfig file
            let tsconfig = &monorepo_root
                .join(manifest.path())
                .parent()
                .unwrap()
                .join("tsconfig.json");
            list_included_files(tsconfig)
        })
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .flatten()
        .collect();

    debug!("tsconfig_includes: {:?}", included_files);
    Ok(included_files)
}
