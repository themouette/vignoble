use anyhow::{bail, Context, Result};
use clap::{Subcommand, ValueEnum};
use cli_core::ui::{color_str, GREEN_TICK_SYMBOL, RED_CROSS_SYMBOL};
use dialoguer::{console::Term, theme::ColorfulTheme, MultiSelect};
use serde_json::{json, Value};
use std::{
    env, fs,
    io::IsTerminal,
    path::{Path, PathBuf},
};

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum AgentSlug {
    Claude,
    Pi,
}

impl AgentSlug {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Pi => "pi",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::Claude => "Claude Code",
            Self::Pi => "Pi",
        }
    }

    pub fn all() -> &'static [AgentSlug] {
        &[Self::Claude, Self::Pi]
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum AgentTarget {
    Pi,
    Claude,
    All,
}

#[derive(Subcommand, Debug, Clone)]
pub enum PluginCommand {
    /// Install vignoble skill and activity hooks for coding agents (interactive picker when omitted)
    Install {
        /// Agent names: claude, pi, or all (omit for interactive picker)
        agents: Vec<String>,

        /// Target coding agent
        #[arg(long, value_enum)]
        agent: Option<AgentTarget>,
    },
    /// Upgrade already-installed agent plugins and skills to match the current version
    Upgrade,
    /// Remove vignoble skill and activity hooks from coding agents (interactive picker when omitted)
    Uninstall {
        /// Agent names: claude, pi, or all (omit for interactive picker)
        agents: Vec<String>,

        /// Target coding agent
        #[arg(long, value_enum)]
        agent: Option<AgentTarget>,
    },
    /// Show installation status for each supported coding agent
    Status,
}

pub fn parse_agent_args(
    agents: &[String],
    flag_agent: Option<AgentTarget>,
) -> Result<Vec<AgentSlug>> {
    let mut resolved = Vec::new();
    let mut push_unique = |slug: AgentSlug| {
        if !resolved.contains(&slug) {
            resolved.push(slug);
        }
    };

    if let Some(target) = flag_agent {
        match target {
            AgentTarget::Claude => push_unique(AgentSlug::Claude),
            AgentTarget::Pi => push_unique(AgentSlug::Pi),
            AgentTarget::All => {
                for &s in AgentSlug::all() {
                    push_unique(s);
                }
            }
        }
    }

    for arg in agents {
        match arg.to_lowercase().as_str() {
            "claude" => push_unique(AgentSlug::Claude),
            "pi" => push_unique(AgentSlug::Pi),
            "all" => {
                for &s in AgentSlug::all() {
                    push_unique(s);
                }
            }
            other => bail!("Unknown agent '{}'. Valid agents: claude, pi, all", other),
        }
    }

    Ok(resolved)
}

fn resolve_agents_or_pick(
    agents: &[String],
    flag_agent: Option<AgentTarget>,
    paths: &PluginPaths,
    prompt: &str,
    subcommand_usage: &str,
) -> Result<Vec<AgentSlug>> {
    let parsed = parse_agent_args(agents, flag_agent)?;
    if !parsed.is_empty() {
        return Ok(parsed);
    }

    if !std::io::stdin().is_terminal() {
        bail!(
            "No agents specified. Run interactively or pass agent names: {}",
            subcommand_usage
        );
    }

    show_agent_picker(paths, prompt)
}

fn show_agent_picker(paths: &PluginPaths, prompt: &str) -> Result<Vec<AgentSlug>> {
    let all = AgentSlug::all();

    let items: Vec<String> = all
        .iter()
        .map(|&slug| {
            let installed = match slug {
                AgentSlug::Claude => is_claude_installed(paths),
                AgentSlug::Pi => is_pi_installed(paths),
            };
            if installed {
                format!("{} (installed)", slug.display_name())
            } else {
                slug.display_name().to_string()
            }
        })
        .collect();

    let defaults: Vec<bool> = all
        .iter()
        .map(|&slug| match slug {
            AgentSlug::Claude => is_claude_installed(paths),
            AgentSlug::Pi => is_pi_installed(paths),
        })
        .collect();

    let selections = MultiSelect::with_theme(&ColorfulTheme::default())
        .with_prompt(prompt)
        .items(&items)
        .defaults(&defaults)
        .interact_on_opt(&Term::stderr())
        .context("Failed to display agent selection menu")?;

    match selections {
        Some(idxs) if !idxs.is_empty() => Ok(idxs.into_iter().map(|i| all[i]).collect()),
        _ => {
            eprintln!("No agents selected.");
            Ok(Vec::new())
        }
    }
}

