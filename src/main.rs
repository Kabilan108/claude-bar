use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::{generate, Shell};
use std::fs::{self, OpenOptions};
use std::io;
use std::path::PathBuf;
use tracing_subscriber::{
    fmt::{self, format::FmtSpan},
    layer::SubscriberExt,
    util::SubscriberInitExt,
    EnvFilter,
};

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

fn log_file_path() -> Option<PathBuf> {
    dirs::data_local_dir().map(|d| d.join("claude-bar").join("claude-bar.log"))
}

fn init_logging(for_daemon: bool) {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    let registry = tracing_subscriber::registry().with(filter);

    if for_daemon {
        let journald_layer = tracing_journald::layer().ok();

        let file_layer = log_file_path().and_then(|path| {
            if let Some(parent) = path.parent() {
                if fs::create_dir_all(parent).is_err() {
                    return None;
                }
            }
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .ok()
                .map(|file| {
                    fmt::layer()
                        .json()
                        .with_writer(file)
                        .with_span_events(FmtSpan::NONE)
                })
        });

        let console_layer = fmt::layer().with_target(true).with_level(true);

        registry
            .with(journald_layer)
            .with(file_layer)
            .with(console_layer)
            .init();
    } else {
        let console_layer = fmt::layer()
            .with_target(false)
            .with_level(true)
            .compact();

        registry.with(console_layer).init();
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Daemon => {
            init_logging(true);
            daemon::run().await
        }
        Commands::Status { json, provider } => {
            init_logging(false);
            cli::status::run(json, provider).await
        }
        Commands::Cost { json, days } => {
            init_logging(false);
            cli::cost::run(json, days).await
        }
        Commands::Refresh => {
            init_logging(false);
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
