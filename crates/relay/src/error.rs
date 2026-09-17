pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("malformed frame: {0}")]
    Malformed(&'static str),

    #[error("frame of {size} bytes exceeds the {max} byte limit")]
    FrameTooLarge { size: usize, max: usize },

    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("the relay closed the connection")]
    Closed,

    #[error("cannot address more than {max} relayed peers at once")]
    TooManyPeers { max: usize },
}
