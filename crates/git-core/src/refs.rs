use crate::{error::Result, runner::GitRunner};
use std::collections::HashMap;

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
    upstream_track: String,
    committer_date_relative: String,
}

const REF_FORMAT: &str =
    "%(refname:short)|%(refname)|%(objectname)|%(upstream:short)|%(upstream:track)|%(committerdate:relative)";

fn parse_for_each_ref(output: &str) -> Vec<RawRefEntry> {
    let mut entries = Vec::new();
    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let parts: Vec<&str> = trimmed.splitn(6, '|').collect();
        if parts.len() < 6 {
            continue;
        }
        entries.push(RawRefEntry {
            short_name: parts[0].to_string(),
            full_name: parts[1].to_string(),
            sha: parts[2].to_string(),
            upstream_short: parts[3].to_string(),
            upstream_track: parts[4].to_string(),
            committer_date_relative: parts[5].to_string(),
        });
    }
    entries
}

/// Collect status for all local branches (or remote branches matching `pattern`).
pub fn collect_branches_status(
    git: &GitRunner,
    remote: &str,
    pattern: Option<&str>,
) -> Result<HashMap<String, BranchInfo>> {
    let remote_ref_prefix = format!("refs/remotes/{}", remote);
    let fmt_arg = format!("--format={}", REF_FORMAT);

    let remote_out = git.git(&[
        "--no-pager",
        "for-each-ref",
        "--sort=committerdate",
        &fmt_arg,
        &remote_ref_prefix,
    ])?;
    let remote_entries = parse_for_each_ref(&remote_out);
    let mut remote_sha_map: HashMap<String, String> = HashMap::new();
    for entry in &remote_entries {
        if entry.short_name == format!("{}/HEAD", remote) || entry.short_name == remote {
            continue;
        }
        remote_sha_map.insert(entry.short_name.clone(), entry.sha.clone());
    }

    let mut result = HashMap::new();

    if pattern.is_some() {
        for entry in remote_entries {
            if entry.short_name == format!("{}/HEAD", remote) || entry.short_name == remote {
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

    let local_out = git.git(&[
        "--no-pager",
        "for-each-ref",
        "--sort=committerdate",
        &fmt_arg,
        "refs/heads",
    ])?;
    let local_entries = parse_for_each_ref(&local_out);

    for entry in local_entries {
        let fallback_remote_branch = format!("{}/{}", remote, entry.short_name);

        let (upstream, remote_sha) = if !entry.upstream_short.is_empty() {
            if let Some(sha) = remote_sha_map.get(&entry.upstream_short) {
                (Some(entry.upstream_short.clone()), Some(sha.clone()))
            } else if let Ok(sha) = git.git(&["rev-parse", &entry.upstream_short]) {
                (Some(entry.upstream_short.clone()), Some(sha))
            } else {
                (None, None)
            }
        } else if let Some(sha) = remote_sha_map.get(&fallback_remote_branch) {
            (Some(fallback_remote_branch), Some(sha.clone()))
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
        if full.starts_with(&format!("refs/remotes/{}", remote)) {
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
