use std::path::{PathBuf, StripPrefixError};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    ReadLernaManifestError(#[from] anyhow::Error),

    #[error("Error parsing {filename:?}")]
    TypescriptConfigParseError {
        #[source]
        source: serde_json::Error,
        filename: PathBuf,
    },

    #[error("Error enumerating files by tsconfig glob")]
    GlobWalkError {
        #[from]
        source: globwalk::WalkError,
    },

    #[error("Error calculating relating path from monorepo root for {filename:?}")]
    RelativePathError { filename: PathBuf },

    #[error("Project is not in a lerna monorepo: {filename:?}")]
    TypescriptProjectNotInMonorepo { filename: String },

    #[error("Error invoking the TypeScript compiler: {source:?}")]
    TypescriptCompilerInvocationError {
        #[from]
        source: std::io::Error,
    },

    #[error("Error pasing response from TypeScript compiler: {source:?}")]
    TypescriptCompilerResponseParseError {
        #[from]
        source: std::string::FromUtf8Error,
    },

    #[error("Error resolving absolute path to relative path {absolute_path:?}")]
    RelativePathStripError {
        #[source]
        source: StripPrefixError,
        absolute_path: PathBuf,
    },
}
