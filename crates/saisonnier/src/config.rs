use cli_core::{
    config::{discover_cascading_paths, load_toml_file, resolve_repo_name},
    Result as CliResult,
};
use git_core::GitRunner;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub const APP_NAME: &str = "vignoble";
pub const DEFAULT_PROFILE: &str = "pi";
pub const DEFAULT_BRANCH_TEMPLATE: &str = "saisonnier/{run_id}";

pub fn discover_config_paths(cwd: &Path, main_repo_root: Option<&Path>) -> Vec<PathBuf> {
    discover_cascading_paths(APP_NAME, cwd, main_repo_root)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum CommandSpec {
    Shell(String),
    Argv(Vec<String>),
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct PartialProfileConfig {
    pub run: Option<CommandSpec>,
    pub resume: Option<CommandSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProfileConfig {
    pub run: Option<CommandSpec>,
    pub resume: Option<CommandSpec>,
}

impl ProfileConfig {
    fn apply(&mut self, partial: PartialProfileConfig) {
        if let Some(run) = partial.run {
            self.run = Some(run);
        }
        if let Some(resume) = partial.resume {
            self.resume = Some(resume);
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ParcelleConfig {
    pub host: String,
    pub path: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct PartialParcelleConfig {
    pub host: Option<String>,
    pub path: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct PartialCepBaseSection {
    pub base_ref: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct PartialSaisonnierSection {
    pub default_profile: Option<String>,
    pub branch_template: Option<String>,
    #[serde(alias = "base_ref")]
    pub base: Option<String>,
    pub profiles: HashMap<String, PartialProfileConfig>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct RepoPartialConfig {
    pub cep: Option<PartialCepBaseSection>,
    pub saisonnier: Option<PartialSaisonnierSection>,
    pub base_ref: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct PartialConfig {
    pub base_ref: Option<String>,
    pub cep: Option<PartialCepBaseSection>,
    pub saisonnier: Option<PartialSaisonnierSection>,
    pub parcelle: HashMap<String, PartialParcelleConfig>,
    pub repos: HashMap<String, RepoPartialConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveConfig {
    pub default_profile: String,
    pub branch_template: String,
    pub base: Option<String>,
    pub cep_base_ref: Option<String>,
    pub profiles: HashMap<String, ProfileConfig>,
    pub parcelles: HashMap<String, ParcelleConfig>,
}

impl Default for EffectiveConfig {
    fn default() -> Self {
        let mut profiles = HashMap::new();
        profiles.insert(
            "pi".to_string(),
            ProfileConfig {
                run: Some(CommandSpec::Shell(
                    r#"pi -p --session "{session_path}" "$SAISONNIER_PROMPT""#.to_string(),
                )),
                resume: Some(CommandSpec::Shell(
                    r#"pi --session "{session_path}""#.to_string(),
                )),
            },
        );
        profiles.insert(
            "claude".to_string(),
            ProfileConfig {
                run: Some(CommandSpec::Shell(
                    r#"claude -p --session-id "{session_id}" "$SAISONNIER_PROMPT""#.to_string(),
                )),
                resume: Some(CommandSpec::Shell(
                    r#"claude --resume "{session_id}""#.to_string(),
                )),
            },
        );

        Self {
            default_profile: DEFAULT_PROFILE.to_string(),
            branch_template: DEFAULT_BRANCH_TEMPLATE.to_string(),
            base: None,
            cep_base_ref: None,
            profiles,
            parcelles: HashMap::new(),
        }
    }
}

impl EffectiveConfig {
    fn apply_saisonnier_section(&mut self, section: PartialSaisonnierSection) {
        if let Some(default_profile) = section.default_profile {
            self.default_profile = default_profile;
        }
        if let Some(branch_template) = section.branch_template {
            self.branch_template = branch_template;
        }
        if let Some(base) = section.base {
            self.base = Some(base);
        }
        for (name, partial_profile) in section.profiles {
            self.profiles
                .entry(name)
                .or_insert(ProfileConfig {
                    run: None,
                    resume: None,
                })
                .apply(partial_profile);
        }
    }

    pub fn apply_partial_for_repo(
        mut self,
        mut partial: PartialConfig,
        repo_name: Option<&str>,
    ) -> Self {
        if let Some(base_ref) = partial.base_ref {
            self.cep_base_ref = Some(base_ref);
        }
        if let Some(cep_section) = partial.cep {
            if let Some(base_ref) = cep_section.base_ref {
                self.cep_base_ref = Some(base_ref);
            }
        }
        if let Some(section) = partial.saisonnier {
            self.apply_saisonnier_section(section);
        }
        for (name, p) in partial.parcelle {
            match (p.host, p.path) {
                (Some(host), Some(path)) => {
                    self.parcelles.insert(name, ParcelleConfig { host, path });
                }
                (host_opt, path_opt) => {
                    if let Some(existing) = self.parcelles.get_mut(&name) {
                        if let Some(host) = host_opt {
                            existing.host = host;
                        }
                        if let Some(path) = path_opt {
                            existing.path = path;
                        }
                    }
                }
            }
        }
        if let Some(name) = repo_name {
            if let Some(repo_cfg) = partial.repos.remove(name) {
                if let Some(base_ref) = repo_cfg.base_ref {
                    self.cep_base_ref = Some(base_ref);
                }
                if let Some(repo_cep) = repo_cfg.cep {
                    if let Some(base_ref) = repo_cep.base_ref {
                        self.cep_base_ref = Some(base_ref);
                    }
                }
                if let Some(repo_saisonnier) = repo_cfg.saisonnier {
                    self.apply_saisonnier_section(repo_saisonnier);
                }
            }
        }
        self
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

    /// Resolve base ref using task override, `[saisonnier].base`, `[cep].base_ref`, then git default.
    pub fn resolve_base_ref(&self, git: &GitRunner, task_base: Option<&str>) -> Option<String> {
        if let Some(explicit) = task_base.filter(|s| !s.is_empty()) {
            return Some(explicit.to_string());
        }
        if let Some(configured) = self.base.as_deref().filter(|s| !s.is_empty()) {
            return Some(configured.to_string());
        }
        if let Some(cep_base) = self.cep_base_ref.as_deref().filter(|s| !s.is_empty()) {
            return Some(cep_base.to_string());
        }
        git.infer_default_base_ref("origin")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_contains_pi_and_claude_profiles() {
        let cfg = EffectiveConfig::default();
        assert_eq!(cfg.default_profile, "pi");
        assert_eq!(cfg.branch_template, "saisonnier/{run_id}");
        assert!(cfg.profiles.contains_key("pi"));
        assert!(cfg.profiles.contains_key("claude"));
        assert!(cfg.profiles["pi"].run.is_some());
        assert!(cfg.profiles["pi"].resume.is_some());
        assert!(cfg.profiles["claude"].run.is_some());
        assert!(cfg.profiles["claude"].resume.is_some());
    }

    #[test]
    fn test_partial_config_overrides_profile_and_parses_argv_and_parcelle() {
        let toml_str = r#"
[cep]
base_ref = "origin/main"

[saisonnier]
default_profile = "claude"
branch_template = "{user}/auto/{slug}-{run_id}"

[saisonnier.profiles.pi]
run = ["pi", "-p", "--session", "{session_path}", "go"]

[saisonnier.profiles.custom]
run = "my-agent --file {prompt_file}"
resume = "my-agent --resume {session_id}"

[parcelle.devbox]
host = "dev@box.internal"
path = "src/my-repo"
"#;
        let partial: PartialConfig = toml::from_str(toml_str).unwrap();
        let cfg = EffectiveConfig::default().apply_partial_for_repo(partial, None);

        assert_eq!(cfg.default_profile, "claude");
        assert_eq!(cfg.branch_template, "{user}/auto/{slug}-{run_id}");
        assert_eq!(cfg.cep_base_ref.as_deref(), Some("origin/main"));

        // Overriding only `run` preserves built-in `resume` for `pi`.
        assert_eq!(
            cfg.profiles["pi"].run,
            Some(CommandSpec::Argv(vec![
                "pi".into(),
                "-p".into(),
                "--session".into(),
                "{session_path}".into(),
                "go".into(),
            ]))
        );
        assert!(cfg.profiles["pi"].resume.is_some());

        assert_eq!(
            cfg.profiles["custom"].run,
            Some(CommandSpec::Shell("my-agent --file {prompt_file}".into()))
        );
        assert_eq!(
            cfg.parcelles["devbox"],
            ParcelleConfig {
                host: "dev@box.internal".into(),
                path: "src/my-repo".into(),
            }
        );
    }

    #[test]
    fn test_repo_specific_override_applies_only_to_matching_repo() {
        let toml_str = r#"
[saisonnier]
default_profile = "pi"
base = "origin/main"

[repos."web-ui".saisonnier]
default_profile = "claude"
base = "origin/prod"
"#;
        let partial_a: PartialConfig = toml::from_str(toml_str).unwrap();
        let cfg_web = EffectiveConfig::default().apply_partial_for_repo(partial_a, Some("web-ui"));
        assert_eq!(cfg_web.default_profile, "claude");
        assert_eq!(cfg_web.base.as_deref(), Some("origin/prod"));

        let partial_b: PartialConfig = toml::from_str(toml_str).unwrap();
        let cfg_other =
            EffectiveConfig::default().apply_partial_for_repo(partial_b, Some("other-repo"));
        assert_eq!(cfg_other.default_profile, "pi");
        assert_eq!(cfg_other.base.as_deref(), Some("origin/main"));
    }
}
