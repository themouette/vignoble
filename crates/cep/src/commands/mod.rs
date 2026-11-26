pub mod cd;
pub mod claim;
pub mod create;
pub mod detach;
pub mod list;
pub mod release;
pub mod remove;

use crate::config::EffectiveConfig;
use git_core::GitRunner;
use std::path::PathBuf;

pub struct ExecutionContext {
    pub git: GitRunner,
    pub main_repo_root: PathBuf,
    pub config: EffectiveConfig,
}
