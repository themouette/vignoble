pub mod agent_state;
pub mod plugin;
pub mod postinstall;

use agent_state::AgentStateCommand;
use anyhow::Result;
use clap::{CommandFactory, Parser, Subcommand, ValueEnum};
use clap_complete::generate;
use cli_core::{
    ui::color_str,
    update::{self, UpdateArgs},
};
use plugin::PluginCommand;
use postinstall::PostinstallArgs;
use std::{env, ffi::OsString, path::Path, process::Command};

pub const VERSION: &str = env!("VIGNOBLE_VERSION");

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

    fn to_clap_shell(self) -> clap_complete::Shell {
        match self {
            Shell::Bash => clap_complete::Shell::Bash,
            Shell::Zsh => clap_complete::Shell::Zsh,
            Shell::Fish => clap_complete::Shell::Fish,
        }
    }
}

/// Render the shell wrapper functions for `cep` (`cep`, `cep-dev`, `cep-debug`) in the target shell.
pub fn render_shell_init(shell: Shell, with_completion: bool) -> String {
    let wrapper = match shell {
        Shell::Bash | Shell::Zsh => {
            r#"cep() {
  if [ "$1" = "cd" ]; then
    eval "$(command cep cd --shell-eval "${@:2}")"
  else
    command cep "$@"
  fi
}
cep-dev() {
  if [ "$1" = "cd" ]; then
    eval "$(command cep-dev cd --shell-eval "${@:2}")"
  else
    command cep-dev "$@"
  fi
}
cep-debug() {
  if [ "$1" = "cd" ]; then
    eval "$(command cep-debug cd --shell-eval "${@:2}")"
  else
    command cep-debug "$@"
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
end
function cep-dev
    if test "$argv[1]" = "cd"
        eval (command cep-dev cd --shell-eval $argv[2..-1])
    else
        command cep-dev $argv
    end
end
function cep-debug
    if test "$argv[1]" = "cd"
        eval (command cep-debug cd --shell-eval $argv[2..-1])
    else
        command cep-debug $argv
    end
end"#
        }
    };

    if !with_completion {
        return wrapper.to_string();
    }

    let mut buf = Vec::new();
    let clap_shell = shell.to_clap_shell();

    let mut vignoble_cmd = Cli::command();
    generate(clap_shell, &mut vignoble_cmd, "vignoble", &mut buf);

    let mut sarment_cmd = sarment::build_cli();
    generate(clap_shell, &mut sarment_cmd, "sarment", &mut buf);

    let mut cep_cmd = cep::build_cli();
    generate(clap_shell, &mut cep_cmd, "cep", &mut buf);

    let completions = String::from_utf8_lossy(&buf);
    format!("{}\n\n{}", wrapper, completions.trim_end())
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
    /// Update vignoble and refresh suite links and installed agent plugins
    Update(UpdateArgs),

    /// Ensure suite CLI links (sarment, cep) exist and refresh installed agent plugins
    Postinstall(PostinstallArgs),

    /// Output shell integration wrapper for cep and optional tab completions
    ShellInit {
        /// Target shell (auto-detected from $SHELL when omitted)
        #[arg(long, value_enum)]
        shell: Option<Shell>,

        /// Include shell tab completions for vignoble, sarment, and cep
        #[arg(long)]
        with_completion: bool,
    },

    /// Manage AI coding-agent skills and activity hooks (Pi, Claude Code)
    Plugin {
        #[command(subcommand)]
        command: PluginCommand,
    },

    /// Internal helper invoked by coding-agent hooks to set or clear worktree status markers
    #[command(hide = true)]
    AgentState {
        #[command(subcommand)]
        command: AgentStateCommand,
    },

    /// Run sarment branch stage commands directly via vignoble
    #[command(external_subcommand)]
    External(Vec<OsString>),
}

/// Extract the invoked binary stem from `argv[0]`, stripping `.exe` and `-dev` / `-debug` suffixes
/// (for example `"cep"` from `"/usr/local/bin/cep-dev"` or `"sarment"` from `"sarment-debug"`).
pub fn invoked_tool_name(arg0: &Path) -> Option<&str> {
    let raw = arg0.to_str()?;
    let file_name = raw.rsplit(['/', '\\']).next()?;
    if file_name.is_empty() {
        return None;
    }
    let no_ext = file_name.strip_suffix(".exe").unwrap_or(file_name);
    let base = no_ext
        .strip_suffix("-dev")
        .or_else(|| no_ext.strip_suffix("-debug"))
        .unwrap_or(no_ext);
    Some(base)
}

