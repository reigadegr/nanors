use async_trait::async_trait;
use chrono::Utc;
use nanors_core::memory::{
    CategoryItem, CategorySalienceScore, MemoryCategory, MemoryItem, Resource,
    ResourceSalienceScore, SalienceScore,
};
use nanors_core::{CategoryItemRepo, MemoryCategoryRepo, MemoryItemRepo, ResourceRepo};
use nanors_entities::{category_items, memory_categories, memory_items, resources};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Database, DatabaseConnection, EntityTrait, ModelTrait,
    QueryFilter, Set,
};
use tracing::info;
use uuid::Uuid;

use crate::convert;
use crate::dedup;
use crate::scoring;

pub struct MemoryManager {
    db: DatabaseConnection,
}

impl MemoryManager {
    pub async fn new(database_url: &str) -> anyhow::Result<Self> {
        info!("Connecting to database for MemoryManager");
        let db = Database::connect(database_url).await?;
        info!("MemoryManager initialized");
        Ok(Self { db })
    }

    /// Insert or update a memory item with deduplication.
    ///
    /// If an item with the same content hash already exists in the scope,
    /// its reinforcement count is incremented instead of creating a
    /// duplicate.
    pub async fn upsert_memory(&self, item: &MemoryItem) -> anyhow::Result<Uuid> {
        let hash = dedup::content_hash(&item.memory_type.to_string(), &item.summary);

        if let Some(existing) =
            MemoryItemRepo::find_by_content_hash(self, &item.user_scope, &hash).await?
        {
            let mut updated = existing.clone();
            updated.reinforcement_count += 1;
            updated.updated_at = Utc::now();
            MemoryItemRepo::update(self, &updated).await?;
            info!(
                "Reinforced existing memory: {} (count={})",
                updated.id, updated.reinforcement_count
            );
            Ok(updated.id)
        } else {
            let mut new_item = item.clone();
            new_item.content_hash = hash;
            MemoryItemRepo::insert(self, &new_item).await?;
            info!("Inserted new memory: {}", new_item.id);
            Ok(new_item.id)
        }
    }
}

#[async_trait]
impl MemoryItemRepo for MemoryManager {
    async fn insert(&self, item: &MemoryItem) -> anyhow::Result<()> {
        let embedding_json = convert::embedding_option_to_json(item.embedding.as_ref());
        let model = memory_items::ActiveModel {
            id: Set(item.id),
            user_scope: Set(item.user_scope.clone()),
            resource_id: Set(item.resource_id),
            memory_type: Set(item.memory_type.to_string()),
            summary: Set(item.summary.clone()),
            embedding: Set(embedding_json),
            happened_at: Set(item.happened_at.into()),
            extra: Set(item.extra.clone()),
            content_hash: Set(item.content_hash.clone()),
            reinforcement_count: Set(item.reinforcement_count),
            created_at: Set(item.created_at.into()),
            updated_at: Set(item.updated_at.into()),
        };
        model.insert(&self.db).await?;
        Ok(())
    }

    async fn find_by_id(&self, id: &Uuid) -> anyhow::Result<Option<MemoryItem>> {
        let result = memory_items::Entity::find_by_id(*id).one(&self.db).await?;
        Ok(result.map(convert::memory_item_from_model))
    }

    async fn find_by_content_hash(
        &self,
        user_scope: &str,
        hash: &str,
    ) -> anyhow::Result<Option<MemoryItem>> {
        let result = memory_items::Entity::find()
            .filter(memory_items::Column::UserScope.eq(user_scope))
            .filter(memory_items::Column::ContentHash.eq(hash))
            .one(&self.db)
            .await?;
        Ok(result.map(convert::memory_item_from_model))
    }

