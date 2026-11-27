use crate::{
    commands::ExecutionContext,
    parcelle,
    template::{build_command, render_shell_command_string, TemplateVars},
    tmux,
};
use anyhow::{anyhow, bail, Context, Result};
use clap::Args;
use cli_core::ui::{color_str, GREEN_TICK_SYMBOL};
use git_core::{
    worktree::{checkout_branch_in_worktree, list_worktrees, lock_worktree, unlock_worktree},
    WorktreeHead, WorktreeInfo,
};
use std::process::Stdio;

#[derive(Args, Debug, Clone)]
pub struct ResumeArgs {
    /// Run ID to resume interactively in the current terminal
    pub run_id: String,

    /// Open or attach to a tmux session instead of running directly in the current terminal
    #[arg(long = "tmux")]
    pub tmux: bool,

    /// Target remote parcelle configured under [parcelle.<name>]
    #[arg(long = "parcelle")]
    pub parcelle: Option<String>,
}

pub fn execute(ctx: &ExecutionContext, args: &ResumeArgs) -> Result<()> {
    if let Some(ref parcelle_name) = args.parcelle {
        let mut forwarded = vec!["resume".to_string(), args.run_id.clone()];
        if args.tmux {
            forwarded.push("--tmux".to_string());
        }
        return parcelle::relay_command(&ctx.config, parcelle_name, &forwarded, None, true);
    }

    let receipt = ctx
        .store
        .find_receipt(&args.run_id)?
        .ok_or_else(|| anyhow!("No run receipt found matching '{}'", args.run_id))?;

    let profile = ctx.config.profiles.get(&receipt.profile).ok_or_else(|| {
        anyhow!(
            "Profile '{}' not found in configuration for run '{}'",
            receipt.profile,
            receipt.run_id
        )
    })?;
    let resume_spec = profile.resume.as_ref().ok_or_else(|| {
        anyhow!(
            "Profile '{}' does not define a 'resume' command.",
            receipt.profile
        )
    })?;

    let worktrees = list_worktrees(&ctx.git)?;
    let cep = select_or_claim_cep_for_resume(
        ctx,
        &worktrees,
        &receipt.run_id,
        &receipt.cep,
        &receipt.branch,
    )?;

    eprintln!(
        "{}Resuming run {} in worktree {} (branch {})",
        color_str(GREEN_TICK_SYMBOL, "green"),
        color_str(&receipt.run_id, "cyan"),
        color_str(&cep.short_name, "green"),
        receipt.branch
    );

    let vars = TemplateVars {
        run_id: &receipt.run_id,
        prompt_file: &receipt.prompt_file,
        branch: &receipt.branch,
        cep: &cep.short_name,
        path: &cep.path,
        session_path: &receipt.session_path,
        session_id: &receipt.session_id,
    };

    if args.tmux && tmux::is_tmux_available() {
        let session = tmux::session_name(&ctx.repo_name, &cep.short_name);
        let shell_cmd = render_shell_command_string(resume_spec, &vars)?;
        return tmux::open_interactive_session(&session, &cep.path, &shell_cmd);
    }

    let mut cmd = build_command(resume_spec, &vars)?;
    cmd.env("SAISONNIER_RUN_ID", &receipt.run_id)
        .env("SAISONNIER_PROMPT_FILE", &receipt.prompt_file)
        .env("SAISONNIER_SESSION_PATH", &receipt.session_path)
        .env("SAISONNIER_SESSION_ID", &receipt.session_id)
        .env("SAISONNIER_CEP", &cep.short_name)
        .env("SAISONNIER_BRANCH", &receipt.branch)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    let status = cmd
        .status()
        .context("Failed to execute profile resume command")?;
    if !status.success() {
        bail!("Resume command exited with status {}", status);
    }
    Ok(())
}

fn select_or_claim_cep_for_resume(
    ctx: &ExecutionContext,
    worktrees: &[WorktreeInfo],
    run_id: &str,
    original_cep: &str,
    branch: &str,
) -> Result<WorktreeInfo> {
    let owned_prefix = format!("saisonnier:{}", run_id);
    let resume_prefix = format!("saisonnier:resume:{}", run_id);

    // 1. Reuse the original cep if it is still claimed by this run.
    if let Some(orig) = worktrees.iter().find(|wt| wt.short_name == original_cep) {
        let owned = orig
            .locked
            .as_deref()
            .map(|r| r.starts_with(&owned_prefix) || r.starts_with(&resume_prefix))
            .unwrap_or(false);
        if owned {
            if orig.branch_name() != Some(branch) {
                checkout_branch_in_worktree(&ctx.git, &orig.path, branch, false, None)?;
            }
            return Ok(orig.clone());
        }
    }

    // 2. If another worktree already has the branch checked out, reuse it if free or fail clearly.
    if let Some(holder) = worktrees.iter().find(|wt| wt.branch_name() == Some(branch)) {
        if holder.is_main {
            bail!(
                "Branch '{}' is currently checked out in the main repository worktree.",
                branch
            );
        }
        if holder.locked.is_some() {
            bail!(
                "Branch '{}' is already checked out in claimed worktree '{}'.",
                branch,
                holder.short_name
            );
        }
        let reason = format!("saisonnier:resume:{} ({})", run_id, branch);
        lock_worktree(&ctx.git, &holder.path, Some(&reason))?;
        return Ok(holder.clone());
    }

    // 3. Claim any free cep from the pool and check out the run's branch.
    let candidate = worktrees
        .iter()
        .filter(|wt| !wt.is_main && wt.locked.is_none())
        .find(|wt| matches!(wt.head, WorktreeHead::Detached))
        .or_else(|| {
            worktrees
                .iter()
                .find(|wt| !wt.is_main && wt.locked.is_none())
        })
        .ok_or_else(|| {
            anyhow!(
                "No unclaimed worktree available in the pool to resume run '{}'. Release a cep or run 'cep create'.",
                run_id
            )
        })?;

    let reason = format!("saisonnier:resume:{} ({})", run_id, branch);
    lock_worktree(&ctx.git, &candidate.path, Some(&reason))?;

    if let Err(err) = checkout_branch_in_worktree(&ctx.git, &candidate.path, branch, false, None) {
        let _ = unlock_worktree(&ctx.git, &candidate.path);
        return Err(err.into());
    }

    Ok(candidate.clone())
}
