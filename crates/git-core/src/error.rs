use thiserror::Error;

pub type Result<T> = std::result::Result<T, GitError>;

#[derive(Debug, Error)]
pub enum GitError {
    #[error("You are not in a git directory")]
    NotAGitRepository,

    #[error("Unexpected error while calling \"git {command}\": {stderr}")]
    CommandFailed { command: String, stderr: String },

    #[error("Stage error for '{stage}': {message}")]
    StageError { stage: String, message: String },

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
