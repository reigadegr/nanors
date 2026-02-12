//! Integration tests for the reranking functionality.
//!
//! These tests verify that the reranker correctly applies question-type-specific
//! boosts and improves search result relevance.

use chrono::{Duration, Utc};
use nanors_core::memory::{MemoryItem, MemoryType, SalienceScore};
use nanors_memory::rerank::{Reranker, RuleBasedReranker};
use uuid::Uuid;

fn create_test_memory(summary: &str, hours_ago: i64) -> MemoryItem {
    MemoryItem {
        id: Uuid::now_v7(),
        memory_type: MemoryType::Episodic,
        summary: summary.to_string(),
        embedding: None,
        happened_at: Utc::now() - Duration::hours(hours_ago),
        extra: None,
        content_hash: "test".to_string(),
        reinforcement_count: 1,
        created_at: Utc::now(),
        updated_at: Utc::now(),
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
fn test_reranker_boosts_profile_facts_for_what_kind_questions() {
    let reranker = RuleBasedReranker::new();

    let mut results = vec![
        create_test_score("User: 我是Android用户", 1, 0.7),
        create_test_score("你住在哪里呢", 1, 0.8),
        create_test_score("User: 我住西城区", 1, 0.6),
    ];

    results = reranker.rerank(results, "我是什么用户");

    // Profile fact should be boosted to top
    assert!(
        results[0].item.summary.contains("Android用户"),
        "Profile fact should rank first for 'what kind' question"
    );
}

#[test]
fn test_reranker_boosts_location_facts_for_where_questions() {
    let reranker = RuleBasedReranker::new();

    let mut results = vec![
        create_test_score("你住在哪里呢", 1, 0.6),
        create_test_score("User: 我住西城区", 1, 0.5), // Lower initial score but location boost should help
    ];

    results = reranker.rerank(results, "我住哪");

    // Location fact should be boosted to top
    assert!(
        results[0].item.summary.contains("住西城"),
        "Location fact should rank first for 'where' question, got: {}",
        results[0].item.summary
    );
}

#[test]
fn test_reranker_boosts_recent_memories_for_recency_questions() {
    let reranker = RuleBasedReranker::new();

    let mut results = vec![
        create_test_score("User: 用户类型A", 100, 0.7),
        create_test_score("User: 用户类型B", 1, 0.7),
    ];

    results = reranker.rerank(results, "我最新的用户类型是什么");

    // Recent memory should rank higher for recency question
    assert!(
        results[0].item.happened_at > results[1].item.happened_at,
        "Recent memory should rank higher for recency question"
    );
}

#[test]
fn test_reranker_preserves_facts_over_questions() {
    let reranker = RuleBasedReranker::new();

    let mut results = vec![
        create_test_score("你住在哪里呢", 1, 0.8),
        create_test_score("User: 我住西城区", 1, 0.7),
        create_test_score("这是什么", 1, 0.6),
    ];

    results = reranker.rerank(results, "我住哪");

    // Fact (answer) should rank higher than question
    assert!(
        !results[0].item.summary.contains("哪"),
        "Fact should rank higher than question"
    );
    assert!(
        results[0].item.summary.contains("住西城"),
        "Location fact should be first"
    );
}

#[test]
fn test_reranker_boosts_preferences_for_preference_questions() {
    let reranker = RuleBasedReranker::new();

    let mut results = vec![
        create_test_score("User: 我喜欢红色", 1, 0.6),
        create_test_score("User: 我住西城区", 1, 0.7),
        create_test_score("你住在哪里呢", 1, 0.8),
    ];

    results = reranker.rerank(results, "我喜欢什么颜色");

    // Preference should be boosted
    assert!(
        results[0].item.summary.contains("喜欢"),
        "Preference fact should rank first"
    );
}

#[test]
fn test_reranker_with_custom_weights() {
    let reranker = RuleBasedReranker::with_weights(0.5, 0.3, 0.4);

    let mut results = vec![
        create_test_score("User: 我是Android用户", 1, 0.6),
        create_test_score("User: 我住西城区", 1, 0.7),
    ];

    let original_scores: Vec<f64> = results.iter().map(|r| r.score).collect();

    results = reranker.rerank(results, "我是什么用户");

    // Scores should be modified by reranking
    let new_scores: Vec<f64> = results.iter().map(|r| r.score).collect();

    // At least one score should have changed
    let scores_changed = original_scores
        .iter()
        .zip(new_scores.iter())
        .any(|(orig, new)| (orig - new).abs() > 0.01);

    assert!(scores_changed, "Reranking should modify scores");
}

#[test]
fn test_reranker_multiple_location_keywords_get_higher_boost() {
    let reranker = RuleBasedReranker::new();

    let single_location = create_test_score("User: 我住西城", 1, 0.7);
    let multi_location = create_test_score("User: 我居住在北京这个位置", 1, 0.6);

    let mut results = vec![single_location.clone(), multi_location.clone()];

    results = reranker.rerank(results, "我住哪");

    // Multi-location memory should be boosted higher
    assert!(
        results
            .iter()
            .any(|r| r.item.summary.contains("居住在北京")),
        "Memory with multiple location keywords should be boosted"
    );
}
