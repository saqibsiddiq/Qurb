pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("websocket error: {0}")]
    WebSocket(#[from] tokio_tungstenite::tungstenite::Error),

    #[error("malformed message: {0}")]
    Malformed(#[from] serde_json::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// A plain `ws://` URL to somewhere other than this machine.
    ///
    /// Rendezvous identifiers are bearer secrets: anyone who sees one can
    /// enumerate that group's addresses. Sending them unencrypted across a
    /// network would hand them to everyone on the path.
    #[error("{url} is not encrypted; use wss:// or allow_insecure() for local testing")]
    InsecureUrl { url: String },

    #[error("the signalling server closed the connection")]
    Closed,

    #[error("server said: {detail}")]
    Server { detail: String },
}
