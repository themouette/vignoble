use crate::{
    error::{GitError, Result},
    refs::{guess_upstream, list_branches_by_merge},
    runner::{split_remote_branch, GitRunner},
    squash::search_for_squash_commit,
};
use std::collections::HashSet;
use std::process::Command;
use std::thread;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StageMode {
    /// Static git branch name (e.g. `prod`, `preprod`, `main`).
    Branch(String),
    /// Shell script executed once; stdout provides the target branch name.
    BranchScript(String),
    /// Shell script executed per candidate branch (`$1`); exit code 0 matches the stage.
    CheckScript(String),
}

#[derive(Debug, Clone)]
pub struct StageDefinition {
    pub name: String,
    pub label: String,
    pub color: String,
    pub mode: StageMode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StageBranchEntry {
    pub branch: String,
    pub annotation: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ResolvedStage {
    pub name: String,
    pub label: String,
    pub color: String,
    pub reference: Option<String>,
    pub branches: Vec<StageBranchEntry>,
}

pub fn resolve_waterfall_stages(
    git: &GitRunner,
    remote: &str,
    pattern: Option<&str>,
    enable_squash: bool,
    definitions: &[StageDefinition],
) -> Result<Vec<ResolvedStage>> {
    let mut resolved_stages = Vec::new();
    let mut reference_branches: HashSet<String> = HashSet::new();
    let mut claimed_branches: HashSet<String> = HashSet::new();
    let mut first_valid_target_ref: Option<String> = None;
    let known_remotes = git.remotes().unwrap_or_default();

    // Pre-collect all candidate branches in committerdate order.
    let all_candidates = list_all_candidates(git, remote, &known_remotes, pattern)?;

    // Register all stage target branches up front so a lower stage branch (such as `preprod`)
    // is never listed as a feature branch inside an earlier stage (such as `prod`).
    let mut stage_targets: Vec<(usize, Option<String>)> = Vec::with_capacity(definitions.len());
    for (idx, def) in definitions.iter().enumerate() {
        match &def.mode {
            StageMode::Branch(branch_name) => {
                if stage_branch_exists(git, branch_name, remote, &known_remotes) {
                    register_reference_branch(
                        &mut reference_branches,
                        branch_name,
                        remote,
                        &known_remotes,
                    );
                    stage_targets.push((idx, Some(branch_name.clone())));
                } else {
                    stage_targets.push((idx, None));
                }
            }
            StageMode::BranchScript(script) => {
                let resolved = match run_branch_script(git, script, remote) {
                    Ok(Some(b)) if stage_branch_exists(git, &b, remote, &known_remotes) => {
                        register_reference_branch(
                            &mut reference_branches,
                            &b,
                            remote,
                            &known_remotes,
                        );
                        Some(b)
                    }
                    Ok(_) => None,
                    Err(e) => {
                        if git.debug() {
                            eprintln!("Warning: stage '{}' branch_script failed: {}", def.name, e);
                        }
                        None
                    }
                };
                stage_targets.push((idx, resolved));
            }
            StageMode::CheckScript(_) => {
                stage_targets.push((idx, None));
            }
        }
    }

    for (idx, def) in definitions.iter().enumerate() {
        match &def.mode {
            StageMode::Branch(_) | StageMode::BranchScript(_) => {
                let target_branch = match &stage_targets[idx].1 {
                    Some(b) => b,
                    None => continue,
                };
                if let Some(stage) = evaluate_branch_stage(
                    git,
                    def,
                    target_branch,
                    remote,
                    &known_remotes,
                    pattern,
                    enable_squash,
                    &mut reference_branches,
                    &mut claimed_branches,
                    &mut first_valid_target_ref,
                )? {
                    resolved_stages.push(stage);
                }
            }
            StageMode::CheckScript(script) => {
                let remaining: Vec<String> = all_candidates
                    .iter()
                    .filter(|b| !claimed_branches.contains(*b) && !reference_branches.contains(*b))
                    .cloned()
                    .collect();

                let matched = run_check_script_parallel(git, script, remote, &remaining);
                for entry in &matched {
                    claimed_branches.insert(entry.branch.clone());
                }

                resolved_stages.push(ResolvedStage {
                    name: def.name.clone(),
                    label: def.label.clone(),
                    color: def.color.clone(),
                    reference: None,
                    branches: matched,
                });
            }
        }
    }

    // Reverse stage order to match original branches-status output:
    // dev (Unmerged) -> staging -> preprod -> prod
    resolved_stages.reverse();

    let dev_branches: Vec<StageBranchEntry> = all_candidates
        .into_iter()
        .filter(|b| !claimed_branches.contains(b) && !reference_branches.contains(b))
        .map(|branch| StageBranchEntry {
            branch,
            annotation: None,
        })
        .collect();

    let mut final_output = Vec::with_capacity(resolved_stages.len() + 1);
    final_output.push(ResolvedStage {
        name: "dev".to_string(),
        label: "Unmerged".to_string(),
        color: "blue".to_string(),
        reference: None,
        branches: dev_branches,
    });
    final_output.extend(resolved_stages);

    Ok(final_output)
}

#[allow(clippy::too_many_arguments)]
fn evaluate_branch_stage(
    git: &GitRunner,
    def: &StageDefinition,
    target_branch: &str,
    remote: &str,
    known_remotes: &[String],
    pattern: Option<&str>,
    enable_squash: bool,
    reference_branches: &mut HashSet<String>,
    claimed_branches: &mut HashSet<String>,
    first_valid_target_ref: &mut Option<String>,
) -> Result<Option<ResolvedStage>> {
    let upstream_ref = match guess_upstream(git, target_branch, remote) {
        Some(r) => r,
        None => return Ok(None),
    };

    if first_valid_target_ref.is_none() {
        *first_valid_target_ref = Some(upstream_ref.clone());
    }

    register_reference_branch(reference_branches, target_branch, remote, known_remotes);

    let merged = list_branches_by_merge(git, &upstream_ref, true, pattern)?;
    let mut stage_entries = Vec::new();

    for b in merged {
        if b != target_branch && !reference_branches.contains(&b) && !claimed_branches.contains(&b)
        {
            claimed_branches.insert(b.clone());
            stage_entries.push(StageBranchEntry {
                branch: b,
                annotation: None,
            });
        }
    }

    if enable_squash {
        let unmerged = list_branches_by_merge(git, &upstream_ref, false, pattern)?;
        let squash_ref = if split_remote_branch(target_branch, known_remotes).is_some() {
            target_branch.to_string()
        } else if git.branch_exists(target_branch, Some(remote)) {
            format!("{}/{}", remote, target_branch)
        } else {
            target_branch.to_string()
        };

        for b in unmerged {
            if b != target_branch
                && !reference_branches.contains(&b)
                && !claimed_branches.contains(&b)
                && search_for_squash_commit(git, &b, &squash_ref, 0.9).is_some()
            {
                claimed_branches.insert(b.clone());
                stage_entries.push(StageBranchEntry {
                    branch: b,
                    annotation: Some("(squashed)".to_string()),
                });
            }
        }
    }

    Ok(Some(ResolvedStage {
        name: def.name.clone(),
        label: def.label.clone(),
        color: def.color.clone(),
        reference: Some(target_branch.to_string()),
        branches: stage_entries,
    }))
}

fn register_reference_branch(
    reference_branches: &mut HashSet<String>,
    branch: &str,
    remote: &str,
    known_remotes: &[String],
) {
    reference_branches.insert(branch.to_string());
    if let Some((stage_remote, short_branch)) = split_remote_branch(branch, known_remotes) {
        reference_branches.insert(short_branch.to_string());
        reference_branches.insert(format!("{}/{}", stage_remote, short_branch));
        reference_branches.insert(format!("{}/{}", remote, short_branch));
    } else {
        reference_branches.insert(format!("{}/{}", remote, branch));
    }
}

fn stage_branch_exists(
    git: &GitRunner,
    branch: &str,
    remote: &str,
    known_remotes: &[String],
) -> bool {
    if let Some((stage_remote, short_branch)) = split_remote_branch(branch, known_remotes) {
        return git.branch_exists(short_branch, Some(stage_remote))
            || git.branch_exists(branch, None);
    }
    git.branch_exists(branch, Some(remote)) || git.branch_exists(branch, None)
}

fn list_all_candidates(
    git: &GitRunner,
    remote: &str,
    known_remotes: &[String],
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
    let out = git.git(&args)?;
    let head_suffix = format!("{}/HEAD", remote);
    Ok(out
        .lines()
        .map(str::trim)
        .filter(|b| {
            !b.is_empty()
                && *b != head_suffix
                && !b.ends_with("/HEAD")
                && *b != remote
                && !known_remotes.iter().any(|r| r == *b)
        })
        .map(String::from)
        .collect())
}

fn run_branch_script(git: &GitRunner, script: &str, remote: &str) -> Result<Option<String>> {
    if git.debug() {
        println!("+ sh -c <branch_script>");
    }

    let output = Command::new("sh")
        .arg("-c")
        .arg(script)
        .current_dir(git.cwd())
        .env("GIT_REMOTE", remote)
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(GitError::StageError {
            stage: "branch_script".to_string(),
            message: stderr,
        });
    }

    let branch = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(String::from);
    Ok(branch)
}

fn run_check_script_parallel(
    git: &GitRunner,
    script: &str,
    remote: &str,
    candidates: &[String],
) -> Vec<StageBranchEntry> {
    thread::scope(|s| {
        let mut handles = Vec::with_capacity(candidates.len());
        for branch in candidates {
            let cwd = git.cwd().to_path_buf();
            handles.push((
                branch.clone(),
                s.spawn(move || {
                    let output = Command::new("sh")
                        .arg("-c")
                        .arg(script)
                        .arg("check_script")
                        .arg(branch)
                        .current_dir(cwd)
                        .env("BRANCH_NAME", branch)
                        .env("GIT_REMOTE", remote)
                        .output()
                        .ok()?;

                    if output.status.success() {
                        let note = String::from_utf8_lossy(&output.stdout)
                            .lines()
                            .map(str::trim)
                            .find(|l| !l.is_empty())
                            .map(String::from);
                        Some(note)
                    } else {
                        None
                    }
                }),
            ));
        }

        let mut matched = Vec::new();
        for (branch, handle) in handles {
            if let Ok(Some(annotation)) = handle.join() {
                matched.push(StageBranchEntry { branch, annotation });
            }
        }
        matched
    })
}

/// Find a resolved stage by name.
/// Supports `prod` aliasing to `main` or `master` when `prod` does not exist in the repository.
pub fn find_stage<'a>(stages: &'a [ResolvedStage], requested: &str) -> Option<&'a ResolvedStage> {
    if let Some(exact) = stages.iter().find(|s| s.name == requested) {
        return Some(exact);
    }

    if requested == "prod" {
        for fallback in ["main", "master"] {
            if let Some(stage) = stages.iter().find(|s| s.name == fallback) {
                return Some(stage);
            }
        }
    }

    None
}
