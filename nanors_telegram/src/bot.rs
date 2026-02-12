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

use crate::{Error, Result};
use nanors_config::Config;
use nanors_conversation::{ConversationConfig, ConversationManager, TurnContext};
use nanors_core::{
    AgentConfig, AgentLoop, DEFAULT_SYSTEM_PROMPT, LLMProvider, MemoryItem, MemoryItemRepo,
    SessionStorage,
};
use nanors_memory::MemoryManager;
use nanors_providers::ZhipuProvider;
use std::{collections::HashMap, sync::Arc, time::Duration};
use teloxide::prelude::*;
use tokio::time::sleep;
use tracing::{debug, info, warn};
use uuid::Uuid;

/// Session data for each chat
#[derive(Clone)]
struct SessionData {
    session_id: Uuid,
    manager: Arc<tokio::sync::Mutex<ConversationManager<ZhipuProvider, Arc<MemoryManager>>>>,
}

/// Build `ConversationConfig` from bot config with parameters.
fn build_conversation_config(
    config: &Config,
    session_id: Uuid,
    session_name: Option<String>,
) -> ConversationConfig {
    let system_prompt = config
        .agents
        .defaults
        .system_prompt
        .clone()
        .unwrap_or_else(|| DEFAULT_SYSTEM_PROMPT.to_string());
    let history_limit = config.agents.defaults.history_limit.unwrap_or(20);

    ConversationConfig {
        session_id,
        session_name,
        system_prompt,
        model: config.agents.defaults.model.clone(),
        temperature: config.agents.defaults.temperature,
        max_tokens: config.agents.defaults.max_tokens,
        history_limit,
    }
}

/// Build `AgentConfig` from bot config.
fn build_agent_config(config: &Config) -> AgentConfig {
    AgentConfig {
        model: config.agents.defaults.model.clone(),
        max_tokens: config.agents.defaults.max_tokens,
        temperature: config.agents.defaults.temperature,
    }
}

/// Telegram Bot with AI integration
pub struct TelegramBot {
    /// Teloxide bot instance
    pub bot: Bot,
    /// Zhipu AI provider
    provider: ZhipuProvider,
    /// Memory manager for session and long-term storage
    pub memory_manager: Arc<MemoryManager>,
    /// Configuration
    pub config: Config,
    /// Session mapping: `chat_id` -> (`session_id`, `conversation_manager`)
    sessions: Arc<tokio::sync::Mutex<HashMap<i64, SessionData>>>,
    /// Allowed chat IDs
    allowed_chats: Vec<i64>,
}

impl TelegramBot {
    /// Create a new Telegram bot
    pub fn new(
        token: String,
        provider: ZhipuProvider,
        memory_manager: Arc<MemoryManager>,
        config: Config,
        allowed_chats: &[String],
    ) -> Result<Self> {
        // Parse allowed chat IDs
        let allowed_chats = allowed_chats
            .iter()
            .filter_map(|s| s.parse::<i64>().ok())
            .collect();

        let bot = Bot::new(token);

        Ok(Self {
            bot,
            provider,
            memory_manager,
            config,
            sessions: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            allowed_chats,
        })
    }

    /// Check if a chat is allowed
    #[must_use]
    pub fn is_allowed(&self, chat_id: i64) -> bool {
        self.allowed_chats.is_empty() || self.allowed_chats.contains(&chat_id)
    }

    /// Get or create a session manager for a chat
    async fn get_or_create_session_manager(&self, chat_id: i64) -> Result<SessionData> {
        // Check authorization
        if !self.is_allowed(chat_id) {
            return Err(Error::Unauthorized(chat_id));
        }

        // Return existing session if available
        {
            let sessions = self.sessions.lock().await;
            if let Some(data) = sessions.get(&chat_id) {
                return Ok(data.clone());
            }
        }

        // Create new session
        let session_id = Uuid::now_v7();

        // Get or create session from storage
        let storage: Arc<dyn SessionStorage> = self.memory_manager.clone();
        storage
            .get_or_create(&session_id)
            .await
            .map_err(|e| Error::Memory(anyhow::anyhow!("Failed to create session: {e}")))?;

        // Create conversation config
        let conversation_config =
            build_conversation_config(&self.config, session_id, Some(format!("TG:{chat_id}")));

        // Create conversation manager
        let manager = ConversationManager::new(
            self.provider.clone(),
            self.memory_manager.clone(),
            conversation_config,
        )
        .await
        .map_err(|e| Error::Config(e.to_string()))?;

        let data = SessionData {
            session_id,
            manager: Arc::new(tokio::sync::Mutex::new(manager)),
        };

        {
            let mut sessions = self.sessions.lock().await;
            sessions.insert(chat_id, data.clone());
        }

        Ok(data)
    }

