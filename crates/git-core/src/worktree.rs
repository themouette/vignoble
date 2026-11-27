use crate::{error::Result, runner::GitRunner};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// State of HEAD in a worktree.
/// Includes `Detached` and `Bare` so `cep` can inspect worktrees without a checked-out branch.
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

pub fn add_worktree_detached(git: &GitRunner, path: &Path, base_ref: Option<&str>) -> Result<()> {
    let path_str = path.to_string_lossy();
    let mut args = vec!["worktree", "add", "--detach", path_str.as_ref()];
    if let Some(base) = base_ref {
        args.push(base);
    }
    git.git(&args)?;
    Ok(())
}

pub fn add_worktree_branch(
    git: &GitRunner,
    path: &Path,
    branch: &str,
    create_branch: bool,
    base_ref: Option<&str>,
) -> Result<()> {
    let path_str = path.to_string_lossy();
    let mut args = vec!["worktree", "add"];
    if create_branch {
        args.push("-b");
        args.push(branch);
        args.push(path_str.as_ref());
        if let Some(base) = base_ref {
            args.push(base);
        }
    } else {
        args.push(path_str.as_ref());
        args.push(branch);
    }
    git.git(&args)?;
    Ok(())
}

pub fn lock_worktree(git: &GitRunner, path: &Path, reason: Option<&str>) -> Result<()> {
    let path_str = path.to_string_lossy();
    let mut args = vec!["worktree", "lock"];
    if let Some(r) = reason.filter(|r| !r.is_empty()) {
        args.push("--reason");
        args.push(r);
    }
    args.push(path_str.as_ref());
    git.git(&args)?;
    Ok(())
}

pub fn unlock_worktree(git: &GitRunner, path: &Path) -> Result<()> {
    let path_str = path.to_string_lossy();
    git.git(&["worktree", "unlock", path_str.as_ref()])?;
    Ok(())
}

pub fn remove_worktree(git: &GitRunner, path: &Path, force: bool) -> Result<()> {
    let path_str = path.to_string_lossy();
    let mut args = vec!["worktree", "remove"];
    if force {
        args.push("--force");
    }
    args.push(path_str.as_ref());
    git.git(&args)?;
    Ok(())
}

pub fn detach_worktree_head(git: &GitRunner, worktree_path: &Path) -> Result<()> {
    let path_str = worktree_path.to_string_lossy();
    git.git(&["-C", path_str.as_ref(), "checkout", "--detach"])?;
    Ok(())
}

pub fn checkout_detached_in_worktree(
    git: &GitRunner,
    worktree_path: &Path,
    base_ref: &str,
) -> Result<()> {
    let path_str = worktree_path.to_string_lossy();
    git.git(&["-C", path_str.as_ref(), "checkout", "--detach", base_ref])?;
    Ok(())
}

pub fn checkout_branch_in_worktree(
    git: &GitRunner,
    worktree_path: &Path,
    branch: &str,
    create: bool,
    start_point: Option<&str>,
) -> Result<()> {
    let path_str = worktree_path.to_string_lossy();
    if create {
        let mut args = vec!["-C", path_str.as_ref(), "checkout", "-b", branch];
        if let Some(base) = start_point {
            args.push(base);
        }
        git.git(&args)?;
    } else {
        git.git(&["-C", path_str.as_ref(), "checkout", branch])?;
    }
    Ok(())
}

pub fn is_worktree_dirty(git: &GitRunner, worktree_path: &Path) -> Result<bool> {
    let path_str = worktree_path.to_string_lossy();
    let out = git.git(&["-C", path_str.as_ref(), "status", "--porcelain"])?;
    Ok(!out.trim().is_empty())
}

pub fn worktree_head_sha(git: &GitRunner, worktree_path: &Path) -> Result<String> {
    let path_str = worktree_path.to_string_lossy();
    git.git(&["-C", path_str.as_ref(), "rev-parse", "HEAD"])
}

