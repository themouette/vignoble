use crate::plugin;
use anyhow::{Context, Result};
use clap::Args;
use cli_core::ui::{color_str, prompt_yes_no, GREEN_TICK_SYMBOL};
use std::{
    env, fs,
    path::{Path, PathBuf},
};

pub const SUITE_SUBTOOLS: &[&str] = &["sarment", "cep"];

#[derive(Args, Debug, Clone, Default)]
pub struct PostinstallArgs {
    /// Skip confirmation prompts when creating or updating suite links
    #[arg(long, short = 'y')]
    pub yes: bool,
}

pub fn execute(args: &PostinstallArgs, version: &str) -> Result<()> {
    let exe_path = env::current_exe().context("Could not determine current executable path")?;
    install_suite_links(&exe_path, SUITE_SUBTOOLS, args.yes)?;

    let upgraded = plugin::upgrade_installed(version)?;
    for agent_name in upgraded {
        println!(
            "{}Upgraded vignoble plugin for {} to v{}",
            color_str(GREEN_TICK_SYMBOL, "green"),
            agent_name,
            version
        );
    }

    Ok(())
}

/// Ensure each tool in `tools` is linked to `exe_path` inside `exe_path.parent()`.
pub fn install_suite_links(exe_path: &Path, tools: &[&str], yes: bool) -> Result<Vec<PathBuf>> {
    let exe_dir = exe_path
        .parent()
        .context("Executable path has no parent directory")?;
    let exe_file_name = exe_path
        .file_name()
        .context("Executable path has no file name")?;

    let mut linked = Vec::new();

    for &tool in tools {
        let tool_file_name = format!("{}{}", tool, env::consts::EXE_SUFFIX);
        let link_path = exe_dir.join(&tool_file_name);

        if is_already_linked_to(exe_path, exe_file_name.as_ref(), &link_path) {
            linked.push(link_path);
            continue;
        }

        let exists = fs::symlink_metadata(&link_path).is_ok();
        if !yes {
            let prompt = if exists {
                format!(
                    "Replace existing {} with link to {}?",
                    link_path.display(),
                    exe_file_name.to_string_lossy()
                )
            } else {
                format!("Install suite CLI '{}' at {}?", tool, link_path.display())
            };
            if !prompt_yes_no(&prompt)? {
                continue;
            }
        }

        if exists {
            fs::remove_file(&link_path).with_context(|| {
                format!("Failed to remove existing file at {}", link_path.display())
            })?;
        }

        create_tool_link(exe_path, exe_file_name.as_ref(), &link_path)?;
        println!(
            "{}Linked {} -> {}",
            color_str(GREEN_TICK_SYMBOL, "green"),
            link_path.display(),
            exe_file_name.to_string_lossy()
        );
        linked.push(link_path);
    }

    Ok(linked)
}

#[cfg(unix)]
fn is_already_linked_to(exe_path: &Path, exe_file_name: &Path, link_path: &Path) -> bool {
    let Ok(meta) = fs::symlink_metadata(link_path) else {
        return false;
    };
    if !meta.file_type().is_symlink() {
        return false;
    }
    let Ok(target) = fs::read_link(link_path) else {
        return false;
    };
    if target == exe_file_name || target == exe_path {
        return true;
    }
    match (link_path.canonicalize(), exe_path.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

#[cfg(windows)]
fn is_already_linked_to(_exe_path: &Path, _exe_file_name: &Path, _link_path: &Path) -> bool {
    // Windows hardlinks point to the old inode after self-update replaces vignoble.exe.
    // Always refresh the hardlink when postinstall runs on Windows.
    false
}

#[cfg(unix)]
fn create_tool_link(_exe_path: &Path, exe_file_name: &Path, link_path: &Path) -> Result<()> {
    std::os::unix::fs::symlink(exe_file_name, link_path).with_context(|| {
        format!(
            "Failed to create symlink {} -> {}",
            link_path.display(),
            exe_file_name.display()
        )
    })
}

#[cfg(windows)]
fn create_tool_link(exe_path: &Path, _exe_file_name: &Path, link_path: &Path) -> Result<()> {
    if fs::hard_link(exe_path, link_path).is_err() {
        fs::copy(exe_path, link_path).with_context(|| {
            format!(
                "Failed to link or copy {} to {}",
                exe_path.display(),
                link_path.display()
            )
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_install_suite_links_creates_and_is_idempotent() {
        let tmp = tempdir().unwrap();
        let exe_path = tmp
            .path()
            .join(format!("vignoble{}", env::consts::EXE_SUFFIX));
        fs::write(&exe_path, b"vignoble-binary").unwrap();

        let linked = install_suite_links(&exe_path, &["sarment", "cep"], true).unwrap();
        assert_eq!(linked.len(), 2);

        let sarment_link = tmp
            .path()
            .join(format!("sarment{}", env::consts::EXE_SUFFIX));
        let cep_link = tmp.path().join(format!("cep{}", env::consts::EXE_SUFFIX));
        assert!(sarment_link.exists());
        assert!(cep_link.exists());

        #[cfg(unix)]
        {
            assert!(fs::symlink_metadata(&sarment_link)
                .unwrap()
                .file_type()
                .is_symlink());
            assert_eq!(
                fs::read_link(&sarment_link).unwrap(),
                PathBuf::from("vignoble")
            );
            // Second run with yes=false should be a silent no-op without prompting
            let linked_again = install_suite_links(&exe_path, &["sarment", "cep"], false).unwrap();
            assert_eq!(linked_again.len(), 2);
        }
    }

    #[test]
    fn test_install_suite_links_replaces_standalone_file_when_yes() {
        let tmp = tempdir().unwrap();
        let exe_path = tmp
            .path()
            .join(format!("vignoble{}", env::consts::EXE_SUFFIX));
        fs::write(&exe_path, b"vignoble-binary").unwrap();

        let sarment_path = tmp
            .path()
            .join(format!("sarment{}", env::consts::EXE_SUFFIX));
        fs::write(&sarment_path, b"old-standalone-binary").unwrap();

        install_suite_links(&exe_path, &["sarment"], true).unwrap();

        #[cfg(unix)]
        assert!(fs::symlink_metadata(&sarment_path)
            .unwrap()
            .file_type()
            .is_symlink());
    }
}
