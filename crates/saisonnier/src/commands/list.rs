use crate::{
    commands::{reconcile_running_receipts, ExecutionContext},
    parcelle,
    store::{
        format_duration, now_unix_secs, parse_iso8601_utc_secs, QueuedTask, RunReceipt, RunStatus,
    },
};
use anyhow::Result;
use clap::Args;
use cli_core::ui::color_str;
use git_core::worktree::list_agent_markers;
use serde::Serialize;

#[derive(Args, Debug, Clone, Default)]
pub struct ListArgs {
    /// Output machine-readable JSON
    #[arg(long = "json")]
    pub json: bool,

    /// Maximum number of recent runs to display
    #[arg(short = 'n', long = "limit", default_value_t = 20)]
    pub limit: usize,

    /// Show all recent runs (ignores --limit)
    #[arg(short = 'a', long = "all")]
    pub all: bool,

    /// Target remote parcelle configured under [parcelle.<name>]
    #[arg(long = "parcelle")]
    pub parcelle: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ActiveRunView {
    #[serde(flatten)]
    pub receipt: RunReceipt,
    pub marker: Option<String>,
    pub elapsed_secs: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct StatusReport {
    pub queued: Vec<QueuedTask>,
    pub active: Vec<ActiveRunView>,
    pub recent: Vec<RunReceipt>,
}

pub fn execute(ctx: &ExecutionContext, args: &ListArgs) -> Result<()> {
    if let Some(ref parcelle_name) = args.parcelle {
        let mut forwarded = vec!["list".to_string()];
        if args.json {
            forwarded.push("--json".to_string());
        }
        if args.all {
            forwarded.push("--all".to_string());
        } else {
            forwarded.push("--limit".to_string());
            forwarded.push(args.limit.to_string());
        }
        return parcelle::relay_command(&ctx.config, parcelle_name, &forwarded, None, false);
    }

    {
        let _lock = ctx.store.acquire_tick_lock()?;
        reconcile_running_receipts(ctx)?;
    }

    let queued = ctx.store.list_tasks()?;
    let receipts = ctx.store.list_receipts()?;
    let markers = list_agent_markers(&ctx.git);
    let now = now_unix_secs();

    let mut active = Vec::new();
    let mut recent = Vec::new();
    for receipt in receipts {
        if receipt.status == RunStatus::Running {
            let marker = markers.get(&receipt.cep).cloned();
            let elapsed_secs =
                parse_iso8601_utc_secs(&receipt.started_at).map(|start| now.saturating_sub(start));
            active.push(ActiveRunView {
                receipt,
                marker,
                elapsed_secs,
            });
        } else {
            recent.push(receipt);
        }
    }

    if !args.all && recent.len() > args.limit {
        recent.truncate(args.limit);
    }

    if args.json {
        let report = StatusReport {
            queued,
            active,
            recent,
        };
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }

    if queued.is_empty() && active.is_empty() && recent.is_empty() {
        println!("No queued tasks, active saisonniers, or past runs.");
        return Ok(());
    }

    let mut printed_section = false;

    if !active.is_empty() {
        println!("{}", color_str("Active saisonniers:", "bold"));
        for item in &active {
            let r = &item.receipt;
            let marker = item.marker.as_deref().unwrap_or("🤖");
            let elapsed = item
                .elapsed_secs
                .map(format_duration)
                .unwrap_or_else(|| "-".to_string());
            println!(
                "  {} {}  {}  {}  [{}]  {}",
                marker,
                color_str(&r.run_id, "cyan"),
                color_str(&r.cep, "green"),
                r.branch,
                r.profile,
                color_str(&elapsed, "grey"),
            );
        }
        printed_section = true;
    }

    if !queued.is_empty() {
        if printed_section {
            println!();
        }
        println!("{}", color_str("Queued tasks:", "bold"));
        for task in &queued {
            let branch = task.branch.as_deref().unwrap_or("(auto)");
            let profile = task
                .profile
                .as_deref()
                .unwrap_or(ctx.config.default_profile.as_str());
            let preview = prompt_preview(&task.prompt, 60);
            println!(
                "  • {}  {}  [{}]  {}",
                color_str(&task.task_id, "cyan"),
                color_str(branch, "yellow"),
                profile,
                color_str(&preview, "grey"),
            );
        }
        printed_section = true;
    }

    if !recent.is_empty() {
        if printed_section {
            println!();
        }
        println!("{}", color_str("Recent runs:", "bold"));
        for r in &recent {
            let badge = color_str(r.status.badge(), r.status.color());
            let status_label = color_str(r.status.as_str(), r.status.color());
            let commit_state = format_commit_state(r);
            let duration = format_run_duration(r);
            println!(
                "  {} {}  {:<10}  {}  {}  ({})  {}",
                badge,
                color_str(&r.run_id, "cyan"),
                status_label,
                color_str(&r.cep, "green"),
                r.branch,
                commit_state,
                color_str(&duration, "grey"),
            );
        }
    }

    Ok(())
}

fn prompt_preview(prompt: &str, max_chars: usize) -> String {
    let first_line = prompt
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("");
    let mut out: String = first_line.chars().take(max_chars).collect();
    if first_line.chars().count() > max_chars {
        out.push_str("...");
    }
    out
}

fn format_commit_state(receipt: &RunReceipt) -> String {
    let start_short = short_sha(&receipt.start_sha);
    match receipt.end_sha.as_deref() {
        Some(end) if end != receipt.start_sha => {
            format!("{}..{}", start_short, short_sha(end))
        }
        Some(_) => "no commits".to_string(),
        None => "no end sha".to_string(),
    }
}

fn short_sha(sha: &str) -> &str {
    &sha[..sha.len().min(7)]
}

fn format_run_duration(receipt: &RunReceipt) -> String {
    let start = parse_iso8601_utc_secs(&receipt.started_at);
    let end = receipt
        .finished_at
        .as_deref()
        .and_then(parse_iso8601_utc_secs);
    match (start, end) {
        (Some(s), Some(e)) => format_duration(e.saturating_sub(s)),
        _ => "-".to_string(),
    }
}
