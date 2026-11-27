use crate::config::CommandSpec;
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone)]
pub struct TemplateVars<'a> {
    pub run_id: &'a str,
    pub prompt_file: &'a Path,
    pub branch: &'a str,
    pub cep: &'a str,
    pub path: &'a Path,
    pub session_path: &'a Path,
    pub session_id: &'a str,
}

pub fn slugify_prompt(prompt: &str) -> String {
    let first_line = prompt
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("");

    let mut slug = String::new();
    let mut prev_dash = false;
    for ch in first_line.chars() {
        let lower = ch.to_ascii_lowercase();
        if lower.is_ascii_alphanumeric() {
            slug.push(lower);
            prev_dash = false;
        } else if !prev_dash && !slug.is_empty() {
            slug.push('-');
            prev_dash = true;
        }
        if slug.len() >= 40 {
            break;
        }
    }

    let trimmed = slug.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "task".to_string()
    } else {
        trimmed
    }
}

pub fn current_user() -> String {
    let raw = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "user".to_string());
    let sanitized: String = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let trimmed = sanitized.trim_matches('-');
    if trimmed.is_empty() {
        "user".to_string()
    } else {
        trimmed.to_string()
    }
}

pub fn render_branch_template(template: &str, run_id: &str, prompt: &str, user: &str) -> String {
    let slug = slugify_prompt(prompt);
    let raw = template
        .replace("{run_id}", run_id)
        .replace("{slug}", &slug)
        .replace("{user}", user);

    sanitize_branch_name(&raw, run_id)
}

fn sanitize_branch_name(raw: &str, fallback_id: &str) -> String {
    let mut parts = Vec::new();
    for segment in raw.split('/') {
        let cleaned: String = segment
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                    c
                } else {
                    '-'
                }
            })
            .collect();
        let trimmed = cleaned.trim_matches(|c| c == '.' || c == '-');
        if !trimmed.is_empty() && !trimmed.contains("..") {
            parts.push(trimmed.to_string());
        }
    }

    if parts.is_empty() {
        format!("saisonnier/{}", fallback_id)
    } else {
        parts.join("/")
    }
}

pub fn render_placeholders(input: &str, vars: &TemplateVars<'_>) -> String {
    input
        .replace("{run_id}", vars.run_id)
        .replace("{prompt_file}", &vars.prompt_file.to_string_lossy())
        .replace("{branch}", vars.branch)
        .replace("{cep}", vars.cep)
        .replace("{path}", &vars.path.to_string_lossy())
        .replace("{session_path}", &vars.session_path.to_string_lossy())
        .replace("{session_id}", vars.session_id)
}

pub fn build_command(spec: &CommandSpec, vars: &TemplateVars<'_>) -> anyhow::Result<Command> {
    match spec {
        CommandSpec::Shell(script) => {
            let rendered = render_placeholders(script, vars);
            let mut cmd = if cfg!(windows) {
                let mut c = Command::new("cmd");
                c.args(["/C", &rendered]);
                c
            } else {
                let mut c = Command::new("sh");
                c.args(["-c", &rendered]);
                c
            };
            cmd.current_dir(vars.path);
            Ok(cmd)
        }
        CommandSpec::Argv(args) => {
            let Some((program, rest)) = args.split_first() else {
                anyhow::bail!("Profile command argv cannot be empty");
            };
            let rendered_program = render_placeholders(program, vars);
            let rendered_rest: Vec<String> = rest
                .iter()
                .map(|arg| render_placeholders(arg, vars))
                .collect();
            let mut cmd = Command::new(rendered_program);
            cmd.args(rendered_rest);
            cmd.current_dir(vars.path);
            Ok(cmd)
        }
    }
}

/// Render a `CommandSpec` into a POSIX shell command string for `tmux` invocation.
pub fn render_shell_command_string(
    spec: &CommandSpec,
    vars: &TemplateVars<'_>,
) -> anyhow::Result<String> {
    match spec {
        CommandSpec::Shell(script) => Ok(render_placeholders(script, vars)),
        CommandSpec::Argv(args) => {
            if args.is_empty() {
                anyhow::bail!("Profile command argv cannot be empty");
            }
            let quoted: Vec<String> = args
                .iter()
                .map(|arg| shell_quote(&render_placeholders(arg, vars)))
                .collect();
            Ok(quoted.join(" "))
        }
    }
}

pub fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slugify_prompt_handles_multiline_punctuation_and_empty() {
        assert_eq!(
            slugify_prompt("\n  Fix: Auth Token Refresh!  \nMore details here"),
            "fix-auth-token-refresh"
        );
        assert_eq!(slugify_prompt("   !!!   "), "task");
    }

    #[test]
    fn test_render_branch_template_substitutes_all_placeholders() {
        let branch = render_branch_template(
            "{user}/saisonnier/{slug}-{run_id}",
            "20261127-120000-ab12cd",
            "Add dark mode support",
            "julien",
        );
        assert_eq!(
            branch,
            "julien/saisonnier/add-dark-mode-support-20261127-120000-ab12cd"
        );
    }

    #[test]
    fn test_render_placeholders_substitutes_all_fields() {
        let vars = TemplateVars {
            run_id: "r1",
            prompt_file: Path::new("/repo/.git/vignoble/runs/r1.prompt.md"),
            branch: "saisonnier/r1",
            cep: "syrah",
            path: Path::new("/wt/syrah"),
            session_path: Path::new("/repo/.git/vignoble/runs/r1.session.jsonl"),
            session_id: "00000000-0000-4000-8000-000000000001",
        };
        let rendered = render_placeholders(
            "run --id {run_id} --prompt {prompt_file} --branch {branch} --cep {cep} --path {path} --session {session_path} --sid {session_id}",
            &vars,
        );
        assert_eq!(
            rendered,
            "run --id r1 --prompt /repo/.git/vignoble/runs/r1.prompt.md --branch saisonnier/r1 --cep syrah --path /wt/syrah --session /repo/.git/vignoble/runs/r1.session.jsonl --sid 00000000-0000-4000-8000-000000000001"
        );
    }
}
