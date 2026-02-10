use async_trait::async_trait;
use chrono::Utc;
use nanors_core::memory::{MemoryItem, SalienceScore};
use nanors_core::{ConversationSegment, ConversationSegmenter, MemoryItemRepo, SessionStorage};
use nanors_entities::memory_items;
use nanors_entities::sessions;
use rayon::prelude::*;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Database, DatabaseConnection, EntityTrait, ModelTrait,
    QueryFilter, Set,
};
use tracing::{debug, info};
use uuid::Uuid;

use crate::convert;
use crate::dedup;
use crate::extraction::DatabaseCardRepository;
use crate::query::{QueryExpander, QuestionTypeDetector};
use crate::rerank::{Reranker, RuleBasedReranker};
use crate::scoring;

/// Core memory management for AI agent conversations.
///
/// This struct provides:
/// - Persistent memory storage with deduplication
/// - Semantic search with vector embeddings
/// - Session management for conversation history
/// - Structured memory extraction and retrieval
pub struct MemoryManager {
    /// Database connection for persistence
    pub(crate) db: DatabaseConnection,
    /// Repository for structured memory cards
    pub(crate) card_repo: DatabaseCardRepository,
    /// Detector for question type analysis
    pub(crate) question_detector: QuestionTypeDetector,
    /// Query expansion for improved recall
    pub(crate) query_expander: QueryExpander,
    /// Reranker for result relevance tuning
    pub(crate) reranker: Box<dyn Reranker>,
    /// Optional conversation segmenter
    pub(crate) segmenter: Option<Box<dyn ConversationSegmenter>>,
}

impl MemoryManager {
    /// Create a new `MemoryManager` with default configuration.
    ///
    /// # Arguments
    /// * `database_url` - Database connection string
    pub async fn new(database_url: &str) -> anyhow::Result<Self> {
        info!("Connecting to database for MemoryManager");
        let db = Database::connect(database_url).await?;
        info!("MemoryManager initialized");
        Ok(Self {
            db: db.clone(),
            card_repo: DatabaseCardRepository::new(db),
            question_detector: QuestionTypeDetector::with_defaults(),
            query_expander: QueryExpander::with_defaults(),
            reranker: Box::new(RuleBasedReranker::new()),
            segmenter: None,
        })
    }

    /// Create a new `MemoryManager` with a custom reranker.
    ///
    /// # Arguments
    /// * `database_url` - Database connection string
    /// * `reranker` - Custom reranker implementation
    ///
    /// # Example
    /// ```no_run
    /// use nanors_memory::MemoryManager;
    /// use nanors_memory::rerank::{NoOpReranker, RuleBasedReranker};
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// // With rule-based reranker (default)
    /// let manager = MemoryManager::with_reranker(
    ///     "postgresql://...",
    ///     RuleBasedReranker::new()
    /// ).await?;
    ///
    /// // With no-op reranker (disabled)
    /// let manager = MemoryManager::with_reranker(
    ///     "postgresql://...",
    ///     NoOpReranker
    /// ).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn with_reranker<R>(database_url: &str, reranker: R) -> anyhow::Result<Self>
    where
        R: Reranker + 'static,
    {
        info!("Connecting to database for MemoryManager");
        let db = Database::connect(database_url).await?;
        info!("MemoryManager initialized with custom reranker");
        Ok(Self {
            db: db.clone(),
            card_repo: DatabaseCardRepository::new(db),
            question_detector: QuestionTypeDetector::with_defaults(),
            query_expander: QueryExpander::with_defaults(),
            reranker: Box::new(reranker),
            segmenter: None,
        })
    }

    /// Get a reference to the card repository.
    #[must_use]
    pub const fn card_repo(&self) -> &DatabaseCardRepository {
        &self.card_repo
    }

    /// Get a reference to the question detector.
    #[must_use]
    pub const fn question_detector(&self) -> &QuestionTypeDetector {
        &self.question_detector
    }

    /// Get a reference to the query expander.
    #[must_use]
    pub const fn query_expander(&self) -> &QueryExpander {
        &self.query_expander
    }

