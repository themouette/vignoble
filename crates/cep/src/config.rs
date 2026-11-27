use cli_core::{
    config::{discover_cascading_paths, load_toml_file, resolve_repo_name},
    Result as CliResult,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub const APP_NAME: &str = "vignoble";
pub const DEFAULT_TEMPLATE: &str = "{repo}-worktrees/{name}";

/// Discover unified `vignoble` configuration files in precedence order.
pub fn discover_config_paths(cwd: &Path, main_repo_root: Option<&Path>) -> Vec<PathBuf> {
    discover_cascading_paths(APP_NAME, cwd, main_repo_root)
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct PartialCepSection {
    pub location: Option<String>,
    pub template: Option<String>,
    pub base_ref: Option<String>,
}

/// Per-repository block under `[repos."<repo>"]` in a config file.
/// Supports both `[repos."<repo>".cep]` and shorthand keys directly under `[repos."<repo>"]`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct RepoPartialConfig {
    pub location: Option<String>,
    pub template: Option<String>,
    pub base_ref: Option<String>,
    pub cep: Option<PartialCepSection>,
}

/// Partial config deserialized from a unified `vignoble` config file.
/// Supports `[cep]`-namespaced keys, top-level keys, and `[repos."<repo>"]` overrides.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct PartialConfig {
    pub location: Option<String>,
    pub template: Option<String>,
    pub base_ref: Option<String>,
    pub cep: Option<PartialCepSection>,
    pub repos: HashMap<String, RepoPartialConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveConfig {
    pub location: Option<String>,
    pub template: String,
    pub base_ref: Option<String>,
}

impl Default for EffectiveConfig {
    fn default() -> Self {
        Self {
            location: None,
            template: DEFAULT_TEMPLATE.to_string(),
            base_ref: None,
        }
    }
}

impl EffectiveConfig {
    fn apply_section(mut self, section: PartialCepSection) -> Self {
        if let Some(location) = section.location {
            self.location = Some(location);
        }
        if let Some(template) = section.template {
            self.template = template;
        }
        if let Some(base_ref) = section.base_ref {
            self.base_ref = Some(base_ref);
        }
        self
    }

    pub fn apply_partial(self, partial: PartialConfig) -> Self {
        self.apply_partial_for_repo(partial, None)
    }

    pub fn apply_partial_for_repo(
        self,
        mut partial: PartialConfig,
        repo_name: Option<&str>,
    ) -> Self {
        let top_level = PartialCepSection {
            location: partial.location,
            template: partial.template,
            base_ref: partial.base_ref,
        };
        let mut updated = self.apply_section(top_level);
        if let Some(cep_section) = partial.cep {
            updated = updated.apply_section(cep_section);
        }
        if let Some(name) = repo_name {
            if let Some(repo_cfg) = partial.repos.remove(name) {
                let repo_top = PartialCepSection {
                    location: repo_cfg.location,
                    template: repo_cfg.template,
                    base_ref: repo_cfg.base_ref,
                };
                updated = updated.apply_section(repo_top);
                if let Some(repo_cep) = repo_cfg.cep {
                    updated = updated.apply_section(repo_cep);
                }
            }
        }
        updated
    }

    pub fn load(cwd: &Path, main_repo_root: Option<&Path>) -> CliResult<Self> {
        let mut config = EffectiveConfig::default();
        let repo_name = resolve_repo_name(cwd, main_repo_root);
        let paths = discover_config_paths(cwd, main_repo_root);
        for path in paths {
            let partial: PartialConfig = load_toml_file(&path)?;
            config = config.apply_partial_for_repo(partial, repo_name.as_deref());
        }
        Ok(config)
    }

    /// Resolve the target filesystem path for a new worktree named `name`.
    pub fn resolve_worktree_path(&self, main_repo_root: &Path, name: &str) -> PathBuf {
        let repo = main_repo_root
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("repo");
        let parent_dir = main_repo_root.parent().unwrap_or(main_repo_root);
        let parent_str = parent_dir.to_string_lossy();

        let expanded_template_str =
            expand_placeholders(&self.template, repo, name, parent_str.as_ref());
        let expanded_template = expand_tilde(&expanded_template_str);

        if expanded_template.is_absolute() {
            return expanded_template;
        }

        let base_dir = match &self.location {
            Some(loc) => {
                let expanded_loc_str = expand_placeholders(loc, repo, name, parent_str.as_ref());
                let expanded_loc = expand_tilde(&expanded_loc_str);
                if expanded_loc.is_absolute() {
                    expanded_loc
                } else {
                    parent_dir.join(expanded_loc)
                }
            }
            None => parent_dir.to_path_buf(),
        };

        base_dir.join(expanded_template)
    }
}

fn expand_placeholders(input: &str, repo: &str, name: &str, parent: &str) -> String {
    input
        .replace("{repo}", repo)
        .replace("{name}", name)
        .replace("{parent}", parent)
}

fn expand_tilde(path_str: &str) -> PathBuf {
    let home_dir = std::env::var_os("HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(dirs::home_dir);

    if path_str == "~" {
        return home_dir.unwrap_or_else(|| PathBuf::from("~"));
    }
    if let Some(rest) = path_str.strip_prefix("~/") {
        if let Some(home) = home_dir {
            return home.join(rest);
        }
    }
    PathBuf::from(path_str)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_resolve_worktree_path_uses_parent_and_repo_worktrees() {
        let cfg = EffectiveConfig::default();
        let main_repo = Path::new("/Users/dev/projects/my-app");
        let resolved = cfg.resolve_worktree_path(main_repo, "syrah");
        assert_eq!(
            resolved,
            PathBuf::from("/Users/dev/projects/my-app-worktrees/syrah")
        );
    }

    #[test]
    fn test_custom_location_and_template_expansion() {
        let toml_str = r#"
[cep]
location = "/tmp/wt-pool"
template = "{repo}/{name}"
base_ref = "origin/main"

[sarment]
remote = "origin"
"#;
        let partial: PartialConfig = toml::from_str(toml_str).unwrap();
        let cfg = EffectiveConfig::default().apply_partial(partial);
        assert_eq!(cfg.base_ref.as_deref(), Some("origin/main"));

        let main_repo = Path::new("/Users/dev/projects/my-app");
        let resolved = cfg.resolve_worktree_path(main_repo, "merlot");
        assert_eq!(resolved, PathBuf::from("/tmp/wt-pool/my-app/merlot"));
    }

    #[test]
    fn test_inline_repos_section_overrides_cep_config() {
        let toml_str = r#"
[cep]
location = "~/worktrees"
template = "{repo}/{name}"

[repos."dd-source".cep]
location = "/Volumes/fast-ssd/worktrees"
base_ref = "origin/main"
"#;
        let partial: PartialConfig = toml::from_str(toml_str).unwrap();
        let cfg = EffectiveConfig::default().apply_partial_for_repo(partial, Some("dd-source"));
        assert_eq!(cfg.location.as_deref(), Some("/Volumes/fast-ssd/worktrees"));
        assert_eq!(cfg.base_ref.as_deref(), Some("origin/main"));
    }
}
