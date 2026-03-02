use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("command execution failed: {0}")]
    Exec(#[from] std::io::Error),
    #[error("store error: {0}")]
    Store(String),
    #[error("pattern error: {0}")]
    Pattern(String),
    #[error("config error: {0}")]
    Config(String),
    #[error("learn error: {0}")]
    Learn(String),
    #[error("help lookup failed: {0}")]
    Help(String),
    #[error("init failed: {0}")]
    Init(String),
}
