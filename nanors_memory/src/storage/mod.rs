//! Storage engine for memory persistence.
//!
//! This module provides database CRUD operations for:
//! - Memory items (with deduplication)
//! - Sessions
//! - Temporal queries

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
    clippy::cast_precision_loss
)]

use async_trait::async_trait;
use chrono::Utc;
use nanors_core::memory::{MemoryItem, SalienceScore};
use nanors_core::{ChatMessage, MemoryItemRepo, Role, SessionStorage};
use nanors_entities::memory_items;
use rayon::prelude::*;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Database, DatabaseConnection, EntityTrait, ModelTrait,
    QueryFilter, Set,
};
use tracing::info;
use uuid::Uuid;

use crate::convert;
use crate::dedup;
use crate::extraction::DatabaseCardRepository;
use crate::scoring;

/// Storage engine for memory persistence.
///
/// Provides CRUD operations for memory items and sessions.
pub struct StorageEngine {
    /// Database connection for persistence
    db: DatabaseConnection,
    /// Repository for structured memory cards
    pub(crate) card_repo: DatabaseCardRepository,
}

impl StorageEngine {
    /// Create a new StorageEngine.
    pub async fn new(database_url: &str) -> anyhow::Result<Self> {
        info!("Connecting to database for StorageEngine");
        let db = Database::connect(database_url).await?;
        info!("StorageEngine initialized");
        Ok(Self {
            db: db.clone(),
            card_repo: DatabaseCardRepository::new(db),
        })
    }

    /// Get a reference to the database connection.
    #[must_use]
    pub const fn db(&self) -> &DatabaseConnection {
        &self.db
    }

    /// Get a reference to the card repository.
    #[must_use]
    pub const fn card_repo(&self) -> &DatabaseCardRepository {
        &self.card_repo
    }

    /// Clear a session by ID.
    pub async fn clear_session(&self, id: &Uuid) -> anyhow::Result<()> {
        sessions::Entity::delete_by_id(*id).exec(&self.db).await?;

        info!("Cleared session: {}", id);
        Ok(())
    }

    /// List all session IDs.
    pub async fn list_sessions(&self) -> anyhow::Result<Vec<Uuid>> {
        let session_models = sessions::Entity::find().all(&self.db).await?;

        Ok(session_models.into_iter().map(|s| s.id).collect())
    }

    /// Insert or update a memory item with deduplication.
    ///
    /// If an item with the same content hash already exists,
    /// its reinforcement count is incremented instead of creating a
    /// duplicate.
    pub async fn upsert_memory(&self, item: &MemoryItem) -> anyhow::Result<Uuid> {
        let hash = dedup::content_hash(&item.memory_type.to_string(), &item.summary);

        if let Some(existing) = self.find_by_content_hash(&hash).await? {
            let mut updated = existing.clone();
            updated.reinforcement_count += 1;
            updated.updated_at = Utc::now();

            // Update in database
            let active_model = memory_items::ActiveModel {
                id: sea_orm::Set(updated.id),
                reinforcement_count: sea_orm::Set(updated.reinforcement_count as i32),
                updated_at: sea_orm::Set(updated.updated_at.into()),
                ..Default::default()
            };

            active_model.update(&self.db).await?;

            info!(
                "Reinforced existing memory: {} (count: {})",
                updated.id, updated.reinforcement_count
            );

            Ok(updated.id)
        } else {
            // Create new memory
            let now = Utc::now();
            let model = memory_items::ActiveModel {
                id: sea_orm::Set(item.id),
                memory_type: sea_orm::Set(item.memory_type.to_string()),
                summary: sea_orm::Set(item.summary.clone()),
                embedding: sea_orm::Set(
                    item.embedding
                        .as_ref()
                        .map(|v| convert::embedding_to_json(v.as_slice())),
                ),
                happened_at: sea_orm::Set(item.happened_at.into()),
                extra: sea_orm::Set(item.extra.clone()),
                content_hash: sea_orm::Set(hash),
                reinforcement_count: sea_orm::Set(item.reinforcement_count as i32),
                created_at: sea_orm::Set(now.into()),
                updated_at: sea_orm::Set(now.into()),
            };
            let inserted = memory_items::Entity::insert(model).exec(&self.db).await?;

            info!("Created new memory: {}", inserted.last_insert_id);
            Ok(inserted.last_insert_id)
        }
    }

    /// Find a memory by content hash.
    pub async fn find_by_content_hash(&self, hash: &str) -> anyhow::Result<Option<MemoryItem>> {
        let result = memory_items::Entity::find()
            .filter(memory_items::Column::ContentHash.eq(hash))
            .one(&self.db)
            .await?;

        Ok(result.map(convert::memory_item_from_model))
    }

