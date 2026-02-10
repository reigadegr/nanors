//! Conversation history management.
//!
//! This module provides utilities for managing conversation history,
//! including sliding windows and token-aware truncation.

use nanors_core::{ChatMessage, Role};

/// Configuration for conversation history management.
#[derive(Debug, Clone)]
pub struct HistoryConfig {
    /// Maximum number of messages to keep in context
    pub max_messages: usize,
    /// Maximum characters in context (approximate token limit)
    pub max_chars: usize,
    /// Whether to always include the first message (usually system prompt)
    pub keep_first_message: bool,
    /// Strategy for handling overflow
    pub overflow_strategy: OverflowStrategy,
}

impl Default for HistoryConfig {
    fn default() -> Self {
        Self {
            max_messages: 20,
            max_chars: 8000,
            keep_first_message: true,
            overflow_strategy: OverflowStrategy::TruncateOldest,
        }
    }
}

/// Strategy for handling conversation history overflow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverflowStrategy {
    /// Remove oldest messages first
    TruncateOldest,
    /// Summarize old messages (requires LLM call)
    Summarize,
    /// Keep only recent messages
    KeepRecent,
}

impl HistoryConfig {
    /// Create a config with specific message limit.
    #[must_use]
    pub const fn with_max_messages(mut self, max: usize) -> Self {
        self.max_messages = max;
        self
    }

    /// Create a config with specific character limit.
    #[must_use]
    pub const fn with_max_chars(mut self, max: usize) -> Self {
        self.max_chars = max;
        self
    }

    /// Set whether to keep the first message.
    #[must_use]
    pub const fn keep_first_message(mut self, keep: bool) -> Self {
        self.keep_first_message = keep;
        self
    }
}

/// A sliding window over conversation history.
///
/// This manages which messages should be included in the context
/// for the next LLM call.
#[derive(Debug, Clone)]
pub struct HistoryWindow {
    config: HistoryConfig,
}

impl HistoryWindow {
    /// Create a new history window with default config.
    #[must_use]
    pub fn new() -> Self {
        Self {
            config: HistoryConfig::default(),
        }
    }

    /// Create with custom config.
    #[must_use]
    pub const fn with_config(config: HistoryConfig) -> Self {
        Self { config }
    }

    /// Select messages to include in context.
    ///
    /// This applies the history window strategy to select which messages
    /// from the full history should be sent to the LLM.
    #[must_use]
    pub fn select_messages(&self, messages: &[ChatMessage]) -> Vec<ChatMessage> {
        if messages.is_empty() {
            return Vec::new();
        }

        let mut selected = Vec::new();

        // Always include first message if configured (usually system prompt)
        if self.config.keep_first_message && !messages.is_empty() {
            selected.push(messages[0].clone());
        }

        // Apply message limit
        let start_idx = if self.config.keep_first_message {
            messages
                .len()
                .saturating_sub(self.config.max_messages.saturating_sub(1))
        } else {
            messages.len().saturating_sub(self.config.max_messages)
        };

        // Add messages within the limit
        for msg in messages.iter().skip(start_idx) {
            // Skip if already added as first message
            if self.config.keep_first_message
                && !selected.is_empty()
                && msg.role == messages[0].role
                && msg.content == messages[0].content
            {
                continue;
            }
            selected.push(msg.clone());
        }

        // Apply character limit
        self.apply_char_limit(selected)
    }

    /// Apply character limit to selected messages.
    fn apply_char_limit(&self, mut messages: Vec<ChatMessage>) -> Vec<ChatMessage> {
        let mut total_chars = 0_usize;

        // Keep first message (system prompt) regardless of length
        let first_msg = if self.config.keep_first_message && !messages.is_empty() {
            total_chars = messages[0].content.len();
            Some(messages.remove(0))
        } else {
            None
        };

        // Truncate from the front (oldest messages)
        while !messages.is_empty()
            && total_chars + messages[0].content.len() > self.config.max_chars
        {
            messages.remove(0);
        }

        // Reconstruct with first message if it was saved
        if let Some(first) = first_msg {
            messages.insert(0, first);
        }

        messages
    }

    /// Get current configuration.
    #[must_use]
    pub const fn config(&self) -> &HistoryConfig {
        &self.config
    }

    /// Update configuration.
    pub const fn set_config(&mut self, config: HistoryConfig) {
        self.config = config;
    }
}

impl Default for HistoryWindow {
    fn default() -> Self {
        Self::new()
    }
}

