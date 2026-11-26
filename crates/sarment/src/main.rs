use anyhow::Result;
use clap::{Parser, Subcommand};
use cli_core::ui::{
    color_str, DIVERGED_SYMBOL, NEED_PULL_SYMBOL, NEED_PUSH_SYMBOL, UNKNOWN_UPSTREAM_SYMBOL,
    UPSTREAM_ONLY_SYMBOL, UP_TO_DATE_SYMBOL,
};
use git_core::GitRunner;
use sarment::{
    commands::{all, clean, list, ExecutionContext},
    config::{discover_config_paths, EffectiveConfig},
};
use std::env;

fn config_help_section() -> String {
    let cwd = env::current_dir().unwrap_or_default();
    let main_repo_root = GitRunner::new(&cwd, false).main_repo_root().ok();
    let active_paths = discover_config_paths(&cwd, main_repo_root.as_deref());

    if active_paths.is_empty() {
        format!(
            "Configuration:\n\
             \x20 No configuration file found (using built-in defaults).\n\
             \x20 You can configure sarment in:\n\
             \x20   - {} (global)\n\
             \x20   - {} (local directory or any parent directory)",
            color_str("~/.config/sarment/config.toml", "cyan"),
            color_str("./.sarment", "cyan"),
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
    let bin_name = env::args()
        .next()
        .and_then(|p| {
            std::path::Path::new(&p)
                .file_name()
                .and_then(|s| s.to_str())
                .map(String::from)
        })
        .unwrap_or_else(|| "sarment".to_string());

    format!(
        "Meaning of the symbols:\n\
         \x20 * {}: upstream only, (there might be a local branch as well)\n\
         \x20 * {}: the branch is up to date with upstream\n\
         \x20 * {}: upstream is behind local branch, you need to push\n\
         \x20 * {}: local branch is behind upstream, you need to pull\n\
         \x20 * {}: local branch and upstream have diverged\n\
         \x20 * {}: local only, there is no known upstream\n\n\
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
        color_str(UPSTREAM_ONLY_SYMBOL, "green"),
        color_str(UP_TO_DATE_SYMBOL, "green"),
        color_str(NEED_PUSH_SYMBOL, "orange"),
        color_str(NEED_PULL_SYMBOL, "orange"),
        color_str(DIVERGED_SYMBOL, "red"),
        color_str(UNKNOWN_UPSTREAM_SYMBOL, "red"),
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
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Prevent repository fetch
    #[arg(short = 'u', long = "no-update", global = true)]
    no_update: bool,

    /// A pattern to filter remote branches (if omitted, only local branches are displayed)
    #[arg(long = "pattern", global = true)]
    pattern: Option<String>,

    /// Override the production branch name
    #[arg(long = "prod-branch", global = true)]
    prod_branch: Option<String>,

    /// The git remote (overrides config `remote`)
    #[arg(long = "remote", global = true)]
    remote: Option<String>,

    /// Enable squash-merge commit detection
    #[arg(long = "squash", global = true)]
    squash: bool,

    /// Answer yes to all non-dangerous actions
    #[arg(short = 'y', long = "yes", global = true)]
    force_yes: bool,

    /// Force dangerous actions
    #[arg(short = 'f', long = "force", global = true)]
    force: bool,

    /// Turn on debug output
    #[arg(short = 'd', long = "debug", env = "SARMENT_DEBUG", global = true)]
    debug: bool,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Pretty print all branches status (default)
    All,

    /// List branches in the given stage (e.g. prod, preprod, staging, dev)
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

fn main() {
    let cli = Cli::parse();
    let debug = cli.debug;

    if let Err(err) = run(cli) {
        if debug {
            eprintln!("{:?}", err);
        } else {
            eprintln!("{}", color_str(&format!("Error: {}", err), "red"));
        }
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<()> {
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
