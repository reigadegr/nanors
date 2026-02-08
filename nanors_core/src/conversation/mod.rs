#![warn(
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

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{ChatMessage, Role};

/// A segment of a conversation for independent processing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationSegment {
    /// Unique identifier for the segment
    pub segment_id: Uuid,
    /// Session this segment belongs to
    pub session_id: Uuid,
    /// Starting message index in the original conversation
    pub start_index: usize,
    /// Ending message index in the original conversation (exclusive)
    pub end_index: usize,
    /// Caption or summary of the segment
    pub caption: String,
    /// Embedding of the segment for semantic search
    pub embedding: Option<Vec<f32>>,
    /// When the segment was created
    pub created_at: DateTime<Utc>,
}

impl ConversationSegment {
    /// Create a new conversation segment
    #[must_use]
    pub fn new(session_id: Uuid, start_index: usize, end_index: usize, caption: String) -> Self {
        Self {
            segment_id: Uuid::now_v7(),
            session_id,
            start_index,
            end_index,
            caption,
            embedding: None,
            created_at: Utc::now(),
        }
    }

    /// Get the number of messages in this segment
    #[must_use]
    pub const fn message_count(&self) -> usize {
        self.end_index - self.start_index
    }
}

/// Configuration for conversation segmentation
#[derive(Debug, Clone)]
pub struct SegmentationConfig {
    /// Number of messages per segment
    pub segment_size: usize,
    /// Minimum messages for a segment to be created
    pub min_segment_size: usize,
    /// Whether to generate embeddings for segments
    pub generate_embeddings: bool,
}

impl Default for SegmentationConfig {
    fn default() -> Self {
        Self {
            segment_size: 10,
            min_segment_size: 3,
            generate_embeddings: false,
        }
    }
}

/// Trait for segmenting conversations
#[async_trait]
pub trait ConversationSegmenter: Send + Sync {
    /// Segment a conversation into smaller parts
    ///
    /// # Arguments
    /// * `session_id` - ID of the session
    /// * `messages` - Messages in the conversation
    /// * `config` - Segmentation configuration
    ///
    /// # Returns
    /// * Vector of conversation segments
    async fn segment(
        &self,
        session_id: &Uuid,
        messages: &[ChatMessage],
        config: &SegmentationConfig,
    ) -> anyhow::Result<Vec<ConversationSegment>>;

    /// Get the current segmentation configuration
    #[must_use]
    fn config(&self) -> &SegmentationConfig;
}

/// Simple conversation segmenter based on message count
pub struct MessageCountSegmenter {
    config: SegmentationConfig,
}

impl MessageCountSegmenter {
    /// Create a new message count segmenter
    #[must_use]
    pub const fn new(config: SegmentationConfig) -> Self {
        Self { config }
    }

    /// Generate a caption for a segment
    fn generate_caption(messages: &[ChatMessage]) -> String {
        if messages.is_empty() {
            return "Empty segment".to_string();
        }

        let first_user_msg = messages
            .iter()
            .find(|m| m.role == Role::User)
            .and_then(|m| {
                m.content
                    .lines()
                    .next()
                    .map(|line| line.chars().take(50).collect::<String>())
            });

        first_user_msg.map_or_else(
            || format!("Segment with {} messages", messages.len()),
            |msg| format!("Conversation about: {msg}..."),
        )
    }
}

#[async_trait]
impl ConversationSegmenter for MessageCountSegmenter {
    async fn segment(
        &self,
        session_id: &Uuid,
        messages: &[ChatMessage],
        config: &SegmentationConfig,
    ) -> anyhow::Result<Vec<ConversationSegment>> {
        let mut segments = Vec::new();

        if messages.len() < config.min_segment_size {
            return Ok(segments);
        }

        let mut start_index = 0_usize;

        while start_index < messages.len() {
            let end_index = (start_index + config.segment_size).min(messages.len());

            // Skip if the remaining segment is too small
            if end_index - start_index < config.min_segment_size {
                break;
            }

            let segment_messages = &messages[start_index..end_index];
            let caption = Self::generate_caption(segment_messages);

            let segment = ConversationSegment::new(*session_id, start_index, end_index, caption);

            segments.push(segment);

            start_index = end_index;
        }

        tracing::info!("Segmented conversation into {} segments", segments.len());

        Ok(segments)
    }

    fn config(&self) -> &SegmentationConfig {
        &self.config
    }
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
                content: format!("Message {i}"),
            })
            .collect()
    }

    #[tokio::test]
    #[expect(clippy::expect_used, reason = "Test failure should panic with context")]
    async fn test_segment_empty_conversation() {
        let segmenter = MessageCountSegmenter::new(SegmentationConfig::default());
        let session_id = Uuid::now_v7();
        let messages = vec![];

        let segments = segmenter
            .segment(&session_id, &messages, segmenter.config())
            .await
            .expect("Failed to segment empty conversation");

        assert!(segments.is_empty());
    }

    #[tokio::test]
    #[expect(clippy::expect_used, reason = "Test failure should panic with context")]
    async fn test_segment_small_conversation() {
        let segmenter = MessageCountSegmenter::new(SegmentationConfig::default());
        let session_id = Uuid::now_v7();
        let messages = create_test_messages(2); // Less than min_segment_size

        let segments = segmenter
            .segment(&session_id, &messages, segmenter.config())
            .await
            .expect("Failed to segment small conversation");

        assert!(segments.is_empty());
    }

    #[tokio::test]
    #[expect(clippy::expect_used, reason = "Test failure should panic with context")]
    async fn test_segment_medium_conversation() {
        let config = SegmentationConfig {
            segment_size: 5,
            min_segment_size: 3,
            generate_embeddings: false,
        };
        let segmenter = MessageCountSegmenter::new(config);
        let session_id = Uuid::now_v7();
        let messages = create_test_messages(12);

        let segments = segmenter
            .segment(&session_id, &messages, segmenter.config())
            .await
            .expect("Failed to segment medium conversation");

        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].start_index, 0);
        assert_eq!(segments[0].end_index, 5);
        assert_eq!(segments[1].start_index, 5);
        assert_eq!(segments[1].end_index, 10);
    }

    #[test]
    fn test_conversation_segment_message_count() {
        let segment = ConversationSegment::new(Uuid::now_v7(), 0, 10, "Test".to_string());
        assert_eq!(segment.message_count(), 10);
    }
}
