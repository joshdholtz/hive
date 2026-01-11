use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

use hive::commands;

#[derive(Parser)]
#[command(name = "hive", version, about = "Hive TUI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Up {
        #[arg(long)]
        daemon: bool,
    },
    Stop,
    Status,
    Nudge { worker: Option<String> },
    Role { worker: Option<String> },
    Doctor,
    Layout { mode: String },
    Attach,
    Detach,
    #[command(hide = true)]
    Serve { config_path: PathBuf },
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();
    let cwd = std::env::current_dir()?;

    match cli.command {
        Commands::Up { daemon } => commands::up::run(&cwd, daemon),
        Commands::Stop => commands::stop::run(&cwd),
        Commands::Status => commands::status::run(&cwd),
        Commands::Nudge { worker } => commands::nudge::run(&cwd, worker.as_deref()),
        Commands::Role { worker } => commands::role::run(&cwd, worker.as_deref()),
        Commands::Doctor => commands::doctor::run(&cwd),
        Commands::Layout { mode } => commands::layout::run(&cwd, &mode),
        Commands::Attach => commands::attach::run(&cwd),
        Commands::Detach => commands::detach::run(&cwd),
        Commands::Serve { config_path } => hive::server::run(&config_path),
    }
}