    /// Reset session for a chat
    pub async fn reset_session(&self, chat_id: i64) -> Result<()> {
        let session_id = {
            let mut sessions = self.sessions.lock().await;
            sessions.remove(&chat_id).map(|d| d.session_id)
        };

        if let Some(id) = session_id {
            self.memory_manager
                .clear_session(&id)
                .await
                .map_err(Error::Memory)?;
        }

        Ok(())
    }

    /// Process a message and get response
    pub async fn process_message(&self, chat_id: i64, text: String) -> Result<String> {
        let session_data = self.get_or_create_session_manager(chat_id).await?;
        let mut manager = session_data.manager.lock().await;

        // Step 1: Use AgentLoop to retrieve memory and build system prompt
        let agent_config = build_agent_config(&self.config);

        let agent_loop = AgentLoop::new(
            self.provider.clone(),
            self.memory_manager.clone(),
            agent_config,
        )
        .with_memory(self.memory_manager.clone());

        // Retrieve memory and build system prompt
        let memory_context = agent_loop.build_system_prompt(&text).await;

        // Step 2: Pass retrieved context to ConversationManager
        let mut context = TurnContext::new(text.clone());
        context.retrieved_context = Some(memory_context);

        // Step 3: Process turn with memory-enhanced context
        let result = manager
            .process_turn(context)
            .await
            .map_err(|e| Error::Provider(anyhow::anyhow!("{e}")))?;

        // Step 4: Save user message to long-term memory
        let now = chrono::Utc::now();
        let user_embedding = match self.provider.embed(&text).await {
            Ok(embedding) => Some(embedding),
            Err(e) => {
                debug!("Failed to generate user embedding: {e}");
                None
            }
        };

        let user_memory = MemoryItem::create_episodic(&text, user_embedding, now);

        // Use semantic upsert to handle fact updates (e.g., location changes)
        match self
            .memory_manager
            .semantic_upsert(&user_memory, 0.85)
            .await
        {
            Ok(id) => {
                debug!("Stored user memory: {}", id);
            }
            Err(e) => {
                debug!("Failed to store user memory: {e}");
            }
        }

        drop(manager);
        Ok(result.response)
    }

    /// Test connection to Telegram API with exponential backoff retry.
    /// Starts at 2s, increases by 2s each attempt, max 10s delay.
    /// Retries indefinitely until connection succeeds.
    async fn test_connection(&self) -> Result<()> {
        const INITIAL_DELAY_SECS: u64 = 2;
        const MAX_DELAY_SECS: u64 = 10;

        let mut attempt = 1u64;
        loop {
            match self.bot.get_me().await {
                Ok(bot_user) => {
                    info!(
                        "Connected to Telegram API: @{} (id: {})",
                        bot_user
                            .user
                            .username
                            .unwrap_or_else(|| "no username".to_string()),
                        bot_user.user.id
                    );
                    return Ok(());
                }
                Err(e) => {
                    // Calculate delay with exponential backoff: 2s, 4s, 6s, 8s, 10s, 10s, ...
                    let delay_secs = (INITIAL_DELAY_SECS * attempt).min(MAX_DELAY_SECS);
                    let delay = Duration::from_secs(delay_secs);

                    warn!("Connection attempt {attempt} failed: {e}. Retrying in {delay_secs}s...");

                    // Only show detailed help on first failure
                    if attempt == 1 {
                        warn!("This may be due to:");
                        warn!("  - Network connectivity issues");
                        warn!("  - Firewall blocking api.telegram.org");
                        warn!("  - Invalid bot token");
                        warn!("  - Telegram API being temporarily unavailable");
                        warn!("  - Proxy or VPN configuration required");
                    }

                    sleep(delay).await;
                    attempt += 1;
                }
            }
        }
    }

    /// Run the bot
    pub async fn run(self) -> Result<()> {
        use teloxide::dispatching::{Dispatcher, UpdateFilterExt};
        use teloxide::dptree;
        use teloxide::types::Update;

        // Test connection with exponential backoff retry before starting dispatcher
        self.test_connection().await?;

        let bot = self.bot.clone();

        let schema = dptree::entry().branch(Update::filter_message().endpoint({
            let bot_clone = self.clone();
            move |_bot: Bot, msg: teloxide::types::Message| {
                let bot_clone = bot_clone.clone();
                async move { crate::handler::handle_message(bot_clone, msg).await }
            }
        }));

        Dispatcher::builder(bot, schema)
            .enable_ctrlc_handler()
            .build()
            .dispatch()
            .await;

        Ok(())
    }
}

impl Clone for TelegramBot {
    fn clone(&self) -> Self {
        Self {
            bot: self.bot.clone(),
            provider: self.provider.clone(),
            memory_manager: Arc::clone(&self.memory_manager),
            config: self.config.clone(),
            sessions: Arc::clone(&self.sessions),
            allowed_chats: self.allowed_chats.clone(),
        }
    }
}
