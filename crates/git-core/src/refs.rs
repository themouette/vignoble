use crate::{error::Result, runner::GitRunner};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncStatus {
    UpToDate,
    NeedPush,
    NeedPull,
    Diverged,
    NoRemote,
    RemoteBranch,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchInfo {
    pub name: String,
    pub fullname: String,
    pub commit_hash: String,
    pub commit_time_relative: String,
    pub upstream: Option<String>,
    pub status: SyncStatus,
}

#[derive(Debug, Clone)]
struct RawRefEntry {
    short_name: String,
    full_name: String,
    sha: String,
    upstream_short: String,
    upstream_full: String,
    upstream_track: String,
    committer_date_relative: String,
}

const REF_FORMAT: &str =
    "%(refname:short)|%(refname)|%(objectname)|%(upstream:short)|%(upstream)|%(upstream:track)|%(committerdate:relative)";
const REF_SHA_FORMAT: &str = "%(refname)|%(objectname)";
const REF_BATCH_SIZE: usize = 500;

fn parse_for_each_ref(output: &str) -> Vec<RawRefEntry> {
    let mut entries = Vec::new();
    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let parts: Vec<&str> = trimmed.splitn(7, '|').collect();
        if parts.len() < 7 {
            continue;
        }
        entries.push(RawRefEntry {
            short_name: parts[0].to_string(),
            full_name: parts[1].to_string(),
            sha: parts[2].to_string(),
            upstream_short: parts[3].to_string(),
            upstream_full: parts[4].to_string(),
            upstream_track: parts[5].to_string(),
            committer_date_relative: parts[6].to_string(),
        });
    }
    entries
}

fn resolve_ref_shas(git: &GitRunner, refs: &[String]) -> Result<HashMap<String, String>> {
    let mut sha_map = HashMap::new();
    if refs.is_empty() {
        return Ok(sha_map);
    }

    let fmt_arg = format!("--format={}", REF_SHA_FORMAT);
    for chunk in refs.chunks(REF_BATCH_SIZE) {
        let mut args: Vec<&str> = Vec::with_capacity(3 + chunk.len());
        args.push("--no-pager");
        args.push("for-each-ref");
        args.push(&fmt_arg);
        args.extend(chunk.iter().map(String::as_str));

        let out = git.git(&args)?;
        for line in out.lines() {
            let trimmed = line.trim();
            if let Some((full_name, sha)) = trimmed.split_once('|') {
                if !full_name.is_empty() && !sha.is_empty() {
                    sha_map.insert(full_name.to_string(), sha.to_string());
                }
            }
        }
    }

    Ok(sha_map)
}

/// Collect sync status for all local branches, or for remote branches when `pattern` is set.
pub fn collect_branches_status(
    git: &GitRunner,
    remote: &str,
    pattern: Option<&str>,
) -> Result<HashMap<String, BranchInfo>> {
    if pattern.is_some() {
        let remote_ref_prefix = format!("refs/remotes/{}", remote);
        let fmt_arg = format!("--format={}", REF_FORMAT);
        let remote_out = git.git(&["--no-pager", "for-each-ref", &fmt_arg, &remote_ref_prefix])?;
        let remote_entries = parse_for_each_ref(&remote_out);
        let head_name = format!("{}/HEAD", remote);
        let mut result = HashMap::new();

        for entry in remote_entries {
            if entry.short_name == head_name || entry.short_name == remote {
                continue;
            }
            result.insert(
                entry.short_name.clone(),
                BranchInfo {
                    name: entry.short_name.clone(),
                    fullname: entry.full_name,
                    commit_hash: entry.sha,
                    commit_time_relative: entry.committer_date_relative,
                    upstream: Some(entry.short_name),
                    status: SyncStatus::RemoteBranch,
                },
            );
        }
        return Ok(result);
    }

    collect_local_branches_status(git, remote, None)
}

/// Collect sync status for only the specified local branch names.
pub fn collect_branches_status_for(
    git: &GitRunner,
    remote: &str,
    branches: &[&str],
) -> Result<HashMap<String, BranchInfo>> {
    collect_local_branches_status(git, remote, Some(branches))
}

