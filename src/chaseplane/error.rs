#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("websocket error: {0}")]
    WebSocket(#[from] tokio_tungstenite::tungstenite::Error),
    #[error("failed to parse JSON message: {0}")]
    Json(#[from] serde_json::Error),
    #[error("no traffic found with callsign {0:?}")]
    UnknownCallsign(String),
    #[error("timed out waiting for a reply to {0}")]
    Timeout(&'static str),
    #[error("background reader task ended before replying")]
    ReaderTaskEnded,
}

pub type Result<T> = std::result::Result<T, Error>;