    /// Delete a memory by ID.
    pub async fn delete_memory(&self, id: &Uuid) -> anyhow::Result<()> {
        let existing = memory_items::Entity::find_by_id(*id)
            .one(&self.db)
            .await?
            .ok_or_else(|| anyhow::anyhow!("MemoryItem not found: {id}"))?;

        existing.delete(&self.db).await?;
        Ok(())
    }

    /// Get or create a session.
    pub async fn get_or_create_session(&self, id: &Uuid) -> anyhow::Result<sessions::Model> {
        if let Some(session) = sessions::Entity::find_by_id(*id).one(&self.db).await? {
            return Ok(session);
        }

        let now = Utc::now();
        let new_session = sessions::ActiveModel {
            id: sea_orm::Set(*id),
            created_at: sea_orm::Set(now.into()),
            updated_at: sea_orm::Set(now.into()),
            ..Default::default()
        };

        Ok(new_session.insert(&self.db).await?)
    }

}

#[async_trait]
impl MemoryItemRepo for StorageEngine {
    async fn insert(&self, item: &MemoryItem) -> anyhow::Result<()> {
        let hash = dedup::content_hash(&item.memory_type.to_string(), &item.summary);
        let now = Utc::now();

        let model = memory_items::ActiveModel {
            id: sea_orm::Set(item.id),
            memory_type: sea_orm::Set(item.memory_type.to_string()),
            summary: sea_orm::Set(item.summary.clone()),
            embedding: sea_orm::Set(
                item.embedding
                    .as_ref()
                    .map(|v| convert::embedding_to_json(v.as_slice())),
            ),
            happened_at: sea_orm::Set(item.happened_at.into()),
            extra: sea_orm::Set(item.extra.clone()),
            content_hash: sea_orm::Set(hash),
            reinforcement_count: sea_orm::Set(item.reinforcement_count as i32),
            created_at: sea_orm::Set(now.into()),
            updated_at: sea_orm::Set(now.into()),
        };
        model.insert(&self.db).await?;
        Ok(())
    }

    async fn find_by_id(&self, id: &Uuid) -> anyhow::Result<Option<MemoryItem>> {
        let result = memory_items::Entity::find_by_id(*id).one(&self.db).await?;
        Ok(result.map(convert::memory_item_from_model))
    }

