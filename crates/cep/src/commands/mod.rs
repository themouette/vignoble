pub mod cd;
pub mod claim;
pub mod create;
pub mod detach;
pub mod list;
pub mod release;
pub mod remove;

use crate::config::EffectiveConfig;
use anyhow::{bail, Result};
use git_core::GitRunner;
use std::path::PathBuf;

pub const SAISONNIER_ENV_VAR: &str = "VIGNOBLE_SAISONNIER";
pub const SAISONNIER_INTERNAL_ENV_VAR: &str = "VIGNOBLE_SAISONNIER_INTERNAL";

/// Prevent agent subprocesses from mutating worktree lifecycle inside a managed saisonnier session.
pub fn ensure_not_managed_saisonnier(cmd: &str) -> Result<()> {
    let managed = std::env::var(SAISONNIER_ENV_VAR).as_deref() == Ok("1");
    let internal = std::env::var(SAISONNIER_INTERNAL_ENV_VAR).as_deref() == Ok("1");
    if managed && !internal {
        bail!(
            "Cannot run 'cep {}' inside a managed saisonnier session (VIGNOBLE_SAISONNIER=1); saisonnier manages worktree lifecycle automatically.",
            cmd
        );
    }
    Ok(())
}

pub struct ExecutionContext {
    pub git: GitRunner,
    pub main_repo_root: PathBuf,
    pub config: EffectiveConfig,
}
