use crate::commands::ExecutionContext;
use anyhow::{anyhow, bail, Result};
use clap::Args;
use git_core::worktree::{
    get_history_path, list_worktrees, resolve_worktree_target, set_history_path,
};

#[derive(Args, Debug, Clone)]
pub struct CdArgs {
    /// Target worktree name, checked-out branch, path, '-' for previous, or 'main' (defaults to main)
    pub target: Option<String>,

    /// Emit shell `cd '<path>'` command for `eval "$(vignoble shell-init)"`
    #[arg(long = "shell-eval")]
    pub shell_eval: bool,
}

pub fn execute(ctx: &ExecutionContext, args: &CdArgs) -> Result<()> {
    let worktrees = list_worktrees(&ctx.git)?;

    let dest_path = match args.target.as_deref() {
        None => worktrees
            .iter()
            .find(|wt| wt.is_main)
            .map(|wt| wt.path.clone())
            .ok_or_else(|| anyhow!("Unable to locate main worktree"))?,
        Some("-") => get_history_path(&ctx.git)
            .ok_or_else(|| anyhow!("No previous worktree recorded in cep.history-path"))?,
        Some(target) => {
            if let Some(wt) = resolve_worktree_target(&ctx.git, &worktrees, target) {
                wt.path
            } else if target == "main" {
                worktrees
                    .iter()
                    .find(|wt| wt.is_main)
                    .map(|wt| wt.path.clone())
                    .ok_or_else(|| anyhow!("Unable to locate main worktree"))?
            } else {
                bail!("No worktree matching '{}'", target);
            }
        }
    };

    if let Some(current_wt) = worktrees.iter().find(|wt| wt.is_current) {
        let _ = set_history_path(&ctx.git, &current_wt.path);
    } else if let Ok(current_root) = ctx.git.ensure_git_repository() {
        let _ = set_history_path(&ctx.git, &current_root);
    }

    if args.shell_eval {
        let escaped = dest_path.to_string_lossy().replace('\'', "'\\''");
        println!("cd '{}'", escaped);
    } else {
        println!("{}", dest_path.display());
        eprintln!(
            "Hint: To change directories in your current shell, enable shell integration:\n  eval \"$(vignoble shell-init)\""
        );
    }

    Ok(())
}
