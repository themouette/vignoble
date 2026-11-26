use cli_core::{
    config::{discover_cascading_paths, load_toml_file},
    Result as CliResult,
};
use git_core::{StageDefinition, StageMode};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

pub const APP_NAME: &str = "branches-status";

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

/// Partial config deserialized from a single `.branches-status` file.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct PartialConfig {
    pub remote: Option<String>,
    pub squash: Option<bool>,
    pub stages: Option<Vec<String>>,
    /// Support both `[stage.<name>]` and `[stages.<name>]` table headers.
    #[serde(alias = "stages_config")]
    pub stage: HashMap<String, StageConfig>,
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
    pub fn apply_partial(mut self, partial: PartialConfig) -> Self {
        if let Some(remote) = partial.remote {
            self.remote = remote;
        }
        if let Some(squash) = partial.squash {
            self.squash = squash;
        }
        if let Some(stages) = partial.stages {
            self.stages = stages;
        }
        for (name, cfg) in partial.stage {
            let merged = match self.stage_configs.remove(&name) {
                Some(existing) => existing.merge(cfg),
                None => cfg,
            };
            self.stage_configs.insert(name, merged);
        }
        self
    }

    pub fn load(cwd: &Path, main_repo_root: Option<&Path>) -> CliResult<Self> {
        let mut config = EffectiveConfig::default();
        let paths = discover_cascading_paths(APP_NAME, cwd, main_repo_root);
        for path in paths {
            let partial: PartialConfig = load_toml_file(&path)?;
            config = config.apply_partial(partial);
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
}
