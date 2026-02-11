use nanors_core::memory::{MemoryItem, SalienceScore};
use nanors_core::MemoryItemRepo;
use rayon::prelude::*;
use tracing::info;

use crate::manager::MemoryManager;
use crate::query::{QuestionType, QuestionTypeDetector};
use crate::rerank::Reranker;
use crate::scoring;

impl MemoryManager {
    /// Enhanced search with query expansion and structured card lookup.
    ///
    /// This method improves retrieval by:
    /// 1. Detecting question type and applying specialized strategies
    /// 2. Expanding queries for better recall
    /// 3. Looking up structured cards for O(1) fact retrieval
    ///
    /// # Arguments
    /// * `query_embedding` - Query vector embedding
    /// * `query_text` - Original query text
    /// * `top_k` - Maximum results to return
    ///
    /// # Returns
    /// Ranked memory results
    pub async fn search_enhanced(
        &self,
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
            .search_by_embedding(query_embedding, query_text, top_k)
            .await?;

        // Step 4: Apply question-type-specific ranking adjustments
        results = Self::rank_by_question_type(results, question_type, query_text);

        // Step 5: Apply reranking for improved relevance
        results = self.reranker.rerank(results, query_text);

        // Step 7: If results are sparse, try query expansion
        if results.len() < top_k / 2 {
            if let Some(expanded_query) = self.query_expander.expand_or(query_text) {
                info!("Expanding query with OR: {}", expanded_query);

                // Note: In a real implementation, you would re-embed the expanded query
                // For now, we just filter existing results more broadly
                results = Self::apply_expansion_boost(results, &expanded_query);
            }
        }

        // Step 8: Apply card boost if found
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
    ) -> anyhow::Result<Option<crate::extraction::MemoryCard>> {
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
    async fn boost_memory_with_card_value(
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
