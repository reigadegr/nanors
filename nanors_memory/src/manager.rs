use async_trait::async_trait;
use chrono::Utc;
use nanors_core::memory::{
    CategoryItem, CategorySalienceScore, MemoryCategory, MemoryItem, Resource,
    ResourceSalienceScore, SalienceScore,
};
use nanors_core::{CategoryItemRepo, MemoryCategoryRepo, MemoryItemRepo, ResourceRepo};
use nanors_entities::{category_items, memory_categories, memory_items, resources};
use rayon::prelude::*;
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

    /// Insert or update a memory item based on semantic similarity.
    ///
    /// This method implements semantic memory versioning:
    /// 1. First checks for exact duplicate via `content_hash`
    /// 2. If no exact match, searches for semantically similar memories
    /// 3. If similarity > threshold, creates a new version of that memory
    /// 4. Otherwise inserts as a new memory
    ///
    /// # Arguments
    /// * `item` - The memory item to insert or use for update
    /// * `similarity_threshold` - Minimum similarity (0.0-1.0) to consider
    ///   memories as semantically equivalent
    ///
    /// # Returns
    /// ID of the inserted or updated memory
    #[tracing::instrument(skip(self, item))]
    pub async fn semantic_upsert_memory(
        &self,
        item: &MemoryItem,
        similarity_threshold: f64,
    ) -> anyhow::Result<Uuid> {
        // Fast path: check for exact duplicate via content_hash
        let hash = dedup::content_hash(&item.memory_type.to_string(), &item.summary);
        if let Some(existing) =
            MemoryItemRepo::find_by_content_hash(self, &item.user_scope, &hash).await?
        {
            let mut updated = existing.clone();
            updated.reinforcement_count += 1;
            updated.updated_at = Utc::now();
            MemoryItemRepo::update(self, &updated).await?;
            info!(
                "Reinforced exact duplicate memory: {} (count={})",
                updated.id, updated.reinforcement_count
            );
            return Ok(updated.id);
        }

        // Semantic similarity check: only if we have an embedding
        if let Some(ref embedding) = item.embedding {
            // Search for semantically similar memories (fetch more than needed)
            let similar_memories =
                MemoryItemRepo::search_by_embedding(self, &item.user_scope, embedding, 20).await?;

            // Determine if this is a user memory (starts with "User:")
            let is_user_memory = item.summary.starts_with("User:");

            // Find the most similar memory above threshold
            for score in &similar_memories {
                // Only compare memories of the same category (user vs assistant)
                // User memories start with "User:", assistant memories start with "Assistant:"
                let is_score_user_memory = score.item.summary.starts_with("User:");
                if is_user_memory != is_score_user_memory {
                    continue;
                }

                let similarity = score
                    .item
                    .embedding
                    .as_ref()
                    .map_or(0.0, |emb| scoring::cosine_similarity(embedding, emb));

                // Check if similarity exceeds threshold
                // Use >0.95 to avoid self-matches (but allow for exact same content updates)
                if similarity > similarity_threshold && similarity < 0.98 {
                    // Found a semantically similar memory - create a new version
                    let mut updated = score.item.clone();
                    updated.summary = item.summary.clone();
                    updated.embedding = item.embedding.clone();
                    updated.happened_at = item.happened_at;
                    updated.updated_at = Utc::now();
                    updated.extra = item.extra.clone();
                    // Note: we keep the original content_hash for version tracking

                    MemoryItemRepo::update(self, &updated).await?;
                    info!(
                        "Created new version {} of memory {} (similarity={:.3})",
                        updated.id, score.item.id, similarity
                    );
                    return Ok(updated.id);
                }
            }
        }

        // No similar memory found - insert as new
        let mut new_item = item.clone();
        new_item.content_hash = hash;
        MemoryItemRepo::insert(self, &new_item).await?;
        info!("Inserted new memory: {}", new_item.id);
        Ok(new_item.id)
    }

    /// Insert or update a memory item using keyword-triggered versioning.
    ///
    /// This method implements rule-based memory versioning without LLM:
    /// 1. Analyzes the memory to detect fact type using keywords
    /// 2. If it's an assistant response, just store it (no versioning)
    /// 3. If it's a user input with a fact type:
    ///    - Find existing active memory with same `fact_type`
    ///    - Mark old memory as inactive
    ///    - Create new version marked as active
    /// 4. Otherwise store as non-fact memory
    ///
    /// # Arguments
    /// * `item` - The memory item to insert or use for update
    ///
    /// # Returns
    /// ID of the inserted or updated memory
    #[tracing::instrument(skip(self, item))]
    pub async fn keyword_versioned_insert(&self, item: &MemoryItem) -> anyhow::Result<Uuid> {
        use nanors_core::memory::{MemoryVersioner, VersioningAction};

        let versioner = MemoryVersioner::new();
        let result = versioner.analyze(item);

        match result.action {
            VersioningAction::NoVersioning => {
                // Assistant response - just store it
                let hash = dedup::content_hash(&item.memory_type.to_string(), &item.summary);
                let mut new_item = item.clone();
                new_item.content_hash = hash;
                MemoryItemRepo::insert(self, &new_item).await?;
                info!("Inserted assistant response: {}", new_item.id);
                Ok(new_item.id)
            }
            VersioningAction::NewFact { fact_type } => {
                // User input with detected fact type
                let fact_type_str = fact_type.as_str();

                // Check if there's an existing active memory with this fact_type
                if let Some(existing_active) = self
                    .find_active_by_fact_type(&item.user_scope, fact_type_str)
                    .await?
                {
                    // Deactivate the old memory
                    self.deactivate_memory(&existing_active.id).await?;
                    info!(
                        "Deactivated old memory {} for fact_type={}",
                        existing_active.id, fact_type_str
                    );

                    // Create new version with parent_id linking
                    let hash = dedup::content_hash(&item.memory_type.to_string(), &item.summary);
                    let new_id = Uuid::now_v7();
                    let embedding_json = item
                        .embedding
                        .as_ref()
                        .map(|v| convert::embedding_to_json(v.as_slice()));
                    let model = memory_items::ActiveModel {
                        id: Set(new_id),
                        user_scope: Set(item.user_scope.clone()),
                        resource_id: Set(item.resource_id),
                        memory_type: Set(item.memory_type.to_string()),
                        summary: Set(item.summary.clone()),
                        embedding: Set(embedding_json),
                        happened_at: Set(item.happened_at.into()),
                        extra: Set(item.extra.clone()),
                        content_hash: Set(hash),
                        reinforcement_count: Set(0),
                        created_at: Set(Utc::now().into()),
                        updated_at: Set(Utc::now().into()),
                        version: Set(existing_active.version + 1),
                        parent_version_id: Set(Some(existing_active.id)),
                        version_relation: Set(Some("Updates".to_string())),
                        fact_type: Set(Some(fact_type_str.to_string())),
                        is_active: Set(true),
                        parent_id: Set(Some(existing_active.id)),
                    };
                    model.insert(&self.db).await?;
                    info!(
                        "Created new version {} of fact_type={} (parent={})",
                        new_id, fact_type_str, existing_active.id
                    );
                    Ok(new_id)
                } else {
                    // No existing memory - insert as new fact
                    let hash = dedup::content_hash(&item.memory_type.to_string(), &item.summary);
                    let embedding_json = item
                        .embedding
                        .as_ref()
                        .map(|v| convert::embedding_to_json(v.as_slice()));
                    let model = memory_items::ActiveModel {
                        id: Set(item.id),
                        user_scope: Set(item.user_scope.clone()),
                        resource_id: Set(item.resource_id),
                        memory_type: Set(item.memory_type.to_string()),
                        summary: Set(item.summary.clone()),
                        embedding: Set(embedding_json),
                        happened_at: Set(item.happened_at.into()),
                        extra: Set(item.extra.clone()),
                        content_hash: Set(hash),
                        reinforcement_count: Set(0),
                        created_at: Set(item.created_at.into()),
                        updated_at: Set(item.updated_at.into()),
                        version: Set(1),
                        parent_version_id: Set(None),
                        version_relation: Set(Some("Sets".to_string())),
                        fact_type: Set(Some(fact_type_str.to_string())),
                        is_active: Set(true),
                        parent_id: Set(None),
                    };
                    model.insert(&self.db).await?;
                    info!(
                        "Inserted new fact memory: {} with fact_type={}",
                        item.id, fact_type_str
                    );
                    Ok(item.id)
                }
            }
            VersioningAction::NonFact => {
                // User input without detected fact type
                let hash = dedup::content_hash(&item.memory_type.to_string(), &item.summary);
                let mut new_item = item.clone();
                new_item.content_hash = hash;
                MemoryItemRepo::insert(self, &new_item).await?;
                info!("Inserted non-fact memory: {}", new_item.id);
                Ok(new_item.id)
            }
            _ => {
                // Other actions - insert as regular memory
                let hash = dedup::content_hash(&item.memory_type.to_string(), &item.summary);
                let mut new_item = item.clone();
                new_item.content_hash = hash;
                MemoryItemRepo::insert(self, &new_item).await?;
                info!("Inserted memory: {}", new_item.id);
                Ok(new_item.id)
            }
        }
    }

    /// Find active memory by `fact_type` for keyword-based retrieval
    pub async fn find_active_by_fact_type(
        &self,
        user_scope: &str,
        fact_type: &str,
    ) -> anyhow::Result<Option<MemoryItem>> {
        let result = memory_items::Entity::find()
            .filter(memory_items::Column::UserScope.eq(user_scope))
            .filter(memory_items::Column::FactType.eq(fact_type))
            .filter(memory_items::Column::IsActive.eq(true))
            .one(&self.db)
            .await?;
        Ok(result.map(convert::memory_item_from_model))
    }

    /// Deactivate a memory (set `is_active` to false)
    async fn deactivate_memory(&self, id: &Uuid) -> anyhow::Result<()> {
        let existing = memory_items::Entity::find_by_id(*id)
            .one(&self.db)
            .await?
            .ok_or_else(|| anyhow::anyhow!("MemoryItem not found: {id}"))?;

        let model = memory_items::ActiveModel {
            id: Set(existing.id),
            user_scope: Set(existing.user_scope.clone()),
            resource_id: Set(existing.resource_id),
            memory_type: Set(existing.memory_type.clone()),
            summary: Set(existing.summary.clone()),
            embedding: Set(existing.embedding.clone()),
            happened_at: Set(existing.happened_at),
            extra: Set(existing.extra.clone()),
            content_hash: Set(existing.content_hash.clone()),
            reinforcement_count: Set(existing.reinforcement_count),
            created_at: Set(existing.created_at),
            updated_at: Set(existing.updated_at),
            version: Set(existing.version),
            parent_version_id: Set(existing.parent_version_id),
            version_relation: Set(existing.version_relation.clone()),
            fact_type: Set(existing.fact_type.clone()),
            is_active: Set(false),
            parent_id: Set(existing.parent_id),
        };
        model.update(&self.db).await?;
        Ok(())
    }
}