    async fn update(&self, item: &MemoryItem) -> anyhow::Result<()> {
        let existing = memory_items::Entity::find_by_id(item.id)
            .one(&self.db)
            .await?
            .ok_or_else(|| anyhow::anyhow!("MemoryItem not found: {}", item.id))?;

        let model = memory_items::ActiveModel {
            id: Set(existing.id),
            user_scope: Set(item.user_scope.clone()),
            resource_id: Set(item.resource_id),
            memory_type: Set(item.memory_type.to_string()),
            summary: Set(item.summary.clone()),
            embedding: Set(convert::embedding_option_to_json(item.embedding.as_ref())),
            happened_at: Set(item.happened_at.into()),
            extra: Set(item.extra.clone()),
            content_hash: Set(item.content_hash.clone()),
            reinforcement_count: Set(item.reinforcement_count),
            created_at: Set(existing.created_at),
            updated_at: Set(item.updated_at.into()),
        };
        model.update(&self.db).await?;
        Ok(())
    }

    async fn delete(&self, id: &Uuid) -> anyhow::Result<()> {
        let existing = memory_items::Entity::find_by_id(*id)
            .one(&self.db)
            .await?
            .ok_or_else(|| anyhow::anyhow!("MemoryItem not found: {id}"))?;

        existing.delete(&self.db).await?;
        Ok(())
    }

    async fn list_by_scope(&self, user_scope: &str) -> anyhow::Result<Vec<MemoryItem>> {
        let results = memory_items::Entity::find()
            .filter(memory_items::Column::UserScope.eq(user_scope))
            .all(&self.db)
            .await?;
        Ok(results
            .into_iter()
            .map(convert::memory_item_from_model)
            .collect())
    }

    async fn search_by_embedding(
        &self,
        user_scope: &str,
        query_embedding: &[f32],
        top_k: usize,
    ) -> anyhow::Result<Vec<SalienceScore>> {
        let items: Vec<MemoryItem> = MemoryItemRepo::list_by_scope(self, user_scope).await?;
        let now = Utc::now();

        let mut scores: Vec<SalienceScore> = items
            .into_iter()
            .map(|item| {
                let salience = if let Some(embedding) = &item.embedding {
                    let similarity = scoring::cosine_similarity(query_embedding, embedding);
                    scoring::compute_salience(
                        similarity,
                        item.reinforcement_count,
                        item.happened_at,
                        now,
                    )
                } else {
                    // Items without embeddings get a low default score based on recency only
                    scoring::compute_salience(0.0, item.reinforcement_count, item.happened_at, now)
                };

                SalienceScore {
                    item,
                    score: salience,
                }
            })
            .collect();

        // Filter out items that are essentially the same as the query (similarity >= 0.95)
        // to avoid returning the exact same question back to the user
        let mut filtered_scores: Vec<SalienceScore> = scores
            .into_iter()
            .filter(|score| {
                let sim = score
                    .item
                    .embedding
                    .as_ref()
                    .map_or(0.0, |emb| scoring::cosine_similarity(query_embedding, emb));
                sim < 0.95
            })
            .collect();

        filtered_scores.sort_by(|a, b| b.score.total_cmp(&a.score));
        filtered_scores.truncate(top_k);

        Ok(filtered_scores)
    }

    async fn backfill_embeddings(
        &self,
        user_scope: &str,
        embed_fn: &(dyn Fn(String) -> anyhow::Result<Vec<f32>> + Send + Sync),
    ) -> anyhow::Result<usize> {
        let items = MemoryItemRepo::list_by_scope(self, user_scope).await?;
        let mut updated = 0_usize;

        for mut item in items {
            if item.embedding.is_none() {
                // Generate embedding from the summary text
                let summary = item.summary.clone();
                match embed_fn(summary) {
                    Ok(embedding) => {
                        item.embedding = Some(embedding);
                        item.updated_at = Utc::now();
                        MemoryItemRepo::update(self, &item).await?;
                        updated += 1;
                        info!("Backfilled embedding for memory: {}", item.id);
                    }
                    Err(e) => {
                        info!("Failed to generate embedding for {}: {e}", item.id);
                    }
                }
            }
        }

        Ok(updated)
    }
}

