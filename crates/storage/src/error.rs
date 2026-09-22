use std::path::PathBuf;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("io error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),

    /// A chunk's payload did not hash to the name it was stored under. Either
    /// the disk corrupted it or something wrote the wrong bytes.
    #[error("chunk {hash} failed integrity check")]
    ChunkCorrupt { hash: String },

    /// The index references a chunk that is not on disk. This is the failure
    /// mode garbage collection must never cause.
    #[error("chunk {hash} referenced by the index but missing from the store")]
    ChunkMissing { hash: String },

    #[error("decryption failed for chunk {hash} (wrong key or tampered data)")]
    Decrypt { hash: String },

    #[error("unrecognised chunk format in {hash}: {detail}")]
    ChunkFormat { hash: String, detail: String },

    #[error("no such file in index: {path}")]
    NotFound { path: String },

    #[error("index is corrupt: {detail}")]
    Corrupt { detail: String },

    /// Dropping this file's bytes would destroy them. Refused rather than
    /// risked: a storage cap is a promise about disk, not about data.
    #[error("cannot drop {path}: {why}")]
    CannotEvict { path: String, why: &'static str },
}

impl Error {
    pub(crate) fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Error::Io { path: path.into(), source }
    }
}
