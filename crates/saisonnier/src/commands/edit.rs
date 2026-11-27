use crate::{commands::ExecutionContext, parcelle};
use anyhow::{bail, Context, Result};
use clap::Args;
use cli_core::ui::{color_str, GREEN_TICK_SYMBOL};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};

#[derive(Args, Debug, Clone)]
pub struct EditArgs {
    /// Queued task ID (or prefix) to edit
    pub id: String,

    /// Replace the task prompt text directly
    #[arg(short = 'p', long = "prompt")]
    pub prompt: Option<String>,

    /// Replace the task prompt from a file
    #[arg(short = 'f', long = "file")]
    pub file: Option<PathBuf>,

    /// Update target branch name (pass "" to reset to auto-generated branch)
    #[arg(short = 'b', long = "branch")]
    pub branch: Option<String>,

    /// Update base ref (pass "" to reset to default base ref)
    #[arg(long = "base")]
    pub base: Option<String>,

    /// Update agent profile name (pass "" to reset to default profile)
    #[arg(long = "profile")]
    pub profile: Option<String>,

    /// Target remote parcelle configured under [parcelle.<name>]
    #[arg(long = "parcelle")]
    pub parcelle: Option<String>,
}

pub fn execute(ctx: &ExecutionContext, args: &EditArgs) -> Result<()> {
    if let Some(ref parcelle_name) = args.parcelle {
        let mut forwarded = vec!["edit".to_string(), args.id.clone()];
        if let Some(ref prompt) = args.prompt {
            forwarded.push("--prompt".to_string());
            forwarded.push(prompt.clone());
        }
        if let Some(ref file) = args.file {
            let text = fs::read_to_string(file)
                .with_context(|| format!("Failed to read prompt file {}", file.display()))?;
            forwarded.push("--prompt".to_string());
            forwarded.push(text);
        }
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
        let interactive = args.prompt.is_none()
            && args.file.is_none()
            && args.branch.is_none()
            && args.base.is_none()
            && args.profile.is_none();
        return parcelle::relay_command(&ctx.config, parcelle_name, &forwarded, None, interactive);
    }

    let Some(mut task) = ctx.store.find_task(&args.id)? else {
        if ctx.store.find_receipt(&args.id)?.is_some() {
            bail!(
                "Task '{}' has already started or finished; only queued tasks can be edited.",
                args.id
            );
        }
        bail!("No queued task found matching '{}'", args.id);
    };

    let has_flag_edits = args.prompt.is_some()
        || args.file.is_some()
        || args.branch.is_some()
        || args.base.is_some()
        || args.profile.is_some();

    if has_flag_edits {
        if let Some(ref file) = args.file {
            let content = fs::read_to_string(file)
                .with_context(|| format!("Failed to read prompt file {}", file.display()))?;
            if content.trim().is_empty() {
                bail!("Task prompt cannot be empty");
            }
            task.prompt = content;
        } else if let Some(ref prompt) = args.prompt {
            if prompt.trim().is_empty() {
                bail!("Task prompt cannot be empty");
            }
            task.prompt = prompt.clone();
        }

        if let Some(ref branch) = args.branch {
            let trimmed = branch.trim();
            task.branch = if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            };
        }
        if let Some(ref base) = args.base {
            let trimmed = base.trim();
            task.base = if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            };
        }
        if let Some(ref profile) = args.profile {
            let trimmed = profile.trim();
            task.profile = if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            };
        }
    } else {
        let edited = edit_prompt_in_editor(&task.task_id, &task.prompt)?;
        if edited.trim().is_empty() {
            bail!(
                "Edited prompt is empty; aborting update for task '{}'",
                task.task_id
            );
        }
        task.prompt = edited;
    }

    ctx.store.write_task(&task)?;
    println!(
        "{}Updated queued task {}",
        color_str(GREEN_TICK_SYMBOL, "green"),
        color_str(&task.task_id, "cyan")
    );
    Ok(())
}

fn edit_prompt_in_editor(task_id: &str, current_prompt: &str) -> Result<String> {
    let editor = env::var("VISUAL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| env::var("EDITOR").ok().filter(|s| !s.trim().is_empty()))
        .unwrap_or_else(|| "vi".to_string());

    let edit_path = env::temp_dir().join(format!(
        "saisonnier-edit-{}-{}.md",
        task_id,
        std::process::id()
    ));
    fs::write(&edit_path, current_prompt).with_context(|| {
        format!(
            "Failed to write temporary prompt file {}",
            edit_path.display()
        )
    })?;

    let status = Command::new("sh")
        .arg("-c")
        .arg(format!("{} \"$1\"", editor))
        .arg("sh")
        .arg(&edit_path)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .with_context(|| format!("Failed to launch editor '{}'", editor))?;

    if !status.success() {
        bail!("Editor '{}' exited with status {}", editor, status);
    }

    let updated = fs::read_to_string(&edit_path)
        .with_context(|| format!("Failed to read edited prompt from {}", edit_path.display()))?;
    Ok(updated)
}