#[async_trait]
impl MemoryCategoryRepo for MemoryManager {
    async fn insert(&self, cat: &MemoryCategory) -> anyhow::Result<()> {
        let model = memory_categories::ActiveModel {
            id: Set(cat.id),
            user_scope: Set(cat.user_scope.clone()),
            name: Set(cat.name.clone()),
            description: Set(cat.description.clone()),
            embedding: Set(convert::embedding_option_to_json(cat.embedding.as_ref())),
            summary: Set(cat.summary.clone()),
            created_at: Set(cat.created_at.into()),
            updated_at: Set(cat.updated_at.into()),
        };
        model.insert(&self.db).await?;
        Ok(())
    }

    async fn find_by_id(&self, id: &Uuid) -> anyhow::Result<Option<MemoryCategory>> {
        let result = memory_categories::Entity::find_by_id(*id)
            .one(&self.db)
            .await?;
        Ok(result.map(convert::memory_category_from_model))
    }

    async fn find_by_name(
        &self,
        user_scope: &str,
        name: &str,
    ) -> anyhow::Result<Option<MemoryCategory>> {
        let result = memory_categories::Entity::find()
            .filter(memory_categories::Column::UserScope.eq(user_scope))
            .filter(memory_categories::Column::Name.eq(name))
            .one(&self.db)
            .await?;
        Ok(result.map(convert::memory_category_from_model))
    }

    async fn update(&self, cat: &MemoryCategory) -> anyhow::Result<()> {
        let existing = memory_categories::Entity::find_by_id(cat.id)
            .one(&self.db)
            .await?
            .ok_or_else(|| anyhow::anyhow!("MemoryCategory not found: {}", cat.id))?;

        let model = memory_categories::ActiveModel {
            id: Set(existing.id),
            user_scope: Set(cat.user_scope.clone()),
            name: Set(cat.name.clone()),
            description: Set(cat.description.clone()),
            embedding: Set(convert::embedding_option_to_json(cat.embedding.as_ref())),
            summary: Set(cat.summary.clone()),
            created_at: Set(existing.created_at),
            updated_at: Set(cat.updated_at.into()),
        };
        model.update(&self.db).await?;
        Ok(())
    }

    async fn delete(&self, id: &Uuid) -> anyhow::Result<()> {
        let existing = memory_categories::Entity::find_by_id(*id)
            .one(&self.db)
            .await?
            .ok_or_else(|| anyhow::anyhow!("MemoryCategory not found: {id}"))?;

        existing.delete(&self.db).await?;
        Ok(())
    }

    async fn list_by_scope(&self, user_scope: &str) -> anyhow::Result<Vec<MemoryCategory>> {
        let results = memory_categories::Entity::find()
            .filter(memory_categories::Column::UserScope.eq(user_scope))
            .all(&self.db)
            .await?;
        Ok(results
            .into_iter()
            .map(convert::memory_category_from_model)
            .collect())
    }

    async fn search_by_embedding(
        &self,
        user_scope: &str,
        query_embedding: &[f32],
        top_k: usize,
    ) -> anyhow::Result<Vec<CategorySalienceScore>> {
        let categories: Vec<MemoryCategory> =
            MemoryCategoryRepo::list_by_scope(self, user_scope).await?;

        let mut scores: Vec<CategorySalienceScore> = categories
            .into_iter()
            .map(|category| {
                // Categories without embeddings get a low default score
                let score = category.embedding.as_ref().map_or(0.0, |embedding| {
                    scoring::cosine_similarity(query_embedding, embedding)
                });
                CategorySalienceScore { category, score }
            })
            .collect();

        scores.sort_by(|a, b| b.score.total_cmp(&a.score));
        scores.truncate(top_k);

        Ok(scores)
    }
}

