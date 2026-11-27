use crate::template::shell_quote;
use anyhow::{bail, Context, Result};
use std::path::Path;
use std::process::{Command, Stdio};

pub fn session_name(repo_name: &str, cep_name: &str) -> String {
    format!("{}/{}", repo_name, cep_name)
}

pub fn is_tmux_available() -> bool {
    Command::new("tmux")
        .arg("-V")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub fn is_session_alive(session: &str) -> bool {
    if session.is_empty() {
        return false;
    }
    let exact = format!("={}", session);
    Command::new("tmux")
        .args(["has-session", "-t", &exact])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub fn kill_session(session: &str) -> Result<()> {
    let exact = format!("={}", session);
    let output = Command::new("tmux")
        .args(["kill-session", "-t", &exact])
        .output()
        .context("Failed to execute tmux kill-session")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        bail!("tmux kill-session '{}' failed: {}", session, stderr);
    }
    Ok(())
}

pub fn spawn_detached_exec(
    session: &str,
    workdir: &Path,
    exe_path: &Path,
    run_id: &str,
) -> Result<()> {
    let exec_cmd = build_exec_shell_command(exe_path, run_id);
    let output = Command::new("tmux")
        .env_remove("TMUX")
        .args([
            "new-session",
            "-d",
            "-s",
            session,
            "-c",
            &workdir.to_string_lossy(),
            &exec_cmd,
        ])
        .output()
        .context("Failed to execute tmux new-session")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        bail!(
            "tmux new-session '{}' failed: {}",
            session,
            if stderr.is_empty() {
                format!("exit status {}", output.status)
            } else {
                stderr
            }
        );
    }
    Ok(())
}

fn build_exec_shell_command(exe_path: &Path, run_id: &str) -> String {
    let exe_str = exe_path.to_string_lossy();
    let is_saisonnier_bin = exe_path
        .file_name()
        .and_then(|s| s.to_str())
        .map(|name| {
            let no_ext = name.strip_suffix(".exe").unwrap_or(name);
            let base = no_ext
                .strip_suffix("-dev")
                .or_else(|| no_ext.strip_suffix("-debug"))
                .unwrap_or(no_ext);
            base == "saisonnier"
        })
        .unwrap_or(false);

    if is_saisonnier_bin {
        format!("{} exec {}", shell_quote(&exe_str), shell_quote(run_id))
    } else {
        format!(
            "{} saisonnier exec {}",
            shell_quote(&exe_str),
            shell_quote(run_id)
        )
    }
}

pub fn open_interactive_session(session: &str, workdir: &Path, shell_cmd: &str) -> Result<()> {
    if !is_session_alive(session) {
        let status = Command::new("tmux")
            .args([
                "new-session",
                "-d",
                "-s",
                session,
                "-c",
                &workdir.to_string_lossy(),
                shell_cmd,
            ])
            .status()
            .context("Failed to start tmux session")?;
        if !status.success() {
            bail!("tmux new-session '{}' failed with {}", session, status);
        }
    }

    let exact = format!("={}", session);
    let inside_tmux = std::env::var_os("TMUX").is_some();
    let args = if inside_tmux {
        ["switch-client", "-t", exact.as_str()]
    } else {
        ["attach-session", "-t", exact.as_str()]
    };

    let status = Command::new("tmux")
        .args(args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .context("Failed to attach to tmux session")?;
    if !status.success() {
        bail!("tmux attach failed with {}", status);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_exec_shell_command_handles_vignoble_and_saisonnier_binaries() {
        assert_eq!(
            build_exec_shell_command(Path::new("/usr/local/bin/vignoble"), "run-123"),
            "'/usr/local/bin/vignoble' saisonnier exec 'run-123'"
        );
        assert_eq!(
            build_exec_shell_command(Path::new("/usr/local/bin/saisonnier-dev"), "run-123"),
            "'/usr/local/bin/saisonnier-dev' exec 'run-123'"
        );
    }
}
