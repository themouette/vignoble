use crate::commands::ExecutionContext;
use anyhow::{anyhow, bail, Result};
use clap::Args;
use cli_core::ui::{color_str, GREEN_TICK_SYMBOL};
use git_core::worktree::{
    detach_worktree_head, is_worktree_dirty, list_worktrees, resolve_worktree_target,
    unlock_worktree,
};

#[derive(Args, Debug, Clone)]
pub struct ReleaseArgs {
    /// Target worktree name, branch, or path (defaults to current worktree)
    pub target: Option<String>,

    /// Keep the currently checked-out branch instead of detaching HEAD
    #[arg(long = "keep-branch")]
    pub keep_branch: bool,

    /// Release even if the worktree has uncommitted changes
    #[arg(short = 'f', long = "force")]
    pub force: bool,
}

pub fn execute(ctx: &ExecutionContext, args: &ReleaseArgs) -> Result<()> {
    let worktrees = list_worktrees(&ctx.git)?;

    let wt = match &args.target {
        Some(target) => resolve_worktree_target(&ctx.git, &worktrees, target)
            .ok_or_else(|| anyhow!("No worktree matching '{}'", target))?,
        None => worktrees
            .iter()
            .find(|wt| wt.is_current)
            .cloned()
            .ok_or_else(|| anyhow!("Unable to determine current worktree"))?,
    };

    if wt.is_main {
        bail!("Cannot release the main repository worktree");
    }

    if !args.force && is_worktree_dirty(&ctx.git, &wt.path)? {
        bail!(
            "Worktree '{}' has uncommitted changes. Commit/stash them or pass '--force'.",
            wt.short_name
        );
    }

    if !args.keep_branch {
        detach_worktree_head(&ctx.git, &wt.path)?;
    }

    if wt.locked.is_some() {
        unlock_worktree(&ctx.git, &wt.path)?;
    }

    println!(
        "{}Released worktree {}",
        color_str(GREEN_TICK_SYMBOL, "green"),
        color_str(&wt.short_name, "cyan")
    );

    Ok(())
}