/// Manager for conversation history with advanced features.
#[derive(Debug, Clone)]
pub struct HistoryManager {
    config: HistoryConfig,
}

impl HistoryManager {
    /// Create a new history manager.
    #[must_use]
    pub const fn new(config: HistoryConfig) -> Self {
        Self { config }
    }

    /// Create with default config.
    #[must_use]
    pub fn default_config() -> Self {
        Self {
            config: HistoryConfig::default(),
        }
    }

    /// Build messages for LLM request from full history.
    ///
    /// This combines system prompt with selected history messages
    /// and the new user message.
    #[must_use]
    pub fn build_llm_messages(
        &self,
        system_prompt: &str,
        history: &[ChatMessage],
        new_message: &str,
    ) -> Vec<ChatMessage> {
        let mut messages = Vec::new();

        // Add system prompt
        messages.push(ChatMessage {
            role: Role::System,
            content: system_prompt.to_string(),
        });

        // Add history window
        let window = HistoryWindow::with_config(self.config.clone());
        messages.extend(window.select_messages(history));

        // Add new user message
        messages.push(ChatMessage {
            role: Role::User,
            content: new_message.to_string(),
        });

        messages
    }

    /// Get conversation statistics.
    #[must_use]
    pub fn stats(&self, history: &[ChatMessage]) -> HistoryStats {
        let total_chars: usize = history.iter().map(|m| m.content.len()).sum();
        let user_count = history.iter().filter(|m| m.role == Role::User).count();
        let assistant_count = history.iter().filter(|m| m.role == Role::Assistant).count();

        HistoryStats {
            total_messages: history.len(),
            user_messages: user_count,
            assistant_messages: assistant_count,
            total_characters: total_chars,
            estimated_tokens: total_chars / 4, // Rough estimate: 4 chars per token
        }
    }
}

/// Statistics about conversation history.
#[derive(Debug, Clone)]
pub struct HistoryStats {
    pub total_messages: usize,
    pub user_messages: usize,
    pub assistant_messages: usize,
    pub total_characters: usize,
    pub estimated_tokens: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_messages(count: usize) -> Vec<ChatMessage> {
        (0..count)
            .map(|i| ChatMessage {
                role: if i % 2 == 0 {
                    Role::User
                } else {
                    Role::Assistant
                },
                content: format!("Message {i}: {}", "x".repeat(100)),
            })
            .collect()
    }

    #[test]
    fn test_history_window_select() {
        let config = HistoryConfig {
            max_messages: 5,
            max_chars: 10000,
            keep_first_message: true,
            overflow_strategy: OverflowStrategy::TruncateOldest,
        };

        let window = HistoryWindow::with_config(config);
        let messages = create_test_messages(20);

        let selected = window.select_messages(&messages);

        // Should have first message + last 4
        assert_eq!(selected.len(), 5);
    }

    #[test]
    fn test_history_char_limit() {
        let config = HistoryConfig {
            max_messages: 100,
            max_chars: 500,
            keep_first_message: true,
            overflow_strategy: OverflowStrategy::TruncateOldest,
        };

        let window = HistoryWindow::with_config(config);
        let messages = create_test_messages(20);

        let selected = window.select_messages(&messages);

        let total_chars: usize = selected.iter().map(|m| m.content.len()).sum();
        assert!(total_chars <= 600); // Allow some margin for first message
    }

    #[test]
    fn test_history_manager_build() {
        let config = HistoryConfig {
            max_messages: 3,
            max_chars: 1000,
            keep_first_message: false,
            overflow_strategy: OverflowStrategy::TruncateOldest,
        };

        let manager = HistoryManager::new(config);
        let history = create_test_messages(10);

        let messages = manager.build_llm_messages("You are helpful.", &history, "New message");

        // System + last 3 history + new message
        assert_eq!(messages.len(), 5);
        assert_eq!(messages[0].role, Role::System);
        let last_msg = &messages[messages.len() - 1];
        assert_eq!(last_msg.content, "New message");
    }

    #[test]
    fn test_history_stats() {
        let manager = HistoryManager::default_config();
        let messages = create_test_messages(10);

        let stats = manager.stats(&messages);

        assert_eq!(stats.total_messages, 10);
        assert_eq!(stats.user_messages, 5);
        assert_eq!(stats.assistant_messages, 5);
        assert!(stats.estimated_tokens > 0);
    }
}
