use async_trait::async_trait;
use uuid::Uuid;

use super::types::{CategoryItem, MemoryCategory, MemoryItem, Resource, SalienceScore};

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
    ) -> anyhow::Result<Vec<SalienceScore>>;
}

#[derive(Debug, Clone)]
pub struct CategorySalienceScore {
    pub category: MemoryCategory,
    pub score: f64,
}

#[derive(Debug, Clone)]
pub struct ResourceSalienceScore {
    pub resource: Resource,
    pub score: f64,
}

#[async_trait]
pub trait MemoryCategoryRepo: Send + Sync {
    async fn insert(&self, cat: &MemoryCategory) -> anyhow::Result<()>;

    async fn find_by_id(&self, id: &Uuid) -> anyhow::Result<Option<MemoryCategory>>;

    async fn find_by_name(
        &self,
        user_scope: &str,
        name: &str,
    ) -> anyhow::Result<Option<MemoryCategory>>;

    async fn update(&self, cat: &MemoryCategory) -> anyhow::Result<()>;

    async fn delete(&self, id: &Uuid) -> anyhow::Result<()>;

    async fn list_by_scope(&self, user_scope: &str) -> anyhow::Result<Vec<MemoryCategory>>;

    async fn search_by_embedding(
        &self,
        user_scope: &str,
        query_embedding: &[f32],
        top_k: usize,
    ) -> anyhow::Result<Vec<CategorySalienceScore>>;
}

#[async_trait]
pub trait CategoryItemRepo: Send + Sync {
    async fn link(&self, item_id: &Uuid, category_id: &Uuid) -> anyhow::Result<()>;

    async fn unlink(&self, item_id: &Uuid, category_id: &Uuid) -> anyhow::Result<()>;

    async fn categories_for_item(&self, item_id: &Uuid) -> anyhow::Result<Vec<CategoryItem>>;

    async fn items_for_category(&self, category_id: &Uuid) -> anyhow::Result<Vec<CategoryItem>>;
}

#[async_trait]
pub trait ResourceRepo: Send + Sync {
    async fn insert(&self, res: &Resource) -> anyhow::Result<()>;

    async fn find_by_id(&self, id: &Uuid) -> anyhow::Result<Option<Resource>>;

    async fn delete(&self, id: &Uuid) -> anyhow::Result<()>;

    async fn list_by_scope(&self, user_scope: &str) -> anyhow::Result<Vec<Resource>>;

    async fn search_by_embedding(
        &self,
        user_scope: &str,
        query_embedding: &[f32],
        top_k: usize,
    ) -> anyhow::Result<Vec<ResourceSalienceScore>>;
}
