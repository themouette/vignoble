use crate::{config::EffectiveConfig, template::shell_quote};
use anyhow::{bail, Context, Result};
use std::io::Write;
use std::process::{Command, Stdio};

pub fn relay_command(
    config: &EffectiveConfig,
    parcelle_name: &str,
    saisonnier_args: &[String],
    stdin_payload: Option<&str>,
    allocate_tty: bool,
) -> Result<()> {
    let Some(parcelle) = config.parcelles.get(parcelle_name) else {
        if config.parcelles.is_empty() {
            bail!(
                "Unknown parcelle '{}'. Configure [parcelle.{}] with 'host' and 'path' in ~/.config/vignoble/config.toml.",
                parcelle_name,
                parcelle_name
            );
        }
        let mut names: Vec<&str> = config.parcelles.keys().map(String::as_str).collect();
        names.sort();
        bail!(
            "Unknown parcelle '{}'. Configured parcelles: {}",
            parcelle_name,
            names.join(", ")
        );
    };

    let quoted_args: Vec<String> = saisonnier_args.iter().map(|a| shell_quote(a)).collect();
    let remote_cmd = format!(
        "cd {} && saisonnier {}",
        shell_quote(&parcelle.path),
        quoted_args.join(" ")
    );

    let mut cmd = Command::new("ssh");
    if allocate_tty {
        cmd.arg("-t");
    }
    cmd.arg(&parcelle.host).arg(&remote_cmd);

    if let Some(payload) = stdin_payload {
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());
        let mut child = cmd
            .spawn()
            .with_context(|| format!("Failed to spawn ssh to '{}'", parcelle.host))?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(payload.as_bytes())?;
        }
        let status = child.wait()?;
        if !status.success() {
            bail!(
                "Remote saisonnier on parcelle '{}' exited with {}",
                parcelle_name,
                status
            );
        }
    } else {
        let status = cmd
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .with_context(|| format!("Failed to execute ssh to '{}'", parcelle.host))?;
        if !status.success() {
            bail!(
                "Remote saisonnier on parcelle '{}' exited with {}",
                parcelle_name,
                status
            );
        }
    }

    Ok(())
}
