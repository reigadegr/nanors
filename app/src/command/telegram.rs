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

use crate::command::CommandStrategy;
use nanors_config::Config;
use nanors_memory::MemoryManager;
use nanors_providers::ZhipuProvider;
use nanors_telegram::TelegramBot;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{info, warn};

/// Connect to memory manager with exponential backoff retry.
///
/// # Retry Behavior
/// - First retry: 1s
/// - Second retry: 2s
/// - Third and beyond: 3s (capped)
/// - Retries indefinitely until connection succeeds
async fn connect_memory_manager_with_retry(database_url: &str) -> anyhow::Result<MemoryManager> {
    const MAX_DELAY: Duration = Duration::from_secs(3);
    const INITIAL_DELAY: Duration = Duration::from_secs(1);

    let mut attempt = 0u32;
    let mut delay = INITIAL_DELAY;

    loop {
        attempt += 1;
        match MemoryManager::new(database_url).await {
            Ok(manager) => {
                info!("Memory manager connected successfully on attempt {attempt}");
                return Ok(manager);
            }
            Err(e) => {
                warn!(
                    "Failed to connect to database (attempt {attempt}): {e}. Retrying in {}s...",
                    delay.as_secs()
                );
                sleep(delay).await;
                // Exponential backoff: 1s -> 2s -> 3s -> 3s -> ...
                delay = (delay * 2).saturating_add(Duration::ZERO).min(MAX_DELAY);
            }
        }
    }
}

/// Input for Telegram bot command.
pub struct TelegramInput {
    /// Optional bot token (overrides config)
    pub token: Option<String>,
    /// Optional allowed chat IDs (overrides config)
    pub allow_from: Option<Vec<String>>,
}

/// Strategy for running Telegram bot.
pub struct TelegramStrategy;

impl CommandStrategy for TelegramStrategy {
    type Input = TelegramInput;

    async fn execute(&self, input: Self::Input) -> anyhow::Result<()> {
        let config = Config::load()?;

        if !config.telegram.enabled {
            anyhow::bail!("Telegram is not enabled in config. Set \"telegram.enabled\": true");
        }

        // Get token from input or config
        let token = if let Some(t) = input.token {
            t
        } else if !config.telegram.token.is_empty() {
            config.telegram.token.clone()
        } else {
            anyhow::bail!("Telegram bot token not configured. Set \"telegram.token\" in config");
        };

        // Get allowed chats from input or config
        let allow_from = input
            .allow_from
            .unwrap_or_else(|| config.telegram.allow_from.clone());

        info!("Starting Telegram bot...");

        // Initialize provider
        let provider = ZhipuProvider::new(config.providers.zhipu.api_key.clone());

        // Initialize memory manager with exponential backoff retry
        let memory_manager =
            Arc::new(connect_memory_manager_with_retry(&config.database.url).await?);

        // Create and run bot
        let bot = TelegramBot::new(token, provider, memory_manager, config, &allow_from)?;

        info!("Telegram bot is running. Press Ctrl+C to stop.");
        bot.run().await?;

        Ok(())
    }
}
