use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::{generate, Shell};
use std::io;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

mod cli;
mod core;
mod cost;
mod daemon;
mod icons;
mod providers;
mod ui;

#[derive(Parser)]
#[command(name = "claude-bar")]
#[command(author, version, about = "Linux system tray for AI coding assistant usage monitoring")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the tray daemon
    Daemon,

    /// Show current usage status
    Status {
        /// Output as JSON
        #[arg(long)]
        json: bool,

        /// Filter by provider name
        #[arg(long)]
        provider: Option<String>,
    },

    /// Show cost summary
    Cost {
        /// Output as JSON
        #[arg(long)]
        json: bool,

        /// Number of days to include (default: 30)
        #[arg(long, default_value = "30")]
        days: u32,
    },

    /// Trigger daemon refresh via D-Bus
    Refresh,

    /// Generate shell completions
    Completions {
        /// Shell to generate completions for
        #[arg(value_enum)]
        shell: Shell,
    },
}

fn init_logging() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer())
        .init();
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Daemon => {
            init_logging();
            daemon::run().await
        }
        Commands::Status { json, provider } => {
            init_logging();
            cli::status::run(json, provider).await
        }
        Commands::Cost { json, days } => {
            init_logging();
            cli::cost::run(json, days).await
        }
        Commands::Refresh => {
            init_logging();
            cli::refresh::run().await
        }
        Commands::Completions { shell } => {
            let mut cmd = Cli::command();
            let name = cmd.get_name().to_string();
            generate(shell, &mut cmd, name, &mut io::stdout());
            Ok(())
        }
    }
}
