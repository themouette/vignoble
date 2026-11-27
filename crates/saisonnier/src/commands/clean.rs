use crate::{
    commands::{
        reconcile_running_receipts,
        tick::{self, TickArgs},
        ExecutionContext,
    },
    parcelle,
    store::{RunReceipt, RunStatus},
};
use anyhow::{anyhow, bail, Result};
use clap::Args;
use cli_core::ui::{color_str, GREEN_TICK_SYMBOL};
use git_core::worktree::{
    clear_agent_marker, detach_worktree_head, is_worktree_dirty, list_worktrees, unlock_worktree,
};

#[derive(Args, Debug, Clone, Default)]
pub struct CleanArgs {
    /// Specific finished run IDs (or prefixes) to remove
    pub ids: Vec<String>,

    /// Remove all completed and no_changes runs (default when no IDs are specified)
    #[arg(long = "completed")]
    pub completed: bool,

    /// Remove all finished runs (including failed, dirty, crashed, and cancelled)
    #[arg(short = 'a', long = "all")]
    pub all: bool,

    /// Force release of a held worktree even if it has uncommitted changes
    #[arg(short = 'f', long = "force")]
    pub force: bool,

    /// Target remote parcelle configured under [parcelle.<name>]
    #[arg(long = "parcelle")]
    pub parcelle: Option<String>,
}

pub fn execute(ctx: &ExecutionContext, args: &CleanArgs) -> Result<()> {
    if let Some(ref parcelle_name) = args.parcelle {
        let mut forwarded = vec!["clean".to_string()];
        if args.completed {
            forwarded.push("--completed".to_string());
        }
        if args.all {
            forwarded.push("--all".to_string());
        }
        if args.force {
            forwarded.push("--force".to_string());
        }
        forwarded.extend(args.ids.iter().cloned());
        return parcelle::relay_command(&ctx.config, parcelle_name, &forwarded, None, false);
    }

    {
        let _lock = ctx.store.acquire_tick_lock()?;
        reconcile_running_receipts(ctx)?;
    }

    let targets = resolve_targets(ctx, args)?;
    if targets.is_empty() {
        println!("No matching finished runs to clean.");
        return Ok(());
    }

    let mut released_any_cep = false;
    for receipt in targets {
        if try_release_owned_cep(ctx, &receipt, args.force)? {
            released_any_cep = true;
        }
        ctx.store.remove_receipt(&receipt.run_id)?;
        println!(
            "{}Removed run {} ({})",
            color_str(GREEN_TICK_SYMBOL, "green"),
            color_str(&receipt.run_id, "cyan"),
            receipt.status.as_str()
        );
    }

    if released_any_cep {
        let _ = tick::execute(ctx, &TickArgs::default());
    }

    Ok(())
}

fn resolve_targets(ctx: &ExecutionContext, args: &CleanArgs) -> Result<Vec<RunReceipt>> {
    if !args.ids.is_empty() {
        let mut selected = Vec::new();
        for id in &args.ids {
            let receipt = ctx
                .store
                .find_receipt(id)?
                .ok_or_else(|| anyhow!("No run receipt found matching '{}'", id))?;
            if receipt.status == RunStatus::Running {
                bail!(
                    "Run '{}' is still running. Stop it first with 'saisonnier cancel {}'.",
                    receipt.run_id,
                    receipt.run_id
                );
            }
            if !selected
                .iter()
                .any(|r: &RunReceipt| r.run_id == receipt.run_id)
            {
                selected.push(receipt);
            }
        }
        return Ok(selected);
    }

    let all_receipts = ctx.store.list_receipts()?;
    let filtered = all_receipts
        .into_iter()
        .filter(|r| {
            if r.status == RunStatus::Running {
                return false;
            }
            if args.all {
                true
            } else {
                matches!(r.status, RunStatus::Completed | RunStatus::NoChanges)
            }
        })
        .collect();
    Ok(filtered)
}

fn try_release_owned_cep(
    ctx: &ExecutionContext,
    receipt: &RunReceipt,
    force: bool,
) -> Result<bool> {
    let worktrees = list_worktrees(&ctx.git)?;
    let Some(wt) = worktrees.iter().find(|w| w.short_name == receipt.cep) else {
        return Ok(false);
    };

    let owned_prefix = format!("saisonnier:{}", receipt.run_id);
    let resume_prefix = format!("saisonnier:resume:{}", receipt.run_id);
    let is_owned_by_run = wt
        .locked
        .as_deref()
        .map(|reason| reason.starts_with(&owned_prefix) || reason.starts_with(&resume_prefix))
        .unwrap_or(false);

    if !is_owned_by_run {
        return Ok(false);
    }

    if !force && is_worktree_dirty(&ctx.git, &wt.path)? {
        bail!(
            "Worktree '{}' held by run '{}' has uncommitted changes. Commit/stash them or pass '--force'.",
            wt.short_name,
            receipt.run_id
        );
    }

    detach_worktree_head(&ctx.git, &wt.path)?;
    if wt.locked.is_some() {
        unlock_worktree(&ctx.git, &wt.path)?;
    }
    let _ = clear_agent_marker(&ctx.git, &wt.short_name);

    println!(
        "{}Released worktree {}",
        color_str(GREEN_TICK_SYMBOL, "green"),
        color_str(&wt.short_name, "cyan")
    );
    Ok(true)
}
