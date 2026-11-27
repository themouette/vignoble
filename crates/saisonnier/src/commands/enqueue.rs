use crate::{
    commands::{
        tick::{self, TickArgs},
        ExecutionContext,
    },
    parcelle,
    store::{generate_sortable_id, now_iso8601_utc, QueuedTask, SCHEMA_VERSION},
};
use anyhow::{bail, Context, Result};
use clap::Args;
use cli_core::ui::{color_str, GREEN_TICK_SYMBOL};
use std::fs;
use std::io::{self, IsTerminal, Read};
use std::path::PathBuf;

#[derive(Args, Debug, Clone)]
pub struct EnqueueArgs {
    /// Prompt text describing the autonomous task
    pub prompt_arg: Option<String>,

    /// Prompt text describing the autonomous task
    #[arg(short = 'p', long = "prompt")]
    pub prompt: Option<String>,

    /// Read prompt text from a file (use '-' for stdin)
    #[arg(short = 'f', long = "file")]
    pub file: Option<PathBuf>,

    /// Target branch name (auto-generated from branch_template when omitted)
    #[arg(short = 'b', long = "branch")]
    pub branch: Option<String>,

    /// Base ref to branch from when creating a new branch
    #[arg(long = "base")]
    pub base: Option<String>,

    /// Agent profile name (defaults to [saisonnier].default_profile)
    #[arg(long = "profile")]
    pub profile: Option<String>,

    /// Target remote parcelle configured under [parcelle.<name>]
    #[arg(long = "parcelle")]
    pub parcelle: Option<String>,

    /// Add task to the queue without immediately running tick
    #[arg(long = "no-tick")]
    pub no_tick: bool,

    /// Execute claimed tasks inline instead of spawning detached tmux sessions
    #[arg(long = "inline")]
    pub inline: bool,
}

fn resolve_prompt(args: &EnqueueArgs) -> Result<String> {
    if let Some(ref text) = args.prompt {
        if !text.trim().is_empty() {
            return Ok(text.clone());
        }
    }

    if let Some(ref path) = args.file {
        let content = if path.as_os_str() == "-" {
            let mut buf = String::new();
            io::stdin()
                .read_to_string(&mut buf)
                .context("Failed to read prompt from stdin")?;
            buf
        } else {
            fs::read_to_string(path)
                .with_context(|| format!("Failed to read prompt file {}", path.display()))?
        };
        if content.trim().is_empty() {
            bail!("Prompt file is empty");
        }
        return Ok(content);
    }

    if let Some(ref text) = args.prompt_arg {
        if !text.trim().is_empty() {
            return Ok(text.clone());
        }
    }

    if !io::stdin().is_terminal() {
        let mut buf = String::new();
        io::stdin()
            .read_to_string(&mut buf)
            .context("Failed to read prompt from stdin")?;
        if !buf.trim().is_empty() {
            return Ok(buf);
        }
    }

    bail!("No prompt provided. Pass a prompt argument, --prompt, --file, or pipe via stdin.")
}

pub fn execute(ctx: &ExecutionContext, args: &EnqueueArgs) -> Result<()> {
    let prompt = resolve_prompt(args)?;

    if let Some(ref parcelle_name) = args.parcelle {
        let mut forwarded = vec!["enqueue".to_string(), "--file".to_string(), "-".to_string()];
        if let Some(ref branch) = args.branch {
            forwarded.push("--branch".to_string());
            forwarded.push(branch.clone());
        }
        if let Some(ref base) = args.base {
            forwarded.push("--base".to_string());
            forwarded.push(base.clone());
        }
        if let Some(ref profile) = args.profile {
            forwarded.push("--profile".to_string());
            forwarded.push(profile.clone());
        }
        if args.no_tick {
            forwarded.push("--no-tick".to_string());
        }
        if args.inline {
            forwarded.push("--inline".to_string());
        }
        return parcelle::relay_command(
            &ctx.config,
            parcelle_name,
            &forwarded,
            Some(&prompt),
            false,
        );
    }

    if let Some(ref profile_name) = args.profile {
        if !ctx.config.profiles.contains_key(profile_name) {
            bail!(
                "Unknown profile '{}'. Configure [saisonnier.profiles.{}] in .vignoble.toml.",
                profile_name,
                profile_name
            );
        }
    }

    let task = QueuedTask {
        schema_version: SCHEMA_VERSION,
        task_id: generate_sortable_id(),
        created_at: now_iso8601_utc(),
        prompt,
        branch: args.branch.clone(),
        base: args.base.clone(),
        profile: args.profile.clone(),
    };

    ctx.store.write_task(&task)?;

    println!(
        "{}Enqueued task {}",
        color_str(GREEN_TICK_SYMBOL, "green"),
        color_str(&task.task_id, "cyan")
    );

    if !args.no_tick {
        tick::execute(
            ctx,
            &TickArgs {
                parcelle: None,
                inline: args.inline,
            },
        )?;
    }

    Ok(())
}
