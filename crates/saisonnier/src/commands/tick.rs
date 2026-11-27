use crate::{
    commands::{exec, reconcile_running_receipts, ExecutionContext},
    parcelle,
    store::{
        generate_sortable_id, generate_uuid_v4, now_iso8601_utc, QueuedTask, RunReceipt, RunStatus,
        SCHEMA_VERSION,
    },
    template::{current_user, render_branch_template},
    tmux,
};
use anyhow::{bail, Result};
use clap::Args;
use git_core::{
    worktree::{
        checkout_branch_in_worktree, list_worktrees, lock_worktree, set_agent_marker,
        unlock_worktree, worktree_head_sha,
    },
    WorktreeHead, WorktreeInfo,
};
use std::env;

#[derive(Args, Debug, Clone, Default)]
pub struct TickArgs {
    /// Target remote parcelle configured under [parcelle.<name>]
    #[arg(long = "parcelle")]
    pub parcelle: Option<String>,

    /// Execute claimed tasks inline instead of spawning detached tmux sessions
    #[arg(long = "inline")]
    pub inline: bool,
}

pub fn execute(ctx: &ExecutionContext, args: &TickArgs) -> Result<()> {
    if let Some(ref parcelle_name) = args.parcelle {
        let mut forwarded = vec!["tick".to_string()];
        if args.inline {
            forwarded.push("--inline".to_string());
        }
        return parcelle::relay_command(&ctx.config, parcelle_name, &forwarded, None, false);
    }

    let inline = ExecutionContext::should_run_inline(args.inline);

    loop {
        let claimed_runs = {
            let _lock = ctx.store.acquire_tick_lock()?;
            reconcile_running_receipts(ctx)?;

            let tasks = ctx.store.list_tasks()?;
            if tasks.is_empty() {
                break;
            }

            let free_ceps = find_free_ceps(ctx)?;
            if free_ceps.is_empty() {
                break;
            }

            let limit = if inline {
                1
            } else {
                tasks.len().min(free_ceps.len())
            };

            let mut started = Vec::with_capacity(limit);
            for (task, cep) in tasks.into_iter().zip(free_ceps).take(limit) {
                let receipt = claim_and_prepare(ctx, &task, &cep, inline)?;
                if !inline {
                    let exe_path = env::current_exe()?;
                    if let Err(err) = tmux::spawn_detached_exec(
                        &receipt.tmux_session,
                        &cep.path,
                        &exe_path,
                        &receipt.run_id,
                    ) {
                        let mut failed = receipt.clone();
                        failed.status = RunStatus::Failed;
                        failed.finished_at = Some(now_iso8601_utc());
                        failed.summary = Some(format!("Failed to start tmux session: {}", err));
                        let _ = ctx.store.write_receipt(&failed);
                        let _ = set_agent_marker(&ctx.git, &cep.short_name, "⚠️");
                        return Err(err);
                    }
                }
                started.push(receipt.run_id);
            }
            started
        };

        if claimed_runs.is_empty() {
            break;
        }

        if inline {
            for run_id in claimed_runs {
                exec::execute_run(ctx, &run_id, true)?;
            }
        } else {
            break;
        }
    }

    Ok(())
}

fn find_free_ceps(ctx: &ExecutionContext) -> Result<Vec<WorktreeInfo>> {
    let worktrees = list_worktrees(&ctx.git)?;
    let mut detached = Vec::new();
    let mut other = Vec::new();
    for wt in worktrees {
        if wt.is_main || wt.locked.is_some() {
            continue;
        }
        if matches!(wt.head, WorktreeHead::Detached) {
            detached.push(wt);
        } else {
            other.push(wt);
        }
    }
    detached.extend(other);
    Ok(detached)
}

fn claim_and_prepare(
    ctx: &ExecutionContext,
    task: &QueuedTask,
    cep: &WorktreeInfo,
    inline: bool,
) -> Result<RunReceipt> {
    let profile_name = task
        .profile
        .as_deref()
        .unwrap_or(ctx.config.default_profile.as_str())
        .to_string();

    let Some(profile) = ctx.config.profiles.get(&profile_name) else {
        bail!(
            "Unknown profile '{}'. Configure [saisonnier.profiles.{}] in .vignoble.toml.",
            profile_name,
            profile_name
        );
    };
    if profile.run.is_none() {
        bail!(
            "Profile '{}' does not define a 'run' command.",
            profile_name
        );
    }

    let run_id = generate_sortable_id();
    let session_id = generate_uuid_v4();
    let base = ctx.config.resolve_base_ref(&ctx.git, task.base.as_deref());
    let branch = match task.branch.as_deref().filter(|b| !b.is_empty()) {
        Some(explicit) => explicit.to_string(),
        None => render_branch_template(
            &ctx.config.branch_template,
            &run_id,
            &task.prompt,
            &current_user(),
        ),
    };

    let lock_reason = format!("saisonnier:{} ({})", run_id, branch);
    lock_worktree(&ctx.git, &cep.path, Some(&lock_reason))?;

    let branch_exists = ctx.git.branch_exists(&branch, None);
    if let Err(err) = checkout_branch_in_worktree(
        &ctx.git,
        &cep.path,
        &branch,
        !branch_exists,
        base.as_deref(),
    ) {
        let _ = unlock_worktree(&ctx.git, &cep.path);
        return Err(err.into());
    }

    let start_sha = worktree_head_sha(&ctx.git, &cep.path)?;
    let prompt_file = ctx.store.write_prompt(&run_id, &task.prompt)?;
    let log_path = ctx.store.log_path(&run_id);
    let session_path = ctx.store.session_path(&run_id);
    let tmux_session = if inline {
        String::new()
    } else {
        tmux::session_name(&ctx.repo_name, &cep.short_name)
    };

    let receipt = RunReceipt {
        schema_version: SCHEMA_VERSION,
        run_id: run_id.clone(),
        task_id: task.task_id.clone(),
        cep: cep.short_name.clone(),
        branch,
        base,
        start_sha,
        end_sha: None,
        profile: profile_name,
        prompt_file,
        log_path,
        session_path,
        session_id,
        tmux_session,
        started_at: now_iso8601_utc(),
        finished_at: None,
        exit_code: None,
        status: RunStatus::Running,
        summary: None,
    };

    ctx.store.write_receipt(&receipt)?;
    ctx.store.remove_task(&task.task_id)?;

    Ok(receipt)
}
