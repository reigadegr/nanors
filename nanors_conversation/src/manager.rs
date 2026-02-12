//! Conversation manager for multi-turn dialogue.
//!
//! The `ConversationManager` is the main entry point for handling
//! persistent conversations with context across turns.

use crate::session::ConversationSession;
use nanors_core::{ChatMessage, LLMProvider, Role, SessionStorage};
use std::io::Write;
use std::sync::Arc;
use thiserror::Error;
use tracing::{debug, info};
use uuid::Uuid;

/// Configuration for conversation management.
#[derive(Debug, Clone)]
pub struct ConversationConfig {
    /// Session identifier (persists across turns)
    pub session_id: Uuid,
    /// Optional session name
    pub session_name: Option<String>,
    /// Model to use for completions
    pub model: String,
    /// System prompt
    pub system_prompt: String,
    /// Maximum messages to keep in context
    pub history_limit: usize,
    /// Temperature for sampling
    pub temperature: f32,
    /// Max tokens in response
    pub max_tokens: usize,
}

impl Default for ConversationConfig {
    fn default() -> Self {
        Self {
            session_id: Uuid::now_v7(),
            session_name: None,
            model: "glm-4-flash".to_string(),
            system_prompt: "You are a helpful AI assistant.".to_string(),
            history_limit: 20,
            temperature: 0.7,
            max_tokens: 8192,
        }
    }
}

impl ConversationConfig {
    /// Create a new config with a specific session ID.
    #[must_use]
    pub const fn with_session_id(mut self, id: Uuid) -> Self {
        self.session_id = id;
        self
    }

    /// Set the model name.
    #[must_use]
    pub fn with_model(mut self, model: String) -> Self {
        self.model = model;
        self
    }

    /// Set the system prompt.
    #[must_use]
    pub fn with_system_prompt(mut self, prompt: String) -> Self {
        self.system_prompt = prompt;
        self
    }

    /// Set the history limit.
    #[must_use]
    pub const fn with_history_limit(mut self, limit: usize) -> Self {
        self.history_limit = limit;
        self
    }
}

/// Errors that can occur during conversation management.
#[derive(Debug, Error)]
pub enum ConversationError {
    #[error("LLM provider error: {0}")]
    LLMError(#[from] anyhow::Error),

    #[error("Session storage error: {0}")]
    SessionError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Empty response from LLM")]
    EmptyResponse,

    #[error("Session not found: {0}")]
    SessionNotFound(Uuid),
}

/// Context for a single conversation turn.
///
/// This contains all the information needed to process one turn
/// of dialogue, including the user input and any retrieved context.
#[derive(Debug, Clone)]
pub struct TurnContext {
    /// User's input message
    pub user_input: String,
    /// Optional retrieved context (e.g., from memory search)
    pub retrieved_context: Option<String>,
    /// Current turn number
    pub turn_number: usize,
}

impl TurnContext {
    /// Create a new turn context.
    #[must_use]
    pub const fn new(user_input: String) -> Self {
        Self {
            user_input,
            retrieved_context: None,
            turn_number: 0,
        }
    }

