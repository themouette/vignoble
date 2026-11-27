use crate::plugin;
use anyhow::{Context, Result};
use clap::Args;
use cli_core::ui::{color_str, prompt_yes_no, GREEN_TICK_SYMBOL};
use std::{
    env, fs,
    path::{Path, PathBuf},
};

pub const SUITE_SUBTOOLS: &[&str] = &["sarment", "cep"];

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum BuildFlavor {
    Release,
    Dev,
    Debug,
}

impl BuildFlavor {
    /// Infer build flavor from the executable path:
    /// - `.../target/release/vignoble` -> `BuildFlavor::Dev` (`-dev`)
    /// - `.../target/debug/vignoble`   -> `BuildFlavor::Debug` (`-debug`)
    /// - any other path                -> `BuildFlavor::Release` (no suffix)
    pub fn from_exe_path(exe_path: &Path) -> Self {
        let Some(parent) = exe_path.parent() else {
            return Self::Release;
        };
        let Some(grandparent) = parent.parent() else {
            return Self::Release;
        };
        if grandparent.file_name().and_then(|s| s.to_str()) != Some("target") {
            return Self::Release;
        }
        match parent.file_name().and_then(|s| s.to_str()) {
            Some("release") => Self::Dev,
            Some("debug") => Self::Debug,
            _ => Self::Release,
        }
    }

    pub fn suffix(self) -> &'static str {
        match self {
            Self::Release => "",
            Self::Dev => "-dev",
            Self::Debug => "-debug",
        }
    }
}

#[derive(Args, Debug, Clone, Default)]
pub struct PostinstallArgs {
    /// Skip confirmation prompts when creating or updating suite links
    #[arg(long, short = 'y')]
    pub yes: bool,

    /// Custom destination directory for links (defaults to ~/.local/bin for -dev/-debug, or the executable directory for releases)
    #[arg(long)]
    pub destination: Option<PathBuf>,
}

pub fn execute(args: &PostinstallArgs, version: &str) -> Result<()> {
    let exe_path = env::current_exe().context("Could not determine current executable path")?;
    let flavor = BuildFlavor::from_exe_path(&exe_path);

    let bin_dir = match &args.destination {
        Some(dir) => dir.clone(),
        None => match flavor {
            BuildFlavor::Release => exe_path
                .parent()
                .context("Executable path has no parent directory")?
                .to_path_buf(),
            BuildFlavor::Dev | BuildFlavor::Debug => {
                let home = env::var_os("HOME")
                    .filter(|v| !v.is_empty())
                    .map(PathBuf::from)
                    .or_else(dirs::home_dir)
                    .context("Could not determine home directory for ~/.local/bin")?;
                home.join(".local").join("bin")
            }
        },
    };

    install_flavored_links(&exe_path, &bin_dir, flavor, SUITE_SUBTOOLS, args.yes)?;

    if flavor == BuildFlavor::Release {
        let upgraded = plugin::upgrade_installed(version)?;
        for agent_name in upgraded {
            println!(
                "{}Upgraded vignoble plugin for {} to v{}",
                color_str(GREEN_TICK_SYMBOL, "green"),
                agent_name,
                version
            );
        }
    }

    Ok(())
}

/// Ensure suite links for `flavor` exist in `bin_dir`.
///
/// For `BuildFlavor::Dev` (`-dev`) and `BuildFlavor::Debug` (`-debug`), first links
/// `<bin_dir>/vignoble-<suffix> -> <exe_path>`, then links `<bin_dir>/<tool>-<suffix> -> vignoble-<suffix>`.
/// For `BuildFlavor::Release`, links `<bin_dir>/<tool> -> vignoble`.
pub fn install_flavored_links(
    exe_path: &Path,
    bin_dir: &Path,
    flavor: BuildFlavor,
    tools: &[&str],
    yes: bool,
) -> Result<Vec<PathBuf>> {
    fs::create_dir_all(bin_dir)
        .with_context(|| format!("Failed to create directory {}", bin_dir.display()))?;

    let suffix = flavor.suffix();
    let mut linked = Vec::new();

    let umbrella_name = format!("vignoble{}{}", suffix, env::consts::EXE_SUFFIX);
    let umbrella_path = bin_dir.join(&umbrella_name);

    if flavor != BuildFlavor::Release && umbrella_path != exe_path {
        ensure_single_link(
            exe_path,
            exe_path,
            &umbrella_path,
            &format!("vignoble{}", suffix),
            yes,
            &mut linked,
        )?;
    }

    let target_for_subtools = if flavor != BuildFlavor::Release && umbrella_path != exe_path {
        Path::new(&umbrella_name)
    } else {
        exe_path
            .file_name()
            .map(Path::new)
            .context("Executable path has no file name")?
    };
    let canonical_umbrella = if umbrella_path.exists() {
        umbrella_path.as_path()
    } else {
        exe_path
    };

    for &tool in tools {
        let tool_label = format!("{}{}", tool, suffix);
        let tool_file_name = format!("{}{}", tool_label, env::consts::EXE_SUFFIX);
        let link_path = bin_dir.join(&tool_file_name);

        ensure_single_link(
            canonical_umbrella,
            target_for_subtools,
            &link_path,
            &tool_label,
            yes,
            &mut linked,
        )?;
    }

    Ok(linked)
}