#[derive(Debug, Clone)]
pub struct PluginPaths {
    pub pi_agent_dir: PathBuf,
    pub claude_plugin_dir: PathBuf,
    pub claude_settings_path: PathBuf,
}

impl PluginPaths {
    pub fn from_env() -> Result<Self> {
        let home = dirs::home_dir().context("Could not resolve home directory")?;
        let pi_agent_dir = env::var("PI_CODING_AGENT_DIR")
            .ok()
            .filter(|v| !v.is_empty())
            .map(PathBuf::from);
        Ok(Self::from_home(&home, pi_agent_dir))
    }

    pub fn from_home(home: &Path, pi_agent_dir_override: Option<PathBuf>) -> Self {
        let pi_agent_dir = pi_agent_dir_override.unwrap_or_else(|| home.join(".pi").join("agent"));
        let claude_plugin_dir = home
            .join(".local")
            .join("share")
            .join("vignoble")
            .join("claude-plugin");
        let claude_settings_path = home.join(".claude").join("settings.json");

        Self {
            pi_agent_dir,
            claude_plugin_dir,
            claude_settings_path,
        }
    }

    pub fn pi_skill_path(&self) -> PathBuf {
        self.pi_agent_dir
            .join("skills")
            .join("vignoble")
            .join("SKILL.md")
    }

    pub fn pi_extension_path(&self) -> PathBuf {
        self.pi_agent_dir.join("extensions").join("vignoble.ts")
    }

    pub fn claude_skill_path(&self) -> PathBuf {
        self.claude_plugin_dir
            .join("skills")
            .join("vignoble")
            .join("SKILL.md")
    }
}

pub fn render_skill_md(version: &str) -> String {
    format!(
        r#"---
name: vignoble
description: "Use when managing git worktrees with cep or inspecting and cleaning branch workflow stages with sarment."
version: {version}
---

# Vignoble Git Workflow Suite (`cep` and `sarment`)

## Worktree Pool Management (`cep`)

`cep` manages a pool of long-living, reusable git worktrees named after French grape varieties (`merlot`, `syrah`, `pinot`, `gamay`, `chenin`, `chardonnay`, etc.).

- **List worktrees:** `cep` or `cep list` (shows `@` current worktree, checked-out branch or `(detached)`, lock state, and `🤖` / `💬` agent status).
- **Create a pooled worktree:** `cep create [name]` (defaults to detached `HEAD` and a random grape variety name).
- **Claim an available worktree for exclusive work:** `cep claim [target] -b <branch> -m "<reason>" [--create]`
- **Detach HEAD while keeping claim lock:** `cep detach [target]`
- **Release worktree back to the pool:** `cep release [target]` (detaches `HEAD` and unlocks the worktree).
- **Target resolution precedence:** `1. short name` > `2. checked-out branch` > `3. filesystem path`.

## Branch Stage Status and Pruning (`sarment`)

- **Show branch progression across stages (`dev`, `staging`, `preprod`, `prod`):** `sarment` (or `sarment -u` to skip fetch).
- **List branches in a stage:** `sarment list <stage>`
- **Delete merged local branches in a stage (skips branches checked out in any worktree):** `sarment clean <stage>`
"#
    )
}

pub fn render_pi_extension() -> &'static str {
    r#"import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";

export default function (pi: ExtensionAPI) {
  pi.on("agent_start", async (_event, ctx) => {
    try {
      await pi.exec("vignoble", ["agent-state", "set", "🤖"], { cwd: ctx.cwd });
    } catch {
      // Ignore errors outside git repositories
    }
  });

  pi.on("agent_settled", async (_event, ctx) => {
    try {
      await pi.exec("vignoble", ["agent-state", "set", "💬"], { cwd: ctx.cwd });
    } catch {
      // Ignore errors outside git repositories
    }
  });

  pi.on("session_shutdown", async (_event, ctx) => {
    try {
      await pi.exec("vignoble", ["agent-state", "clear"], { cwd: ctx.cwd });
    } catch {
      // Ignore errors outside git repositories
    }
  });
}
"#
}

fn render_claude_marketplace_json(version: &str) -> String {
    serde_json::to_string_pretty(&json!({
        "name": "vignoble-local",
        "version": version,
        "description": "Local marketplace for vignoble git workflow suite",
        "plugins": [
            {
                "name": "vignoble",
                "source": ".",
                "version": version,
                "description": "Vignoble (cep & sarment) worktree and branch workflow integration"
            }
        ]
    }))
    .unwrap()
}

