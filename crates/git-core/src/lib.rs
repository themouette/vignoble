pub mod error;
pub mod refs;
pub mod runner;
pub mod squash;
pub mod stages;
pub mod worktree;

pub use error::{GitError, Result};
pub use refs::{BranchInfo, SyncStatus};
pub use runner::{split_remote_branch, GitRunner};
pub use stages::{ResolvedStage, StageBranchEntry, StageDefinition, StageMode};
pub use worktree::{WorktreeHead, WorktreeInfo};
