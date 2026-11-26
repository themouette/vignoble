use crate::{commands::ExecutionContext, names::pick_unused_cepage};
use anyhow::{bail, Result};
use clap::Args;
use cli_core::ui::{color_str, GREEN_TICK_SYMBOL};
use git_core::worktree::{
    add_worktree_branch, add_worktree_detached, list_worktrees, lock_worktree,
};
use std::collections::HashSet;
use std::fs;

#[derive(Args, Debug, Clone)]
pub struct CreateArgs {
    /// Optional worktree short name (defaults to an unused French grape variety)
    pub name: Option<String>,

    /// Check out (or create) a branch in the new worktree instead of detached HEAD
    #[arg(short = 'b', long = "branch")]
    pub branch: Option<String>,

    /// Base ref to create the worktree from (overrides config `base_ref`)
    #[arg(long = "base")]
    pub base: Option<String>,

    /// Immediately claim (lock) the created worktree
    #[arg(long = "claim")]
    pub claim: bool,

    /// Lock reason when claiming the worktree (implies --claim)
    #[arg(short = 'm', long = "reason")]
    pub reason: Option<String>,
}

pub fn execute(ctx: &ExecutionContext, args: &CreateArgs) -> Result<()> {
    let worktrees = list_worktrees(&ctx.git)?;
    let existing_names: HashSet<String> = worktrees.into_iter().map(|wt| wt.short_name).collect();

    let wt_name = match &args.name {
        Some(explicit) => {
            if existing_names.contains(explicit) {
                bail!("Worktree '{}' already exists", explicit);
            }
            explicit.clone()
        }
        None => pick_unused_cepage(&existing_names),
    };

    let target_path = ctx
        .config
        .resolve_worktree_path(&ctx.main_repo_root, &wt_name);
    if let Some(parent) = target_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let base_ref = args.base.as_deref().or(ctx.config.base_ref.as_deref());

    if let Some(branch) = &args.branch {
        let branch_exists = ctx.git.branch_exists(branch, None);
        add_worktree_branch(&ctx.git, &target_path, branch, !branch_exists, base_ref)?;
    } else {
        add_worktree_detached(&ctx.git, &target_path, base_ref)?;
    }

    let should_claim = args.claim || args.reason.is_some();
    if should_claim {
        let lock_reason = args
            .reason
            .as_deref()
            .or(args.branch.as_deref())
            .unwrap_or("claimed");
        lock_worktree(&ctx.git, &target_path, Some(lock_reason))?;
    }

    println!(
        "{}Created worktree {} at {}",
        color_str(GREEN_TICK_SYMBOL, "green"),
        color_str(&wt_name, "cyan"),
        target_path.display()
    );

    Ok(())
}
