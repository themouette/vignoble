use anyhow::Result;
use clap::Subcommand;
use git_core::{
    worktree::{clear_agent_marker, list_worktrees, set_agent_marker},
    GitRunner,
};
use std::{env, path::Path};

#[derive(Subcommand, Debug, Clone)]
pub enum AgentStateCommand {
    /// Set the agent status marker for the current worktree
    Set {
        /// Status badge (for example "🤖" or "💬")
        marker: String,
    },
    /// Clear the agent status marker for the current worktree
    Clear,
}

pub fn execute(cmd: &AgentStateCommand) -> Result<()> {
    let Ok(cwd) = env::current_dir() else {
        return Ok(());
    };
    execute_in_dir(&cwd, cmd)
}

/// Agent lifecycle hooks run in arbitrary directories.
/// Ignore non-repository directories so hooks never fail an agent turn.
pub fn execute_in_dir(cwd: &Path, cmd: &AgentStateCommand) -> Result<()> {
    let git = GitRunner::new(cwd, false);
    let Ok(worktrees) = list_worktrees(&git) else {
        return Ok(());
    };
    let Some(current) = worktrees.into_iter().find(|wt| wt.is_current) else {
        return Ok(());
    };

    match cmd {
        AgentStateCommand::Set { marker } => {
            let _ = set_agent_marker(&git, &current.short_name, marker);
        }
        AgentStateCommand::Clear => {
            let _ = clear_agent_marker(&git, &current.short_name);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use git_core::worktree::get_agent_marker;
    use std::process::Command;
    use tempfile::tempdir;

    fn init_repo(dir: &Path) {
        Command::new("git")
            .args(["init", "-b", "main"])
            .current_dir(dir)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(dir)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(dir)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "--allow-empty", "-m", "initial"])
            .current_dir(dir)
            .output()
            .unwrap();
    }

    #[test]
    fn test_agent_state_set_and_clear_in_repo() {
        let tmp = tempdir().unwrap();
        let repo_dir = tmp.path().join("my-repo");
        std::fs::create_dir_all(&repo_dir).unwrap();
        init_repo(&repo_dir);

        execute_in_dir(
            &repo_dir,
            &AgentStateCommand::Set {
                marker: "🤖".to_string(),
            },
        )
        .unwrap();

        let git = GitRunner::new(&repo_dir, false);
        assert_eq!(get_agent_marker(&git, "my-repo").as_deref(), Some("🤖"));

        execute_in_dir(&repo_dir, &AgentStateCommand::Clear).unwrap();
        assert_eq!(get_agent_marker(&git, "my-repo"), None);
    }

    #[test]
    fn test_agent_state_outside_repo_is_silent_noop() {
        let tmp = tempdir().unwrap();
        let res = execute_in_dir(
            tmp.path(),
            &AgentStateCommand::Set {
                marker: "🤖".to_string(),
            },
        );
        assert!(res.is_ok());
    }
}
