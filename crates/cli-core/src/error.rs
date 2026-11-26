use thiserror::Error;

pub type Result<T> = std::result::Result<T, CliError>;

#[derive(Debug, Error)]
pub enum CliError {
    #[error("Failed to load config from {path}: {error}")]
    ConfigLoad { path: String, error: String },

    #[error("Update error: {0}")]
    UpdateError(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
