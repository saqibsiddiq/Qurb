use std::path::PathBuf;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("storage: {0}")]
    Storage(#[from] qurb_storage::Error),

    #[error("watcher: {0}")]
    Watcher(#[from] qurb_watcher::Error),

    #[error("no source can supply content {hash}")]
    ContentUnavailable { hash: String },

    /// A [`ContentSource`](crate::ContentSource) failed. The engine cannot know
    /// why -- it may be a network, a disk, or another process -- so the detail
    /// is carried as text.
    #[error("fetching content failed: {detail}")]
    Source { detail: String },

    /// Two paths that a case-insensitive filesystem cannot tell apart.
    ///
    /// Refused rather than written: on such a filesystem the second write
    /// destroys the first, and the next reconciliation then reports the lost
    /// one as deleted and propagates that deletion to every other device.
    #[error("{wanted} cannot be stored: {existing} already exists and differs only in case")]
    CaseCollision { wanted: String, existing: String },

    /// A shared file whose name is taken by a private one here: one somebody
    /// sent to this device, or this device's own in its vault.
    ///
    /// Refused rather than written, for the same reason as a case collision:
    /// the write would destroy the file already there. The shared one waits
    /// until the name is free.
    #[error("{path} cannot be stored: a private file here already has that name")]
    TakenPrivately { path: String },

    /// A version naming somewhere this device will not write: outside the
    /// folder, or inside qurb's own store. See [`qurb_sync::is_safe_path`].
    #[error("{path:?} is not somewhere another device may write")]
    UnsafePath { path: String },

    #[error("io error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// A failure affecting one file.
///
/// Kept separate from [`Error`] because these do not stop the engine. A file
/// that cannot be read — no permission, deleted mid-read, a device that went
/// away — must not prevent every other file from syncing. It is recorded,
/// reported, and the run continues.
#[derive(Debug)]
pub struct FileFailure {
    pub path: PathBuf,
    pub error: Error,
}
