use crate::commands::ExecutionContext;
use anyhow::Result;
use cli_core::ui::{
    color_str, DIVERGED_SYMBOL, NEED_PULL_SYMBOL, NEED_PUSH_SYMBOL, UNKNOWN_UPSTREAM_SYMBOL,
    UPSTREAM_ONLY_SYMBOL, UP_TO_DATE_SYMBOL,
};
use console::{measure_text_width, pad_str, Alignment};
use git_core::{
    refs::collect_branches_status,
    worktree::{get_agent_marker, list_worktrees},
    SyncStatus, WorktreeHead,
};

struct Row {
    current: String,
    name: String,
    head: String,
    claim: String,
    path: String,
}

pub fn execute(ctx: &ExecutionContext) -> Result<()> {
    let worktrees = list_worktrees(&ctx.git)?;
    let branches_info = collect_branches_status(&ctx.git, "origin", None).unwrap_or_default();

    let mut rows = Vec::with_capacity(worktrees.len());
    for wt in &worktrees {
        let current = if wt.is_current {
            color_str("@", "green")
        } else {
            " ".to_string()
        };

        let name = color_str(&wt.short_name, "cyan");

        let head = match &wt.head {
            WorktreeHead::Branch(branch) => {
                let sym = match branches_info.get(branch).map(|b| b.status) {
                    Some(SyncStatus::UpToDate) => color_str(UP_TO_DATE_SYMBOL.trim(), "green"),
                    Some(SyncStatus::NeedPush) => color_str(NEED_PUSH_SYMBOL.trim(), "orange"),
                    Some(SyncStatus::NeedPull) => color_str(NEED_PULL_SYMBOL.trim(), "orange"),
                    Some(SyncStatus::Diverged) => color_str(DIVERGED_SYMBOL.trim(), "red"),
                    Some(SyncStatus::RemoteBranch) => {
                        color_str(UPSTREAM_ONLY_SYMBOL.trim(), "green")
                    }
                    Some(SyncStatus::NoRemote) | None => {
                        color_str(UNKNOWN_UPSTREAM_SYMBOL.trim(), "red")
                    }
                };
                format!("{} {}", color_str(branch, "bold"), sym)
            }
            WorktreeHead::Detached => {
                let short_sha = &wt.head_sha[..wt.head_sha.len().min(7)];
                color_str(&format!("(detached @ {})", short_sha), "grey")
            }
            WorktreeHead::Bare => color_str("(bare)", "grey"),
        };

        let mut claim = if wt.is_main {
            color_str("(main)", "grey")
        } else if let Some(reason) = &wt.locked {
            if reason.is_empty() || reason == "claimed" {
                color_str("🔒 claimed", "orange")
            } else {
                color_str(&format!("🔒 claimed ({})", reason), "orange")
            }
        } else {
            color_str("available", "green")
        };

        if let Some(marker) = get_agent_marker(&ctx.git, &wt.short_name) {
            claim = format!("{} {}", claim, marker);
        }

        let path = color_str(&wt.path.display().to_string(), "grey");

        rows.push(Row {
            current,
            name,
            head,
            claim,
            path,
        });
    }

    let name_width = rows
        .iter()
        .map(|r| measure_text_width(&r.name))
        .max()
        .unwrap_or(0);
    let head_width = rows
        .iter()
        .map(|r| measure_text_width(&r.head))
        .max()
        .unwrap_or(0);
    let claim_width = rows
        .iter()
        .map(|r| measure_text_width(&r.claim))
        .max()
        .unwrap_or(0);

    for row in rows {
        println!(
            "{} {}  {}  {}  {}",
            row.current,
            pad_str(&row.name, name_width, Alignment::Left, None),
            pad_str(&row.head, head_width, Alignment::Left, None),
            pad_str(&row.claim, claim_width, Alignment::Left, None),
            row.path,
        );
    }

    Ok(())
}
