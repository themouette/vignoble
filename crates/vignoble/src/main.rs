use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use cli_core::{
    ui::color_str,
    update::{self, UpdateArgs},
};
use std::env;

pub const VERSION: &str = env!("VIGNOBLE_VERSION");
pub const SUITE_TOOLS: &[&str] = &["vignoble", "sarment", "cep"];

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum Shell {
    Bash,
    Zsh,
    Fish,
}

impl Shell {
    /// Detect the active shell from a `$SHELL` value, defaulting to `Bash`.
    pub fn from_env_value(shell_env: Option<&str>) -> Self {
        match shell_env {
            Some(s) if s.ends_with("fish") => Shell::Fish,
            Some(s) if s.ends_with("zsh") => Shell::Zsh,
            _ => Shell::Bash,
        }
    }
}

/// Render the shell wrapper function for `cep` in the target shell.
pub fn render_shell_init(shell: Shell) -> &'static str {
    match shell {
        Shell::Bash | Shell::Zsh => {
            r#"cep() {
  if [ "$1" = "cd" ]; then
    eval "$(command cep cd --shell-eval "${@:2}")"
  else
    command cep "$@"
  fi
}"#
        }
        Shell::Fish => {
            r#"function cep
    if test "$argv[1]" = "cd"
        eval (command cep cd --shell-eval $argv[2..-1])
    else
        command cep $argv
    end
end"#
        }
    }
}

#[derive(Parser, Debug)]
#[command(
    name = "vignoble",
    version = VERSION,
    about = "Manage the vignoble git workflow suite (vignoble, sarment, cep)"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Update vignoble and installed suite binaries from GitHub releases
    Update(UpdateArgs),

    /// Output shell integration wrapper for cep
    ShellInit {
        /// Target shell (auto-detected from $SHELL when omitted)
        #[arg(long, value_enum)]
        shell: Option<Shell>,
    },
}

fn main() {
    let cli = Cli::parse();

    if let Err(err) = run(cli) {
        eprintln!("{}", color_str(&format!("Error: {}", err), "red"));
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Commands::Update(ref args) => {
            update::execute_suite(SUITE_TOOLS, VERSION, args)?;
            Ok(())
        }
        Commands::ShellInit { shell } => {
            let detected =
                shell.unwrap_or_else(|| Shell::from_env_value(env::var("SHELL").ok().as_deref()));
            println!("{}", render_shell_init(detected));
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_shell_init_bash() {
        let out = render_shell_init(Shell::Bash);
        assert!(out.contains("cep() {"));
        assert!(out.contains(r#"eval "$(command cep cd --shell-eval "${@:2}")""#));
        assert!(out.contains(r#"command cep "$@""#));
    }

    #[test]
    fn test_render_shell_init_zsh() {
        let out = render_shell_init(Shell::Zsh);
        assert!(out.contains("cep() {"));
        assert!(out.contains(r#"eval "$(command cep cd --shell-eval "${@:2}")""#));
        assert!(out.contains(r#"command cep "$@""#));
    }

    #[test]
    fn test_render_shell_init_fish() {
        let out = render_shell_init(Shell::Fish);
        assert!(out.contains("function cep"));
        assert!(out.contains("eval (command cep cd --shell-eval $argv[2..-1])"));
        assert!(out.contains("command cep $argv"));
    }

    #[test]
    fn test_shell_detection_from_env() {
        assert_eq!(Shell::from_env_value(Some("/bin/zsh")), Shell::Zsh);
        assert_eq!(
            Shell::from_env_value(Some("/opt/homebrew/bin/fish")),
            Shell::Fish
        );
        assert_eq!(Shell::from_env_value(Some("/bin/bash")), Shell::Bash);
        assert_eq!(Shell::from_env_value(None), Shell::Bash);
    }
}
