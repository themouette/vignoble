use crate::{
    commands::ExecutionContext,
    parcelle,
    store::{now_iso8601_utc, RunStatus},
    tmux,
};
use anyhow::{bail, Result};
use clap::Args;
use cli_core::ui::{color_str, GREEN_TICK_SYMBOL};
use git_core::worktree::{
    clear_agent_marker, detach_worktree_head, is_worktree_dirty, list_worktrees, set_agent_marker,
    unlock_worktree,
};

#[derive(Args, Debug, Clone)]
pub struct CancelArgs {
    /// Queued task ID or running saisonnier run ID to cancel
    pub id: String,

    /// Release the cep back to the pool if cancelling a clean running saisonnier
    #[arg(long = "release")]
    pub release: bool,

    /// Target remote parcelle configured under [parcelle.<name>]
    #[arg(long = "parcelle")]
    pub parcelle: Option<String>,
}

pub fn execute(ctx: &ExecutionContext, args: &CancelArgs) -> Result<()> {
    if let Some(ref parcelle_name) = args.parcelle {
        let mut forwarded = vec!["cancel".to_string(), args.id.clone()];
        if args.release {
            forwarded.push("--release".to_string());
        }
        return parcelle::relay_command(&ctx.config, parcelle_name, &forwarded, None, false);
    }

    if let Some(task) = ctx.store.find_task(&args.id)? {
        ctx.store.remove_task(&task.task_id)?;
        println!(
            "{}Removed queued task {}",
            color_str(GREEN_TICK_SYMBOL, "green"),
            color_str(&task.task_id, "cyan")
        );
        return Ok(());
    }

    if let Some(mut receipt) = ctx.store.find_receipt(&args.id)? {
        if receipt.status != RunStatus::Running {
            bail!(
                "Run '{}' is not running (status: {})",
                receipt.run_id,
                receipt.status.as_str()
            );
        }

        if !receipt.tmux_session.is_empty() && tmux::is_session_alive(&receipt.tmux_session) {
            let _ = tmux::kill_session(&receipt.tmux_session);
        }

        receipt.status = RunStatus::Cancelled;
        receipt.finished_at = Some(now_iso8601_utc());
        receipt.summary = Some("cancelled by user".to_string());
        ctx.store.write_receipt(&receipt)?;

        let mut released = false;
        if let Ok(worktrees) = list_worktrees(&ctx.git) {
            if let Some(wt) = worktrees.iter().find(|w| w.short_name == receipt.cep) {
                let dirty = is_worktree_dirty(&ctx.git, &wt.path).unwrap_or(true);
                if args.release && !dirty {
                    detach_worktree_head(&ctx.git, &wt.path)?;
                    if wt.locked.is_some() {
                        unlock_worktree(&ctx.git, &wt.path)?;
                    }
                    clear_agent_marker(&ctx.git, &receipt.cep)?;
                    released = true;
                } else {
                    set_agent_marker(&ctx.git, &receipt.cep, "⚠️")?;
                }
            }
        }

        if released {
            println!(
                "{}Cancelled run {} and released worktree {}",
                color_str(GREEN_TICK_SYMBOL, "green"),
                color_str(&receipt.run_id, "cyan"),
                color_str(&receipt.cep, "green")
            );
        } else {
            println!(
                "{}Cancelled run {} (worktree {} preserved with ⚠️)",
                color_str(GREEN_TICK_SYMBOL, "green"),
                color_str(&receipt.run_id, "cyan"),
                color_str(&receipt.cep, "yellow")
            );
        }
        return Ok(());
    }

    bail!(
        "No queued task or running saisonnier found matching '{}'",
        args.id
    )
}