fn render_claude_plugin_json(version: &str) -> String {
    serde_json::to_string_pretty(&json!({
        "name": "vignoble",
        "version": version,
        "description": "Vignoble (cep & sarment) worktree and branch workflow integration"
    }))
    .unwrap()
}

fn render_claude_hooks_json() -> String {
    serde_json::to_string_pretty(&json!({
        "hooks": {
            "UserPromptSubmit": [
                {
                    "hooks": [
                        {
                            "type": "command",
                            "command": "vignoble agent-state set \"🤖\" || true"
                        }
                    ]
                }
            ],
            "Notification": [
                {
                    "hooks": [
                        {
                            "type": "command",
                            "command": "vignoble agent-state set \"💬\" || true"
                        }
                    ]
                }
            ],
            "PermissionRequest": [
                {
                    "hooks": [
                        {
                            "type": "command",
                            "command": "vignoble agent-state set \"💬\" || true"
                        }
                    ]
                }
            ],
            "Stop": [
                {
                    "hooks": [
                        {
                            "type": "command",
                            "command": "vignoble agent-state set \"💬\" || true"
                        }
                    ]
                }
            ],
            "SessionEnd": [
                {
                    "hooks": [
                        {
                            "type": "command",
                            "command": "vignoble agent-state clear || true"
                        }
                    ]
                }
            ]
        }
    }))
    .unwrap()
}

pub fn execute(cmd: &PluginCommand, version: &str) -> Result<()> {
    let paths = PluginPaths::from_env()?;
    execute_with_paths(cmd, &paths, version)
}

pub fn execute_with_paths(cmd: &PluginCommand, paths: &PluginPaths, version: &str) -> Result<()> {
    match cmd {
        PluginCommand::Install { agents, agent } => {
            let selected = resolve_agents_or_pick(
                agents,
                *agent,
                paths,
                "Select agents to install (Space to toggle, Enter to confirm)",
                "vignoble plugin install claude pi",
            )?;
            for slug in selected {
                match slug {
                    AgentSlug::Pi => {
                        install_pi(paths, version)?;
                        println!(
                            "{}Installed vignoble plugin for Pi ({})",
                            color_str(GREEN_TICK_SYMBOL, "green"),
                            paths.pi_agent_dir.display()
                        );
                    }
                    AgentSlug::Claude => {
                        install_claude(paths, version)?;
                        println!(
                            "{}Installed vignoble plugin for Claude Code ({})",
                            color_str(GREEN_TICK_SYMBOL, "green"),
                            paths.claude_plugin_dir.display()
                        );
                    }
                }
            }
            Ok(())
        }
        PluginCommand::Upgrade => {
            let upgraded = upgrade_installed_with_paths(paths, version)?;
            if upgraded.is_empty() {
                println!("No installed agent plugins found to upgrade.");
            } else {
                for name in upgraded {
                    println!(
                        "{}Upgraded vignoble plugin for {} to v{}",
                        color_str(GREEN_TICK_SYMBOL, "green"),
                        name,
                        version
                    );
                }
            }
            Ok(())
        }
        PluginCommand::Uninstall { agents, agent } => {
            let selected = resolve_agents_or_pick(
                agents,
                *agent,
                paths,
                "Select agents to uninstall (Space to toggle, Enter to confirm)",
                "vignoble plugin uninstall claude pi",
            )?;
            for slug in selected {
                match slug {
                    AgentSlug::Pi => {
                        uninstall_pi(paths)?;
                        println!(
                            "{}Removed vignoble plugin from Pi",
                            color_str(GREEN_TICK_SYMBOL, "green")
                        );
                    }
                    AgentSlug::Claude => {
                        uninstall_claude(paths)?;
                        println!(
                            "{}Removed vignoble plugin from Claude Code",
                            color_str(GREEN_TICK_SYMBOL, "green")
                        );
                    }
                }
            }
            Ok(())
        }
        PluginCommand::Status => {
            print_status(paths, version);
            Ok(())
        }
    }
}

pub fn upgrade_installed(version: &str) -> Result<Vec<&'static str>> {
    let paths = PluginPaths::from_env()?;
    upgrade_installed_with_paths(&paths, version)
}

pub fn upgrade_installed_with_paths(
    paths: &PluginPaths,
    version: &str,
) -> Result<Vec<&'static str>> {
    let mut upgraded = Vec::new();
    if is_pi_installed(paths) {
        install_pi(paths, version)?;
        upgraded.push("Pi");
    }
    if is_claude_installed(paths) {
        install_claude(paths, version)?;
        upgraded.push("Claude Code");
    }
    Ok(upgraded)
}

