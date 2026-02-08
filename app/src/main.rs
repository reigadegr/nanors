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
    AgentInput, AgentStrategy, CommandStrategy, InfoStrategy, InitStrategy, VersionStrategy,
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
    /// Run agent interactively
    Agent {
        /// Single message to send
        #[arg(short = 'm', long)]
        message: Option<String>,

        /// Model to use
        #[arg(short = 'M', long)]
        model: Option<String>,
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