    /// Set a conversation segmenter for automatic conversation segmentation
    #[must_use]
    pub fn with_segmenter(mut self, segmenter: Box<dyn ConversationSegmenter>) -> Self {
        self.segmenter = Some(segmenter);
        self
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

    /// Segment a conversation into smaller parts for processing.
    pub async fn segment_conversation(
        &self,
        session_id: &Uuid,
    ) -> anyhow::Result<Vec<ConversationSegment>> {
        let session = self.get_or_create(session_id).await?;

        if let Some(segmenter) = &self.segmenter {
            let config = segmenter.config();
            let segments = segmenter
                .segment(session_id, &session.messages, config)
                .await?;

            info!(
                "Created {} segments for session {}",
                segments.len(),
                session_id
            );

            Ok(segments)
        } else {
            debug!("No segmenter configured, returning empty segments");
            Ok(vec![])
        }
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
            // Use item.summary as query text for hybrid similarity matching
            let similar_memories = MemoryItemRepo::search_by_embedding(
                self,
                &item.user_scope,
                embedding,
                &item.summary,
                20,
            )
            .await?;

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

                // For near-identical content (similarity > 0.97), just reinforce the existing memory
                // For semantically similar but not identical content (threshold < similarity <= 0.97),
                // create a new version (useful for fact updates like address changes)
                if similarity > 0.97 {
                    // Near-identical content - reinforce existing memory instead of creating duplicate
                    let mut updated = score.item.clone();
                    updated.reinforcement_count += 1;
                    updated.updated_at = Utc::now();
                    MemoryItemRepo::update(self, &updated).await?;
                    info!(
                        "Reinforced near-duplicate memory: {} (similarity={:.3}, count={})",
                        updated.id, similarity, updated.reinforcement_count
                    );
                    return Ok(updated.id);
                }

                if similarity > similarity_threshold {
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
        query_text: &str,
        top_k: usize,
    ) -> anyhow::Result<Vec<SalienceScore<MemoryItem>>> {
        let items: Vec<MemoryItem> = MemoryItemRepo::list_by_scope(self, user_scope).await?;
        let now = Utc::now();

        // Filter out items that are essentially the same as the query (similarity >= 0.95)
        // to avoid returning the exact same question back to the user
        let filtered_scores: Vec<SalienceScore<MemoryItem>> = items
            .into_par_iter()
            .map(|item| {
                let (similarity, salience) = if let Some(embedding) = &item.embedding {
                    let vector_sim = scoring::cosine_similarity(query_embedding, embedding);
                    // Compute keyword overlap score
                    let keyword_overlap = scoring::keyword_overlap(query_text, &item.summary);
                    // Use hybrid similarity: 70% vector + 30% keyword
                    let hybrid_sim = scoring::hybrid_similarity(vector_sim, keyword_overlap);
                    // Apply question penalty: penalize memories that are questions when query is also a question
                    let question_penalty = scoring::question_penalty(query_text, &item.summary);
                    let penalized_sim = hybrid_sim * question_penalty;
                    let sal = scoring::compute_salience(
                        penalized_sim,
                        item.reinforcement_count,
                        item.happened_at,
                        now,
                    );
                    (penalized_sim, sal)
                } else {
                    // Items without embeddings get a low default score based on recency only
                    // Still compute keyword overlap for relevance
                    let keyword_overlap = scoring::keyword_overlap(query_text, &item.summary);
                    let hybrid_sim = scoring::hybrid_similarity(0.0, keyword_overlap);
                    // Apply question penalty even for items without embeddings
                    let question_penalty = scoring::question_penalty(query_text, &item.summary);
                    let penalized_sim = hybrid_sim * question_penalty;
                    (
                        penalized_sim,
                        scoring::compute_salience(
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

        // Apply reranker for question-type-specific score boosting
        let boosted = self.reranker.rerank(filtered_scores, query_text);

        // Sort memories with a multi-tier priority system:
        // 1. Primary tier: Facts (no question keywords) > Questions (with question keywords)
        // 2. Secondary tier: For facts, time-weighted similarity (newest wins when close)
        //                     For questions, salience score (higher is better)
        // 3. Tertiary tier: Recency (newest first)
        //
        // This ensures that:
        // - When a user asks a question, factual answers rank higher than questions
        // - Among facts, newer memories are preferred when semantic similarity is close
        // - The time-based tiebreaker uses a small epsilon threshold for "closeness"
        let similarity_epsilon = 0.05_f64;

        let mut sorted = boosted;
        sorted.par_sort_unstable_by(|a, b| {
            // Primary: Facts (no question keywords) rank higher than questions
            let a_is_question = scoring::count_question_keywords(&a.item.summary) > 0;
            let b_is_question = scoring::count_question_keywords(&b.item.summary) > 0;
            match (!a_is_question, !b_is_question) {
                (true, false) => return std::cmp::Ordering::Less, // a is fact, b is question: a < b (a first)
                (false, true) => return std::cmp::Ordering::Greater, // a is question, b is fact: a > b (b first)
                _ => {}
            }

            // Secondary: different strategies for facts vs questions
            if !a_is_question && !b_is_question {
                // Both are facts: prefer newer memory when similarities are close
                let sim_diff = (a.similarity - b.similarity).abs();
                if sim_diff < similarity_epsilon {
                    // Similarities are close: newest wins
                    return b.item.happened_at.cmp(&a.item.happened_at);
                }
                // Otherwise: higher similarity wins
                b.similarity.total_cmp(&a.similarity)
            } else {
                // Both are questions: higher salience score (with reranker boost applied) wins
                match b.score.total_cmp(&a.score) {
                    std::cmp::Ordering::Equal => b.item.happened_at.cmp(&a.item.happened_at),
                    other => other,
                }
            }
        });

        // Deduplicate: keep only the highest-similarity item for each unique summary
        let mut deduped = Vec::new();
        let mut seen_summaries = std::collections::HashSet::new();
        for score in sorted {
            if seen_summaries.insert(score.item.summary.clone()) {
                deduped.push(score);
            }
        }
        deduped.truncate(top_k);

        Ok(deduped)
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
}
