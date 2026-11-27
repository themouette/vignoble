pub mod commands;
pub mod config;
pub mod parcelle;
pub mod store;
pub mod template;
pub mod tmux;

use anyhow::Result;
use clap::{CommandFactory, Parser, Subcommand};
use cli_core::{config::resolve_repo_name, ui::color_str};
use commands::{
    cancel::{self, CancelArgs},
    clean::{self, CleanArgs},
    edit::{self, EditArgs},
    enqueue::{self, EnqueueArgs},
    exec::{self, ExecArgs},
    inspect::{self, InspectArgs},
    list::{self, ListArgs},
    logs::{self, LogsArgs},
    resume::{self, ResumeArgs},
    tick::{self, TickArgs},
    ExecutionContext,
};
use config::{discover_config_paths, EffectiveConfig};
use git_core::GitRunner;
use std::{env, ffi::OsString, path::PathBuf};
use store::VignobleStore;

fn config_help_section() -> String {
    let cwd = env::current_dir().unwrap_or_default();
    let main_repo_root = GitRunner::new(&cwd, false).main_repo_root().ok();
    let active_paths = discover_config_paths(&cwd, main_repo_root.as_deref());

    if active_paths.is_empty() {
        format!(
            "Configuration:\n\
             \x20 No configuration file found (using built-in defaults).\n\
             \x20 You can configure saisonnier in:\n\
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

fn grouped_commands_help() -> String {
    format!(
        "{}\n\
         \x20 {:<10} Show queued tasks, active saisonniers, and recent runs (default) [aliases: status, ls]\n\
         \x20 {:<10} Enqueue an autonomous task and trigger the worker queue [aliases: run, add]\n\
         \x20 {:<10} Edit a queued task's prompt or options ($EDITOR or flags)\n\
         \x20 {:<10} Remove a queued task or stop an active saisonnier [alias: stop]\n\
         \x20 {:<10} Remove finished run receipts and release held ceps [aliases: rm, prune]\n\n\
         {}\n\
         \x20 {:<10} View or follow (-f) output logs for an active or past run\n\
         \x20 {:<10} Inspect a queued task or run receipt, prompt, and trailing log output\n\
         \x20 {:<10} Resume a past run interactively in the current terminal on an available cep\n\
         \x20 {:<10} Claim available ceps for queued tasks and reconcile crashed sessions\n\
         \x20 {:<10} Print this message or the help of the given subcommand(s)",
        color_str("Task & Queue Management:", "bold"),
        color_str("list", "cyan"),
        color_str("enqueue", "cyan"),
        color_str("edit", "cyan"),
        color_str("cancel", "cyan"),
        color_str("clean", "cyan"),
        color_str("Worker & Run Execution:", "bold"),
        color_str("logs", "cyan"),
        color_str("inspect", "cyan"),
        color_str("resume", "cyan"),
        color_str("tick", "cyan"),
        color_str("help", "cyan"),
    )
}

fn colored_after_help() -> String {
    let bin_name = "saisonnier";

    format!(
        "More usage examples:\n\n\
         \x20 # Show queued tasks, active saisonniers, and recent runs\n\
         \x20 {bin_name}\n\n\
         \x20 # Enqueue a new autonomous task and drain the queue\n\
         \x20 {bin_name} enqueue \"Add unit tests for auth token refresh\"\n\n\
         \x20 # Enqueue with an explicit branch and agent profile\n\
         \x20 {bin_name} run -b feat/token-tests --profile claude -f task.md\n\n\
         \x20 # Edit a queued task in $EDITOR or via flags\n\
         \x20 {bin_name} edit <task-id> -b feat/new-branch\n\n\
         \x20 # Follow live logs or inspect a run's receipt, prompt, and log tail\n\
         \x20 {bin_name} logs -f <run-id>\n\
         \x20 {bin_name} inspect <run-id>\n\n\
         \x20 # Resume a past run interactively in the current terminal\n\
         \x20 {bin_name} resume <run-id>\n\n\
         \x20 # Cancel a queued task, stop a running worker, or clean finished runs\n\
         \x20 {bin_name} cancel <task-or-run-id>\n\
         \x20 {bin_name} clean <run-id>\n\n\
         {}",
        config_help_section(),
        bin_name = bin_name,
    )
}

fn top_level_help_template() -> String {
    format!(
        "{{about-with-newline}}\n\
         {{usage-heading}} {{usage}}\n\n\
         {}\n\n\
         {}\n{{options}}\n\n\
         {{after-help}}",
        grouped_commands_help(),
        color_str("Options:", "bold"),
    )
}

#[derive(Parser, Debug)]
#[command(
    name = "saisonnier",
    version = env!("SAISONNIER_VERSION"),
    about = "Run autonomous coding-agent tasks across a pool of cep worktrees",
    help_template = top_level_help_template(),
    after_help = colored_after_help()
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Turn on debug output
    #[arg(short = 'd', long = "debug", env = "SAISONNIER_DEBUG", global = true)]
    pub debug: bool,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Show queued tasks, active saisonniers, and recent runs (default)
    #[command(visible_alias = "status", visible_alias = "ls")]
    List(ListArgs),

    /// Enqueue an autonomous task and trigger the worker queue
    #[command(visible_alias = "run", visible_alias = "add")]
    Enqueue(EnqueueArgs),

    /// Edit a queued task's prompt or options ($EDITOR or flags)
    Edit(EditArgs),

    /// Remove a queued task or stop an active saisonnier
    #[command(visible_alias = "stop")]
    Cancel(CancelArgs),

    /// Remove finished run receipts and release held ceps
    #[command(visible_alias = "rm", visible_alias = "prune")]
    Clean(CleanArgs),

    /// View or follow (-f) output logs for an active or past run
    Logs(LogsArgs),

    /// Inspect a queued task or run receipt, prompt, and trailing log output
    Inspect(InspectArgs),

    /// Resume a past run interactively in the current terminal on an available cep
    Resume(ResumeArgs),

    /// Claim available ceps for queued tasks and reconcile crashed sessions
    Tick(TickArgs),
}

impl Commands {
    fn has_parcelle(&self) -> bool {
        match self {
            Commands::List(a) => a.parcelle.is_some(),
            Commands::Enqueue(a) => a.parcelle.is_some(),
            Commands::Edit(a) => a.parcelle.is_some(),
            Commands::Cancel(a) => a.parcelle.is_some(),
            Commands::Clean(a) => a.parcelle.is_some(),
            Commands::Logs(a) => a.parcelle.is_some(),
            Commands::Inspect(a) => a.parcelle.is_some(),
            Commands::Resume(a) => a.parcelle.is_some(),
            Commands::Tick(a) => a.parcelle.is_some(),
        }
    }
}

pub fn build_cli() -> clap::Command {
    Cli::command()
}

fn try_parse_internal_exec(args: &[OsString]) -> Option<(bool, ExecArgs)> {
    let mut debug = false;
    let mut idx = 1;
    while idx < args.len() {
        let s = args[idx].to_str()?;
        if s == "-d" || s == "--debug" {
            debug = true;
            idx += 1;
        } else {
            break;
        }
    }
    if args.get(idx).and_then(|s| s.to_str()) != Some("exec") {
        return None;
    }
    idx += 1;
    let mut inline = false;
    let mut run_id: Option<String> = None;
    while idx < args.len() {
        match args[idx].to_str()? {
            "-d" | "--debug" => debug = true,
            "--inline" => inline = true,
            val if !val.starts_with('-') && run_id.is_none() => {
                run_id = Some(val.to_string());
            }
            _ => return None,
        }
        idx += 1;
    }
    Some((
        debug,
        ExecArgs {
            run_id: run_id?,
            inline,
        },
    ))
}

fn build_local_context(debug: bool) -> Result<ExecutionContext> {
    let cwd = env::current_dir()?;
    let git = GitRunner::new(&cwd, debug);
    let main_repo_root = git.main_repo_root()?;
    let common_dir = git.git_common_dir()?;
    let config = EffectiveConfig::load(&cwd, Some(&main_repo_root))?;
    let repo_name =
        resolve_repo_name(&cwd, Some(&main_repo_root)).unwrap_or_else(|| "repo".to_string());
    let store = VignobleStore::from_git_common_dir(&common_dir);
    Ok(ExecutionContext {
        git,
        main_repo_root,
        repo_name,
        store,
        config,
    })
}

pub fn run_from_args<I, T>(args: I) -> Result<()>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let collected: Vec<OsString> = args.into_iter().map(Into::into).collect();
    if let Some((debug, exec_args)) = try_parse_internal_exec(&collected) {
        let ctx = build_local_context(debug)?;
        return exec::execute(&ctx, &exec_args);
    }
    let cli = Cli::try_parse_from(collected)?;
    run(cli)
}

pub fn run(cli: Cli) -> Result<()> {
    let cwd = env::current_dir()?;
    let git = GitRunner::new(&cwd, cli.debug);
    let command = cli
        .command
        .unwrap_or_else(|| Commands::List(ListArgs::default()));

    let ctx = if command.has_parcelle() {
        let main_root = git.main_repo_root().ok();
        let config = EffectiveConfig::load(&cwd, main_root.as_deref())?;
        let repo_name =
            resolve_repo_name(&cwd, main_root.as_deref()).unwrap_or_else(|| "repo".to_string());
        let store = VignobleStore::from_git_common_dir(&PathBuf::new());
        ExecutionContext {
            git,
            main_repo_root: main_root.unwrap_or_else(|| cwd.clone()),
            repo_name,
            store,
            config,
        }
    } else {
        build_local_context(cli.debug)?
    };

    match command {
        Commands::List(args) => list::execute(&ctx, &args),
        Commands::Enqueue(args) => enqueue::execute(&ctx, &args),
        Commands::Edit(args) => edit::execute(&ctx, &args),
        Commands::Cancel(args) => cancel::execute(&ctx, &args),
        Commands::Clean(args) => clean::execute(&ctx, &args),
        Commands::Logs(args) => logs::execute(&ctx, &args),
        Commands::Inspect(args) => inspect::execute(&ctx, &args),
        Commands::Resume(args) => resume::execute(&ctx, &args),
        Commands::Tick(args) => tick::execute(&ctx, &args),
    }
}

pub fn main_from_args<I, T>(args: I) -> !
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let collected: Vec<OsString> = args.into_iter().map(Into::into).collect();
    if let Some((debug, exec_args)) = try_parse_internal_exec(&collected) {
        let res = build_local_context(debug).and_then(|ctx| exec::execute(&ctx, &exec_args));
        if let Err(err) = res {
            if debug {
                eprintln!("{:?}", err);
            } else {
                eprintln!("{}", color_str(&format!("Error: {}", err), "red"));
            }
            std::process::exit(1);
        }
        std::process::exit(0);
    }

    let cli = Cli::parse_from(collected);
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