fn collect_local_branches_status(
    git: &GitRunner,
    remote: &str,
    branch_filter: Option<&[&str]>,
) -> Result<HashMap<String, BranchInfo>> {
    let fmt_arg = format!("--format={}", REF_FORMAT);
    let local_entries = match branch_filter {
        Some(names) => {
            let mut wanted = HashSet::new();
            let mut ref_paths = Vec::new();
            for &name in names {
                if wanted.insert(name) {
                    ref_paths.push(format!("refs/heads/{}", name));
                }
            }
            if ref_paths.is_empty() {
                return Ok(HashMap::new());
            }

            let mut entries = Vec::new();
            for chunk in ref_paths.chunks(REF_BATCH_SIZE) {
                let mut args: Vec<&str> = Vec::with_capacity(3 + chunk.len());
                args.push("--no-pager");
                args.push("for-each-ref");
                args.push(&fmt_arg);
                args.extend(chunk.iter().map(String::as_str));

                let out = git.git(&args)?;
                // `for-each-ref refs/heads/foo` also matches `refs/heads/foo/bar`.
                entries.extend(
                    parse_for_each_ref(&out)
                        .into_iter()
                        .filter(|e| wanted.contains(e.short_name.as_str())),
                );
            }
            entries
        }
        None => {
            let local_out = git.git(&["--no-pager", "for-each-ref", &fmt_arg, "refs/heads"])?;
            parse_for_each_ref(&local_out)
        }
    };

    if local_entries.is_empty() {
        return Ok(HashMap::new());
    }

    let mut seen_candidates = HashSet::new();
    let mut candidate_refs = Vec::new();
    for entry in &local_entries {
        let candidate = if !entry.upstream_full.is_empty() {
            entry.upstream_full.clone()
        } else {
            format!("refs/remotes/{}/{}", remote, entry.short_name)
        };
        if seen_candidates.insert(candidate.clone()) {
            candidate_refs.push(candidate);
        }
    }

    let remote_sha_map = resolve_ref_shas(git, &candidate_refs)?;
    let mut result = HashMap::with_capacity(local_entries.len());

    for entry in local_entries {
        let fallback_remote_short = format!("{}/{}", remote, entry.short_name);
        let fallback_remote_full = format!("refs/remotes/{}/{}", remote, entry.short_name);

        let (upstream, remote_sha) = if !entry.upstream_full.is_empty() {
            if let Some(sha) = remote_sha_map.get(&entry.upstream_full) {
                (Some(entry.upstream_short.clone()), Some(sha.clone()))
            } else {
                (None, None)
            }
        } else if let Some(sha) = remote_sha_map.get(&fallback_remote_full) {
            (Some(fallback_remote_short), Some(sha.clone()))
        } else {
            (None, None)
        };

        let status = match (&upstream, &remote_sha) {
            (None, _) | (_, None) => SyncStatus::NoRemote,
            (Some(_), Some(r_sha)) => {
                if &entry.sha == r_sha {
                    SyncStatus::UpToDate
                } else if !entry.upstream_track.is_empty() && entry.upstream_track != "[gone]" {
                    let has_ahead = entry.upstream_track.contains("ahead ");
                    let has_behind = entry.upstream_track.contains("behind ");
                    match (has_ahead, has_behind) {
                        (true, true) => SyncStatus::Diverged,
                        (true, false) => SyncStatus::NeedPush,
                        (false, true) => SyncStatus::NeedPull,
                        (false, false) => SyncStatus::UpToDate,
                    }
                } else if git.is_ancestor(&entry.sha, r_sha) {
                    SyncStatus::NeedPull
                } else if git.is_ancestor(r_sha, &entry.sha) {
                    SyncStatus::NeedPush
                } else {
                    SyncStatus::Diverged
                }
            }
        };

        result.insert(
            entry.short_name.clone(),
            BranchInfo {
                name: entry.short_name,
                fullname: entry.full_name,
                commit_hash: entry.sha,
                commit_time_relative: entry.committer_date_relative,
                upstream,
                status,
            },
        );
    }

    Ok(result)
}

