#![warn(
    clippy::all,
    clippy::nursery,
    clippy::pedantic,
    clippy::style,
    clippy::complexity,
    clippy::perf,
    clippy::correctness,
    clippy::suspicious,
    clippy::unwrap_used,
    clippy::expect_used
)]
#![allow(
    clippy::similar_names,
    clippy::missing_safety_doc,
    clippy::missing_panics_doc,
    clippy::missing_errors_doc
)]

mod command;

use clap::{Parser, Subcommand};
use tracing::Level;
use tracing_subscriber::FmtSubscriber;

use command::{
    AgentInput, AgentStrategy, CommandStrategy, InfoStrategy, InitStrategy, TelegramInput,
    TelegramStrategy, VersionStrategy,
};

#[derive(Parser)]
#[command(name = "nanors")]
#[command(about = "nanors AI assistant", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run agent interactively (single-turn, creates new session per message)
    Agent {
        /// Single message to send
        #[arg(short = 'm', long)]
        message: Option<String>,

        /// Model to use
        #[arg(short = 'M', long)]
        model: Option<String>,

        /// Working directory for tools
        #[arg(short = 'd', long)]
        working_dir: Option<String>,
    },
    /// Initialize configuration
    Init,
    /// Show version
    Version,
    /// Show configuration information
    Info,
    /// Run Telegram bot
    Telegram {
        /// Bot token (overrides config)
        #[arg(short = 't', long)]
        token: Option<String>,

        /// Allowed chat IDs (comma-separated, overrides config)
        #[arg(short = 'a', long)]
        allow_from: Option<String>,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();

    tracing::subscriber::set_global_default(subscriber)?;

    let cli = Cli::parse();

    // Static dispatch to command strategies.
    // Each strategy is a zero-sized type (ZST) with no runtime overhead.
    // The compiler will monomorphize each call, enabling full optimization.
    match cli.command {
        Commands::Agent {
            message,
            model,
            working_dir,
        } => {
            AgentStrategy
                .execute(AgentInput {
                    message,
                    model,
                    working_dir,
                })
                .await?;
        }
        Commands::Init => {
            InitStrategy.execute(()).await?;
        }
        Commands::Version => {
            VersionStrategy.execute(()).await?;
        }
        Commands::Info => {
            InfoStrategy.execute(()).await?;
        }
        Commands::Telegram { token, allow_from } => {
            let allow_from = allow_from.map(|s| s.split(',').map(String::from).collect());
            TelegramStrategy
                .execute(TelegramInput { token, allow_from })
                .await?;
        }
    }

    Ok(())
}
