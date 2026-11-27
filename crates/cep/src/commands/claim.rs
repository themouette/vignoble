use crate::{commands::ExecutionContext, names::pick_unused_cepage};
use anyhow::{anyhow, bail, Result};
use clap::Args;
use cli_core::ui::{color_str, GREEN_TICK_SYMBOL};
use git_core::{
    worktree::{
        add_worktree_detached, checkout_branch_in_worktree, checkout_detached_in_worktree,
        is_worktree_dirty, list_worktrees, lock_worktree, resolve_worktree_target, unlock_worktree,
    },
    WorktreeHead,
};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

#[derive(Args, Debug, Clone)]
pub struct ClaimArgs {
    /// Optional target worktree (by name, branch, or path); picks an unclaimed worktree when omitted
    pub target: Option<String>,

    /// Check out (or create) a branch in the claimed worktree
    #[arg(short = 'b', long = "branch")]
    pub branch: Option<String>,

    /// Base ref to branch from when creating a new branch or claiming a detached worktree
    #[arg(long = "base")]
    pub base: Option<String>,

    /// Reason recorded in the worktree lock
    #[arg(short = 'm', long = "reason")]
    pub reason: Option<String>,

    /// Create a new worktree if no unclaimed worktree is available in the pool
    #[arg(short = 'c', long = "create")]
    pub create: bool,

    /// Re-claim an already claimed worktree
    #[arg(short = 'f', long = "force")]
    pub force: bool,

    /// Print only the claimed worktree path to stdout (for scripting/agents)
    #[arg(long = "print-path")]
    pub print_path: bool,
}

pub fn execute(ctx: &ExecutionContext, args: &ClaimArgs) -> Result<()> {
    let worktrees = list_worktrees(&ctx.git)?;
    let base_ref = ctx.config.resolve_base_ref(&ctx.git, args.base.as_deref());

    let (wt_name, wt_path, was_detached): (String, PathBuf, bool) = match &args.target {
        Some(target) => {
            let wt = resolve_worktree_target(&ctx.git, &worktrees, target)
                .ok_or_else(|| anyhow!("No worktree matching '{}'", target))?;
            if wt.is_main {
                bail!("Cannot claim the main repository worktree");
            }
            if wt.locked.is_some() {
                if !args.force {
                    bail!(
                        "Worktree '{}' is already claimed. Pass '--force' to override.",
                        wt.short_name
                    );
                }
                unlock_worktree(&ctx.git, &wt.path)?;
            }
            let detached = matches!(wt.head, WorktreeHead::Detached);
            (wt.short_name, wt.path, detached)
        }
        None => {
            let candidate = worktrees
                .iter()
                .filter(|wt| !wt.is_main && wt.locked.is_none())
                .find(|wt| matches!(wt.head, WorktreeHead::Detached))
                .or_else(|| {
                    worktrees
                        .iter()
                        .find(|wt| !wt.is_main && wt.locked.is_none())
                });

            if let Some(wt) = candidate {
                let detached = matches!(wt.head, WorktreeHead::Detached);
                (wt.short_name.clone(), wt.path.clone(), detached)
            } else if args.create {
                let existing_names: HashSet<String> =
                    worktrees.iter().map(|wt| wt.short_name.clone()).collect();
                let new_name = pick_unused_cepage(&existing_names);
                let new_path = ctx
                    .config
                    .resolve_worktree_path(&ctx.main_repo_root, &new_name);
                if let Some(parent) = new_path.parent() {
                    fs::create_dir_all(parent)?;
                }
                add_worktree_detached(&ctx.git, &new_path, base_ref.as_deref())?;
                (new_name, new_path, true)
            } else {
                bail!(
                    "No unclaimed worktree available in the pool. Run 'cep create' or pass '--create'."
                );
            }
        }
    };

    let lock_reason = args
        .reason
        .as_deref()
        .or(args.branch.as_deref())
        .unwrap_or("claimed");
    lock_worktree(&ctx.git, &wt_path, Some(lock_reason))?;

    if let Some(branch) = &args.branch {
        let exists = ctx.git.branch_exists(branch, None);
        checkout_branch_in_worktree(&ctx.git, &wt_path, branch, !exists, base_ref.as_deref())?;
    } else if was_detached {
        if let Some(base) = base_ref.as_deref() {
            if !is_worktree_dirty(&ctx.git, &wt_path).unwrap_or(true) {
                let _ = checkout_detached_in_worktree(&ctx.git, &wt_path, base);
            }
        }
    }

    if args.print_path {
        println!("{}", wt_path.display());
    } else {
        println!(
            "{}Claimed worktree {} ({})",
            color_str(GREEN_TICK_SYMBOL, "green"),
            color_str(&wt_name, "cyan"),
            wt_path.display()
        );
    }

    Ok(())
}
