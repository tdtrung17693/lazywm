use x11rb::errors::{ConnectError, ConnectionError, ReplyError};

#[derive(Debug, thiserror::Error)]

pub enum Error {
    #[error("Unknown error: {0}")]
    Unknown(String),

    #[error(transparent)]
    X11rbConnect(#[from] ConnectError),

    #[error(transparent)]
    X11rbConnection(#[from] ConnectionError),

    #[error(transparent)]
    X11rbReplyError(#[from] ReplyError),
}

pub type Result<T> = std::result::Result<T, Error>;
