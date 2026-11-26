use crate::{
    error::{CliError, Result},
    version::{self, REPO_NAME, REPO_OWNER},
};
use clap::Args;

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
        perform_update(bin_name, current_version, args.version.as_deref(), args.yes)
    }
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

fn perform_update(
    bin_name: &str,
    current_version: &str,
    target: Option<&str>,
    skip_confirm: bool,
) -> Result<()> {
    println!("Current version: {}", current_version);

    let requested_version = match target {
        Some("latest") | None => None,
        Some(v) => Some(v.trim_start_matches('v').to_string()),
    };

    let target_version = match requested_version {
        Some(v) => Some(v),
        None => match get_latest_version()? {
            Some(latest) => {
                if !version::is_newer_version(current_version, &latest) {
                    println!("You're already running the latest version");
                    return Ok(());
                }
                println!("New version available: {}", latest);
                Some(latest)
            }
            None => {
                return Err(CliError::UpdateError(
                    "Unable to fetch latest version".into(),
                ))
            }
        },
    };

    println!("\nDownloading update...");

    let clean_current = current_version.split('-').next().unwrap_or(current_version);
    let mut update_builder = self_update::backends::github::Update::configure();
    update_builder
        .repo_owner(REPO_OWNER)
        .repo_name(REPO_NAME)
        .bin_name(bin_name)
        .identifier(bin_name)
        .target(&version::current_platform()?)
        .current_version(clean_current)
        .show_download_progress(true)
        .no_confirm(skip_confirm);

    if let Some(v) = target_version {
        update_builder.target_version_tag(&format!("v{}", v));
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

    println!("\nSuccessfully updated to version {}", status.version());
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
            .first()
            .map(|r| r.version.trim_start_matches('v').to_string())),
        Err(_) => Ok(None),
    }
}
