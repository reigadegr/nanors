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

use crate::command::{CommandStrategy, init_common_components};
use nanors_telegram::TelegramBot;
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
        let common = init_common_components().await?;

        // Get token from input or config
        let token = if let Some(t) = input.token {
            t
        } else if !common.config.telegram.token.is_empty() {
            common.config.telegram.token.clone()
        } else {
            anyhow::bail!("Telegram bot token not configured. Set \"telegram.token\" in config");
        };

        // Get allowed chats from input or config
        let allow_from = input
            .allow_from
            .unwrap_or_else(|| common.config.telegram.allow_from.clone());

        info!("Starting Telegram bot...");

        // Create and run bot (tools use current directory)
        let bot = TelegramBot::new(
            token,
            common.provider,
            common.memory_manager,
            common.config,
            &allow_from,
            ".".to_string(),
        )?;

        info!("Telegram bot is running. Press Ctrl+C to stop.");
        bot.run().await?;

        Ok(())
    }
}
