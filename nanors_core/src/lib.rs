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
    clippy::missing_errors_doc,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::suboptimal_flops
)]

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub mod agent;
pub mod conversation;
pub mod memory;
pub mod retrieval;
pub mod tools;

pub use agent::{AgentConfig, AgentLoop};
pub use conversation::{ConversationSegment, ConversationSegmenter, SegmentationConfig};
pub use memory::{MemoryItem, MemoryItemRepo, MemoryType, SalienceScore};
pub use retrieval::{CategoryCompressor, SufficiencyChecker};
pub use tools::{Tool, ToolRegistry};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
    System,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: Role,
    pub content: String,
}

#[derive(Debug, Clone)]
pub struct LLMResponse {
    pub content: String,
    pub usage: Option<Usage>,
}

#[derive(Debug, Clone)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

#[async_trait]
pub trait LLMProvider: Send + Sync {
    async fn chat(&self, messages: &[ChatMessage], model: &str) -> anyhow::Result<LLMResponse>;
    async fn embed(&self, text: &str) -> anyhow::Result<Vec<f32>>;
    fn get_default_model(&self) -> &str;
}

#[async_trait]
pub trait SessionStorage: Send + Sync {
    async fn get_or_create(&self, id: &Uuid) -> anyhow::Result<Session>;
    async fn add_message(&self, id: &Uuid, role: Role, content: &str) -> anyhow::Result<()>;
}

#[derive(Debug, Clone)]
pub struct Session {
    pub id: Uuid,
    pub messages: Vec<ChatMessage>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}
