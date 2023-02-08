# tsconfig-includes

[![Build Status]](https://github.com/typescript-tools/tsconfig-includes/actions/workflows/release.yml)

[build status]: https://github.com/typescript-tools/tsconfig-includes/actions/workflows/release.yml/badge.svg?event=push

**tsconfig-includes** enumerates files used in the TypeScript compilation
process for monorepo packages. While `tsc --listFilesOnly` only lists input
files in the target package, **tsconfig-includes** lists input files in the
target package and all of its internal dependencies. You can use this list to
determine when inputs to a package has changed, to decide whether to rebuild
the package or used a cached version.

This library requires Rust nightly.