/// Resolve the upstream ref for a branch name (`<remote>/<branch>` if it exists, or `<branch>`).
pub fn guess_upstream(git: &GitRunner, branch: &str, remote: &str) -> Option<String> {
    if let Ok(full) = git.git(&["rev-parse", "--symbolic-full-name", branch]) {
        if full.starts_with("refs/remotes/") {
            return Some(branch.to_string());
        }
    }

    let tracked = format!("{}@{{u}}", branch);
    if git
        .git(&["rev-parse", "--symbolic-full-name", &tracked])
        .is_ok()
    {
        return Some(tracked);
    }

    let remote_candidate = format!("remotes/{}/{}", remote, branch);
    if git
        .git(&["rev-parse", "--symbolic-full-name", &remote_candidate])
        .is_ok()
    {
        return Some(remote_candidate);
    }

    if git.branch_exists(branch, None) {
        return Some(branch.to_string());
    }

    None
}

/// Run `git branch --sort=committerdate --format=%(refname:short) [--merged|--no-merged] <target_ref>`.
pub fn list_branches_by_merge(
    git: &GitRunner,
    target_ref: &str,
    merged: bool,
    pattern: Option<&str>,
) -> Result<Vec<String>> {
    let mut args = vec![
        "--no-pager",
        "branch",
        "--sort=committerdate",
        "--format=%(refname:short)",
    ];

    if let Some(pat) = pattern {
        args.push("--remotes");
        args.push("--list");
        args.push(pat);
    }

    if merged {
        args.push("--merged");
    } else {
        args.push("--no-merged");
    }

    args.push(target_ref);

    let out = git.git(&args)?;
    let branches = out
        .lines()
        .map(str::trim)
        .filter(|b| !b.is_empty() && !b.ends_with("/HEAD"))
        .map(String::from)
        .collect();
    Ok(branches)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use tempfile::TempDir;

    fn run_git(dir: &Path, args: &[&str]) {
        let runner = GitRunner::new(dir, false);
        runner
            .git(args)
            .unwrap_or_else(|e| panic!("git {:?} failed: {}", args, e));
    }

    fn init_repo_with_remotes() -> (TempDir, GitRunner) {
        let tmp = TempDir::new().unwrap();
        let origin_dir = tmp.path().join("origin.git");
        let upstream_dir = tmp.path().join("upstream.git");
        let work_dir = tmp.path().join("work");

        std::fs::create_dir_all(&origin_dir).unwrap();
        std::fs::create_dir_all(&upstream_dir).unwrap();
        std::fs::create_dir_all(&work_dir).unwrap();

        run_git(&origin_dir, &["init", "--bare", "-b", "main"]);
        run_git(&upstream_dir, &["init", "--bare", "-b", "main"]);

        run_git(&work_dir, &["init", "-b", "main"]);
        run_git(&work_dir, &["config", "user.name", "Test"]);
        run_git(&work_dir, &["config", "user.email", "test@example.com"]);
        run_git(&work_dir, &["config", "commit.gpgsign", "false"]);
        run_git(
            &work_dir,
            &["remote", "add", "origin", origin_dir.to_str().unwrap()],
        );
        run_git(
            &work_dir,
            &["remote", "add", "upstream", upstream_dir.to_str().unwrap()],
        );

        std::fs::write(work_dir.join("README.md"), "init\n").unwrap();
        run_git(&work_dir, &["add", "."]);
        run_git(&work_dir, &["commit", "-m", "initial"]);
        run_git(&work_dir, &["push", "-u", "origin", "main"]);
        run_git(&work_dir, &["push", "upstream", "main"]);

        let git = GitRunner::new(&work_dir, false);
        (tmp, git)
    }

    fn commit_file(dir: &Path, name: &str, content: &str, msg: &str) {
        std::fs::write(dir.join(name), content).unwrap();
        run_git(dir, &["add", name]);
        run_git(dir, &["commit", "-m", msg]);
    }

    #[test]
    fn test_collect_branches_status_local_remote_and_filtered() {
        let (_tmp, git) = init_repo_with_remotes();
        let work_dir = git.cwd();

        // Ahead of origin/ahead-branch.
        run_git(work_dir, &["checkout", "-b", "ahead-branch", "main"]);
        run_git(work_dir, &["push", "-u", "origin", "ahead-branch"]);
        commit_file(work_dir, "ahead.txt", "1\n", "ahead commit");

        // Behind origin/behind-branch.
        run_git(work_dir, &["checkout", "-b", "behind-branch", "main"]);
        commit_file(work_dir, "behind.txt", "1\n", "remote commit");
        run_git(work_dir, &["push", "-u", "origin", "behind-branch"]);
        run_git(work_dir, &["reset", "--hard", "HEAD~1"]);

        // Diverged from origin/diverged-branch.
        run_git(work_dir, &["checkout", "-b", "diverged-branch", "main"]);
        commit_file(work_dir, "div-r.txt", "1\n", "remote side");
        run_git(work_dir, &["push", "-u", "origin", "diverged-branch"]);
        run_git(work_dir, &["reset", "--hard", "HEAD~1"]);
        commit_file(work_dir, "div-l.txt", "2\n", "local side");

        // Local-only branch without remote ref.
        run_git(work_dir, &["checkout", "-b", "local-only", "main"]);

        // Untracked local branch whose matching origin/<name> exists.
        run_git(
            work_dir,
            &["checkout", "-b", "untracked-with-remote", "main"],
        );
        run_git(work_dir, &["push", "origin", "untracked-with-remote"]);

        // Branch whose tracked remote ref was deleted ([gone]).
        run_git(work_dir, &["checkout", "-b", "gone-branch", "main"]);
        run_git(work_dir, &["push", "-u", "origin", "gone-branch"]);
        run_git(work_dir, &["push", "origin", "--delete", "gone-branch"]);

        // Branch tracking a non-default remote (upstream/main).
        run_git(work_dir, &["checkout", "-b", "tracks-upstream", "main"]);
        run_git(
            work_dir,
            &[
                "branch",
                "--set-upstream-to=upstream/main",
                "tracks-upstream",
            ],
        );

        // Remote-only branch on origin that has no local branch.
        run_git(work_dir, &["checkout", "-b", "unrelated-remote", "main"]);
        run_git(work_dir, &["push", "origin", "unrelated-remote"]);
        run_git(work_dir, &["checkout", "main"]);
        run_git(work_dir, &["branch", "-D", "unrelated-remote"]);

        // Branch under a namespace prefix so querying `"feat"` alone must not match `"feat/sub"`.
        run_git(work_dir, &["branch", "feat/sub", "main"]);

        let all_local = collect_branches_status(&git, "origin", None).unwrap();
        assert_eq!(all_local["main"].status, SyncStatus::UpToDate);
        assert_eq!(all_local["main"].upstream.as_deref(), Some("origin/main"));
        assert_eq!(all_local["ahead-branch"].status, SyncStatus::NeedPush);
        assert_eq!(all_local["behind-branch"].status, SyncStatus::NeedPull);
        assert_eq!(all_local["diverged-branch"].status, SyncStatus::Diverged);
        assert_eq!(all_local["local-only"].status, SyncStatus::NoRemote);
        assert_eq!(
            all_local["untracked-with-remote"].status,
            SyncStatus::UpToDate
        );
        assert_eq!(
            all_local["untracked-with-remote"].upstream.as_deref(),
            Some("origin/untracked-with-remote")
        );
        assert_eq!(all_local["gone-branch"].status, SyncStatus::NoRemote);
        assert_eq!(all_local["tracks-upstream"].status, SyncStatus::UpToDate);
        assert_eq!(
            all_local["tracks-upstream"].upstream.as_deref(),
            Some("upstream/main")
        );
        assert!(!all_local.contains_key("origin/unrelated-remote"));

        // Filtered query: empty list returns empty map.
        assert!(collect_branches_status_for(&git, "origin", &[])
            .unwrap()
            .is_empty());

        // Filtered query: exact match excludes prefix child `feat/sub` and missing branches.
        let filtered = collect_branches_status_for(
            &git,
            "origin",
            &["feat", "ahead-branch", "does-not-exist"],
        )
        .unwrap();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered["ahead-branch"].status, SyncStatus::NeedPush);
        assert!(!filtered.contains_key("feat/sub"));

        // Pattern mode returns remote branches with RemoteBranch status.
        let remote_only = collect_branches_status(&git, "origin", Some("*")).unwrap();
        assert_eq!(
            remote_only["origin/unrelated-remote"].status,
            SyncStatus::RemoteBranch
        );
        assert!(!remote_only.contains_key("origin/HEAD"));

        // Secondary remote ref resolves directly even when default remote is `origin`.
        assert_eq!(
            guess_upstream(&git, "upstream/main", "origin").as_deref(),
            Some("upstream/main")
        );
    }
}
