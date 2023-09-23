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
#![deny(missing_debug_implementations)]

pub mod estimate;
pub mod exact;
pub mod io;
pub mod path;
pub mod typescript_package;
