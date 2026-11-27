pub mod cancel;
pub mod clean;
pub mod edit;
pub mod enqueue;
pub mod exec;
pub mod inspect;
pub mod list;
pub mod logs;
pub mod resume;
pub mod tick;

use crate::{
    config::EffectiveConfig,
    store::{now_iso8601_utc, RunStatus, VignobleStore},
    tmux,
};
use anyhow::Result;
use git_core::{
    worktree::{
        clear_agent_marker, detach_worktree_head, is_worktree_dirty, list_worktrees,
        set_agent_marker, unlock_worktree, worktree_head_sha,
    },
    GitRunner,
};
use std::path::PathBuf;
use std::process::{Command, Stdio};

pub struct ExecutionContext {
    pub git: GitRunner,
    pub main_repo_root: PathBuf,
    pub repo_name: String,
    pub store: VignobleStore,
    pub config: EffectiveConfig,
}

impl ExecutionContext {
    pub fn should_run_inline(cli_inline: bool) -> bool {
        if cli_inline {
            return true;
        }
        std::env::var("SAISONNIER_RUNNER")
            .map(|v| v.eq_ignore_ascii_case("inline"))
            .unwrap_or(false)
    }
}

/// Mark any `running` receipt whose tmux session or inline PID no longer exists as `crashed`.
/// If the crashed run made no commits and left the worktree clean, automatically release its cep.
pub fn reconcile_running_receipts(ctx: &ExecutionContext) -> Result<usize> {
    let mut reconciled = 0;
    let worktrees = list_worktrees(&ctx.git).unwrap_or_default();

    for mut receipt in ctx.store.list_receipts()? {
        if receipt.status != RunStatus::Running {
            continue;
        }

        let alive = if !receipt.tmux_session.is_empty() {
            tmux::is_session_alive(&receipt.tmux_session)
        } else if let Some(pid) = receipt.pid {
            pid == std::process::id() || is_pid_alive(pid)
        } else {
            false
        };

        if !alive {
            receipt.status = RunStatus::Crashed;
            receipt.finished_at = Some(now_iso8601_utc());
            receipt.summary = Some(if !receipt.tmux_session.is_empty() {
                "tmux session terminated unexpectedly".to_string()
            } else {
                "inline worker process terminated unexpectedly".to_string()
            });
            ctx.store.write_receipt(&receipt)?;

            let owned_prefix = format!("saisonnier:{}", receipt.run_id);
            if let Some(wt) = worktrees.iter().find(|w| w.short_name == receipt.cep) {
                let is_owned = wt
                    .locked
                    .as_deref()
                    .map(|r| r.starts_with(&owned_prefix))
                    .unwrap_or(false);
                let clean = !is_worktree_dirty(&ctx.git, &wt.path).unwrap_or(true);
                let head_unchanged = worktree_head_sha(&ctx.git, &wt.path).ok().as_deref()
                    == Some(receipt.start_sha.as_str());

                if is_owned && clean && head_unchanged {
                    let _ = detach_worktree_head(&ctx.git, &wt.path);
                    let _ = unlock_worktree(&ctx.git, &wt.path);
                    let _ = clear_agent_marker(&ctx.git, &receipt.cep);
                } else {
                    let _ = set_agent_marker(&ctx.git, &receipt.cep, "⚠️");
                }
            } else {
                let _ = set_agent_marker(&ctx.git, &receipt.cep, "⚠️");
            }
            reconciled += 1;
        }
    }
    Ok(reconciled)
}

fn is_pid_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        false
    }
}
