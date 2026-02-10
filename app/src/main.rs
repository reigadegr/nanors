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
use uuid::Uuid;

use command::{
    AgentInput, AgentStrategy, ChatInput, ChatStrategy, CommandStrategy, InfoStrategy,
    InitStrategy, VersionStrategy,
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
    },
    /// Multi-turn conversation with persistent session
    Chat {
        /// Resume existing session by ID
        #[arg(short = 's', long)]
        session: Option<String>,

        /// Single message to send (non-interactive mode)
        #[arg(short = 'm', long)]
        message: Option<String>,

        /// Model to use
        #[arg(short = 'M', long)]
        model: Option<String>,

        /// Session name (for new sessions)
        #[arg(short = 'n', long)]
        name: Option<String>,

        /// Number of messages to keep in context
        #[arg(short = 'H', long)]
        history: Option<usize>,
    },
    /// Initialize configuration
    Init,
    /// Show version
    Version,
    /// Show configuration information
    Info,
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
        Commands::Agent { message, model } => {
            AgentStrategy.execute(AgentInput { message, model }).await?;
        }
        Commands::Chat {
            session,
            message,
            model,
            name,
            history,
        } => {
            let session_id = session.and_then(|s| Uuid::parse_str(&s).ok());
            ChatStrategy
                .execute(ChatInput {
                    session_id,
                    message,
                    model,
                    session_name: name,
                    history_limit: history,
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
    }

    Ok(())
}
