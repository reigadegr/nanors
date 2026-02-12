//! Session management for multi-turn conversations.
//!
//! A session represents an ongoing conversation with a user, maintaining
//! all message history and metadata across multiple turns.

use chrono::{DateTime, Utc};
use uuid::Uuid;

use nanors_core::{ChatMessage, Role};

/// A conversation session with full message history.
///
/// This represents the complete state of a conversation, including
/// all messages exchanged so far.
#[derive(Debug, Clone)]
pub struct ConversationSession {
    /// Session identifier
    pub id: Uuid,
    /// Session name (optional)
    pub name: Option<String>,
    /// Message history
    pub messages: Vec<ChatMessage>,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Last update timestamp
    pub updated_at: DateTime<Utc>,
}

impl ConversationSession {
    /// Create a new empty conversation session.
    #[must_use]
    pub fn new() -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::now_v7(),
            name: None,
            messages: Vec::new(),
            created_at: now,
            updated_at: now,
        }
    }

    /// Set session name.
    #[must_use]
    pub fn with_name(mut self, name: String) -> Self {
        self.name = Some(name);
        self
    }

    /// Add a message to the session.
    pub fn add_message(&mut self, role: Role, content: String) {
        self.messages.push(ChatMessage { role, content });
        self.updated_at = Utc::now();
    }

    /// Get the last N messages from history.
    #[must_use]
    pub fn last_n_messages(&self, n: usize) -> &[ChatMessage] {
        let start = self.messages.len().saturating_sub(n);
        &self.messages[start..]
    }

    /// Get all user messages.
    #[must_use]
    pub fn user_messages(&self) -> Vec<&ChatMessage> {
        self.messages
            .iter()
            .filter(|m| m.role == Role::User)
            .collect()
    }

    /// Get message count.
    #[must_use]
    pub const fn message_count(&self) -> usize {
        self.messages.len()
    }

    /// Check if session is empty.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    /// Clear all messages from the session.
    pub fn clear(&mut self) {
        self.messages.clear();
        self.updated_at = Utc::now();
    }
}

impl Default for ConversationSession {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_conversation_session() {
        let mut session = ConversationSession::new().with_name("Test".to_string());

        assert!(session.is_empty());

        session.add_message(Role::User, "Hello".to_string());
        session.add_message(Role::Assistant, "Hi there!".to_string());

        assert_eq!(session.message_count(), 2);
        assert!(!session.is_empty());

        let last = session.last_n_messages(1);
        assert_eq!(last.len(), 1);
        assert_eq!(last[0].content, "Hi there!");

        let user_msgs = session.user_messages();
        assert_eq!(user_msgs.len(), 1);
        assert_eq!(user_msgs[0].content, "Hello");
    }

    #[test]
    fn test_last_n_messages() {
        let mut session = ConversationSession::new();

        for i in 0..10 {
            session.add_message(Role::User, format!("Message {i}"));
        }

        assert_eq!(session.last_n_messages(3).len(), 3);
        assert_eq!(session.last_n_messages(100).len(), 10);
        assert_eq!(session.last_n_messages(0).len(), 0);
    }
}
