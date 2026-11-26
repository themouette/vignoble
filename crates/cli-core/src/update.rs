use crate::{
    error::{CliError, Result},
    version::{self, REPO_NAME, REPO_OWNER},
};
use clap::Args;
use std::path::{Path, PathBuf};

#[derive(Args, Debug, Clone)]
pub struct UpdateArgs {
    /// Version to update to (for example v0.2.0, 0.2.0, or latest)
    pub version: Option<String>,

    /// Check for updates without installing
    #[arg(long)]
    pub check: bool,

    /// Skip confirmation prompt
    #[arg(long, short = 'y')]
    pub yes: bool,
}

pub fn execute(bin_name: &str, current_version: &str, args: &UpdateArgs) -> Result<()> {
    if args.check {
        check_and_display(bin_name, current_version)
    } else {
        let target_version = match resolve_target_version(current_version, args.version.as_deref())?
        {
            Some(v) => v,
            None => return Ok(()),
        };
        perform_update_binary(bin_name, None, current_version, &target_version, args.yes)
    }
}

/// Update the primary binary (`tools[0]`) and any additional suite binaries from `tools[1..]`
/// found in the current executable directory or `~/.local/bin`.
pub fn execute_suite(tools: &[&str], current_version: &str, args: &UpdateArgs) -> Result<()> {
    let Some(&primary_bin) = tools.first() else {
        return Ok(());
    };

    if args.check {
        return check_and_display(primary_bin, current_version);
    }

    let target_version = match resolve_target_version(current_version, args.version.as_deref())? {
        Some(v) => v,
        None => return Ok(()),
    };

    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf));
    let fallback_dir = dirs::home_dir().map(|h| h.join(".local").join("bin"));
    let targets = resolve_suite_targets(tools, exe_dir.as_deref(), fallback_dir.as_deref());

    for (idx, (bin_name, install_path)) in targets.iter().enumerate() {
        let skip_confirm = args.yes || idx > 0;
        perform_update_binary(
            bin_name,
            install_path.as_deref(),
            current_version,
            &target_version,
            skip_confirm,
        )?;
    }

    Ok(())
}

/// Resolve suite binaries to update.
///
/// The first entry in `tools` is always included with `None` install path (representing the
/// running executable). Remaining tools are included when found in `exe_dir` or `fallback_dir`.
pub fn resolve_suite_targets(
    tools: &[&str],
    exe_dir: Option<&Path>,
    fallback_dir: Option<&Path>,
) -> Vec<(String, Option<PathBuf>)> {
    let mut targets = Vec::new();
    let Some((&primary, rest)) = tools.split_first() else {
        return targets;
    };

    targets.push((primary.to_string(), None));

    for &tool in rest {
        let exe_name = format!("{}{}", tool, std::env::consts::EXE_SUFFIX);
        let found = exe_dir
            .map(|d| d.join(&exe_name))
            .filter(|p| p.is_file())
            .or_else(|| {
                fallback_dir
                    .map(|d| d.join(&exe_name))
                    .filter(|p| p.is_file())
            });

        if let Some(path) = found {
            targets.push((tool.to_string(), Some(path)));
        }
    }

    targets
}

fn check_and_display(bin_name: &str, current_version: &str) -> Result<()> {
    println!("Current version: {}", current_version);
    println!("\nChecking for updates...");

    match get_latest_version()? {
        Some(latest) if version::is_newer_version(current_version, &latest) => {
            println!("New version available: {}", latest);
            println!("\nRun '{} update' to upgrade", bin_name);
        }
        Some(_) => println!("You're already running the latest version"),
        None => println!("Unable to check for updates"),
    }
    Ok(())
}