fn main() {
    let args: Vec<OsString> = env::args_os().collect();
    if let Some(first) = args.first() {
        match invoked_tool_name(Path::new(first)) {
            Some("sarment") => sarment::main_from_args(args),
            Some("cep") => cep::main_from_args(args),
            _ => {}
        }
    }

    let cli = Cli::parse_from(args);

    if let Err(err) = run(cli) {
        eprintln!("{}", color_str(&format!("Error: {}", err), "red"));
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Commands::Update(ref args) => {
            let updated = update::execute("vignoble", VERSION, args)?;
            if updated {
                spawn_postinstall_from_new_binary(args.yes)?;
            }
            Ok(())
        }
        Commands::Postinstall(ref args) => postinstall::execute(args, VERSION),
        Commands::ShellInit {
            shell,
            with_completion,
        } => {
            let detected =
                shell.unwrap_or_else(|| Shell::from_env_value(env::var("SHELL").ok().as_deref()));
            println!("{}", render_shell_init(detected, with_completion));
            Ok(())
        }
        Commands::Plugin { ref command } => plugin::execute(command, VERSION),
        Commands::AgentState { ref command } => agent_state::execute(command),
        Commands::External(ext_args) => {
            let Some((subtool, rest)) = ext_args.split_first() else {
                anyhow::bail!("Unknown subcommand");
            };
            let subtool_str = subtool.to_string_lossy();
            let mut forwarded = Vec::with_capacity(rest.len() + 1);
            forwarded.push(subtool.clone());
            forwarded.extend_from_slice(rest);

            match subtool_str.as_ref() {
                "sarment" => sarment::main_from_args(forwarded),
                "cep" => cep::main_from_args(forwarded),
                other => anyhow::bail!(
                    "Unknown command '{}'. Available suite tools: sarment, cep",
                    other
                ),
            }
        }
    }
}

/// Spawn `postinstall` on the newly replaced executable on disk so new suite tools
/// and updated embedded agent plugins from the target release are applied.
fn spawn_postinstall_from_new_binary(yes: bool) -> Result<()> {
    let exe_path = env::current_exe()?;
    let mut cmd = Command::new(&exe_path);
    cmd.arg("postinstall");
    if yes {
        cmd.arg("--yes");
    }
    let status = cmd.status()?;
    if !status.success() {
        anyhow::bail!("vignoble postinstall exited with status {}", status);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_shell_init_bash() {
        let out = render_shell_init(Shell::Bash, false);
        assert!(out.contains("cep() {"));
        assert!(out.contains(r#"eval "$(command cep cd --shell-eval "${@:2}")""#));
        assert!(out.contains(r#"command cep "$@""#));
    }

    #[test]
    fn test_render_shell_init_zsh() {
        let out = render_shell_init(Shell::Zsh, false);
        assert!(out.contains("cep() {"));
        assert!(out.contains(r#"eval "$(command cep cd --shell-eval "${@:2}")""#));
        assert!(out.contains(r#"command cep "$@""#));
    }

    #[test]
    fn test_render_shell_init_fish() {
        let out = render_shell_init(Shell::Fish, false);
        assert!(out.contains("function cep"));
        assert!(out.contains("eval (command cep cd --shell-eval $argv[2..-1])"));
        assert!(out.contains("command cep $argv"));
    }

    #[test]
    fn test_render_shell_init_with_completion_includes_all_suite_tools() {
        for shell in [Shell::Bash, Shell::Zsh, Shell::Fish] {
            let out = render_shell_init(shell, true);
            assert!(out.contains("vignoble"));
            assert!(out.contains("sarment"));
            assert!(out.contains("cep"));
        }
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

    #[test]
    fn test_invoked_tool_name() {
        assert_eq!(
            invoked_tool_name(Path::new("/Users/dev/.local/bin/cep")),
            Some("cep")
        );
        assert_eq!(
            invoked_tool_name(Path::new("/Users/dev/.local/bin/cep-dev")),
            Some("cep")
        );
        assert_eq!(
            invoked_tool_name(Path::new("/Users/dev/.local/bin/sarment-debug")),
            Some("sarment")
        );
        assert_eq!(
            invoked_tool_name(Path::new("C:\\Users\\dev\\bin\\sarment.exe")),
            Some("sarment")
        );
        assert_eq!(
            invoked_tool_name(Path::new("vignoble-dev")),
            Some("vignoble")
        );
    }
}
