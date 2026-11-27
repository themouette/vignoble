pub mod commands;
pub mod config;
pub mod names;

use anyhow::Result;
use clap::{CommandFactory, Parser, Subcommand};
use cli_core::ui::{agent_symbols_help_section, color_str, sync_symbols_help_section};
use commands::{
    cd::{self, CdArgs},
    claim::{self, ClaimArgs},
    create::{self, CreateArgs},
    detach::{self, DetachArgs},
    list,
    release::{self, ReleaseArgs},
    remove::{self, RemoveArgs},
    ExecutionContext,
};
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
             \x20 You can configure cep in:\n\
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
    let bin_name = "cep";

    format!(
        "{}\n\n\
         {}\n\n\
         Target resolution precedence:\n\
         \x20 1. Worktree short name (e.g. 'syrah', 'merlot')\n\
         \x20 2. Checked-out branch name (e.g. 'feat/login')\n\
         \x20 3. Filesystem path\n\n\
         More usage examples:\n\n\
         \x20 # List all worktrees (including detached/unclaimed pool worktrees)\n\
         \x20 {bin_name}\n\n\
         \x20 # Create a new detached worktree with a random French grape variety name\n\
         \x20 {bin_name} create\n\n\
         \x20 # Claim an available worktree and check out a branch\n\
         \x20 {bin_name} claim -b feat/login -m \"working on login\"\n\n\
         \x20 # Release a worktree back to the pool (detaches HEAD and unlocks)\n\
         \x20 {bin_name} release syrah\n\n\
         \x20 # Navigate between worktrees (requires `eval \"$(vignoble shell-init)\"`)\n\
         \x20 {bin_name} cd syrah\n\
         \x20 {bin_name} cd -\n\n\
         {}",
        sync_symbols_help_section(),
        agent_symbols_help_section(),
        config_help_section(),
        bin_name = bin_name,
    )
}

#[derive(Parser, Debug)]
#[command(
    name = "cep",
    version = env!("CEP_VERSION"),
    about = "Manage a pool of long-living, reusable git worktrees",
    after_help = colored_after_help()
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Turn on debug output
    #[arg(short = 'd', long = "debug", env = "CEP_DEBUG", global = true)]
    pub debug: bool,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// List all worktrees (including detached/unclaimed worktrees)
    #[command(visible_alias = "ls")]
    List,

    /// Create a new worktree (defaults to random French grape variety in detached HEAD)
    #[command(visible_alias = "new", alias = "add")]
    Create(CreateArgs),

    /// Change directory to a worktree (by name, branch, path, '-' for previous, or main)
    Cd(CdArgs),

    /// Detach HEAD in a worktree so it points to no branch (preserves claim status)
    Detach(DetachArgs),

    /// Claim an available worktree for exclusive use (fails if none free unless --create)
    Claim(ClaimArgs),

    /// Release a worktree back to the pool (detaches HEAD and unlocks)
    Release(ReleaseArgs),

    /// Remove a worktree by name, branch, or path
    #[command(visible_alias = "rm")]
    Remove(RemoveArgs),
}

pub fn build_cli() -> clap::Command {
    Cli::command()
}

pub fn run(cli: Cli) -> Result<()> {
    let cwd = env::current_dir()?;
    let git = GitRunner::new(&cwd, cli.debug);
    let main_repo_root = git.main_repo_root()?;
    let config = EffectiveConfig::load(&cwd, Some(&main_repo_root))?;

    let ctx = ExecutionContext {
        git,
        main_repo_root,
        config,
    };

    match cli.command.unwrap_or(Commands::List) {
        Commands::List => list::execute(&ctx),
        Commands::Create(args) => create::execute(&ctx, &args),
        Commands::Cd(args) => cd::execute(&ctx, &args),
        Commands::Detach(args) => detach::execute(&ctx, &args),
        Commands::Claim(args) => claim::execute(&ctx, &args),
        Commands::Release(args) => release::execute(&ctx, &args),
        Commands::Remove(args) => remove::execute(&ctx, &args),
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
