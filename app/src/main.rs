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
use uuid::Uuid;

fn mask_database_url(url: &str) -> String {
    url.find("://").map_or_else(
        || url.to_string(),
        |start| {
            let scheme = &url[..start + 3];
            let rest = &url[start + 3..];

            rest.find('@').map_or_else(
                || url.to_string(),
                |at_pos| {
                    let credentials = &rest[..at_pos];
                    let after_at = &rest[at_pos..];

                    credentials.find(':').map_or_else(
                        || url.to_string(),
                        |colon_pos| {
                            let username = &credentials[..colon_pos];
                            format!("{scheme}{username}:***{after_at}")
                        },
                    )
                },
            )
        },
    )
}

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

    match cli.command {
        Commands::Agent { message, model } => {
            let config = Config::load()?;
            info!("Loaded config from ~/nanors/config.json");

            let provider = ZhipuProvider::new(config.providers.zhipu.api_key);

            info!("Connecting to database");
            let session_manager = SessionManager::new(&config.database.url).await?;
            let agent_config = AgentConfig {
                model: model.unwrap_or_else(|| config.agents.defaults.model.clone()),
                max_tokens: config.agents.defaults.max_tokens,
                temperature: config.agents.defaults.temperature,
            };

            let agent = AgentLoop::new(provider, session_manager, agent_config);

            if let Some(msg) = message {
                let session_id = Uuid::now_v7();
                let response = agent.process_message(&session_id, &msg).await?;
                println!("{response}");
            } else {
                agent.run_interactive().await?;
            }
        }
        Commands::Init => {
            Config::create_config()?;
        }
        Commands::Version => {
            println!("nanors {}", env!("CARGO_PKG_VERSION"));
        }
        Commands::Info => {
            let config = Config::load()?;

            println!("=== nanors Configuration ===\n");

            println!("API Key:");
            let api_key = &config.providers.zhipu.api_key;
            if api_key.len() > 8 {
                let masked = format!("{}...{}", &api_key[..4], &api_key[api_key.len() - 4..]);
                println!("  Zhipu: {masked}");
            } else {
                println!("  Zhipu: ***");
            }
            println!();

            println!("Database:");
            let db_url = &config.database.url;
            println!("  URL: {}", mask_database_url(db_url));

            info!("Testing database connection");
            match SessionManager::new(db_url).await {
                Ok(_) => {
                    println!("  Status: ✅ Connected");
                }
                Err(e) => {
                    println!("  Status: ❌ Connection failed");
                    println!("  Error: {e}");
                }
            }
            println!();

            println!("Agent Defaults:");
            println!("  Model: {}", config.agents.defaults.model);
            println!("  Max Tokens: {}", config.agents.defaults.max_tokens);
            println!("  Temperature: {}", config.agents.defaults.temperature);
        }
    }

    Ok(())
}
