use crate::{
    commands::ExecutionContext,
    parcelle,
    store::{QueuedTask, RunReceipt},
};
use anyhow::{bail, Result};
use clap::Args;
use cli_core::ui::color_str;
use serde::Serialize;
use std::fs;

#[derive(Args, Debug, Clone)]
pub struct InspectArgs {
    /// Run ID or queued task ID to inspect
    pub id: String,

    /// Number of trailing log lines to display
    #[arg(short = 'n', long = "lines", default_value_t = 50)]
    pub lines: usize,

    /// Output machine-readable JSON
    #[arg(long = "json")]
    pub json: bool,

    /// Target remote parcelle configured under [parcelle.<name>]
    #[arg(long = "parcelle")]
    pub parcelle: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum InspectOutput {
    Run {
        receipt: Box<RunReceipt>,
        prompt: Option<String>,
        log_tail: Option<String>,
    },
    Queued {
        task: QueuedTask,
    },
}

pub fn execute(ctx: &ExecutionContext, args: &InspectArgs) -> Result<()> {
    if let Some(ref parcelle_name) = args.parcelle {
        let mut forwarded = vec![
            "inspect".to_string(),
            args.id.clone(),
            "--lines".to_string(),
            args.lines.to_string(),
        ];
        if args.json {
            forwarded.push("--json".to_string());
        }
        return parcelle::relay_command(&ctx.config, parcelle_name, &forwarded, None, false);
    }

    if let Some(receipt) = ctx.store.find_receipt(&args.id)? {
        let prompt = fs::read_to_string(&receipt.prompt_file).ok();
        let log_tail = fs::read_to_string(&receipt.log_path)
            .ok()
            .map(|content| tail_lines(&content, args.lines));

        if args.json {
            let output = InspectOutput::Run {
                receipt: Box::new(receipt),
                prompt,
                log_tail,
            };
            println!("{}", serde_json::to_string_pretty(&output)?);
            return Ok(());
        }

        let badge = color_str(receipt.status.badge(), receipt.status.color());
        let status_str = color_str(receipt.status.as_str(), receipt.status.color());
        println!(
            "{} {} ({})",
            badge,
            color_str(&receipt.run_id, "bold"),
            status_str
        );
        println!("  Task ID:     {}", receipt.task_id);
        println!("  Cep:         {}", color_str(&receipt.cep, "cyan"));
        println!("  Branch:      {}", receipt.branch);
        if let Some(ref base) = receipt.base {
            println!("  Base:        {}", base);
        }
        let commit_range = match receipt.end_sha.as_deref() {
            Some(end) => format!("{}..{}", short_sha(&receipt.start_sha), short_sha(end)),
            None => format!("{}..(in progress)", short_sha(&receipt.start_sha)),
        };
        println!("  Commits:     {}", commit_range);
        println!("  Profile:     {}", receipt.profile);
        println!("  Started:     {}", receipt.started_at);
        if let Some(ref finished) = receipt.finished_at {
            println!("  Finished:    {}", finished);
        }
        if let Some(code) = receipt.exit_code {
            println!("  Exit code:   {}", code);
        }
        println!("  Log:         {}", receipt.log_path.display());
        println!(
            "  Session:     {} ({})",
            receipt.session_path.display(),
            receipt.session_id
        );

        if let Some(ref text) = prompt {
            println!("\n{}", color_str("Prompt:", "bold"));
            println!("{}", text.trim_end());
        }

        if let Some(ref tail) = log_tail {
            println!(
                "\n{}",
                color_str(&format!("Log tail (last {} lines):", args.lines), "bold")
            );
            if tail.trim().is_empty() {
                println!("  (empty)");
            } else {
                println!("{}", tail);
            }
        }

        return Ok(());
    }

    if let Some(task) = ctx.store.find_task(&args.id)? {
        if args.json {
            let output = InspectOutput::Queued { task };
            println!("{}", serde_json::to_string_pretty(&output)?);
            return Ok(());
        }

        println!(
            "• {} ({})",
            color_str(&task.task_id, "bold"),
            color_str("queued", "yellow")
        );
        println!("  Created:     {}", task.created_at);
        println!(
            "  Branch:      {}",
            task.branch.as_deref().unwrap_or("(auto)")
        );
        if let Some(ref base) = task.base {
            println!("  Base:        {}", base);
        }
        println!(
            "  Profile:     {}",
            task.profile
                .as_deref()
                .unwrap_or(ctx.config.default_profile.as_str())
        );
        println!("\n{}", color_str("Prompt:", "bold"));
        println!("{}", task.prompt.trim_end());
        return Ok(());
    }

    bail!("No run or queued task found matching '{}'", args.id)
}

fn short_sha(sha: &str) -> &str {
    &sha[..sha.len().min(12)]
}

fn tail_lines(content: &str, max_lines: usize) -> String {
    if max_lines == 0 {
        return String::new();
    }
    let lines: Vec<&str> = content.lines().collect();
    let start = lines.len().saturating_sub(max_lines);
    lines[start..].join("\n")
}
