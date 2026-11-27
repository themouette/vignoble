pub mod commands;
pub mod config;
pub mod printer;

use anyhow::Result;
use clap::{CommandFactory, Parser, Subcommand};
use cli_core::ui::{color_str, sync_symbols_help_section};
use commands::{all, clean, list, ExecutionContext};
use config::{discover_config_paths, EffectiveConfig};
use git_core::GitRunner;
use std::{env, ffi::OsString};

fn config_help_section() -> String {
    let cwd = env::current_dir().unwrap_or_default();
    let main_repo_root = GitRunner::new(&cwd, false).main_repo_root().ok();
    let active_paths = discover_config_paths(&cwd, main_repo_root.as_deref());

    if active_paths.is_empty() {
        format!(
            "Configuration:\n\
             \x20 No configuration file found (using built-in defaults).\n\
             \x20 You can configure sarment in:\n\
             \x20   - {} or {} (global)\n\
             \x20   - {} or {} (local directory or any parent directory)",
            color_str("~/.config/vignoble/config.toml", "cyan"),
            color_str("~/.config/vignoble.toml", "cyan"),
            color_str("./.vignoble", "cyan"),
            color_str("./.vignoble.toml", "cyan"),
        )
    } else {
        let list = active_paths
            .iter()
            .map(|p| format!("  * {}", color_str(&p.display().to_string(), "cyan")))
            .collect::<Vec<_>>()
            .join("\n");
        format!(
            "Configuration files in use (lowest to highest precedence):\n{}",
            list
        )
    }
}

fn colored_after_help() -> String {
    let bin_name = "sarment";

    format!(
        "{}\n\n\
         More usage examples:\n\n\
         \x20 # Show local status\n\
         \x20 {bin_name}\n\n\
         \x20 # Show status of all your remote branches\n\
         \x20 {bin_name} --no-update --pattern '*themouette*'\n\n\
         \x20 # Delete branches merged in production\n\
         \x20 {bin_name} clean prod\n\n\
         \x20 # Show all your branches in production\n\
         \x20 {bin_name} list prod\n\n\
         {}",
        sync_symbols_help_section(),
        config_help_section(),
        bin_name = bin_name,
    )
}

#[derive(Parser, Debug)]
#[command(
    name = "sarment",
    version = env!("SARMENT_VERSION"),
    about = "Check the status of your branches across workflow stages",
    after_help = colored_after_help()
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Prevent repository fetch
    #[arg(short = 'u', long = "no-update", global = true)]
    pub no_update: bool,

    /// A pattern to filter remote branches (if omitted, only local branches are displayed)
    #[arg(long = "pattern", global = true)]
    pub pattern: Option<String>,

    /// Override the production branch name
    #[arg(long = "prod-branch", global = true)]
    pub prod_branch: Option<String>,

    /// The git remote (overrides config `remote`)
    #[arg(long = "remote", global = true)]
    pub remote: Option<String>,

    /// Enable squash-merge commit detection
    #[arg(long = "squash", global = true)]
    pub squash: bool,

    /// Answer yes to all non-dangerous actions
    #[arg(short = 'y', long = "yes", global = true)]
    pub force_yes: bool,

    /// Force dangerous actions
    #[arg(short = 'f', long = "force", global = true)]
    pub force: bool,

    /// Turn on debug output
    #[arg(short = 'd', long = "debug", env = "SARMENT_DEBUG", global = true)]
    pub debug: bool,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Pretty print all branches status (default)
    All,

    /// List branches in the given stage (e.g. prod, preprod, staging, dev)
    #[command(visible_alias = "ls")]
    List {
        /// Stage name (defaults to "prod" if omitted)
        #[arg(default_value = "prod")]
        stage: String,
    },

    /// Remove local branches merged in the given stage
    Clean {
        /// Stage name (defaults to "prod" if omitted)
        #[arg(default_value = "prod")]
        stage: String,
    },
}

pub fn build_cli() -> clap::Command {
    Cli::command()
}

pub fn run(cli: Cli) -> Result<()> {
    let cwd = env::current_dir()?;
    let git = GitRunner::new(&cwd, cli.debug);
    let main_repo_root = git.main_repo_root()?;

    let mut config = EffectiveConfig::load(&cwd, Some(&main_repo_root))?;
    if let Some(remote) = cli.remote {
        config.remote = remote;
    }
    if let Some(prod_branch) = cli.prod_branch {
        config
            .stage_configs
            .entry("prod".to_string())
            .or_default()
            .branch = Some(prod_branch);
        if !config.stages.iter().any(|s| s == "prod") {
            config.stages.insert(0, "prod".to_string());
        }
    }

    let ctx = ExecutionContext::build(
        git,
        &config,
        cli.no_update,
        cli.pattern,
        cli.squash,
        cli.force_yes,
        cli.force,
    )?;

    match cli.command.unwrap_or(Commands::All) {
        Commands::All => all::execute(&ctx),
        Commands::List { stage } => list::execute(&ctx, &stage),
        Commands::Clean { stage } => clean::execute(&ctx, &stage),
    }
}

pub fn main_from_args<I, T>(args: I) -> !
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let cli = Cli::parse_from(args);
    let debug = cli.debug;

    if let Err(err) = run(cli) {
        if debug {
            eprintln!("{:?}", err);
        } else {
            eprintln!("{}", color_str(&format!("Error: {}", err), "red"));
        }
        std::process::exit(1);
    }
    std::process::exit(0);
}
