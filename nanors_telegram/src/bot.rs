use crate::{Error, Result};
use nanors_config::Config;
use nanors_core::{AgentConfig, AgentLoop, SessionStorage};
use nanors_memory::MemoryManager;
use nanors_providers::ZhipuProvider;
use nanors_tools::{
    BashTool, EditFileTool, GlobTool, GrepTool, ReadFileTool, ToolRegistry, WriteFileTool,
};
use std::{collections::HashMap, sync::Arc, time::Duration};
use teloxide::prelude::*;
use tokio::time::sleep;
use tracing::{info, warn};
use uuid::Uuid;

/// Session data for each chat
#[derive(Clone)]
struct SessionData {
    session_id: Uuid,
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
    /// Working directory for tools
    working_dir: String,
}

impl TelegramBot {
    /// Create a new Telegram bot
    pub fn new(
        token: String,
        provider: ZhipuProvider,
        memory_manager: Arc<MemoryManager>,
        config: Config,
        allowed_chats: &[String],
        working_dir: String,
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
            working_dir,
        })
    }

    /// Check if a chat is allowed
    #[must_use]
    pub fn is_allowed(&self, chat_id: i64) -> bool {
        self.allowed_chats.is_empty() || self.allowed_chats.contains(&chat_id)
    }

    /// Get or create a session ID for a chat
    async fn get_or_create_session_id(&self, chat_id: i64) -> Result<Uuid> {
        // Check authorization
        if !self.is_allowed(chat_id) {
            return Err(Error::Unauthorized(chat_id));
        }

        // Return existing session if available
        {
            let sessions = self.sessions.lock().await;
            if let Some(data) = sessions.get(&chat_id) {
                return Ok(data.session_id);
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

        let data = SessionData { session_id };

        {
            let mut sessions = self.sessions.lock().await;
            sessions.insert(chat_id, data.clone());
        }

        Ok(session_id)
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
        let session_id = self.get_or_create_session_id(chat_id).await?;

        // Build agent config
        let agent_config = build_agent_config(&self.config);

        // Register tools
        let mut tool_registry = ToolRegistry::new();
        tool_registry.add_tool(Box::new(BashTool::new(&self.working_dir)));
        tool_registry.add_tool(Box::new(ReadFileTool::new(&self.working_dir)));
        tool_registry.add_tool(Box::new(WriteFileTool::new(&self.working_dir)));
        tool_registry.add_tool(Box::new(EditFileTool::new(&self.working_dir)));
        tool_registry.add_tool(Box::new(GlobTool::new(&self.working_dir)));
        tool_registry.add_tool(Box::new(GrepTool::new(&self.working_dir)));

        // Use AgentLoop with tool support
        let agent_loop = AgentLoop::new(
            self.provider.clone(),
            self.memory_manager.clone(),
            agent_config,
        )
        .with_memory(self.memory_manager.clone())
        .with_tools(tool_registry);

        // Process message with tool calling support
        let response = agent_loop
            .process_message(&session_id, &text)
            .await
            .map_err(|e| Error::Provider(anyhow::anyhow!("{e}")))?;

        Ok(response)
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
            working_dir: self.working_dir.clone(),
        }
    }
}
