use thiserror::Error;

pub type Result<T> = std::result::Result<T, TomeError>;

#[derive(Debug, Error)]
pub enum TomeError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("integrity check failed: {0}")]
    Integrity(String),

    #[error("api error: {0}")]
    Api(String),

    #[error("storage error: {0}")]
    Storage(String),

    #[error("dump error: {0}")]
    Dump(String),

    #[error("config error: {0}")]
    Config(String),

    #[error("circuit breaker open: try again later")]
    CircuitBreakerOpen,

    #[error("kill switch active: outbound traffic disabled")]
    KillSwitch,

    #[error("{0}")]
    Other(String),
}
