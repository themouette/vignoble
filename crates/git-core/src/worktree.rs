use crate::{error::Result, runner::GitRunner};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// State of HEAD in a worktree.
/// Includes `Detached` and `Bare` so `git-wt` can inspect worktrees without a checked-out branch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorktreeHead {
    Branch(String),
    Detached,
    Bare,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeInfo {
    pub path: PathBuf,
    pub short_name: String,
    pub head_sha: String,
    pub head: WorktreeHead,
    pub is_current: bool,
    pub is_main: bool,
    pub locked: Option<String>,
    pub prunable: Option<String>,
}

impl WorktreeInfo {
    pub fn branch_name(&self) -> Option<&str> {
        match &self.head {
            WorktreeHead::Branch(name) => Some(name.as_str()),
            _ => None,
        }
    }
}

pub fn list_worktrees(git: &GitRunner) -> Result<Vec<WorktreeInfo>> {
    let raw = git.git(&["worktree", "list", "--porcelain"])?;
    let current_toplevel = git
        .ensure_git_repository()
        .ok()
        .and_then(|p| p.canonicalize().ok().or(Some(p)));
    Ok(parse_porcelain_worktrees(&raw, current_toplevel.as_deref()))
}

pub fn map_worktrees_by_branch(git: &GitRunner) -> Result<HashMap<String, WorktreeInfo>> {
    let worktrees = list_worktrees(git)?;
    let mut map = HashMap::new();
    for wt in worktrees {
        if let Some(branch) = wt.branch_name() {
            map.insert(branch.to_string(), wt);
        }
    }
    Ok(map)
}

pub fn parse_porcelain_worktrees(
    porcelain: &str,
    current_worktree_root: Option<&Path>,
) -> Vec<WorktreeInfo> {
    let mut worktrees = Vec::new();
    let canonical_current =
        current_worktree_root.map(|p| p.canonicalize().unwrap_or_else(|_| p.to_path_buf()));

    let mut path: Option<PathBuf> = None;
    let mut head_sha = String::new();
    let mut head = WorktreeHead::Detached;
    let mut locked: Option<String> = None;
    let mut prunable: Option<String> = None;

    let flush_entry = |worktrees: &mut Vec<WorktreeInfo>,
                       path: Option<PathBuf>,
                       head_sha: String,
                       head: WorktreeHead,
                       locked: Option<String>,
                       prunable: Option<String>| {
        if let Some(wt_path) = path {
            let short_name = wt_path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("worktree")
                .to_string();
            let canonical_wt = wt_path.canonicalize().unwrap_or_else(|_| wt_path.clone());
            let is_current = canonical_current
                .as_ref()
                .map(|c| c == &canonical_wt)
                .unwrap_or(false);
            let is_main = worktrees.is_empty();

            worktrees.push(WorktreeInfo {
                path: wt_path,
                short_name,
                head_sha,
                head,
                is_current,
                is_main,
                locked,
                prunable,
            });
        }
    };

    for line in porcelain.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            flush_entry(
                &mut worktrees,
                path.take(),
                std::mem::take(&mut head_sha),
                std::mem::replace(&mut head, WorktreeHead::Detached),
                locked.take(),
                prunable.take(),
            );
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("worktree ") {
            if path.is_some() {
                flush_entry(
                    &mut worktrees,
                    path.take(),
                    std::mem::take(&mut head_sha),
                    std::mem::replace(&mut head, WorktreeHead::Detached),
                    locked.take(),
                    prunable.take(),
                );
            }
            path = Some(PathBuf::from(rest));
        } else if let Some(rest) = trimmed.strip_prefix("HEAD ") {
            head_sha = rest.to_string();
        } else if let Some(rest) = trimmed.strip_prefix("branch ") {
            let short_branch = rest.strip_prefix("refs/heads/").unwrap_or(rest);
            head = WorktreeHead::Branch(short_branch.to_string());
        } else if trimmed == "detached" {
            head = WorktreeHead::Detached;
        } else if trimmed == "bare" {
            head = WorktreeHead::Bare;
        } else if trimmed == "locked" {
            locked = Some(String::new());
        } else if let Some(reason) = trimmed.strip_prefix("locked ") {
            locked = Some(reason.to_string());
        } else if trimmed == "prunable" {
            prunable = Some(String::new());
        } else if let Some(reason) = trimmed.strip_prefix("prunable ") {
            prunable = Some(reason.to_string());
        }
    }

    flush_entry(&mut worktrees, path, head_sha, head, locked, prunable);
    worktrees
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_porcelain_worktrees_with_branch_detached_and_locked() {
        let sample = "\
worktree /Users/dev/repo
HEAD 1111111111111111111111111111111111111111
branch refs/heads/main

worktree /Users/dev/worktrees/pool-1
HEAD 2222222222222222222222222222222222222222
branch refs/heads/themouette/feature-a
locked claimed by agent-1

worktree /Users/dev/worktrees/pool-2
HEAD 3333333333333333333333333333333333333333
detached
";

        let list =
            parse_porcelain_worktrees(sample, Some(Path::new("/Users/dev/worktrees/pool-1")));
        assert_eq!(list.len(), 3);

        assert_eq!(list[0].short_name, "repo");
        assert!(list[0].is_main);
        assert!(!list[0].is_current);
        assert_eq!(list[0].head, WorktreeHead::Branch("main".to_string()));

        assert_eq!(list[1].short_name, "pool-1");
        assert!(!list[1].is_main);
        assert!(list[1].is_current);
        assert_eq!(
            list[1].head,
            WorktreeHead::Branch("themouette/feature-a".to_string())
        );
        assert_eq!(list[1].locked.as_deref(), Some("claimed by agent-1"));

        assert_eq!(list[2].short_name, "pool-2");
        assert_eq!(list[2].head, WorktreeHead::Detached);
        assert_eq!(list[2].branch_name(), None);
    }
}
