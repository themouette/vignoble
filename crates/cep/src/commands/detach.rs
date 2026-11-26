use crate::commands::ExecutionContext;
use anyhow::{anyhow, Result};
use clap::Args;
use cli_core::ui::{color_str, GREEN_TICK_SYMBOL};
use git_core::worktree::{detach_worktree_head, list_worktrees, resolve_worktree_target};

#[derive(Args, Debug, Clone)]
pub struct DetachArgs {
    /// Target worktree name, branch, or path (defaults to current worktree)
    pub target: Option<String>,
}

pub fn execute(ctx: &ExecutionContext, args: &DetachArgs) -> Result<()> {
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

    detach_worktree_head(&ctx.git, &wt.path)?;

    println!(
        "{}Detached HEAD in worktree {}",
        color_str(GREEN_TICK_SYMBOL, "green"),
        color_str(&wt.short_name, "cyan")
    );

    Ok(())
}
