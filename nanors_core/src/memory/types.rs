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
    pub user_scope: String,
    pub resource_id: Option<Uuid>,
    pub memory_type: MemoryType,
    pub summary: String,
    pub embedding: Option<Vec<f32>>,
    pub happened_at: DateTime<Utc>,
    pub extra: Option<serde_json::Value>,
    pub content_hash: String,
    pub reinforcement_count: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// Version number for this memory
    pub version: i32,
    /// Parent version ID for version chain
    pub parent_version_id: Option<Uuid>,
    /// Version relation type (Sets, Updates, etc.)
    pub version_relation: Option<String>,
}

#[derive(Debug, Clone)]
pub struct MemoryCategory {
    pub id: Uuid,
    pub user_scope: String,
    pub name: String,
    pub description: Option<String>,
    pub embedding: Option<Vec<f32>>,
    pub summary: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct CategoryItem {
    pub item_id: Uuid,
    pub category_id: Uuid,
}

#[derive(Debug, Clone)]
pub struct Resource {
    pub id: Uuid,
    pub user_scope: String,
    pub url: Option<String>,
    pub modality: String,
    pub local_path: Option<String>,
    pub caption: Option<String>,
    pub embedding: Option<Vec<f32>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct SalienceScore<T> {
    pub item: T,
    pub score: f64,
}
