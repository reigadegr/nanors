//! Multi-turn conversation command with persistent sessions.
//!
//! Unlike the `agent` command which creates a new session per message,
//! this command maintains conversation context across multiple turns.

use nanors_conversation::{ConversationManager, TurnContext};
use tracing::info;
use uuid::Uuid;

use super::{build_conversation_config, init_common_components};

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
        let common = init_common_components().await?;

        // Use session_id from input or generate new one
        let session_id = input.session_id.unwrap_or_else(Uuid::now_v7);
        let session_name = input.session_name.clone();

        let conversation_config = build_conversation_config(
            &common.config,
            session_id,
            input.session_name,
            input.model,
            input.history_limit,
            true, // use memory-enhanced prompt
        );

        info!(
            "Starting conversation session: {} (name: {:?})",
            session_id, session_name
        );

        // Create conversation manager
        let mut manager = ConversationManager::new(
            common.provider,
            common.memory_manager.clone(),
            conversation_config,
        )
        .await?;

        // Note: memory integration will be added later
        info!("Memory feature enabled - integrating with conversation");

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
