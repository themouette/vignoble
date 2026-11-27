pub mod all;
pub mod clean;
pub mod list;

use crate::config::EffectiveConfig;
use anyhow::{anyhow, Result};
use cli_core::ui::color_str;
use git_core::{
    refs::collect_branches_status,
    split_remote_branch,
    stages::{find_stage, resolve_waterfall_stages, ResolvedStage},
    worktree::map_worktrees_by_branch,
    BranchInfo, GitRunner, StageMode, WorktreeInfo,
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
        let definitions = config.to_stage_definitions();

        if !no_update {
            git.fetch(&remote)?;
            let known_remotes = git.remotes().unwrap_or_default();
            let mut secondary_fetches: Vec<(String, Vec<String>)> = Vec::new();
            for def in &definitions {
                if let StageMode::Branch(branch_spec) = &def.mode {
                    if let Some((stage_remote, short_branch)) =
                        split_remote_branch(branch_spec, &known_remotes)
                    {
                        if stage_remote != remote {
                            if let Some((_, branches)) = secondary_fetches
                                .iter_mut()
                                .find(|(r, _)| r == stage_remote)
                            {
                                if !branches.iter().any(|b| b == short_branch) {
                                    branches.push(short_branch.to_string());
                                }
                            } else {
                                secondary_fetches.push((
                                    stage_remote.to_string(),
                                    vec![short_branch.to_string()],
                                ));
                            }
                        }
                    }
                }
            }
            for (stage_remote, branches) in &secondary_fetches {
                git.fetch_branches(stage_remote, branches)?;
            }
        }

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
