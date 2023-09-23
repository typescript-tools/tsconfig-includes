use std::{
    error::Error,
    fmt::Display,
    path::{self, Path, PathBuf},
};

#[derive(Debug)]
#[non_exhaustive]
pub struct StripPrefixError {
    absolute_path: PathBuf,
    kind: StripPrefixErrorKind,
}

impl Display for StripPrefixError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.kind {
            StripPrefixErrorKind::Strip { ancestor, inner: _ } => write!(
                f,
                "cannot strip prefix {:?} from path {:?}",
                ancestor, self.absolute_path
            ),
            StripPrefixErrorKind::PrefixNotFound { prefix } => write!(
                f,
                "never encountered prefix {:?} in path {:?}",
                prefix, self.absolute_path
            ),
        }
    }
}

impl Error for StripPrefixError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match &self.kind {
            StripPrefixErrorKind::Strip { ancestor: _, inner } => Some(inner),
            StripPrefixErrorKind::PrefixNotFound { prefix: _ } => None,
        }
    }
}

#[derive(Debug)]
pub enum StripPrefixErrorKind {
    #[non_exhaustive]
    Strip {
        ancestor: PathBuf,
        inner: path::StripPrefixError,
    },
    #[non_exhaustive]
    PrefixNotFound { prefix: PathBuf },
}

// DISCUSS: can this function name be improved?
pub(crate) fn remove_relative_path_prefix_from_absolute_path(
    prefix: &Path,
    absolute_path: &Path,
) -> Result<PathBuf, StripPrefixError> {
    (|| {
        for ancestor in absolute_path.ancestors() {
            if ancestor.ends_with(prefix) {
                let relative_path = absolute_path
                    .strip_prefix(ancestor)
                    .map(ToOwned::to_owned)
                    .map_err(|inner| StripPrefixErrorKind::Strip {
                        ancestor: ancestor.to_owned(),
                        inner,
                    })?;
                return Ok(relative_path);
            }
        }

        return Err(StripPrefixErrorKind::PrefixNotFound {
            prefix: prefix.to_owned(),
        })?;
    })()
    .map_err(|kind| StripPrefixError {
        absolute_path: absolute_path.to_owned(),
        kind,
    })
}

pub(crate) fn is_glob(string: &str) -> bool {
    string.contains('*')
}

pub(crate) fn glob_file_extension(glob: &str) -> Option<String> {
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

pub(crate) fn is_monorepo_file(monorepo_root: &Path, file: &Path) -> bool {
    for ancestor in file.ancestors() {
        if ancestor.ends_with(monorepo_root) {
            return true;
        }
    }
    false
}
