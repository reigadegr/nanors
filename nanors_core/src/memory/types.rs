use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MemoryType {
    Episodic,
    Semantic,
    Procedural,
}

impl std::fmt::Display for MemoryType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Episodic => write!(f, "episodic"),
            Self::Semantic => write!(f, "semantic"),
            Self::Procedural => write!(f, "procedural"),
        }
    }
}

impl std::str::FromStr for MemoryType {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "episodic" => Ok(Self::Episodic),
            "semantic" => Ok(Self::Semantic),
            "procedural" => Ok(Self::Procedural),
            _ => Err(anyhow::anyhow!("unknown memory type: {s}")),
        }
    }
}

#[derive(Debug, Clone)]
pub struct MemoryItem {
    pub id: Uuid,
    pub memory_type: MemoryType,
    pub summary: String,
    pub embedding: Option<Vec<f32>>,
    pub happened_at: DateTime<Utc>,
    pub extra: Option<serde_json::Value>,
    pub content_hash: String,
    pub reinforcement_count: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl MemoryItem {
    /// Create a new episodic memory item for user input.
    #[must_use]
    pub fn create_episodic(
        content: &str,
        embedding: Option<Vec<f32>>,
        happened_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id: Uuid::now_v7(),
            memory_type: MemoryType::Episodic,
            summary: format!("User: {content}"),
            embedding,
            happened_at,
            extra: None,
            content_hash: crate::content_hash("episodic", content),
            reinforcement_count: 0,
            created_at: happened_at,
            updated_at: happened_at,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SalienceScore<T> {
    pub item: T,
    pub score: f64,
    /// Raw cosine similarity score (0.0 - 1.0) used for primary ranking
    pub similarity: f64,
}
