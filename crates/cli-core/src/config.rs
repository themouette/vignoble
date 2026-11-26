use crate::error::{CliError, Result};
use serde::de::DeserializeOwned;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Return ordered config file paths from lowest precedence to highest precedence.
///
/// Order:
/// 1. `~/.config/<app_name>/config.toml` and `dirs::config_dir()/<app_name>/config.toml`
/// 2. `~/.config/<app_name>` (when it is a file)
/// 3. `~/.<app_name>`
/// 4. `<parent>/.<app_name>` walking down from `/` to `cwd`
/// 5. `<main_repo_root>/.<app_name>` (if provided and not already in the chain)
/// 6. `<cwd>/.<app_name>`
pub fn discover_cascading_paths(
    app_name: &str,
    cwd: &Path,
    main_repo_root: Option<&Path>,
) -> Vec<PathBuf> {
    let dot_name = format!(".{}", app_name);
    let mut candidates = Vec::new();

    if let Some(home) = dirs::home_dir() {
        let xdg_dir_config = home.join(".config").join(app_name).join("config.toml");
        candidates.push(xdg_dir_config);

        let xdg_file_config = home.join(".config").join(app_name);
        candidates.push(xdg_file_config);
    }

    if let Some(os_config) = dirs::config_dir() {
        candidates.push(os_config.join(app_name).join("config.toml"));
    }

    if let Some(home) = dirs::home_dir() {
        candidates.push(home.join(&dot_name));
    }

    // Collect ancestors from root down to parent of cwd
    let mut ancestors: Vec<&Path> = cwd.ancestors().collect();
    ancestors.reverse();
    for dir in ancestors {
        if dir != cwd {
            candidates.push(dir.join(&dot_name));
        }
    }

    if let Some(repo_root) = main_repo_root {
        candidates.push(repo_root.join(&dot_name));
    }

    candidates.push(cwd.join(&dot_name));

    let mut seen = HashSet::new();
    let mut existing = Vec::new();
    for path in candidates {
        if path.is_file() {
            let canonical = path.canonicalize().unwrap_or_else(|_| path.clone());
            if seen.insert(canonical) {
                existing.push(path);
            }
        }
    }

    existing
}

pub fn load_toml_file<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let contents = fs::read_to_string(path).map_err(|e| CliError::ConfigLoad {
        path: path.display().to_string(),
        error: e.to_string(),
    })?;

    toml::from_str(&contents).map_err(|e| CliError::ConfigLoad {
        path: path.display().to_string(),
        error: e.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_discover_cascading_paths_orders_parent_before_child() {
        let tmp = tempdir().unwrap();
        let parent = tmp.path().join("workspace");
        let child = parent.join("repo").join("subdir");
        fs::create_dir_all(&child).unwrap();

        let parent_cfg = parent.join(".branches-status");
        let repo_cfg = parent.join("repo").join(".branches-status");
        let child_cfg = child.join(".branches-status");

        fs::write(&parent_cfg, "remote = 'upstream'\n").unwrap();
        fs::write(&repo_cfg, "remote = 'origin'\n").unwrap();
        fs::write(&child_cfg, "squash = true\n").unwrap();

        let found = discover_cascading_paths("branches-status", &child, None);
        let p_idx = found.iter().position(|p| p == &parent_cfg).unwrap();
        let r_idx = found.iter().position(|p| p == &repo_cfg).unwrap();
        let c_idx = found.iter().position(|p| p == &child_cfg).unwrap();

        assert!(p_idx < r_idx);
        assert!(r_idx < c_idx);
    }

    #[test]
    fn test_discover_includes_main_repo_root_for_linked_worktree() {
        let tmp = tempdir().unwrap();
        let main_repo = tmp.path().join("main-repo");
        let linked_wt = tmp.path().join("worktrees").join("wt-1");
        fs::create_dir_all(&main_repo).unwrap();
        fs::create_dir_all(&linked_wt).unwrap();

        let main_cfg = main_repo.join(".branches-status");
        let wt_cfg = linked_wt.join(".branches-status");
        fs::write(&main_cfg, "remote = 'origin'\n").unwrap();
        fs::write(&wt_cfg, "squash = true\n").unwrap();

        let found = discover_cascading_paths("branches-status", &linked_wt, Some(&main_repo));
        let main_idx = found.iter().position(|p| p == &main_cfg).unwrap();
        let wt_idx = found.iter().position(|p| p == &wt_cfg).unwrap();
        assert!(main_idx < wt_idx);
    }
}