pub fn is_pi_installed(paths: &PluginPaths) -> bool {
    paths.pi_skill_path().is_file() || paths.pi_extension_path().is_file()
}

pub fn is_claude_installed(paths: &PluginPaths) -> bool {
    paths.claude_skill_path().is_file()
}

pub fn installed_skill_version(skill_path: &Path) -> Option<String> {
    let content = fs::read_to_string(skill_path).ok()?;
    for line in content.lines() {
        if let Some(rest) = line.strip_prefix("version:") {
            return Some(rest.trim().to_string());
        }
    }
    None
}

fn install_pi(paths: &PluginPaths, version: &str) -> Result<()> {
    let skill_path = paths.pi_skill_path();
    if let Some(parent) = skill_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&skill_path, render_skill_md(version))?;

    let ext_path = paths.pi_extension_path();
    if let Some(parent) = ext_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&ext_path, render_pi_extension())?;
    Ok(())
}

fn uninstall_pi(paths: &PluginPaths) -> Result<()> {
    let skill_dir = paths.pi_agent_dir.join("skills").join("vignoble");
    if skill_dir.exists() {
        fs::remove_dir_all(&skill_dir)?;
    }
    let ext_path = paths.pi_extension_path();
    if ext_path.exists() {
        fs::remove_file(&ext_path)?;
    }
    Ok(())
}

fn install_claude(paths: &PluginPaths, version: &str) -> Result<()> {
    let meta_dir = paths.claude_plugin_dir.join(".claude-plugin");
    let skill_dir = paths.claude_plugin_dir.join("skills").join("vignoble");
    let hooks_dir = paths.claude_plugin_dir.join("hooks");

    fs::create_dir_all(&meta_dir)?;
    fs::create_dir_all(&skill_dir)?;
    fs::create_dir_all(&hooks_dir)?;

    fs::write(
        meta_dir.join("marketplace.json"),
        render_claude_marketplace_json(version),
    )?;
    fs::write(
        meta_dir.join("plugin.json"),
        render_claude_plugin_json(version),
    )?;
    fs::write(skill_dir.join("SKILL.md"), render_skill_md(version))?;
    fs::write(hooks_dir.join("hooks.json"), render_claude_hooks_json())?;

    let mut settings = read_claude_settings(&paths.claude_settings_path)?;
    let obj = settings
        .as_object_mut()
        .context("~/.claude/settings.json root must be a JSON object")?;

    let enabled = obj
        .entry("enabledPlugins")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .context("enabledPlugins in ~/.claude/settings.json must be an object")?;
    enabled.insert("vignoble@vignoble-local".to_string(), json!(true));

    let marketplaces = obj
        .entry("extraKnownMarketplaces")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .context("extraKnownMarketplaces in ~/.claude/settings.json must be an object")?;
    marketplaces.insert(
        "vignoble-local".to_string(),
        json!({
            "source": {
                "source": "directory",
                "path": paths.claude_plugin_dir.to_string_lossy()
            }
        }),
    );

    if let Some(parent) = paths.claude_settings_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        &paths.claude_settings_path,
        serde_json::to_string_pretty(&settings)?,
    )?;

    Ok(())
}

fn uninstall_claude(paths: &PluginPaths) -> Result<()> {
    if paths.claude_plugin_dir.exists() {
        fs::remove_dir_all(&paths.claude_plugin_dir)?;
    }

    if paths.claude_settings_path.is_file() {
        let mut settings = read_claude_settings(&paths.claude_settings_path)?;
        if let Some(obj) = settings.as_object_mut() {
            if let Some(enabled) = obj.get_mut("enabledPlugins").and_then(Value::as_object_mut) {
                enabled.remove("vignoble@vignoble-local");
            }
            if let Some(marketplaces) = obj
                .get_mut("extraKnownMarketplaces")
                .and_then(Value::as_object_mut)
            {
                marketplaces.remove("vignoble-local");
            }
        }
        fs::write(
            &paths.claude_settings_path,
            serde_json::to_string_pretty(&settings)?,
        )?;
    }

    Ok(())
}

fn read_claude_settings(path: &Path) -> Result<Value> {
    if !path.is_file() {
        return Ok(json!({}));
    }
    let raw = fs::read_to_string(path)?;
    if raw.trim().is_empty() {
        return Ok(json!({}));
    }
    let parsed: Value = serde_json::from_str(&raw)
        .with_context(|| format!("Failed to parse JSON in {}", path.display()))?;
    Ok(parsed)
}