pub fn get_history_path(git: &GitRunner) -> Option<PathBuf> {
    git.git(&["config", "--get", "cep.history-path"])
        .ok()
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

pub fn set_history_path(git: &GitRunner, path: &Path) -> Result<()> {
    let path_str = path.to_string_lossy();
    git.git(&["config", "cep.history-path", path_str.as_ref()])?;
    Ok(())
}

pub fn get_agent_marker(git: &GitRunner, worktree_name: &str) -> Option<String> {
    let key = format!("cep.state.{}.marker", worktree_name);
    git.git(&["config", "--get", &key])
        .ok()
        .filter(|v| !v.is_empty())
}

pub fn list_agent_markers(git: &GitRunner) -> HashMap<String, String> {
    let Ok(out) = git.git(&[
        "config",
        "--null",
        "--get-regexp",
        r"^cep\.state\..+\.marker$",
    ]) else {
        return HashMap::new();
    };
    parse_agent_markers(&out)
}

fn parse_agent_markers(output: &str) -> HashMap<String, String> {
    let mut markers = HashMap::new();
    for record in output.split('\0') {
        let Some((key, value)) = record.split_once('\n') else {
            continue;
        };
        let Some(rest) = key.strip_prefix("cep.state.") else {
            continue;
        };
        let Some(worktree_name) = rest.strip_suffix(".marker") else {
            continue;
        };
        if worktree_name.is_empty() {
            continue;
        }
        let value = value.trim();
        if value.is_empty() {
            markers.remove(worktree_name);
        } else {
            markers.insert(worktree_name.to_string(), value.to_string());
        }
    }
    markers
}

pub fn set_agent_marker(git: &GitRunner, worktree_name: &str, marker: &str) -> Result<()> {
    let key = format!("cep.state.{}.marker", worktree_name);
    git.git(&["config", &key, marker])?;
    Ok(())
}

pub fn clear_agent_marker(git: &GitRunner, worktree_name: &str) -> Result<()> {
    let key = format!("cep.state.{}.marker", worktree_name);
    let _ = git.git(&["config", "--unset-all", &key]);
    Ok(())
}

fn normalize_path(path: &Path) -> PathBuf {
    if let Ok(canon) = path.canonicalize() {
        return canon;
    }
    if let (Some(parent), Some(name)) = (path.parent(), path.file_name()) {
        if let Ok(canon_parent) = parent.canonicalize() {
            return canon_parent.join(name);
        }
    }
    path.to_path_buf()
}

/// Resolve `target` against `worktrees` in strict precedence order:
/// 1. Exact match on `short_name`
/// 2. Exact match on checked-out branch name
/// 3. Filesystem path (canonicalized or relative to `git.cwd()`)
pub fn resolve_worktree_target(
    git: &GitRunner,
    worktrees: &[WorktreeInfo],
    target: &str,
) -> Option<WorktreeInfo> {
    if let Some(by_name) = worktrees.iter().find(|wt| wt.short_name == target) {
        return Some(by_name.clone());
    }

    if let Some(by_branch) = worktrees.iter().find(|wt| wt.branch_name() == Some(target)) {
        return Some(by_branch.clone());
    }

    let raw_target = Path::new(target);
    let candidate = if raw_target.is_absolute() {
        raw_target.to_path_buf()
    } else {
        git.cwd().join(raw_target)
    };
    let norm_candidate = normalize_path(&candidate);
    let norm_raw = normalize_path(raw_target);

    worktrees
        .iter()
        .find(|wt| {
            let norm_wt = normalize_path(&wt.path);
            norm_wt == norm_candidate
                || norm_wt == norm_raw
                || wt.path == candidate
                || wt.path == raw_target
        })
        .cloned()
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

    #[test]
    fn test_resolve_worktree_target_strict_precedence() {
        let git = GitRunner::new("/Users/dev/repo", false);
        let worktrees = vec![
            WorktreeInfo {
                path: PathBuf::from("/Users/dev/repo"),
                short_name: "repo".to_string(),
                head_sha: "1111111".to_string(),
                head: WorktreeHead::Branch("main".to_string()),
                is_current: true,
                is_main: true,
                locked: None,
                prunable: None,
            },
            WorktreeInfo {
                path: PathBuf::from("/Users/dev/worktrees/syrah"),
                short_name: "syrah".to_string(),
                head_sha: "2222222".to_string(),
                head: WorktreeHead::Branch("feat/login".to_string()),
                is_current: false,
                is_main: false,
                locked: None,
                prunable: None,
            },
            WorktreeInfo {
                path: PathBuf::from("/Users/dev/worktrees/merlot"),
                short_name: "merlot".to_string(),
                head_sha: "3333333".to_string(),
                head: WorktreeHead::Branch("syrah".to_string()),
                is_current: false,
                is_main: false,
                locked: None,
                prunable: None,
            },
        ];

        // 1. Short name beats checked-out branch with the same name ("syrah")
        let resolved = resolve_worktree_target(&git, &worktrees, "syrah").unwrap();
        assert_eq!(resolved.short_name, "syrah");

        // 2. Checked-out branch matches when no short name matches
        let by_branch = resolve_worktree_target(&git, &worktrees, "feat/login").unwrap();
        assert_eq!(by_branch.short_name, "syrah");

        // 3. Path matches when neither short name nor branch matches
        let by_path =
            resolve_worktree_target(&git, &worktrees, "/Users/dev/worktrees/merlot").unwrap();
        assert_eq!(by_path.short_name, "merlot");
    }

    #[test]
    fn test_parse_agent_markers_handles_spaces_overrides_and_empty_values() {
        let raw = concat!(
            "cep.state.syrah.marker\n🤖\0",
            "cep.state.my repo.marker\n🧪\0",
            "cep.state.syrah.marker\n⚡\0",
            "cep.state.merlot.marker\n🤖\0",
            "cep.state.merlot.marker\n\0",
        );
        let markers = parse_agent_markers(raw);
        assert_eq!(markers.len(), 2);
        assert_eq!(markers.get("syrah").map(String::as_str), Some("⚡"));
        assert_eq!(markers.get("my repo").map(String::as_str), Some("🧪"));
        assert!(!markers.contains_key("merlot"));
    }

    #[test]
    fn test_list_agent_markers_reads_git_config() {
        let tmp = tempfile::TempDir::new().unwrap();
        let git = GitRunner::new(tmp.path(), false);
        git.git(&["init", "-b", "main"]).unwrap();

        assert!(list_agent_markers(&git).is_empty());

        set_agent_marker(&git, "syrah", "🤖").unwrap();
        set_agent_marker(&git, "my repo", "🧪").unwrap();
        let markers = list_agent_markers(&git);
        assert_eq!(markers.len(), 2);
        assert_eq!(markers.get("syrah").map(String::as_str), Some("🤖"));
        assert_eq!(markers.get("my repo").map(String::as_str), Some("🧪"));

        clear_agent_marker(&git, "syrah").unwrap();
        let markers = list_agent_markers(&git);
        assert_eq!(markers.len(), 1);
        assert_eq!(markers.get("my repo").map(String::as_str), Some("🧪"));
    }
}
