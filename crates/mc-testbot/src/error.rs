use mc_protocol::codec::{DecodeError, EncodeError};
use mc_protocol::net::NetError;

#[derive(Debug, thiserror::Error)]
pub enum BotError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("network: {0}")]
    Net(#[from] NetError),
    #[error("decode: {0}")]
    Decode(#[from] DecodeError),
    /// A packet the bot refuses to send (vanilla's `EncoderException` while sending).
    #[error("encode: {0}")]
    Encode(#[from] EncodeError),
    /// The server closed the connection without a reason.
    #[error("connection closed")]
    Closed,
    /// The server sent a disconnect/login-disconnect with this (plain text) reason.
    #[error("disconnected: {0}")]
    Disconnected(String),
    #[error("timeout: {0}")]
    Timeout(String),
    #[error("protocol: {0}")]
    Protocol(String),
}

pub type Result<T, E = BotError> = std::result::Result<T, E>;