    /// Set turn number.
    #[must_use]
    pub const fn with_turn_number(mut self, turn: usize) -> Self {
        self.turn_number = turn;
        self
    }
}

/// Result of processing a conversation turn.
#[derive(Debug, Clone)]
pub struct TurnResult {
    /// Assistant's response
    pub response: String,
    /// Token usage information
    pub usage: Option<TurnUsage>,
    /// Current session state
    pub session: ConversationSession,
    /// Turn number
    pub turn_number: usize,
}

/// Token usage information for a turn.
#[derive(Debug, Clone)]
pub struct TurnUsage {
    pub prompt: u32,
    pub completion: u32,
    pub total: u32,
}

/// Multi-turn conversation manager.
///
/// This manages persistent conversations with full context history,
/// unlike the single-turn `AgentLoop`.
pub struct ConversationManager<P = Arc<dyn LLMProvider>, S = Arc<dyn SessionStorage>>
where
    P: Send + Sync,
    S: Send + Sync,
{
    provider: P,
    storage: S,
    config: ConversationConfig,
    current_session: ConversationSession,
}

impl<P, S> ConversationManager<P, S>
where
    P: LLMProvider + Send + Sync,
    S: SessionStorage + Send + Sync,
{
    /// Create a new conversation manager.
    pub async fn new(
        provider: P,
        storage: S,
        config: ConversationConfig,
    ) -> Result<Self, ConversationError> {
        info!(
            "Creating conversation manager for session: {}",
            config.session_id
        );

        // Try to load existing session
        let current_session = Self::load_or_create_session(&storage, &config).await?;

        Ok(Self {
            provider,
            storage,
            config,
            current_session,
        })
    }

    /// Process a single conversation turn.
    ///
    /// This adds the user message to history, retrieves appropriate context,
    /// sends to the LLM, and saves the response.
    pub async fn process_turn(
        &mut self,
        context: TurnContext,
    ) -> Result<TurnResult, ConversationError> {
        let turn_number = self.current_session.message_count() / 2 + 1;
        info!(
            "Processing turn {turn_number} for session: {}",
            self.config.session_id
        );

        // Add user message to session
        self.current_session
            .add_message(Role::User, context.user_input.clone());

        // Build messages for LLM
        let system_prompt = self.build_system_prompt(&context);

        // Apply history limit - keep last N messages plus system prompt
        let history: Vec<ChatMessage> = self
            .current_session
            .last_n_messages(self.config.history_limit)
            .to_vec();

        // Build full message list: system prompt + history (excluding new user message which we'll add)
        let mut messages = Vec::new();
        messages.push(ChatMessage {
            role: Role::System,
            content: system_prompt,
        });
        messages.extend(history);
        messages.push(ChatMessage {
            role: Role::User,
            content: context.user_input.clone(),
        });

        // Send to LLM
        let llm_response = self
            .provider
            .chat(&messages, &self.config.model)
            .await
            .map_err(ConversationError::LLMError)?;

        let response_content = llm_response.content.clone();

        if response_content.trim().is_empty() {
            return Err(ConversationError::EmptyResponse);
        }

        // Add assistant response to session
        self.current_session
            .add_message(Role::Assistant, response_content.clone());

        // Save to storage
        self.save_session().await?;

        debug!("Turn {turn_number} completed successfully");

        Ok(TurnResult {
            response: response_content,
            usage: llm_response.usage.map(|u| TurnUsage {
                prompt: u.prompt_tokens,
                completion: u.completion_tokens,
                total: u.total_tokens,
            }),
            session: self.current_session.clone(),
            turn_number,
        })
    }

    /// Run an interactive conversation loop.
    ///
    /// This reads from stdin and writes to stdout, maintaining
    /// conversation context across turns.
    pub async fn run_interactive(&mut self) -> Result<(), ConversationError> {
        println!("=== Conversation Session: {} ===", self.config.session_id);
        println!("Type 'exit', 'quit', or Ctrl+C to end the session.\n");

        loop {
            print!("> ");
            std::io::stdout().flush()?;

            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;
            let input = input.trim();

            // Exit commands
            if matches!(input, "exit" | "quit" | "q") {
                println!(
                    "\nSession ended. Total turns: {}",
                    self.current_session.message_count() / 2
                );
                break;
            }

            if input.is_empty() {
                continue;
            }

            // Process the turn
            let context = TurnContext::new(input.to_string());

            match self.process_turn(context).await {
                Ok(result) => {
                    println!("\n{}\n", result.response);

                    if let Some(usage) = result.usage {
                        debug!(
                            "Tokens: {} prompt + {} completion = {} total",
                            usage.prompt, usage.completion, usage.total
                        );
                    }
                }
                Err(e) => {
                    eprintln!("Error: {e}");
                }
            }
        }

        Ok(())
    }

    /// Get the current session state.
    #[must_use]
    pub const fn session(&self) -> &ConversationSession {
        &self.current_session
    }

    /// Load or create a session from storage.
    async fn load_or_create_session(
        storage: &S,
        config: &ConversationConfig,
    ) -> Result<ConversationSession, ConversationError> {
        let stored = storage
            .get_or_create(&config.session_id)
            .await
            .map_err(|e| ConversationError::SessionError(e.to_string()))?;

        let mut session = ConversationSession {
            id: stored.id,
            name: config.session_name.clone(),
            messages: stored.messages,
            created_at: stored.created_at,
            updated_at: stored.updated_at,
        };

        // If empty, add system prompt if configured
        if session.is_empty() && !config.system_prompt.is_empty() {
            session.add_message(Role::System, config.system_prompt.clone());
        }

        Ok(session)
    }

    /// Save current session to storage.
    async fn save_session(&self) -> Result<(), ConversationError> {
        for msg in &self.current_session.messages {
            self.storage
                .add_message(&self.config.session_id, msg.role.clone(), &msg.content)
                .await
                .map_err(|e| ConversationError::SessionError(e.to_string()))?;
        }

        Ok(())
    }

    /// Build system prompt with optional retrieved context.
    fn build_system_prompt(&self, context: &TurnContext) -> String {
        context.retrieved_context.as_ref().map_or_else(
            || self.config.system_prompt.clone(),
            |ctx| {
                format!(
                    "{}\n\n# Relevant Context\n\n{}",
                    self.config.system_prompt, ctx
                )
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = ConversationConfig::default();
        assert!(config.history_limit > 0);
        assert!(!config.model.is_empty());
    }

    #[test]
    fn test_turn_context() {
        let mut ctx = TurnContext::new("Hello".to_string());
        ctx.retrieved_context = Some("Some context".to_string());
        ctx = ctx.with_turn_number(5);

        assert_eq!(ctx.user_input, "Hello");
        assert_eq!(ctx.retrieved_context, Some("Some context".to_string()));
        assert_eq!(ctx.turn_number, 5);
    }
}