fn print_status(paths: &PluginPaths, current_version: &str) {
    let pi_ver = installed_skill_version(&paths.pi_skill_path());
    if let Some(v) = pi_ver {
        let state = if v == current_version {
            color_str(&format!("installed (v{})", v), "green")
        } else {
            color_str(
                &format!("outdated (v{}, current: v{})", v, current_version),
                "orange",
            )
        };
        println!(
            "{}Pi: {} ({})",
            color_str(GREEN_TICK_SYMBOL, "green"),
            state,
            paths.pi_agent_dir.display()
        );
    } else {
        println!(
            "{}Pi: {}",
            color_str(RED_CROSS_SYMBOL, "red"),
            color_str("not installed", "grey")
        );
    }

    let claude_ver = installed_skill_version(&paths.claude_skill_path());
    if let Some(v) = claude_ver {
        let state = if v == current_version {
            color_str(&format!("installed (v{})", v), "green")
        } else {
            color_str(
                &format!("outdated (v{}, current: v{})", v, current_version),
                "orange",
            )
        };
        println!(
            "{}Claude Code: {} ({})",
            color_str(GREEN_TICK_SYMBOL, "green"),
            state,
            paths.claude_plugin_dir.display()
        );
    } else {
        println!(
            "{}Claude Code: {}",
            color_str(RED_CROSS_SYMBOL, "red"),
            color_str("not installed", "grey")
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_plugin_install_upgrade_and_uninstall_preserves_claude_settings() {
        let tmp = tempdir().unwrap();
        let paths = PluginPaths::from_home(tmp.path(), None);

        // Pre-populate ~/.claude/settings.json with existing user config
        fs::create_dir_all(paths.claude_settings_path.parent().unwrap()).unwrap();
        fs::write(
            &paths.claude_settings_path,
            r#"{"theme": "dark", "enabledPlugins": {"other@plugin": true}}"#,
        )
        .unwrap();

        execute_with_paths(
            &PluginCommand::Install {
                agents: vec!["claude".to_string(), "pi".to_string()],
                agent: None,
            },
            &paths,
            "0.1.0",
        )
        .unwrap();

        assert!(paths.pi_skill_path().is_file());
        assert!(paths.pi_extension_path().is_file());
        assert!(paths.claude_skill_path().is_file());
        assert_eq!(
            installed_skill_version(&paths.pi_skill_path()).as_deref(),
            Some("0.1.0")
        );

        let settings: Value =
            serde_json::from_str(&fs::read_to_string(&paths.claude_settings_path).unwrap())
                .unwrap();
        assert_eq!(settings["theme"], "dark");
        assert_eq!(settings["enabledPlugins"]["other@plugin"], true);
        assert_eq!(settings["enabledPlugins"]["vignoble@vignoble-local"], true);

        // Upgrade installed plugins to 0.2.0
        let upgraded = upgrade_installed_with_paths(&paths, "0.2.0").unwrap();
        assert_eq!(upgraded, vec!["Pi", "Claude Code"]);
        assert_eq!(
            installed_skill_version(&paths.pi_skill_path()).as_deref(),
            Some("0.2.0")
        );
        assert_eq!(
            installed_skill_version(&paths.claude_skill_path()).as_deref(),
            Some("0.2.0")
        );

        // Uninstall all
        execute_with_paths(
            &PluginCommand::Uninstall {
                agents: vec!["all".to_string()],
                agent: None,
            },
            &paths,
            "0.2.0",
        )
        .unwrap();

        assert!(!paths.pi_skill_path().exists());
        assert!(!paths.pi_extension_path().exists());
        assert!(!paths.claude_plugin_dir.exists());

        let settings_after: Value =
            serde_json::from_str(&fs::read_to_string(&paths.claude_settings_path).unwrap())
                .unwrap();
        assert_eq!(settings_after["theme"], "dark");
        assert_eq!(settings_after["enabledPlugins"]["other@plugin"], true);
        assert!(settings_after["enabledPlugins"]
            .get("vignoble@vignoble-local")
            .is_none());
    }

    #[test]
    fn test_parse_agent_args_validates_and_deduplicates() {
        assert_eq!(
            parse_agent_args(&["pi".to_string()], None).unwrap(),
            vec![AgentSlug::Pi]
        );
        assert_eq!(
            parse_agent_args(&["claude".to_string(), "PI".to_string()], None).unwrap(),
            vec![AgentSlug::Claude, AgentSlug::Pi]
        );
        assert_eq!(
            parse_agent_args(&["all".to_string()], None).unwrap(),
            vec![AgentSlug::Claude, AgentSlug::Pi]
        );
        assert!(parse_agent_args(&["unknown".to_string()], None).is_err());
    }
}
