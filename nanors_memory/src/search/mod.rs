//! Search engine for memory retrieval.
//!
//! This module provides advanced search capabilities including:
//! - Question type detection and specialized ranking
//! - Query expansion for improved recall
//! - Structured card lookup for O(1) fact retrieval

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

use nanors_core::MemoryItemRepo;
use nanors_core::memory::{MemoryItem, SalienceScore};
use rayon::prelude::*;
use tracing::{debug, info};
use uuid::Uuid;

use crate::extraction::{CardRepository, DatabaseCardRepository, MemoryCard};
use crate::query::{QueryExpander, QuestionType, QuestionTypeDetector};
use crate::rerank::Reranker;
use crate::scoring;

/// Search engine for advanced memory retrieval.
///
/// Combines multiple search strategies:
/// - Question type detection for specialized ranking
/// - Query expansion
/// - Structured card lookup
pub struct SearchEngine {
    /// Detector for question type analysis
    question_detector: QuestionTypeDetector,
    /// Query expansion for improved recall
    query_expander: QueryExpander,
    /// Reranker for result relevance tuning
    reranker: Box<dyn Reranker>,
    /// Repository for structured memory cards
    card_repo: DatabaseCardRepository,
}

impl SearchEngine {
    /// Create a new SearchEngine with default configuration.
    #[must_use]
    pub fn new(card_repo: DatabaseCardRepository) -> Self {
        Self {
            question_detector: QuestionTypeDetector::with_defaults(),
            query_expander: QueryExpander::with_defaults(),
            reranker: Box::new(crate::rerank::RuleBasedReranker::new()),
            card_repo,
        }
    }

