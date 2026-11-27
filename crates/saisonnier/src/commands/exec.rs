use crate::{
    commands::{
        tick::{self, TickArgs},
        ExecutionContext,
    },
    store::{now_iso8601_utc, RunStatus},
    template::{build_command, TemplateVars},
};
use anyhow::{anyhow, bail, Context, Result};
use clap::Args;
use git_core::worktree::{
    clear_agent_marker, detach_worktree_head, is_worktree_dirty, list_worktrees, set_agent_marker,
    unlock_worktree, worktree_head_sha,
};
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::Path;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::thread;

#[derive(Args, Debug, Clone)]
pub struct ExecArgs {
    /// Run ID to execute
    pub run_id: String,

    /// Propagate inline mode when triggering the post-release tick
    #[arg(long = "inline", hide = true)]
    pub inline: bool,
}

pub fn execute(ctx: &ExecutionContext, args: &ExecArgs) -> Result<()> {
    let inline = ExecutionContext::should_run_inline(args.inline);
    execute_run(ctx, &args.run_id, inline)
}

pub fn execute_run(ctx: &ExecutionContext, run_id: &str, inline: bool) -> Result<()> {
    let mut receipt = ctx.store.read_receipt(run_id)?;

    let worktrees = list_worktrees(&ctx.git)?;
    let cep = worktrees
        .iter()
        .find(|wt| wt.short_name == receipt.cep)
        .ok_or_else(|| anyhow!("Worktree '{}' not found for run '{}'", receipt.cep, run_id))?;

    let profile = ctx.config.profiles.get(&receipt.profile).ok_or_else(|| {
        anyhow!(
            "Profile '{}' not found in configuration for run '{}'",
            receipt.profile,
            run_id
        )
    })?;
    let run_spec = profile.run.as_ref().ok_or_else(|| {
        anyhow!(
            "Profile '{}' has no 'run' command configured",
            receipt.profile
        )
    })?;

    let prompt = fs::read_to_string(&receipt.prompt_file).with_context(|| {
        format!(
            "Failed to read prompt file {}",
            receipt.prompt_file.display()
        )
    })?;

    set_agent_marker(&ctx.git, &receipt.cep, "🤖")?;

    let vars = TemplateVars {
        run_id: &receipt.run_id,
        prompt_file: &receipt.prompt_file,
        branch: &receipt.branch,
        cep: &receipt.cep,
        path: &cep.path,
        session_path: &receipt.session_path,
        session_id: &receipt.session_id,
    };

    let mut cmd = build_command(run_spec, &vars)?;
    cmd.env("VIGNOBLE_SAISONNIER", "1")
        .env_remove("VIGNOBLE_SAISONNIER_INTERNAL")
        .env("SAISONNIER_RUN_ID", &receipt.run_id)
        .env("SAISONNIER_PROMPT", &prompt)
        .env("SAISONNIER_PROMPT_FILE", &receipt.prompt_file)
        .env("SAISONNIER_SESSION_PATH", &receipt.session_path)
        .env("SAISONNIER_SESSION_ID", &receipt.session_id)
        .env("SAISONNIER_CEP", &receipt.cep)
        .env("SAISONNIER_BRANCH", &receipt.branch)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    if let Some(parent) = receipt.log_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let log_file = File::create(&receipt.log_path)
        .with_context(|| format!("Failed to create log file {}", receipt.log_path.display()))?;
    let shared_log = Arc::new(Mutex::new(log_file));

    let spawn_result = cmd.spawn();
    let exit_code = match spawn_result {
        Ok(mut child) => {
            let stdout_pipe = child.stdout.take();
            let stderr_pipe = child.stderr.take();

            let log_out = Arc::clone(&shared_log);
            let out_handle =
                stdout_pipe.map(|pipe| thread::spawn(move || tee_stream(pipe, log_out, false)));

            let log_err = Arc::clone(&shared_log);
            let err_handle =
                stderr_pipe.map(|pipe| thread::spawn(move || tee_stream(pipe, log_err, true)));

            let status = child
                .wait()
                .context("Failed while waiting for agent child process")?;

            if let Some(h) = out_handle {
                let _ = h.join();
            }
            if let Some(h) = err_handle {
                let _ = h.join();
            }

            status.code().unwrap_or(-1)
        }
        Err(err) => {
            if let Ok(mut f) = shared_log.lock() {
                let _ = writeln!(f, "Failed to spawn agent process: {}", err);
            }
            127
        }
    };

    drop(shared_log);

    let end_sha = worktree_head_sha(&ctx.git, &cep.path).ok();
    let dirty = is_worktree_dirty(&ctx.git, &cep.path).unwrap_or(true);
    let summary = extract_log_summary(&receipt.log_path);

    receipt.finished_at = Some(now_iso8601_utc());
    receipt.exit_code = Some(exit_code);
    receipt.end_sha = end_sha.clone();
    receipt.summary = summary;

    let has_new_commits = end_sha
        .as_deref()
        .map(|sha| sha != receipt.start_sha)
        .unwrap_or(false);

    if exit_code == 0 && !dirty && has_new_commits {
        receipt.status = RunStatus::Completed;
        ctx.store.write_receipt(&receipt)?;

        detach_worktree_head(&ctx.git, &cep.path)?;
        unlock_worktree(&ctx.git, &cep.path)?;
        clear_agent_marker(&ctx.git, &receipt.cep)?;

        tick::execute(
            ctx,
            &TickArgs {
                parcelle: None,
                inline,
            },
        )?;
        Ok(())
    } else if exit_code == 0 && !dirty {
        receipt.status = RunStatus::NoChanges;
        ctx.store.write_receipt(&receipt)?;
        // Set marker after child exits so agent session_shutdown hooks cannot clear it.
        set_agent_marker(&ctx.git, &receipt.cep, "💬")?;
        Ok(())
    } else if exit_code == 0 {
        receipt.status = RunStatus::Dirty;
        ctx.store.write_receipt(&receipt)?;
        set_agent_marker(&ctx.git, &receipt.cep, "⚠️")?;
        Ok(())
    } else {
        receipt.status = RunStatus::Failed;
        ctx.store.write_receipt(&receipt)?;
        set_agent_marker(&ctx.git, &receipt.cep, "⚠️")?;
        if inline {
            Ok(())
        } else {
            bail!(
                "Run '{}' failed with exit code {}",
                receipt.run_id,
                exit_code
            );
        }
    }
}

fn tee_stream<R: Read>(mut reader: R, log: Arc<Mutex<File>>, is_stderr: bool) {
    let mut buf = [0u8; 4096];
    loop {
        match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                let chunk = &buf[..n];
                if let Ok(mut file) = log.lock() {
                    let _ = file.write_all(chunk);
                    let _ = file.flush();
                }
                if is_stderr {
                    let mut stderr = io::stderr();
                    let _ = stderr.write_all(chunk);
                    let _ = stderr.flush();
                } else {
                    let mut stdout = io::stdout();
                    let _ = stdout.write_all(chunk);
                    let _ = stdout.flush();
                }
            }
            Err(_) => break,
        }
    }
}

pub fn extract_log_summary(log_path: &Path) -> Option<String> {
    let content = fs::read_to_string(log_path).ok()?;
    let last_line = content
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| !line.is_empty())?;
    let mut summary: String = last_line.chars().take(120).collect();
    if last_line.chars().count() > 120 {
        summary.push_str("...");
    }
    Some(summary)
}
