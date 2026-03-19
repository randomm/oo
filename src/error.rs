use thiserror::Error;

/// Unified error type for the `oo` library.
///
/// Categorizes errors by their origin: command execution, storage,
/// pattern parsing, configuration, learning, and more.
#[derive(Debug, Error)]
pub enum Error {
    /// Command execution failed (I/O error).
    #[error("command execution failed: {0}")]
    Exec(#[from] std::io::Error),

    /// Storage operation failed.
    #[error("store error: {0}")]
    Store(String),

    /// Pattern parsing or matching error.
    #[error("pattern error: {0}")]
    Pattern(String),

    /// Configuration file or environment error.
    #[error("config error: {0}")]
    Config(String),

    /// Learning/initiated error (LLM integration error).
    #[error("learn error: {0}")]
    Learn(String),

    /// Help generation or lookup error.
    #[error("help lookup failed: {0}")]
    Help(String),

    /// Initialization error (e.g., hook creation failed).
    #[error("init failed: {0}")]
    Init(String),
}
