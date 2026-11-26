use crate::runner::GitRunner;
use strsim::normalized_levenshtein;

/// Search for a squash-merge commit on `reference_ref` that matches `branch`.
/// Returns `Some(commit_sha)` when a commit on `reference_ref` contains all commit subjects from
/// `branch` and its patch similarity exceeds `minimum_similarity`.
pub fn search_for_squash_commit(
    git: &GitRunner,
    branch: &str,
    reference_ref: &str,
    minimum_similarity: f64,
) -> Option<String> {
    let common_ancestor = git.git(&["merge-base", reference_ref, branch]).ok()?;
    if common_ancestor.is_empty() {
        return None;
    }

    let range_branch = format!("{}..{}", common_ancestor, branch);
    let subjects_raw = git
        .git(&["--no-pager", "log", "--pretty=format:%s", &range_branch])
        .ok()?;
    let subjects: Vec<&str> = subjects_raw
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();

    if subjects.is_empty() {
        return None;
    }

    let mut log_args: Vec<String> = vec![
        "--no-pager".to_string(),
        "log".to_string(),
        "--fixed-strings".to_string(),
        "--all-match".to_string(),
        "--format=%H".to_string(),
    ];
    for subject in &subjects {
        log_args.push(format!("--grep={}", subject));
    }
    log_args.push(format!("{}..{}", common_ancestor, reference_ref));

    let log_arg_refs: Vec<&str> = log_args.iter().map(String::as_str).collect();
    let commits_raw = git.git(&log_arg_refs).ok()?;
    let candidate_commits: Vec<&str> = commits_raw
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();

    if candidate_commits.is_empty() {
        return None;
    }

    let branch_patch = git
        .git(&[
            "--no-pager",
            "diff",
            "--no-ext-diff",
            "--unified=0",
            &range_branch,
        ])
        .ok()?;

    let mut best_similarity = 0.0_f64;
    let mut selected_commit: Option<String> = None;

    for commit in candidate_commits {
        let commit_patch = match git.git(&[
            "--no-pager",
            "show",
            "--format=",
            "--patch",
            "--unified=0",
            commit,
        ]) {
            Ok(p) => p,
            Err(_) => continue,
        };

        if branch_patch == commit_patch {
            return Some(commit.to_string());
        }

        let similarity = normalized_levenshtein(&branch_patch, &commit_patch);
        if similarity > best_similarity {
            best_similarity = similarity;
            selected_commit = Some(commit.to_string());
        }
    }

    if best_similarity >= minimum_similarity {
        selected_commit
    } else {
        None
    }
}
