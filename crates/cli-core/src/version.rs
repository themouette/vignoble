use crate::error::{CliError, Result};
use semver::Version;

pub const REPO_OWNER: &str = "themouette";
pub const REPO_NAME: &str = "git-workflow";

/// Return the target platform identifier used in GitHub release asset names.
pub fn current_platform() -> Result<String> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => Ok("macos-aarch64".to_string()),
        ("macos", "x86_64") => Ok("macos-x86_64".to_string()),
        ("linux", "aarch64") => Ok("linux-aarch64".to_string()),
        ("linux", "x86_64") => Ok("linux-x86_64".to_string()),
        (os, arch) => Err(CliError::UpdateError(format!(
            "Unsupported platform: {}-{}",
            os, arch
        ))),
    }
}

pub fn is_newer_version(current_version: &str, candidate_version: &str) -> bool {
    let clean_current = current_version.split('-').next().unwrap_or(current_version);
    let clean_candidate = candidate_version.trim_start_matches('v');
    match (
        Version::parse(clean_current),
        Version::parse(clean_candidate),
    ) {
        (Ok(current), Ok(latest)) => latest > current,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_current_platform() {
        let platform = current_platform().unwrap();
        assert!(matches!(
            platform.as_str(),
            "macos-aarch64" | "macos-x86_64" | "linux-aarch64" | "linux-x86_64"
        ));
    }

    #[test]
    fn test_is_newer_version() {
        assert!(is_newer_version("0.1.0", "0.2.0"));
        assert!(is_newer_version("0.1.0-dev+abc1234", "0.1.1"));
        assert!(!is_newer_version("0.2.0", "0.1.0"));
        assert!(!is_newer_version("0.2.0", "0.2.0"));
        assert!(!is_newer_version("0.2.0", "invalid"));
    }
}