#[async_trait]
impl CategoryItemRepo for MemoryManager {
    async fn link(&self, item_id: &Uuid, category_id: &Uuid) -> anyhow::Result<()> {
        let model = category_items::ActiveModel {
            item_id: Set(*item_id),
            category_id: Set(*category_id),
        };
        model.insert(&self.db).await?;
        Ok(())
    }

    async fn unlink(&self, item_id: &Uuid, category_id: &Uuid) -> anyhow::Result<()> {
        let existing = category_items::Entity::find()
            .filter(category_items::Column::ItemId.eq(*item_id))
            .filter(category_items::Column::CategoryId.eq(*category_id))
            .one(&self.db)
            .await?
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "CategoryItem link not found: item_id={item_id}, category_id={category_id}"
                )
            })?;

        existing.delete(&self.db).await?;
        Ok(())
    }

    async fn categories_for_item(&self, item_id: &Uuid) -> anyhow::Result<Vec<CategoryItem>> {
        let results = category_items::Entity::find()
            .filter(category_items::Column::ItemId.eq(*item_id))
            .all(&self.db)
            .await?;
        Ok(results
            .into_iter()
            .map(|m| convert::category_item_from_model(&m))
            .collect())
    }

    async fn items_for_category(&self, category_id: &Uuid) -> anyhow::Result<Vec<CategoryItem>> {
        let results = category_items::Entity::find()
            .filter(category_items::Column::CategoryId.eq(*category_id))
            .all(&self.db)
            .await?;
        Ok(results
            .into_iter()
            .map(|m| convert::category_item_from_model(&m))
            .collect())
    }
}

#[async_trait]
impl ResourceRepo for MemoryManager {
    async fn insert(&self, res: &Resource) -> anyhow::Result<()> {
        let model = resources::ActiveModel {
            id: Set(res.id),
            user_scope: Set(res.user_scope.clone()),
            url: Set(res.url.clone()),
            modality: Set(res.modality.clone()),
            local_path: Set(res.local_path.clone()),
            caption: Set(res.caption.clone()),
            embedding: Set(convert::embedding_option_to_json(res.embedding.as_ref())),
            created_at: Set(res.created_at.into()),
            updated_at: Set(res.updated_at.into()),
        };
        model.insert(&self.db).await?;
        Ok(())
    }

    async fn find_by_id(&self, id: &Uuid) -> anyhow::Result<Option<Resource>> {
        let result = resources::Entity::find_by_id(*id).one(&self.db).await?;
        Ok(result.map(convert::resource_from_model))
    }

    async fn delete(&self, id: &Uuid) -> anyhow::Result<()> {
        let existing = resources::Entity::find_by_id(*id)
            .one(&self.db)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Resource not found: {id}"))?;

        existing.delete(&self.db).await?;
        Ok(())
    }

    async fn list_by_scope(&self, user_scope: &str) -> anyhow::Result<Vec<Resource>> {
        let results = resources::Entity::find()
            .filter(resources::Column::UserScope.eq(user_scope))
            .all(&self.db)
            .await?;
        Ok(results
            .into_iter()
            .map(convert::resource_from_model)
            .collect())
    }

    async fn search_by_embedding(
        &self,
        user_scope: &str,
        query_embedding: &[f32],
        top_k: usize,
    ) -> anyhow::Result<Vec<ResourceSalienceScore>> {
        let resources: Vec<Resource> = ResourceRepo::list_by_scope(self, user_scope).await?;

        let mut scores: Vec<ResourceSalienceScore> = resources
            .into_iter()
            .map(|resource| {
                // Resources without embeddings get a low default score
                let score = resource.embedding.as_ref().map_or(0.0, |embedding| {
                    scoring::cosine_similarity(query_embedding, embedding)
                });
                ResourceSalienceScore { resource, score }
            })
            .collect();

        scores.sort_by(|a, b| b.score.total_cmp(&a.score));
        scores.truncate(top_k);

        Ok(scores)
    }
}