    /// Create a new SearchEngine with a custom reranker.
    pub fn with_reranker<R>(card_repo: DatabaseCardRepository, reranker: R) -> Self
    where
        R: Reranker + 'static,
    {
        Self {
            question_detector: QuestionTypeDetector::with_defaults(),
            query_expander: QueryExpander::with_defaults(),
            reranker: Box::new(reranker),
            card_repo,
        }
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

    /// Get a reference to the card repository.
    #[must_use]
    pub const fn card_repo(&self) -> &DatabaseCardRepository {
        &self.card_repo
    }

    /// Get a reference to the reranker.
    #[must_use]
    pub const fn reranker(&self) -> &dyn Reranker {
        self.reranker.as_ref()
    }

    /// Enhanced search with query expansion and structured card lookup.
    ///
    /// This method improves retrieval by:
    /// 1. Detecting question type and applying specialized strategies
    /// 2. Expanding queries for better recall
    /// 3. Looking up structured cards for O(1) fact retrieval
    ///
    /// # Arguments
    /// * `repo` - Memory repository for fetching items
    /// * `user_scope` - User namespace
    /// * `query_embedding` - Query vector embedding
    /// * `query_text` - Original query text
    /// * `top_k` - Maximum results to return
    ///
    /// # Returns
    /// Ranked memory results
    pub async fn search_enhanced<R: MemoryItemRepo>(
        &self,
        repo: &R,
        query_embedding: &[f32],
        query_text: &str,
        top_k: usize,
    ) -> anyhow::Result<Vec<SalienceScore<MemoryItem>>> {
        // Step 1: Detect question type
        let question_type = self.question_detector.detect(query_text);
        info!(
            "Detected question type: {:?} for query: {}",
            question_type, query_text
        );

        // Step 2: Try structured card lookup for known question types
        let card_boost = if let Some(card) = self
            .lookup_card_for_question(question_type, query_text)
            .await?
        {
            info!(
                "Found structured card: entity={}, slot={}, value={}",
                card.entity, card.slot, card.value
            );
            // Find the memory that contains this card's value
            self.boost_memory_with_card_value(&card.value, query_embedding, top_k)
                .await?
        } else {
            None
        };

        // Step 3: Perform standard vector search
        let mut results = self
            .search_by_embedding(repo, query_embedding, query_text, top_k)
            .await?;

        // Step 4: Apply question-type-specific ranking adjustments
        results = Self::rank_by_question_type(results, question_type, query_text);

        // Step 5: Apply reranking for improved relevance
        results = self.reranker.rerank(results, query_text);

        // Step 6: If results are sparse, try query expansion
        if results.len() < top_k / 2 {
            if let Some(expanded_query) = self.query_expander.expand_or(query_text) {
                info!("Expanding query with OR: {}", expanded_query);

                // Note: In a real implementation, you would re-embed the expanded query
                // For now, we just filter existing results more broadly
                results = Self::apply_expansion_boost(results, &expanded_query);
            }
        }

        // Step 7: If card boost was found, ensure it's in results
        if let Some(boosted_memory) = card_boost {
            // Ensure the boosted memory is in results
            if !results.iter().any(|r| r.item.id == boosted_memory.item.id) {
                results.insert(0, boosted_memory);
            }
        }

        // Truncate to top_k
        results.truncate(top_k);

        Ok(results)
    }

    /// Look up a structured card based on question type.
    async fn lookup_card_for_question(
        &self,
        question_type: QuestionType,
        _query_text: &str,
    ) -> anyhow::Result<Option<MemoryCard>> {
        let (entity, slot) = match question_type {
            QuestionType::Where => ("user", "location"),
            QuestionType::Preference => ("user", "preference"),
            QuestionType::WhatKind | QuestionType::Recency => ("user", "user_type"),
            _ => return Ok(None),
        };

        self.card_repo
            .find_by_entity_slot(entity, slot)
            .await
    }

    /// Find memory containing a specific card value.
    async fn boost_memory_with_card_value<R: MemoryItemRepo>(
        &self,
        value: &str,
        query_embedding: &[f32],
        _top_k: usize,
    ) -> anyhow::Result<Option<SalienceScore<MemoryItem>>> {
        let all_items = MemoryItemRepo::list_all(self).await?;

        // Find memories that contain the card value
        let matching_memory = all_items
            .into_iter()
            .find(|item| item.summary.contains(value));

        if let Some(item) = matching_memory {
            let similarity = item
                .embedding
                .as_ref()
                .map_or(0.0, |emb| scoring::cosine_similarity(query_embedding, emb));

            Ok(Some(SalienceScore {
                item,
                score: 1.0, // Max score for card match
                similarity,
            }))
        } else {
            Ok(None)
        }
    }

    /// Perform vector similarity search.
    async fn search_by_embedding<R: MemoryItemRepo>(
        &self,
        repo: &R,
        query_embedding: &[f32],
        query_text: &str,
        top_k: usize,
    ) -> anyhow::Result<Vec<SalienceScore<MemoryItem>>> {
        let all_items = MemoryItemRepo::list_all(self).await?;

        let mut results: Vec<SalienceScore<MemoryItem>> = all_items
            .into_iter()
            .filter_map(|item| {
                let embedding = item.embedding.as_ref()?;
                let similarity = scoring::cosine_similarity(query_embedding, embedding);
                Some(SalienceScore {
                    item,
                    score: similarity,
                    similarity,
                })
            })
            .collect();

        // Sort by score
        results.par_sort_unstable_by(|a, b| b.score.total_cmp(&a.score));

        // Truncate to top_k
        results.truncate(top_k);

        debug!(
            "Vector search returned {} results for query: {}",
            results.len(),
            query_text
        );

        Ok(results)
    }

    /// Rank results by question type.
    fn rank_by_question_type(
        mut results: Vec<SalienceScore<MemoryItem>>,
        question_type: QuestionType,
        _query_text: &str,
    ) -> Vec<SalienceScore<MemoryItem>> {
        match question_type {
            QuestionType::Recency => {
                // For recency questions, prefer newer memories
                results.par_sort_unstable_by(|a, b| b.item.happened_at.cmp(&a.item.happened_at));
            }
            QuestionType::WhatKind => {
                // For "what kind" questions, prefer profile facts
                results.par_sort_unstable_by(|a, b| {
                    let a_is_profile = a.item.summary.to_lowercase().contains("用户")
                        || a.item.summary.to_lowercase().contains("user");
                    let b_is_profile = b.item.summary.to_lowercase().contains("用户")
                        || b.item.summary.to_lowercase().contains("user");

                    match (b_is_profile, a_is_profile) {
                        (true, false) => std::cmp::Ordering::Greater,
                        (false, true) => std::cmp::Ordering::Less,
                        _ => b.score.total_cmp(&a.score),
                    }
                });
            }
            _ => {}
        }
        results
    }

    /// Apply boost based on expanded query terms.
    fn apply_expansion_boost(
        results: Vec<SalienceScore<MemoryItem>>,
        expanded_query: &str,
    ) -> Vec<SalienceScore<MemoryItem>> {
        let expansion_terms: Vec<&str> = expanded_query
            .split(" OR ")
            .filter(|t| !t.is_empty())
            .collect();

        results
            .into_iter()
            .map(|mut score| {
                // Boost score if memory matches any expansion term
                for term in &expansion_terms {
                    if score
                        .item
                        .summary
                        .to_lowercase()
                        .contains(&term.to_lowercase())
                    {
                        score.score *= 1.2; // 20% boost for expansion matches
                        break;
                    }
                }
                score
            })
            .collect()
    }
}

impl Default for SearchEngine {
    fn default() -> Self {
        Self::new(DatabaseCardRepository::new(
            sea_orm::DatabaseConnection::connect("sqlite::memory:").unwrap(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn create_test_memory(summary: &str, hours_ago: i64) -> MemoryItem {
        MemoryItem {
            id: uuid::Uuid::now_v7(),
            memory_type: nanors_core::memory::MemoryType::Episodic,
            summary: summary.to_string(),
            embedding: None,
            happened_at: chrono::Utc::now() - Duration::hours(hours_ago),
            extra: None,
            content_hash: "test".to_string(),
            reinforcement_count: 1,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    fn create_test_score(summary: &str, hours_ago: i64, score: f64) -> SalienceScore<MemoryItem> {
        SalienceScore {
            item: create_test_memory(summary, hours_ago),
            score,
            similarity: 0.8,
        }
    }

    #[test]
    fn test_search_engine_creation() {
        let card_repo = DatabaseCardRepository::new(
            sea_orm::DatabaseConnection::connect("sqlite::memory:").unwrap(),
        );
        let engine = SearchEngine::new(card_repo);
        assert_eq!(engine.question_detector().detect("test"), QuestionType::Generic);
    }

    #[test]
    fn test_question_type_detection() {
        let detector = QuestionTypeDetector::with_defaults();

        assert_eq!(detector.detect("我是什么用户"), QuestionType::WhatKind);
        assert_eq!(detector.detect("有多少个"), QuestionType::HowMany);
        assert_eq!(detector.detect("现在的"), QuestionType::Recency);
        assert_eq!(detector.detect("在哪"), QuestionType::Where);
        assert_eq!(detector.detect("喜欢什么"), QuestionType::Preference);
        assert_eq!(detector.detect("什么时候"), QuestionType::When);
        assert_eq!(detector.detect("有谁"), QuestionType::Have);
        assert_eq!(detector.detect("会什么"), QuestionType::Can);
        assert_eq!(detector.detect("之前vs现在"), QuestionType::Update);
        assert_eq!(detector.detect("test"), QuestionType::Generic);
    }

    #[test]
    fn test_query_expansion() {
        let expander = QueryExpander::with_defaults();

        // Test Chinese
        let expanded = expander.expand_or("我是什么用户");
        assert!(expanded.contains("我"));
        assert!(expanded.contains("用户"));
        assert!(expanded.contains("安卓"));

        // Test English
        let expanded = expander.expand_or("what is my type");
        assert!(expanded.contains("what") || expanded.contains("is"));
        assert!(expanded.contains("my") || expanded.contains("type"));
    }

    #[test]
    fn test_rank_by_question_type_recency() {
        let engine = SearchEngine::new(DatabaseCardRepository::new(
            sea_orm::DatabaseConnection::connect("sqlite::memory:").unwrap(),
        ));

        let mut results = vec![
            create_test_score("User: type A", 100, 0.7),
            create_test_score("User: type B", 1, 0.7),
        ];

        results = engine
            .rank_by_question_type(results, QuestionType::Recency, "latest type");

        assert!(
            results[0].item.happened_at > results[1].item.happened_at,
            "Recent memory should rank higher for recency question"
        );
    }

    #[test]
    fn test_rank_by_question_type_profile() {
        let engine = SearchEngine::new(DatabaseCardRepository::new(
            sea_orm::DatabaseConnection::connect("sqlite::memory:").unwrap(),
        ));

        let mut results = vec![
            create_test_score("你住在哪里", 1, 0.8),
            create_test_score("User: 我是用户", 1, 0.7),
            create_test_score("这是什么", 1, 0.6),
        ];

        results = engine
            .rank_by_question_type(results, QuestionType::WhatKind, "what kind");

        assert!(
            results[0].item.summary.contains("User") || results[0].item.summary.contains("用户"),
            "Profile fact should rank higher for what kind question"
        );
    }

    #[test]
    fn test_apply_expansion_boost() {
        let results = vec![
            create_test_score("Test A", 1, 0.8),
            create_test_score("Test B", 1, 0.6),
            create_test_score("Test C", 1, 0.5),
        ];

        let boosted = SearchEngine::apply_expansion_boost(results.clone(), "A OR C");

        // Test A should get boost
        assert_eq!(boosted[0].item.summary, "Test A");
        assert_eq!(boosted[0].score, 0.8 * 1.2);

        // Test B should not get boost
        assert_eq!(boosted[1].item.summary, "Test B");
        assert_eq!(boosted[1].score, 0.6);

        // Test C should get boost
        assert_eq!(boosted[2].item.summary, "Test C");
        assert_eq!(boosted[2].score, 0.5 * 1.2);
    }
}
