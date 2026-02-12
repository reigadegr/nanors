//! Multi-turn conversation command with persistent sessions.
//!
//! Unlike the `agent` command which creates a new session per message,
//! this command maintains conversation context across multiple turns.

use nanors_config::Config;
use nanors_conversation::{ConversationConfig, ConversationManager, TurnContext};
use nanors_memory::MemoryManager;
use nanors_providers::ZhipuProvider;
use std::sync::Arc;
use tracing::info;
use uuid::Uuid;

/// Input parameters for the Chat command strategy.
#[derive(Debug, Clone)]
pub struct ChatInput {
    /// Optional session ID to resume (creates new if not provided)
    pub session_id: Option<Uuid>,
    /// Optional single message to send (non-interactive mode)
    pub message: Option<String>,
    /// Optional model override
    pub model: Option<String>,
    /// Session name (for new sessions)
    pub session_name: Option<String>,
    /// Number of messages to keep in context
    pub history_limit: Option<usize>,
}

/// Strategy for executing the Chat command.
///
/// This strategy handles multi-turn conversations:
/// - Creates or resumes a persistent session
/// - Maintains conversation history across turns
/// - Optional memory retrieval integration
/// - Configurable history window
#[derive(Debug, Clone, Copy)]
pub struct ChatStrategy;

impl super::CommandStrategy for ChatStrategy {
    type Input = ChatInput;

    async fn execute(&self, input: Self::Input) -> anyhow::Result<()> {
        let config = Config::load()?;

        let provider = ZhipuProvider::new(config.providers.zhipu.api_key.clone());

        info!("Connecting to database");
        let memory_manager = Arc::new(MemoryManager::new(&config.database.url).await?);

        // Use session_id from input or generate new one
        let session_id = input.session_id.unwrap_or_else(Uuid::now_v7);
        let session_name = input.session_name.clone();

        let conversation_config = ConversationConfig {
            session_id,
            session_name: input.session_name,
            model: input
                .model
                .unwrap_or_else(|| config.agents.defaults.model.clone()),
            system_prompt: config.agents.defaults.system_prompt.clone().unwrap_or_else(||
                "You are a helpful AI assistant with memory of past conversations. Provide clear, concise responses.".to_string()
            ),
            history_limit: input.history_limit.unwrap_or_else(|| {
                config.agents.defaults.history_limit.unwrap_or(20)
            }),
            temperature: config.agents.defaults.temperature,
            max_tokens: config.agents.defaults.max_tokens,
        };

        info!(
            "Starting conversation session: {} (name: {:?})",
            session_id, session_name
        );

        // Create conversation manager
        let mut manager =
            ConversationManager::new(provider, memory_manager.clone(), conversation_config).await?;

        // Note: memory integration will be added later
        if config.memory.enabled {
            info!("Memory feature enabled - integrating with conversation");
        }

        if let Some(msg) = input.message {
            // Single message mode
            let session = manager.session();
            info!("Session state: {} messages", session.message_count());

            let context = TurnContext::new(msg);
            let result = manager.process_turn(context).await?;

            println!("{}", result.response);
            info!("Turn {} completed.", result.turn_number);
        } else {
            // Interactive mode
            manager.run_interactive().await?;

            let session = manager.session();
            info!(
                "Conversation ended: {} total messages",
                session.message_count()
            );
        }

        Ok(())
    }
}
