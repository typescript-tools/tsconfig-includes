#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    ReadLernaManifestError(#[from] anyhow::Error),

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
}
