use cli_core::{
    config::{discover_cascading_paths, load_toml_file, resolve_repo_name},
    Result as CliResult,
};
use git_core::{StageDefinition, StageMode};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub const APP_NAME: &str = "vignoble";

/// Discover unified `vignoble` configuration files in precedence order.
pub fn discover_config_paths(cwd: &Path, main_repo_root: Option<&Path>) -> Vec<PathBuf> {
    discover_cascading_paths(APP_NAME, cwd, main_repo_root)
}

pub fn default_stages_list() -> Vec<String> {
    vec![
        "main".to_string(),
        "master".to_string(),
        "prod".to_string(),
        "preprod".to_string(),
        "staging".to_string(),
    ]
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct StageConfig {
    pub branch: Option<String>,
    /// Alias `script` to `branch_script` for convenience if someone writes `script = "..."`.
    #[serde(alias = "script")]
    pub branch_script: Option<String>,
    pub check_script: Option<String>,
    pub label: Option<String>,
    pub color: Option<String>,
}

impl StageConfig {
    pub fn merge(self, other: StageConfig) -> StageConfig {
        let (branch, branch_script, check_script) = if other.check_script.is_some() {
            (None, None, other.check_script)
        } else if other.branch_script.is_some() {
            (None, other.branch_script, None)
        } else if other.branch.is_some() {
            (other.branch, None, None)
        } else {
            (self.branch, self.branch_script, self.check_script)
        };

        StageConfig {
            branch,
            branch_script,
            check_script,
            label: other.label.or(self.label),
            color: other.color.or(self.color),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct PartialSarmentSection {
    pub remote: Option<String>,
    pub squash: Option<bool>,
    pub stages: Option<Vec<String>>,
    /// Support both `[sarment.stage.<name>]` and `[sarment.stages.<name>]` table headers.
    #[serde(alias = "stages_config")]
    pub stage: HashMap<String, StageConfig>,
}

/// Per-repository block under `[repos."<repo>"]` in a config file.
/// Supports both `[repos."<repo>".sarment]` and shorthand keys directly under `[repos."<repo>"]`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct RepoPartialConfig {
    pub remote: Option<String>,
    pub squash: Option<bool>,
    pub stages: Option<Vec<String>>,
    #[serde(alias = "stages_config")]
    pub stage: HashMap<String, StageConfig>,
    pub sarment: Option<PartialSarmentSection>,
}

/// Partial config deserialized from a unified `vignoble` config file.
/// Supports `[sarment]`-namespaced keys, top-level keys, and `[repos."<repo>"]` overrides.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct PartialConfig {
    pub remote: Option<String>,
    pub squash: Option<bool>,
    pub stages: Option<Vec<String>>,
    /// Support both `[stage.<name>]` and `[stages.<name>]` table headers.
    #[serde(alias = "stages_config")]
    pub stage: HashMap<String, StageConfig>,
    pub sarment: Option<PartialSarmentSection>,
    pub repos: HashMap<String, RepoPartialConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveConfig {
    pub remote: String,
    pub squash: bool,
    pub stages: Vec<String>,
    pub stage_configs: HashMap<String, StageConfig>,
}

impl Default for EffectiveConfig {
    fn default() -> Self {
        Self {
            remote: "origin".to_string(),
            squash: false,
            stages: default_stages_list(),
            stage_configs: HashMap::new(),
        }
    }
}

impl EffectiveConfig {
    fn apply_section(mut self, section: PartialSarmentSection) -> Self {
        if let Some(remote) = section.remote {
            self.remote = remote;
        }
        if let Some(squash) = section.squash {
            self.squash = squash;
        }
        if let Some(stages) = section.stages {
            self.stages = stages;
        }
        for (name, cfg) in section.stage {
            let merged = match self.stage_configs.remove(&name) {
                Some(existing) => existing.merge(cfg),
                None => cfg,
            };
            self.stage_configs.insert(name, merged);
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
        let top_level = PartialSarmentSection {
            remote: partial.remote,
            squash: partial.squash,
            stages: partial.stages,
            stage: partial.stage,
        };
        let mut updated = self.apply_section(top_level);
        if let Some(sarment_section) = partial.sarment {
            updated = updated.apply_section(sarment_section);
        }
        if let Some(name) = repo_name {
            if let Some(repo_cfg) = partial.repos.remove(name) {
                let repo_top = PartialSarmentSection {
                    remote: repo_cfg.remote,
                    squash: repo_cfg.squash,
                    stages: repo_cfg.stages,
                    stage: repo_cfg.stage,
                };
                updated = updated.apply_section(repo_top);
                if let Some(repo_sarment) = repo_cfg.sarment {
                    updated = updated.apply_section(repo_sarment);
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

    pub fn to_stage_definitions(&self) -> Vec<StageDefinition> {
        self.stages
            .iter()
            .map(|stage_name| {
                let custom = self.stage_configs.get(stage_name);
                let (default_label, default_color) = default_stage_presentation(stage_name);

                let label = custom
                    .and_then(|c| c.label.clone())
                    .unwrap_or(default_label);
                let color = custom
                    .and_then(|c| c.color.clone())
                    .unwrap_or(default_color);

                let mode = if let Some(script) = custom.and_then(|c| c.check_script.clone()) {
                    StageMode::CheckScript(script)
                } else if let Some(script) = custom.and_then(|c| c.branch_script.clone()) {
                    StageMode::BranchScript(script)
                } else if let Some(branch) = custom.and_then(|c| c.branch.clone()) {
                    StageMode::Branch(branch)
                } else {
                    StageMode::Branch(stage_name.clone())
                };

                StageDefinition {
                    name: stage_name.clone(),
                    label,
                    color,
                    mode,
                }
            })
            .collect()
    }
}

pub fn default_stage_presentation(stage_name: &str) -> (String, String) {
    match stage_name {
        "main" | "master" | "prod" => ("In production".to_string(), "green".to_string()),
        "preprod" => ("In pre-production".to_string(), "magenta".to_string()),
        "staging" => ("In staging".to_string(), "orange".to_string()),
        "partial-staging" => ("In partial staging".to_string(), "yellow".to_string()),
        "dev" => ("Unmerged".to_string(), "blue".to_string()),
        other => (format!("In {}", other), "cyan".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_default_config_has_expected_stages() {
        let cfg = EffectiveConfig::default();
        assert_eq!(cfg.remote, "origin");
        assert!(!cfg.squash);
        assert_eq!(
            cfg.stages,
            vec!["main", "master", "prod", "preprod", "staging"]
        );
    }

    #[test]
    fn test_partial_config_merges_stage_without_resetting_stages_list() {
        let base = EffectiveConfig::default();
        let toml_str = r#"
[stage.staging]
branch_script = "echo staging-42"
"#;
        let partial: PartialConfig = toml::from_str(toml_str).unwrap();
        let merged = base.apply_partial(partial);

        // Stages order is preserved from defaults
        assert_eq!(
            merged.stages,
            vec!["main", "master", "prod", "preprod", "staging"]
        );
        let defs = merged.to_stage_definitions();
        let staging = defs.iter().find(|d| d.name == "staging").unwrap();
        assert_eq!(
            staging.mode,
            StageMode::BranchScript("echo staging-42".to_string())
        );
    }

    #[test]
    fn test_partial_config_replaces_stages_when_specified() {
        let base = EffectiveConfig::default();
        let toml_str = r#"
stages = ["prod", "staging", "partial-staging"]

[stage.staging]
check_script = "exit 0"

[stage.partial-staging]
check_script = "exit 1"
"#;
        let partial: PartialConfig = toml::from_str(toml_str).unwrap();
        let merged = base.apply_partial(partial);

        assert_eq!(merged.stages, vec!["prod", "staging", "partial-staging"]);
        let defs = merged.to_stage_definitions();
        assert_eq!(defs.len(), 3);
        assert_eq!(defs[0].mode, StageMode::Branch("prod".to_string()));
        assert_eq!(defs[1].mode, StageMode::CheckScript("exit 0".to_string()));
        assert_eq!(defs[2].mode, StageMode::CheckScript("exit 1".to_string()));
    }

    #[test]
    fn test_partial_config_supports_sarment_section_and_coexists_with_cep() {
        let base = EffectiveConfig::default();
        let toml_str = r#"
[cep]
location = "~/worktrees"
template = "{repo}-worktrees/{name}"

[sarment]
remote = "upstream"
squash = true
stages = ["prod", "staging"]

[sarment.stage.staging]
branch_script = "echo staging-42"
"#;
        let partial: PartialConfig = toml::from_str(toml_str).unwrap();
        let merged = base.apply_partial(partial);

        assert_eq!(merged.remote, "upstream");
        assert!(merged.squash);
        assert_eq!(merged.stages, vec!["prod", "staging"]);
        let defs = merged.to_stage_definitions();
        assert_eq!(
            defs[1].mode,
            StageMode::BranchScript("echo staging-42".to_string())
        );
    }

    #[test]
    fn test_discover_config_paths_orders_vignoble_before_vignoble_toml() {
        let tmp = tempdir().unwrap();
        let repo = tmp.path().join("repo");
        fs::create_dir_all(&repo).unwrap();

        let extless_cfg = repo.join(".vignoble");
        let toml_cfg = repo.join(".vignoble.toml");
        fs::write(&extless_cfg, "remote = 'upstream'\nsquash = true\n").unwrap();
        fs::write(&toml_cfg, "[sarment]\nremote = 'origin'\n").unwrap();

        let paths = discover_config_paths(&repo, None);
        let extless_idx = paths.iter().position(|p| p == &extless_cfg).unwrap();
        let toml_idx = paths.iter().position(|p| p == &toml_cfg).unwrap();
        assert!(extless_idx < toml_idx);
    }

    #[test]
    fn test_inline_repos_section_applies_for_matching_repo_only() {
        let toml_str = r#"
[sarment]
remote = "origin"
stages = ["main", "staging"]

[repos."web-ui".sarment]
stages = ["prod", "preprod", "staging"]

[repos."web-ui".sarment.stage.staging]
branch_script = "echo staging-42"

[repos."dd-source"]
squash = true
"#;
        let partial: PartialConfig = toml::from_str(toml_str).unwrap();

        let web_ui_cfg =
            EffectiveConfig::default().apply_partial_for_repo(partial.clone(), Some("web-ui"));
        assert_eq!(web_ui_cfg.stages, vec!["prod", "preprod", "staging"]);
        assert!(!web_ui_cfg.squash);
        let defs = web_ui_cfg.to_stage_definitions();
        assert_eq!(
            defs[2].mode,
            StageMode::BranchScript("echo staging-42".to_string())
        );

        let dd_source_cfg =
            EffectiveConfig::default().apply_partial_for_repo(partial, Some("dd-source"));
        assert_eq!(dd_source_cfg.stages, vec!["main", "staging"]);
        assert!(dd_source_cfg.squash);
    }
}
