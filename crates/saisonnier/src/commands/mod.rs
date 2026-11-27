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
use git_core::{worktree::set_agent_marker, GitRunner};
use std::path::PathBuf;

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

/// Mark any `running` receipt whose tmux session no longer exists as `crashed`.
pub fn reconcile_running_receipts(ctx: &ExecutionContext) -> Result<usize> {
    let mut reconciled = 0;
    for mut receipt in ctx.store.list_receipts()? {
        if receipt.status != RunStatus::Running || receipt.tmux_session.is_empty() {
            continue;
        }
        if !tmux::is_session_alive(&receipt.tmux_session) {
            receipt.status = RunStatus::Crashed;
            receipt.finished_at = Some(now_iso8601_utc());
            receipt.summary = Some("tmux session terminated unexpectedly".to_string());
            ctx.store.write_receipt(&receipt)?;
            let _ = set_agent_marker(&ctx.git, &receipt.cep, "⚠️");
            reconciled += 1;
        }
    }
    Ok(reconciled)
}
