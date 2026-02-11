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
use tracing::info;

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

        // Initialize memory manager
        let memory_manager = Arc::new(
            MemoryManager::new(&config.database.url)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to initialize memory manager: {e}"))?,
        );

        // Create and run bot
        let bot = TelegramBot::new(token, provider, memory_manager, config, &allow_from)?;

        info!("Telegram bot is running. Press Ctrl+C to stop.");
        bot.run().await?;

        Ok(())
    }
}