    async fn find_by_content_hash(&self, hash: &str) -> anyhow::Result<Option<MemoryItem>> {
        let result = memory_items::Entity::find()
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
            memory_type: Set(item.memory_type.to_string()),
            summary: Set(item.summary.clone()),
            embedding: Set(item
                .embedding
                .as_ref()
                .map(|v| convert::embedding_to_json(v.as_slice()))),
            happened_at: Set(item.happened_at.naive_utc()),
            extra: Set(item.extra.clone()),
            content_hash: Set(item.content_hash.clone()),
            reinforcement_count: Set(item.reinforcement_count),
            created_at: Set(existing.created_at),
            updated_at: Set(item.updated_at.naive_utc()),
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

    async fn list_all(&self) -> anyhow::Result<Vec<MemoryItem>> {
        let results = memory_items::Entity::find().all(&self.db).await?;
        Ok(results
            .into_iter()
            .map(convert::memory_item_from_model)
            .collect())
    }

    async fn search_by_embedding(
        &self,
        query_embedding: &[f32],
        query_text: &str,
        top_k: usize,
    ) -> anyhow::Result<Vec<SalienceScore<MemoryItem>>> {
        let items: Vec<MemoryItem> = self.list_all().await?;
        let now = Utc::now();

        // Filter out items that are essentially the same as the query (similarity >= 0.95)
        // to avoid returning the exact same question back to the user
        let filtered_scores: Vec<SalienceScore<MemoryItem>> = items
            .into_par_iter()
            .map(|item| {
                let (similarity, salience) = if let Some(embedding) = &item.embedding {
                    let vector_sim = crate::scoring::cosine_similarity(query_embedding, embedding);
                    // Compute keyword overlap score
                    let keyword_overlap =
                        crate::scoring::keyword_overlap(query_text, &item.summary);
                    // Use hybrid similarity: 70% vector + 30% keyword
                    let hybrid_sim = crate::scoring::hybrid_similarity(vector_sim, keyword_overlap);
                    // Apply question penalty: penalize memories that are questions when query is also a question
                    let question_penalty =
                        crate::scoring::question_penalty(query_text, &item.summary);
                    let penalized_sim = hybrid_sim * question_penalty;
                    let sal = crate::scoring::compute_salience(
                        penalized_sim,
                        item.reinforcement_count,
                        item.happened_at,
                        now,
                    );
                    (penalized_sim, sal)
                } else {
                    // Items without embeddings get a low default score based on recency only
                    // Still compute keyword overlap for relevance
                    let keyword_overlap =
                        crate::scoring::keyword_overlap(query_text, &item.summary);
                    let hybrid_sim = crate::scoring::hybrid_similarity(0.0, keyword_overlap);
                    // Apply question penalty even for items without embeddings
                    let question_penalty =
                        crate::scoring::question_penalty(query_text, &item.summary);
                    let penalized_sim = hybrid_sim * question_penalty;
                    (
                        penalized_sim,
                        crate::scoring::compute_salience(
                            penalized_sim,
                            item.reinforcement_count,
                            item.happened_at,
                            now,
                        ),
                    )
                };

                SalienceScore {
                    item,
                    score: salience,
                    similarity,
                }
            })
            .filter(|score| score.similarity < 0.95)
            .collect();

        // Sort and truncate to top_k
        // Use default sorting (by salience score)
        let mut sorted = filtered_scores;
        sorted.par_sort_unstable_by(|a, b| b.score.total_cmp(&a.score));
        sorted.truncate(top_k);

        Ok(sorted)
    }

    async fn backfill_embeddings(
        &self,
        embed_fn: &(dyn Fn(String) -> anyhow::Result<Vec<f32>> + Send + Sync),
    ) -> anyhow::Result<usize> {
        let items = self.list_all().await?;
        let mut count = 0;

        for item in items {
            if item.embedding.is_none() {
                let embedding = embed_fn(item.summary)?;
                let mut updated = item.clone();
                updated.embedding = Some(embedding);
                self.update(&updated).await?;
                count += 1;
            }
        }

        Ok(count)
    }

    async fn semantic_upsert(
        &self,
        item: &MemoryItem,
        similarity_threshold: f64,
    ) -> anyhow::Result<Uuid> {
        let query_embedding = item
            .embedding
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Item must have embedding for semantic upsert"))?;

        // Search for semantically similar memories
        let similar = self
            .search_by_embedding(query_embedding, &item.summary, 10)
            .await?;

        // Check if any memory is above the similarity threshold
        if let Some(best_match) = similar.first() {
            if best_match.similarity >= similarity_threshold {
                // Update the existing memory with the new content
                let mut updated = best_match.item.clone();
                updated.summary = item.summary.clone();
                updated.embedding = item.embedding.clone();
                updated.extra = item.extra.clone();
                updated.updated_at = Utc::now();
                self.update(&updated).await?;
                return Ok(updated.id);
            }
        }

        // No similar memory found, insert as new
        let id = item.id;
        self.insert(item).await?;
        Ok(id)
    }
}

#[async_trait]
impl SessionStorage for StorageEngine {
    async fn get_or_create(&self, id: &Uuid) -> anyhow::Result<nanors_core::Session> {
        use nanors_core::{ChatMessage, Role};
        use serde_json;

        let session_model = sessions::Entity::find_by_id(*id).one(&self.db).await?;

        if let Some(model) = session_model {
            let messages: Vec<ChatMessage> = serde_json::from_str(&model.messages)?;

            Ok(nanors_core::Session {
                id: model.id,
                messages,
                created_at: model.created_at.and_utc(),
                updated_at: model.updated_at.and_utc(),
            })
        } else {
            let now = chrono::Utc::now();
            Ok(nanors_core::Session {
                id: *id,
                messages: vec![],
                created_at: now,
                updated_at: now,
            })
        }
    }

    async fn add_message(&self, id: &Uuid, role: Role, content: &str) -> anyhow::Result<()> {
        use nanors_core::ChatMessage;
        use serde_json;

        let now = chrono::Utc::now().naive_utc();

        if let Some(model) = sessions::Entity::find_by_id(*id).one(&self.db).await? {
            let mut messages: Vec<ChatMessage> = serde_json::from_str(&model.messages)?;
            messages.push(ChatMessage {
                role,
                content: content.to_string(),
            });
            let messages_json = serde_json::to_string(&messages)?;

            sessions::Entity::update(sessions::ActiveModel {
                id: Set(model.id),
                messages: Set(messages_json),
                created_at: Set(model.created_at),
                updated_at: Set(now.into()),
            })
            .exec(&self.db)
            .await?;
        } else {
            let messages = vec![ChatMessage {
                role,
                content: content.to_string(),
            }];
            let messages_json = serde_json::to_string(&messages)?;

            sessions::ActiveModel {
                id: Set(*id),
                messages: Set(messages_json),
                created_at: Set(now),
                updated_at: Set(now.into()),
            }
            .insert(&self.db)
            .await?;
        }

        info!("Added message to session: {}", id);
        Ok(())
    }
}
