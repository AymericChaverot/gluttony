use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("scan failed: {0}")]
    Scan(#[from] walkdir::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("update check failed: {0}")]
    Update(#[from] Box<ureq::Error>),

    #[error("failed to parse release info: {0}")]
    Parse(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