fn resolve_target_version(current_version: &str, target: Option<&str>) -> Result<Option<String>> {
    println!("Current version: {}", current_version);

    let requested_version = match target {
        Some("latest") | None => None,
        Some(v) => Some(v.trim_start_matches('v').to_string()),
    };

    match requested_version {
        Some(v) => Ok(Some(v)),
        None => match get_latest_version()? {
            Some(latest) => {
                if !version::is_newer_version(current_version, &latest) {
                    println!("You're already running the latest version");
                    return Ok(None);
                }
                println!("New version available: {}", latest);
                Ok(Some(latest))
            }
            None => Err(CliError::UpdateError(
                "Unable to fetch latest version".into(),
            )),
        },
    }
}

fn perform_update_binary(
    bin_name: &str,
    install_path: Option<&Path>,
    current_version: &str,
    target_version: &str,
    skip_confirm: bool,
) -> Result<()> {
    println!("\nDownloading update for {}...", bin_name);

    let clean_current = current_version.split('-').next().unwrap_or(current_version);
    let mut update_builder = self_update::backends::github::Update::configure();
    update_builder
        .repo_owner(REPO_OWNER)
        .repo_name(REPO_NAME)
        .bin_name(bin_name)
        .asset_identifier(bin_name)
        .target(version::current_platform()?)
        .current_version(clean_current)
        .release_tag(format!("v{}", target_version))
        .show_download_progress(true)
        .no_confirm(skip_confirm);

    if let Some(path) = install_path {
        update_builder.bin_install_path(path);
    }

    let updater = update_builder
        .build()
        .map_err(|e| CliError::UpdateError(e.to_string()))?;

    let status = updater.update().map_err(|e| {
        let msg = e.to_string();
        if msg.contains("Permission denied") || msg.contains("EACCES") {
            CliError::PermissionDenied(format!(
                "Cannot replace binary. Try: sudo {} update",
                bin_name
            ))
        } else {
            CliError::UpdateError(msg)
        }
    })?;

    println!(
        "\nSuccessfully updated {} to version {}",
        bin_name,
        status.version()
    );
    Ok(())
}

pub fn get_latest_version() -> Result<Option<String>> {
    let releases = match self_update::backends::github::ReleaseList::configure()
        .repo_owner(REPO_OWNER)
        .repo_name(REPO_NAME)
        .build()
    {
        Ok(list) => list,
        Err(_) => return Ok(None),
    };

    match releases.fetch() {
        Ok(list) => Ok(list
            .into_vec()
            .first()
            .map(|r| r.version().trim_start_matches('v').to_string())),
        Err(_) => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_resolve_suite_targets_includes_primary_and_installed_siblings() {
        let tmp = tempdir().unwrap();
        let exe_dir = tmp.path().join("bin");
        fs::create_dir_all(&exe_dir).unwrap();

        let sarment_path = exe_dir.join(format!("sarment{}", std::env::consts::EXE_SUFFIX));
        fs::write(&sarment_path, b"binary").unwrap();

        let targets = resolve_suite_targets(&["vignoble", "sarment", "cep"], Some(&exe_dir), None);
        assert_eq!(
            targets,
            vec![
                ("vignoble".to_string(), None),
                ("sarment".to_string(), Some(sarment_path)),
            ]
        );
    }

    #[test]
    fn test_resolve_suite_targets_uses_fallback_dir() {
        let tmp = tempdir().unwrap();
        let exe_dir = tmp.path().join("exe");
        let fallback_dir = tmp.path().join("fallback");
        fs::create_dir_all(&exe_dir).unwrap();
        fs::create_dir_all(&fallback_dir).unwrap();

        let cep_path = fallback_dir.join(format!("cep{}", std::env::consts::EXE_SUFFIX));
        fs::write(&cep_path, b"binary").unwrap();

        let targets = resolve_suite_targets(
            &["vignoble", "sarment", "cep"],
            Some(&exe_dir),
            Some(&fallback_dir),
        );
        assert_eq!(
            targets,
            vec![
                ("vignoble".to_string(), None),
                ("cep".to_string(), Some(cep_path)),
            ]
        );
    }
}
