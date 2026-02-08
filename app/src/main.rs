#![deny(
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

use clap::{Parser, Subcommand};
use nanors_config::Config;
use nanors_core::{AgentConfig, AgentLoop};
use nanors_providers::ZhipuProvider;
use nanors_session::SessionManager;
use tracing::{Level, info};
use tracing_subscriber::FmtSubscriber;

#[derive(Parser)]
#[command(name = "nanobot")]
#[command(about = "nanobot AI assistant", long_about = None)]
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
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();

    tracing::subscriber::set_global_default(subscriber)?;

    let cli = Cli::parse();

    match cli.command {
        Commands::Agent { message, model } => {
            let config = Config::load()?;
            info!("Loaded config from ~/nanors/config.json");

            let provider = ZhipuProvider::new(config.providers.zhipu.api_key);
            let home_dir =
                dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Cannot find home directory"))?;
            let nanobot_dir = home_dir.join("nanors");
            let db_path = nanobot_dir.join("sessions.db");

            info!("Database path: {}", db_path.display());

            let session_manager = SessionManager::new(db_path).await?;
            let agent_config = AgentConfig {
                model: model.unwrap_or_else(|| config.agents.defaults.model.clone()),
                max_tokens: config.agents.defaults.max_tokens,
                temperature: config.agents.defaults.temperature,
            };

            let agent = AgentLoop::new(provider, session_manager, agent_config);

            if let Some(msg) = message {
                let response = agent.process_message("cli:default", &msg).await?;
                println!("{response}");
            } else {
                agent.run_interactive().await?;
            }
        }
        Commands::Init => {
            Config::create_config()?;
        }
        Commands::Version => {
            println!("nanobot {}", env!("CARGO_PKG_VERSION"));
        }
    }

    Ok(())
}
