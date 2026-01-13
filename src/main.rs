use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

use hive::commands;

#[derive(Parser)]
#[command(name = "hive", version, about = "Hive TUI")]
struct Cli {
    /// Run as if hive was started in <dir> instead of the current directory
    #[arg(short = 'C', global = true, value_name = "dir")]
    directory: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start or attach to hive
    Up {
        #[arg(long)]
        daemon: bool,
    },
    /// Stop the hive server
    Down,
    /// Show session status
    Status,
    /// Send nudge message to workers
    Nudge { worker: Option<String> },
    /// Regenerate role files
    Role { worker: Option<String> },
    /// Check and fix hive configuration
    Doctor,
    /// Remove hive configuration
    Deinit,
    /// Change pane layout
    Layout { mode: String },
    /// Attach to running hive session
    Attach,
    /// Detach from hive session
    Detach,
    /// List all workspaces
    List,
    /// Open a workspace by name
    Open {
        /// Workspace name
        name: String,
        #[arg(long)]
        daemon: bool,
    },
    #[command(hide = true)]
    Serve { config_path: PathBuf },
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();
    let cwd = match cli.directory {
        Some(dir) => std::fs::canonicalize(&dir)?,
        None => std::env::current_dir()?,
    };

    match cli.command {
        Commands::Up { daemon } => commands::up::run(&cwd, daemon),
        Commands::Down => commands::down::run(&cwd),
        Commands::Status => commands::status::run(&cwd),
        Commands::Nudge { worker } => commands::nudge::run(&cwd, worker.as_deref()),
        Commands::Role { worker } => commands::role::run(&cwd, worker.as_deref()),
        Commands::Doctor => commands::doctor::run(&cwd),
        Commands::Deinit => commands::deinit::run(&cwd),
        Commands::Layout { mode } => commands::layout::run(&cwd, &mode),
        Commands::Attach => commands::attach::run(&cwd),
        Commands::Detach => commands::detach::run(&cwd),
        Commands::List => commands::list::run(),
        Commands::Open { name, daemon } => commands::open::run(&name, daemon),
        Commands::Serve { config_path } => hive::server::run(&config_path),
    }
}
