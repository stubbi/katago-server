//! Error type for everything that can go wrong talking to KataGo.

use thiserror::Error;

/// Errors produced by the analysis engine.
#[derive(Debug, Error)]
pub enum EngineError {
    /// KataGo could not be spawned (bad path, missing model, ...).
    #[error("failed to start KataGo: {0}")]
    ProcessStartFailed(String),

    /// KataGo is not running (it exited, or was never started successfully).
    #[error("KataGo process is not running")]
    ProcessDied,

    /// KataGo did not answer within the configured time.
    #[error("KataGo did not respond within {0} seconds")]
    Timeout(u64),

    /// KataGo rejected the query because of a problem in the request itself.
    #[error("KataGo rejected the query: {message}")]
    Rejected {
        /// KataGo's error message.
        message: String,
        /// The request field KataGo complained about, when it said.
        field: Option<String>,
    },

    /// KataGo reported an error that was not attributable to the request.
    #[error("KataGo returned an error: {0}")]
    Katago(String),

    /// KataGo produced output the server could not understand.
    #[error("failed to parse KataGo response: {0}")]
    Parse(String),

    /// Writing to or reading from the KataGo process failed.
    #[error("I/O error communicating with KataGo: {0}")]
    Io(#[from] std::io::Error),

    /// JSON (de)serialisation failed.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// The engine is shutting down and accepts no new work.
    #[error("engine is shutting down")]
    ShuttingDown,
}

/// Convenience alias.
pub type Result<T> = std::result::Result<T, EngineError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_are_human_readable() {
        assert_eq!(
            EngineError::Timeout(30).to_string(),
            "KataGo did not respond within 30 seconds"
        );
        assert_eq!(
            EngineError::Rejected {
                message: "Illegal move 1: D4".into(),
                field: Some("moves".into()),
            }
            .to_string(),
            "KataGo rejected the query: Illegal move 1: D4"
        );
        let io: EngineError = std::io::Error::new(std::io::ErrorKind::NotFound, "gone").into();
        assert!(io.to_string().contains("gone"));
    }
}
