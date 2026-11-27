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

    /// Resolve the main repository root directory even when invoked inside a linked worktree.
    pub fn main_repo_root(&self) -> Result<PathBuf> {
        let toplevel = self.ensure_git_repository()?;
        let common_dir_raw = self.git(&["rev-parse", "--git-common-dir"])?;
        let common_dir = Path::new(&common_dir_raw);
        let abs_common = if common_dir.is_absolute() {
            common_dir.to_path_buf()
        } else {
            self.cwd.join(common_dir)
        };
        let canonical_common = abs_common.canonicalize().unwrap_or(abs_common);
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
