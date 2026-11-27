use crate::error::{GitError, Result};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Debug, Clone)]
pub struct GitRunner {
    cwd: PathBuf,
    debug: bool,
}

impl GitRunner {
    pub fn new(cwd: impl Into<PathBuf>, debug: bool) -> Self {
        Self {
            cwd: cwd.into(),
            debug,
        }
    }

    pub fn cwd(&self) -> &Path {
        &self.cwd
    }

    pub fn debug(&self) -> bool {
        self.debug
    }

    /// Run `git <args>` and return trimmed stdout.
    pub fn git(&self, args: &[&str]) -> Result<String> {
        if self.debug {
            println!("+ git {}", args.join(" "));
        }

        let output = Command::new("git")
            .args(args)
            .current_dir(&self.cwd)
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(GitError::CommandFailed {
                command: args.join(" "),
                stderr,
            });
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    /// Run `git <args>` inheriting stdout and stderr (used for `git fetch`).
    pub fn call_git_interactive(&self, args: &[&str]) -> Result<()> {
        if self.debug {
            println!("+ git {}", args.join(" "));
        }

        let status = Command::new("git")
            .args(args)
            .current_dir(&self.cwd)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()?;

        if !status.success() {
            return Err(GitError::CommandFailed {
                command: args.join(" "),
                stderr: format!("exit status {}", status),
            });
        }
        Ok(())
    }

    pub fn ensure_git_repository(&self) -> Result<PathBuf> {
        match self.git(&["rev-parse", "--show-toplevel"]) {
            Ok(root) if !root.is_empty() => Ok(PathBuf::from(root)),
            _ => Err(GitError::NotAGitRepository),
        }
    }

    /// Resolve the shared git common directory (canonicalized when possible).
    pub fn git_common_dir(&self) -> Result<PathBuf> {
        self.ensure_git_repository()?;
        let common_dir_raw = self.git(&["rev-parse", "--git-common-dir"])?;
        let common_dir = Path::new(&common_dir_raw);
        let abs_common = if common_dir.is_absolute() {
            common_dir.to_path_buf()
        } else {
            self.cwd.join(common_dir)
        };
        Ok(abs_common.canonicalize().unwrap_or(abs_common))
    }

    /// Resolve the main repository root directory even when invoked inside a linked worktree.
    pub fn main_repo_root(&self) -> Result<PathBuf> {
        let toplevel = self.ensure_git_repository()?;
        let canonical_common = self.git_common_dir()?;
        if canonical_common.file_name().and_then(|s| s.to_str()) == Some(".git") {
            if let Some(parent) = canonical_common.parent() {
                return Ok(parent.to_path_buf());
            }
        }
        Ok(toplevel)
    }

    pub fn current_branch(&self) -> Result<Option<String>> {
        let head = self.git(&["rev-parse", "--abbrev-ref", "HEAD"])?;
        if head == "HEAD" || head.is_empty() {
            Ok(None)
        } else {
            Ok(Some(head))
        }
    }

    pub fn remotes(&self) -> Result<Vec<String>> {
        let out = self.git(&["remote"])?;
        let mut list: Vec<String> = out
            .lines()
            .map(str::trim)
            .filter(|r| !r.is_empty())
            .map(String::from)
            .collect();
        // Longest remote name wins when one remote name is a path prefix of another.
        list.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
        Ok(list)
    }

    pub fn branch_exists(&self, branch: &str, remote: Option<&str>) -> bool {
        let full_ref = match remote {
            Some(r) => format!("refs/remotes/{}/{}", r, branch),
            None => format!("refs/heads/{}", branch),
        };
        self.git(&["show-ref", "--verify", "--quiet", &full_ref])
            .or_else(|_| {
                let fallback = match remote {
                    Some(r) => format!("{}/{}", r, branch),
                    None => branch.to_string(),
                };
                self.git(&["show-ref", "--quiet", &fallback])
            })
            .is_ok()
    }

    pub fn is_ancestor(&self, ancestor: &str, descendant: &str) -> bool {
        self.git(&["merge-base", "--is-ancestor", ancestor, descendant])
            .is_ok()
    }

    /// Infer the default base ref for new worktrees or branches:
    /// 1. `refs/remotes/<remote>/HEAD` symbolic ref (for example `origin/main` or `origin/prod`)
    /// 2. First existing remote branch in `<remote>/main`, `<remote>/master`, `<remote>/prod`
    /// 3. First existing local branch in `main`, `master`, `prod`
    pub fn infer_default_base_ref(&self, remote: &str) -> Option<String> {
        let remote_head_ref = format!("refs/remotes/{}/HEAD", remote);
        if let Ok(sym) = self.git(&["symbolic-ref", "--quiet", "--short", &remote_head_ref]) {
            if !sym.is_empty()
                && self
                    .git(&["rev-parse", "--verify", "--quiet", &sym])
                    .is_ok()
            {
                return Some(sym);
            }
        }

        for candidate in ["main", "master", "prod"] {
            if self.branch_exists(candidate, Some(remote)) {
                return Some(format!("{}/{}", remote, candidate));
            }
        }

        for candidate in ["main", "master", "prod"] {
            if self.branch_exists(candidate, None) {
                return Some(candidate.to_string());
            }
        }

        None
    }

    pub fn fetch(&self, remote: &str) -> Result<()> {
        println!("Fetching from {}...", remote);
        self.call_git_interactive(&["fetch", "-p", "--no-tags", remote])
    }

    pub fn fetch_branches(&self, remote: &str, branches: &[String]) -> Result<()> {
        if branches.is_empty() {
            return Ok(());
        }
        println!("Fetching from {} ({})...", remote, branches.join(", "));
        let mut args = Vec::with_capacity(3 + branches.len());
        args.push("fetch");
        args.push("--no-tags");
        args.push(remote);
        args.extend(branches.iter().map(String::as_str));
        self.call_git_interactive(&args)
    }

    pub fn delete_local_branch(&self, branch: &str, force: bool) -> Result<()> {
        let flag = if force { "-D" } else { "-d" };
        self.git(&["branch", flag, branch])?;
        Ok(())
    }

    pub fn delete_remote_branch(&self, remote: &str, branch: &str) -> Result<()> {
        let refspec = format!(":{}", branch);
        self.git(&["push", remote, &refspec])?;
        Ok(())
    }
}

/// Match `<remote>/<branch>` only when `<remote>` is a configured git remote, so slashed
/// branch names (such as `release/v1`) are not mistaken for remote prefixes.
pub fn split_remote_branch<'a>(
    spec: &'a str,
    known_remotes: &[String],
) -> Option<(&'a str, &'a str)> {
    for remote in known_remotes {
        if let Some(rest) = spec.strip_prefix(remote.as_str()) {
            if let Some(branch) = rest.strip_prefix('/') {
                if !branch.is_empty() {
                    return Some((&spec[..remote.len()], branch));
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_remote_branch_only_matches_known_remotes() {
        let remotes = vec!["deploy".to_string(), "origin".to_string()];

        assert_eq!(
            split_remote_branch("origin/main", &remotes),
            Some(("origin", "main"))
        );
        assert_eq!(
            split_remote_branch("deploy/prod", &remotes),
            Some(("deploy", "prod"))
        );
        assert_eq!(
            split_remote_branch("deploy/release/2026", &remotes),
            Some(("deploy", "release/2026"))
        );
        assert_eq!(split_remote_branch("release/2026", &remotes), None);
        assert_eq!(split_remote_branch("main", &remotes), None);
        assert_eq!(split_remote_branch("origin/", &remotes), None);
    }
}
