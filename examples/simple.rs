use std::io::{self, Write};
use std::path::PathBuf;

use clap::{Parser, ValueEnum};
use tsconfig_includes::{estimate, exact};

#[derive(Clone, Debug, ValueEnum)]
enum EnumerationMethod {
    Estimate,
    Exact,
}

#[derive(Debug, Parser)]
struct Cli {
    /// Which enumeration method to use
    #[arg(long, value_enum)]
    pub enumeration_method: EnumerationMethod,

    /// Path to monorepo root directory
    #[arg(long)]
    pub monorepo_root: PathBuf,

    /// List of tsconfig files to enumerate dependencies of
    #[arg()]
    pub tsconfig_files: Vec<PathBuf>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.enumeration_method {
        EnumerationMethod::Estimate => {
            let result =
                estimate::tsconfig_includes_by_package_name(cli.monorepo_root, cli.tsconfig_files)?;
            writeln!(io::stdout(), "{}", serde_json::to_string_pretty(&result)?)?;
        }
        EnumerationMethod::Exact => {
            let result =
                exact::tsconfig_includes_by_package_name(cli.monorepo_root, cli.tsconfig_files)?;
            writeln!(io::stdout(), "{}", serde_json::to_string_pretty(&result)?)?;
        }
    };

    Ok(())
}