#[async_trait]
impl MemoryItemRepo for MemoryManager {
    async fn insert(&self, item: &MemoryItem) -> anyhow::Result<()> {
        let embedding_json = item
            .embedding
            .as_ref()
            .map(|v| convert::embedding_to_json(v.as_slice()));
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
            version: Set(item.version),
            parent_version_id: Set(item.parent_version_id),
            version_relation: Set(item.version_relation.clone()),
            fact_type: Set(item.fact_type.clone()),
            is_active: Set(item.is_active),
            parent_id: Set(item.parent_id),
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
            embedding: Set(item
                .embedding
                .as_ref()
                .map(|v| convert::embedding_to_json(v.as_slice()))),
            happened_at: Set(item.happened_at.into()),
            extra: Set(item.extra.clone()),
            content_hash: Set(item.content_hash.clone()),
            reinforcement_count: Set(item.reinforcement_count),
            created_at: Set(existing.created_at),
            updated_at: Set(item.updated_at.into()),
            version: Set(existing.version + 1),
            parent_version_id: Set(Some(existing.id)),
            version_relation: Set(Some("Updates".to_string())),
            fact_type: Set(item.fact_type.clone()),
            is_active: Set(item.is_active),
            parent_id: Set(item.parent_id),
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
            .filter(memory_items::Column::IsActive.eq(true))
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
    ) -> anyhow::Result<Vec<SalienceScore<MemoryItem>>> {
        let items: Vec<MemoryItem> = MemoryItemRepo::list_by_scope(self, user_scope).await?;
        let now = Utc::now();

        // Filter out items that are essentially the same as the query (similarity >= 0.95)
        // to avoid returning the exact same question back to the user
        let mut filtered_scores: Vec<SalienceScore<MemoryItem>> = items
            .into_par_iter()
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
            .filter(|score| {
                let sim = score
                    .item
                    .embedding
                    .as_ref()
                    .map_or(0.0, |emb| scoring::cosine_similarity(query_embedding, emb));
                sim < 0.95
            })
            .collect();

        // Use parallel unstable sort for better performance (no extra allocation)
        filtered_scores.par_sort_unstable_by(|a, b| b.score.total_cmp(&a.score));
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

    async fn semantic_upsert(
        &self,
        item: &MemoryItem,
        similarity_threshold: f64,
    ) -> anyhow::Result<Uuid> {
        self.semantic_upsert_memory(item, similarity_threshold)
            .await
    }

    async fn keyword_versioned_insert(&self, item: &MemoryItem) -> anyhow::Result<Uuid> {
        self.keyword_versioned_insert(item).await
    }

    async fn find_active_by_fact_type(
        &self,
        user_scope: &str,
        fact_type: &str,
    ) -> anyhow::Result<Option<MemoryItem>> {
        self.find_active_by_fact_type(user_scope, fact_type).await
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
            embedding: Set(cat
                .embedding
                .as_ref()
                .map(|v| convert::embedding_to_json(v.as_slice()))),
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
            embedding: Set(cat
                .embedding
                .as_ref()
                .map(|v| convert::embedding_to_json(v.as_slice()))),
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
            .into_par_iter()
            .map(|category| {
                // Categories without embeddings get a low default score
                let score = category.embedding.as_ref().map_or(0.0, |embedding| {
                    scoring::cosine_similarity(query_embedding, embedding)
                });
                CategorySalienceScore {
                    item: category,
                    score,
                }
            })
            .collect();

        // Use parallel unstable sort for better performance (no extra allocation)
        scores.par_sort_unstable_by(|a, b| b.score.total_cmp(&a.score));
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
            embedding: Set(res
                .embedding
                .as_ref()
                .map(|v| convert::embedding_to_json(v.as_slice()))),
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
            .into_par_iter()
            .map(|resource| {
                // Resources without embeddings get a low default score
                let score = resource.embedding.as_ref().map_or(0.0, |embedding| {
                    scoring::cosine_similarity(query_embedding, embedding)
                });
                ResourceSalienceScore {
                    item: resource,
                    score,
                }
            })
            .collect();

        // Use parallel unstable sort for better performance (no extra allocation)
        scores.par_sort_unstable_by(|a, b| b.score.total_cmp(&a.score));
        scores.truncate(top_k);

        Ok(scores)
    }
}
