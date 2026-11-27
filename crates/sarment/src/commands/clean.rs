use crate::{commands::ExecutionContext, printer::pretty_print_block};
use anyhow::Result;
use cli_core::ui::{color_str, prompt_yes_no, GREEN_TICK_SYMBOL, RED_CROSS_SYMBOL};
use git_core::{split_remote_branch, StageBranchEntry, SyncStatus};
use std::collections::HashSet;

pub fn execute(ctx: &ExecutionContext, target_stage: &str) -> Result<()> {
    let stage = ctx.require_stage(target_stage)?;
    let known_remotes = ctx.git.remotes().unwrap_or_default();

    let mut reference_branches: HashSet<String> = HashSet::new();
    for reference in ctx.stages.iter().filter_map(|s| s.reference.as_deref()) {
        reference_branches.insert(reference.to_string());
        if let Some((_, short_branch)) = split_remote_branch(reference, &known_remotes) {
            reference_branches.insert(short_branch.to_string());
        }
    }

    let current_branch = ctx.git.current_branch().unwrap_or(None);

    let mut deletable_entries: Vec<StageBranchEntry> = Vec::new();
    for entry in &stage.branches {
        if reference_branches.contains(entry.branch.as_str()) {
            continue;
        }

        if current_branch.as_deref() == Some(entry.branch.as_str()) {
            let bold_branch = color_str(&entry.branch, "bold");
            println!(
                "{}",
                color_str(
                    &format!("Unable to delete current branch {}", bold_branch),
                    "red"
                )
            );
            continue;
        }

        if let Some(wt) = ctx.worktrees_by_branch.get(&entry.branch) {
            let bold_branch = color_str(&entry.branch, "bold");
            println!(
                "{}",
                color_str(
                    &format!(
                        "Unable to delete branch {} (checked out in worktree {})",
                        bold_branch,
                        wt.path.display()
                    ),
                    "orange"
                )
            );
            continue;
        }

        deletable_entries.push(entry.clone());
    }

    if deletable_entries.is_empty() {
        println!("Nothing to do.");
        return Ok(());
    }

    pretty_print_block(
        &color_str("The following branches will be deleted:", "red"),
        &deletable_entries,
        &stage.color,
        &ctx.branches_info,
        &ctx.worktrees_by_branch,
    );

    let should_prompt = !ctx.force_yes;
    if should_prompt && !prompt_yes_no("Continue?")? {
        std::process::exit(1);
    }

    for entry in &deletable_entries {
        delete_single_branch(ctx, &entry.branch)?;
    }

    Ok(())
}

fn delete_single_branch(ctx: &ExecutionContext, branch: &str) -> Result<()> {
    let bold_branch = color_str(branch, "bold");
    let green_tick = color_str(GREEN_TICK_SYMBOL, "green");
    let red_cross = color_str(RED_CROSS_SYMBOL, "red");

    match ctx.git.delete_local_branch(branch, false) {
        Ok(()) => {
            println!("  {}{} has been deleted", green_tick, bold_branch);
        }
        Err(err) => {
            println!("  {}{}", color_str("Error: ", "orange"), err);
            let confirm_force = ctx.force
                || prompt_yes_no(&color_str(
                    &format!("  Do you want to force delete {}?", branch),
                    "red",
                ))?;
            if confirm_force {
                ctx.git.delete_local_branch(branch, true)?;
                println!("  {}{} has been force-deleted", green_tick, bold_branch);
            } else {
                println!("  {}{} has not been deleted", red_cross, bold_branch);
            }
        }
    }

    // Check remote deletion when branch has a remote tracking branch and clean is interactive or forced
    if let Some(info) = ctx.branches_info.get(branch) {
        if matches!(info.status, SyncStatus::NoRemote | SyncStatus::RemoteBranch) {
            return Ok(());
        }

        // Only delete remote when user confirms or when cleaning non-prod or with explicit confirmation
        if !ctx.force_yes {
            if !prompt_yes_no(&format!(
                "Do you want to delete remote branch {} {}",
                ctx.remote, bold_branch
            ))? {
                return Ok(());
            }

            if !matches!(info.status, SyncStatus::UpToDate | SyncStatus::NeedPush) {
                println!(
                    "{}",
                    color_str(&format!("Branch has a status {:?}, ", info.status), "red")
                );
                if !ctx.force && !prompt_yes_no("Continue?")? {
                    return Ok(());
                }
            }

            println!(
                "{}",
                color_str(
                    &format!("Deleting branch {} {}...", ctx.remote, bold_branch),
                    "red"
                )
            );
            ctx.git.delete_remote_branch(&ctx.remote, branch)?;
        }
    }

    Ok(())
}
