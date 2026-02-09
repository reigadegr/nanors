use async_trait::async_trait;
use uuid::Uuid;

use super::types::{MemoryItem, SalienceScore};

#[async_trait]
pub trait MemoryItemRepo: Send + Sync {
    async fn insert(&self, item: &MemoryItem) -> anyhow::Result<()>;

    async fn find_by_id(&self, id: &Uuid) -> anyhow::Result<Option<MemoryItem>>;

    async fn find_by_content_hash(
        &self,
        user_scope: &str,
        hash: &str,
    ) -> anyhow::Result<Option<MemoryItem>>;

    async fn update(&self, item: &MemoryItem) -> anyhow::Result<()>;

    async fn delete(&self, id: &Uuid) -> anyhow::Result<()>;

    async fn list_by_scope(&self, user_scope: &str) -> anyhow::Result<Vec<MemoryItem>>;

    async fn search_by_embedding(
        &self,
        user_scope: &str,
        query_embedding: &[f32],
        top_k: usize,
    ) -> anyhow::Result<Vec<SalienceScore<MemoryItem>>>;

    /// Backfill embeddings for items that don't have them.
    /// Returns the number of items updated.
    async fn backfill_embeddings(
        &self,
        user_scope: &str,
        embed_fn: &(dyn Fn(String) -> anyhow::Result<Vec<f32>> + Send + Sync),
    ) -> anyhow::Result<usize>;

    /// Insert or update a memory item based on semantic similarity.
    ///
    /// Searches for semantically similar memories using embedding similarity.
    /// If a memory with similarity above the threshold exists, creates a new
    /// version of that memory (update). Otherwise, inserts as a new memory.
    ///
    /// # Arguments
    /// * `item` - The memory item to insert or use for update
    /// * `similarity_threshold` - Minimum similarity (0.0-1.0) to consider
    ///   memories as semantically equivalent. Default: 0.85
    ///
    /// # Returns
    /// * `Ok(uuid)` - ID of the inserted or updated memory
    /// * `Err(e)` - Error if operation fails
    async fn semantic_upsert(
        &self,
        item: &MemoryItem,
        similarity_threshold: f64,
    ) -> anyhow::Result<Uuid>;
}
