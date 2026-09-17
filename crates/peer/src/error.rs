use std::path::PathBuf;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("malformed message from peer: {detail}")]
    Protocol { detail: String },

    #[error("peer is not one we trust")]
    UntrustedPeer,

    #[error("pairing code is not valid: {detail}")]
    BadInvite { detail: String },

    #[error("pairing code has expired")]
    InviteExpired,

    #[error("the other device refused to pair")]
    PairingRefused,

    #[error("pairing ended before a device joined")]
    PairingAbandoned,

    #[error("no STUN server answered; UDP may be blocked outbound")]
    NoStunResponse,

    #[error("signalling: {detail}")]
    Signalling { detail: String },

    #[error("the peer offered no address to try")]
    NoCandidates,

    #[error("the peer did not answer the request to connect")]
    PeerDidNotAnswer,

    #[error("no relay is configured to fall back to")]
    NoRelay,

    /// Every candidate failed. In production this is where a relay takes over.
    #[error("could not reach peer {peer} at any of its addresses")]
    Unreachable { peer: String },

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
