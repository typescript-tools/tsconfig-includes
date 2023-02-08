use std::path::PathBuf;

use anyhow::Result;

extern crate tsconfig_includes;

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let included_files = tsconfig_includes::tsconfig_includes_by_package_name(
        PathBuf::from("/path-to-your-monorepo").as_path(),
        &[PathBuf::from("/path-to-your-monorepo/packages/package-a/tsconfig.json").as_path()],
        tsconfig_includes::Calculation::Exact,
    )?;
    println!("{:#?}", included_files);
    Ok(())
}
