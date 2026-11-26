use crate::commands::ExecutionContext;
use anyhow::{anyhow, bail, Result};
use clap::Args;
use cli_core::ui::{color_str, GREEN_TICK_SYMBOL};
use git_core::worktree::{
    list_worktrees, remove_worktree, resolve_worktree_target, unlock_worktree,
};

#[derive(Args, Debug, Clone)]
pub struct RemoveArgs {
    /// Target worktree name, branch, or path
    pub target: String,

    /// Force removal of a claimed or dirty worktree
    #[arg(short = 'f', long = "force")]
    pub force: bool,
}

pub fn execute(ctx: &ExecutionContext, args: &RemoveArgs) -> Result<()> {
    let worktrees = list_worktrees(&ctx.git)?;

    let wt = resolve_worktree_target(&ctx.git, &worktrees, &args.target)
        .ok_or_else(|| anyhow!("No worktree matching '{}'", args.target))?;

    if wt.is_main {
        bail!("Cannot remove the main repository worktree");
    }

    if wt.is_current {
        bail!(
            "Cannot remove the currently active worktree '{}'",
            wt.short_name
        );
    }

    if wt.locked.is_some() {
        if !args.force {
            bail!(
                "Worktree '{}' is claimed. Run 'cep release {}' first or pass '--force'.",
                wt.short_name,
                wt.short_name
            );
        }
        unlock_worktree(&ctx.git, &wt.path)?;
    }

    remove_worktree(&ctx.git, &wt.path, args.force)?;

    println!(
        "{}Removed worktree {}",
        color_str(GREEN_TICK_SYMBOL, "green"),
        color_str(&wt.short_name, "cyan")
    );

    Ok(())
}
