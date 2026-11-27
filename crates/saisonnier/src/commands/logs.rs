use crate::{commands::ExecutionContext, parcelle, store::RunStatus};
use anyhow::{anyhow, Context, Result};
use clap::Args;
use std::fs::{self, File};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::thread;
use std::time::Duration;

#[derive(Args, Debug, Clone)]
pub struct LogsArgs {
    /// Run ID (or prefix) whose log to view
    pub run_id: String,

    /// Follow log output as the saisonnier runs
    #[arg(short = 'f', long = "follow")]
    pub follow: bool,

    /// Show only the last N lines (shows entire log when omitted)
    #[arg(short = 'n', long = "lines")]
    pub lines: Option<usize>,

    /// Target remote parcelle configured under [parcelle.<name>]
    #[arg(long = "parcelle")]
    pub parcelle: Option<String>,
}

pub fn execute(ctx: &ExecutionContext, args: &LogsArgs) -> Result<()> {
    if let Some(ref parcelle_name) = args.parcelle {
        let mut forwarded = vec!["logs".to_string(), args.run_id.clone()];
        if args.follow {
            forwarded.push("--follow".to_string());
        }
        if let Some(n) = args.lines {
            forwarded.push("--lines".to_string());
            forwarded.push(n.to_string());
        }
        return parcelle::relay_command(&ctx.config, parcelle_name, &forwarded, None, args.follow);
    }

    let receipt = ctx
        .store
        .find_receipt(&args.run_id)?
        .ok_or_else(|| anyhow!("No run receipt found matching '{}'", args.run_id))?;

    let initial_content = if receipt.log_path.is_file() {
        fs::read_to_string(&receipt.log_path)
            .with_context(|| format!("Failed to read log file {}", receipt.log_path.display()))?
    } else {
        String::new()
    };

    let mut offset = initial_content.len() as u64;
    let to_print = match args.lines {
        Some(n) => tail_lines_with_newline(&initial_content, n),
        None => initial_content,
    };

    if !to_print.is_empty() {
        let mut stdout = io::stdout();
        stdout.write_all(to_print.as_bytes())?;
        stdout.flush()?;
    }

    if !args.follow {
        return Ok(());
    }

    loop {
        if let Ok(mut file) = File::open(&receipt.log_path) {
            let len = file.metadata().map(|m| m.len()).unwrap_or(0);
            if len > offset && file.seek(SeekFrom::Start(offset)).is_ok() {
                let mut buf = Vec::new();
                if file.read_to_end(&mut buf).is_ok() && !buf.is_empty() {
                    offset += buf.len() as u64;
                    let mut stdout = io::stdout();
                    stdout.write_all(&buf)?;
                    stdout.flush()?;
                }
            }
        }

        let current = ctx.store.read_receipt(&receipt.run_id)?;
        if current.status != RunStatus::Running {
            if let Ok(mut file) = File::open(&receipt.log_path) {
                let len = file.metadata().map(|m| m.len()).unwrap_or(0);
                if len > offset && file.seek(SeekFrom::Start(offset)).is_ok() {
                    let mut buf = Vec::new();
                    if file.read_to_end(&mut buf).is_ok() && !buf.is_empty() {
                        let mut stdout = io::stdout();
                        stdout.write_all(&buf)?;
                        stdout.flush()?;
                    }
                }
            }
            break;
        }

        thread::sleep(Duration::from_millis(150));
    }

    Ok(())
}

fn tail_lines_with_newline(content: &str, max_lines: usize) -> String {
    if max_lines == 0 || content.is_empty() {
        return String::new();
    }
    let lines: Vec<&str> = content.lines().collect();
    let start = lines.len().saturating_sub(max_lines);
    let mut joined = lines[start..].join("\n");
    if content.ends_with('\n') && !joined.is_empty() {
        joined.push('\n');
    }
    joined
}
