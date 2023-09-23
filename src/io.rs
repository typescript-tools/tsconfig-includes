use std::{
    error::Error,
    fmt::Display,
    fs::File,
    io::{self, Read},
    path::{Path, PathBuf},
};

use serde::Deserialize;

#[derive(Debug)]
#[non_exhaustive]
pub struct FromFileError {
    path: PathBuf,
    kind: FromFileErrorKind,
}

impl Display for FromFileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.kind {
            FromFileErrorKind::Open(_) => write!(f, "unable to open file {:?}", self.path),
            FromFileErrorKind::Read(_) => write!(f, "unable to read file {:?}", self.path),
            FromFileErrorKind::Parse(_) => write!(f, "unable to parse file {:?}", self.path),
        }
    }
}

impl Error for FromFileError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match &self.kind {
            FromFileErrorKind::Open(err) => Some(err),
            FromFileErrorKind::Read(err) => Some(err),
            FromFileErrorKind::Parse(err) => Some(err),
        }
    }
}

#[derive(Debug)]
pub enum FromFileErrorKind {
    #[non_exhaustive]
    Open(io::Error),
    #[non_exhaustive]
    Read(io::Error),
    #[non_exhaustive]
    Parse(serde_json::Error),
}

pub(crate) fn read_json_from_file<P, T>(path: P) -> Result<T, FromFileError>
where
    P: AsRef<Path>,
    for<'de> T: Deserialize<'de>,
{
    fn inner<T>(path: &Path) -> Result<T, FromFileError>
    where
        for<'de> T: Deserialize<'de>,
    {
        // Reading a file into a string before invoking Serde is faster than
        // invoking Serde from a BufReader, see
        // https://github.com/serde-rs/json/issues/160
        (|| {
            let mut string = String::new();
            File::open(path)
                .map_err(FromFileErrorKind::Open)?
                .read_to_string(&mut string)
                .map_err(FromFileErrorKind::Read)?;
            let json = serde_json::from_str(&string).map_err(FromFileErrorKind::Parse)?;
            Ok(json)
        })()
        .map_err(|kind| FromFileError {
            path: path.to_owned(),
            kind,
        })
    }
    inner(path.as_ref())
}