/// Ensure each tool in `tools` is linked to `exe_path` inside `exe_path.parent()`.
pub fn install_suite_links(exe_path: &Path, tools: &[&str], yes: bool) -> Result<Vec<PathBuf>> {
    let exe_dir = exe_path
        .parent()
        .context("Executable path has no parent directory")?;
    install_flavored_links(exe_path, exe_dir, BuildFlavor::Release, tools, yes)
}

fn ensure_single_link(
    source_exe: &Path,
    link_target: &Path,
    link_path: &Path,
    label: &str,
    yes: bool,
    linked: &mut Vec<PathBuf>,
) -> Result<()> {
    if is_already_linked_to(source_exe, link_target, link_path) {
        linked.push(link_path.to_path_buf());
        return Ok(());
    }

    let exists = fs::symlink_metadata(link_path).is_ok();
    if !yes {
        let prompt = if exists {
            format!(
                "Replace existing {} with link to {}?",
                link_path.display(),
                link_target.display()
            )
        } else {
            format!("Install '{}' at {}?", label, link_path.display())
        };
        if !prompt_yes_no(&prompt)? {
            return Ok(());
        }
    }

    if exists {
        fs::remove_file(link_path).with_context(|| {
            format!("Failed to remove existing file at {}", link_path.display())
        })?;
    }

    create_tool_link(source_exe, link_target, link_path)?;
    println!(
        "{}Linked {} -> {}",
        color_str(GREEN_TICK_SYMBOL, "green"),
        link_path.display(),
        link_target.display()
    );
    linked.push(link_path.to_path_buf());
    Ok(())
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

    #[test]
    fn test_build_flavor_detection_and_dev_debug_links() {
        assert_eq!(
            BuildFlavor::from_exe_path(Path::new("/repo/target/release/vignoble")),
            BuildFlavor::Dev
        );
        assert_eq!(
            BuildFlavor::from_exe_path(Path::new("/repo/target/debug/vignoble")),
            BuildFlavor::Debug
        );
        assert_eq!(
            BuildFlavor::from_exe_path(Path::new("/Users/dev/.local/bin/vignoble")),
            BuildFlavor::Release
        );

        let tmp = tempdir().unwrap();
        let target_release = tmp.path().join("target").join("release");
        let local_bin = tmp.path().join("local_bin");
        fs::create_dir_all(&target_release).unwrap();

        let exe_path = target_release.join(format!("vignoble{}", env::consts::EXE_SUFFIX));
        fs::write(&exe_path, b"dev-binary").unwrap();

        let linked = install_flavored_links(
            &exe_path,
            &local_bin,
            BuildFlavor::Dev,
            &["sarment", "cep"],
            true,
        )
        .unwrap();
        assert_eq!(linked.len(), 3);

        let vignoble_dev = local_bin.join(format!("vignoble-dev{}", env::consts::EXE_SUFFIX));
        let sarment_dev = local_bin.join(format!("sarment-dev{}", env::consts::EXE_SUFFIX));
        let cep_dev = local_bin.join(format!("cep-dev{}", env::consts::EXE_SUFFIX));

        assert!(vignoble_dev.exists());
        assert!(sarment_dev.exists());
        assert!(cep_dev.exists());

        #[cfg(unix)]
        {
            assert_eq!(fs::read_link(&vignoble_dev).unwrap(), exe_path);
            assert_eq!(
                fs::read_link(&sarment_dev).unwrap(),
                PathBuf::from("vignoble-dev")
            );
            assert_eq!(
                fs::read_link(&cep_dev).unwrap(),
                PathBuf::from("vignoble-dev")
            );
        }
    }
}
