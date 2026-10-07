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

    #[error("the other device did not give its key: the code may already have been used, or that device may need updating")]
    NoKeyGiven,

    #[error("setting this device up from the key failed: {detail}")]
    SetUpFailed { detail: String },

    #[error("the other device said no")]
    Declined,

    #[error("these two devices have different keys: set this one up again by joining the other with its code")]
    DifferentKey,

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
    ///
    /// `tried` says how each address failed -- "timed out", or what the
    /// handshake said -- because "could not reach" alone cannot tell a path a
    /// network would not open from a device that answered and refused.
    #[error("could not reach peer {peer}: {tried}")]
    Unreachable { peer: String, tried: String },

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

    /// Asked for content, the peer said it does not hold it -- before any
    /// bytes moved. An answer rather than a failure: the content was freed
    /// there, is kept for it elsewhere, or is gone.
    #[error("the other device does not have content {hash}")]
    NotHeld { hash: String },

    /// Bytes arrived, but not the bytes that were asked for.
    #[error("content {hash} did not match what the peer sent")]
    ContentMismatch { hash: String },
}
