use std::path::PathBuf;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("failed to start watching {path}: {source}")]
    Start {
        path: PathBuf,
        #[source]
        source: notify::Error,
    },

    #[error("error while scanning {path}: {detail}")]
    Scan { path: PathBuf, detail: String },
}
