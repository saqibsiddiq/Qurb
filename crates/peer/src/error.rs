use std::path::PathBuf;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("malformed message from peer: {detail}")]
    Protocol { detail: String },

    #[error("peer is not one we trust")]
    UntrustedPeer,

    #[error("connection failed: {0}")]
    Connect(#[from] quinn::ConnectError),

    #[error("connection lost: {0}")]
    Connection(#[from] quinn::ConnectionError),

    #[error("stream write failed: {0}")]
    Write(#[from] quinn::WriteError),

    #[error("stream was already closed: {0}")]
    ClosedStream(#[from] quinn::ClosedStream),

    #[error("stream read failed: {0}")]
    Read(#[from] quinn::ReadError),

    #[error("reading the response failed: {0}")]
    ReadToEnd(#[from] quinn::ReadToEndError),

    #[error("tls setup failed: {0}")]
    Tls(String),

    #[error("io error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("storage: {0}")]
    Storage(#[from] qurb_storage::Error),

    #[error("peer does not have content {hash}")]
    ContentUnavailable { hash: String },

    /// Bytes arrived, but not the bytes that were asked for.
    #[error("content {hash} did not match what the peer sent")]
    ContentMismatch { hash: String },
}
