use std::path::PathBuf;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("recovery phrase is not valid: {detail}")]
    BadPhrase { detail: String },

    #[error("key file at {path} is not one of ours")]
    NotAKeyFile { path: PathBuf },

    #[error("key file at {path} was written by a newer version (format {found})")]
    UnsupportedFormat { path: PathBuf, found: u8 },

    #[error("io error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}
