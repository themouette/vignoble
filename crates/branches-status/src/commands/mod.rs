pub mod all;
pub mod clean;
pub mod list;
pub mod update;

use crate::config::EffectiveConfig;
use anyhow::{anyhow, Result};
use cli_core::ui::color_str;
use git_core::{
    refs::collect_branches_status,
    stages::{find_stage, resolve_waterfall_stages, ResolvedStage},
    worktree::map_worktrees_by_branch,
    BranchInfo, GitRunner, WorktreeInfo,
};
use std::collections::HashMap;

pub struct ExecutionContext {
    pub git: GitRunner,
    pub remote: String,
    pub pattern: Option<String>,
    pub squash: bool,
    pub force_yes: bool,
    pub force: bool,
    pub stages: Vec<ResolvedStage>,
    pub branches_info: HashMap<String, BranchInfo>,
    pub worktrees_by_branch: HashMap<String, WorktreeInfo>,
}

impl ExecutionContext {
    pub fn build(
        git: GitRunner,
        config: &EffectiveConfig,
        no_update: bool,
        pattern: Option<String>,
        squash_override: bool,
        force_yes: bool,
        force: bool,
    ) -> Result<Self> {
        let remote = config.remote.clone();
        let squash = squash_override || config.squash;

        if !no_update {
            git.fetch(&remote)?;
        }

        let definitions = config.to_stage_definitions();
        let stages =
            resolve_waterfall_stages(&git, &remote, pattern.as_deref(), squash, &definitions)?;
        let branches_info = collect_branches_status(&git, &remote, pattern.as_deref())?;
        let worktrees_by_branch = map_worktrees_by_branch(&git)?;

        Ok(Self {
            git,
            remote,
            pattern,
            squash,
            force_yes,
            force,
            stages,
            branches_info,
            worktrees_by_branch,
        })
    }

    pub fn require_stage(&self, requested: &str) -> Result<&ResolvedStage> {
        find_stage(&self.stages, requested).ok_or_else(|| {
            let available: Vec<String> = self
                .stages
                .iter()
                .map(|s| format!(" - {}", s.name))
                .collect();
            anyhow!(
                "{}\nAvailable stages for this repository are:\n{}",
                color_str(
                    &format!("Unable to find status for stage {}", requested),
                    "red"
                ),
                available.join("\n")
            )
        })
    }
}
