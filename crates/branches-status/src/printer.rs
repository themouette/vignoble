use cli_core::ui::{
    color_str, DIVERGED_SYMBOL, NEED_PULL_SYMBOL, NEED_PUSH_SYMBOL, UNKNOWN_UPSTREAM_SYMBOL,
    UPSTREAM_ONLY_SYMBOL, UP_TO_DATE_SYMBOL,
};
use git_core::{BranchInfo, StageBranchEntry, SyncStatus, WorktreeInfo};
use std::collections::HashMap;

pub fn pretty_print_block(
    title: &str,
    entries: &[StageBranchEntry],
    color: &str,
    branches_info: &HashMap<String, BranchInfo>,
    worktrees_by_branch: &HashMap<String, WorktreeInfo>,
) {
    if !title.is_empty() {
        println!("{}", title);
    }

    if entries.is_empty() {
        println!("  {}", color_str("None", color));
        println!();
        return;
    }

    let max_len = entries.iter().map(|e| e.branch.len()).max().unwrap_or(0) + 1;

    for entry in entries {
        println!(
            "{}",
            format_branch_line(
                entry,
                color,
                max_len,
                branches_info.get(&entry.branch),
                worktrees_by_branch.get(&entry.branch),
            )
        );
    }

    println!();
}

pub fn format_branch_line(
    entry: &StageBranchEntry,
    color: &str,
    ljust: usize,
    info: Option<&BranchInfo>,
    worktree: Option<&WorktreeInfo>,
) -> String {
    let padded_name = format!("{:<width$}", entry.branch, width = ljust);
    let colored_name = color_str(&padded_name, color);

    let (symbol, status_note, commit_age) = match info {
        Some(b) => {
            let (sym, note) = match b.status {
                SyncStatus::NoRemote => {
                    (color_str(UNKNOWN_UPSTREAM_SYMBOL, "red"), "(no upstream)")
                }
                SyncStatus::RemoteBranch => (color_str(UPSTREAM_ONLY_SYMBOL, "green"), ""),
                SyncStatus::UpToDate => (color_str(UP_TO_DATE_SYMBOL, "green"), ""),
                SyncStatus::NeedPull => (
                    color_str(NEED_PULL_SYMBOL, "orange"),
                    "(new remote commits)",
                ),
                SyncStatus::NeedPush => {
                    (color_str(NEED_PUSH_SYMBOL, "orange"), "(new local commits)")
                }
                SyncStatus::Diverged => (color_str(DIVERGED_SYMBOL, "red"), ""),
            };
            (sym, note, format!("({})", b.commit_time_relative))
        }
        None => (
            color_str(UNKNOWN_UPSTREAM_SYMBOL, "red"),
            "(no upstream)",
            "()".to_string(),
        ),
    };

    let mut extras = Vec::new();
    if !status_note.is_empty() {
        extras.push(status_note.to_string());
    }
    if let Some(wt) = worktree {
        let wt_badge = if wt.is_current {
            color_str(&format!("[wt: {} (current)]", wt.short_name), "cyan")
        } else {
            color_str(&format!("[wt: {}]", wt.short_name), "cyan")
        };
        extras.push(wt_badge);
    }
    if let Some(annotation) = &entry.annotation {
        extras.push(color_str(annotation, "grey"));
    }

    let suffix = if extras.is_empty() {
        String::new()
    } else {
        format!(" {}", extras.join(" "))
    };

    format!("  {} {} {}{}", colored_name, symbol, commit_age, suffix)
}
