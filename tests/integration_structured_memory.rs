//! Integration tests for structured memory features.
//!
//! These tests verify the complete flow of:
//! - Question type detection
//! - Query expansion
//! - Structured memory card extraction and retrieval

use std::sync::Arc;

use nanors_memory::{
    ExtractionEngine, ExtractionConfig, QuestionTypeDetector, QueryExpander, QueryExpanderConfig,
};
use nanors_memory::{DatabaseCardRepository, MemoryCard, QuestionType};

/// Test question type detection with Chinese queries.
#[test]
fn test_chinese_question_detection() {
    let detector = QuestionTypeDetector::with_defaults();

    // WhatKind questions
    assert_eq!(detector.detect("我是什么用户"), QuestionType::WhatKind);
    assert_eq!(detector.detect("我是谁"), QuestionType::WhatKind);
    assert_eq!(detector.detect("我的身份是什么"), QuestionType::WhatKind);

    // Recency questions
    assert_eq!(detector.detect("我现在在哪"), QuestionType::Recency);
    assert_eq!(detector.detect("我的最新地址是"), QuestionType::Recency);

    // Location questions
    assert_eq!(detector.detect("我在哪"), QuestionType::Where);
    assert_eq!(detector.detect("我住在哪里"), QuestionType::Where);

    // Generic queries
    assert_eq!(detector.detect("你好"), QuestionType::Generic);
    assert_eq!(detector.detect("告诉我的情况"), QuestionType::Generic);
}

/// Test question type detection with English queries.
#[test]
fn test_english_question_detection() {
    let detector = QuestionTypeDetector::with_defaults();

    // WhatKind questions
    assert_eq!(detector.detect("what kind of user am i"), QuestionType::WhatKind);
    assert_eq!(detector.detect("who am i"), QuestionType::WhatKind);

    // Recency questions
    assert_eq!(detector.detect("what is my current location"), QuestionType::Recency);

    // Location questions
    assert_eq!(detector.detect("where am i"), QuestionType::Where);

    // Generic queries
    assert_eq!(detector.detect("hello"), QuestionType::Generic);
}

/// Test query expansion with Chinese text.
#[test]
fn test_chinese_query_expansion() {
    let expander = QueryExpander::with_defaults();

    // OR query expansion
    let or_result = expander.expand_or("我是什么用户");
    assert!(or_result.is_some());
    let expanded = or_result.unwrap();
    assert!(!expanded.contains("什么")); // Stopword removed
    assert!(expanded.contains("用户")); // Content kept

    // Stopword removal
    let filtered = expander.remove_stopwords("我是什么用户");
    assert!(filtered.is_some());
    assert_eq!(filtered.unwrap(), "用户");
}

/// Test query expansion with English text.
#[test]
fn test_english_query_expansion() {
    let expander = QueryExpander::with_defaults();

    // OR query expansion
    let or_result = expander.expand_or("what is my user type");
    assert!(or_result.is_some());
    let expanded = or_result.unwrap();
    assert!(expanded.contains("OR"));
    assert!(!expanded.contains("what"));
    assert!(!expanded.contains("is"));
}

/// Test structured memory extraction.
#[test]
fn test_memory_extraction() {
    let engine = ExtractionEngine::with_defaults().unwrap();

    // Test user identity extraction
    let cards = engine.extract("我是安卓玩机用户", "test_user");
    assert!(!cards.is_empty());

    let user_type = cards.iter().find(|c| c.slot == "user_type");
    assert!(user_type.is_some());
    let card = user_type.unwrap();
    assert_eq!(card.entity, "user");
    assert!(card.value.contains("用户"));

    // Test location extraction
    let cards = engine.extract("我住在湖南长沙", "test_user");
    let location = cards.iter().find(|c| c.slot == "location");
    assert!(location.is_some());
    assert!(location.unwrap().value.contains("长沙"));

    // Test preference extraction
    let cards = engine.extract("我喜欢吃辣的食物", "test_user");
    let pref = cards.iter().find(|c| c.slot == "preference");
    assert!(pref.is_some());
    assert_eq!(pref.unwrap().polarity, Some(nanors_memory::Polarity::Positive));
}

/// Test extraction with source memory linking.
#[test]
fn test_extraction_with_source_memory() {
    let engine = ExtractionEngine::with_defaults().unwrap();
    let source_id = uuid::Uuid::now_v7();

    let cards = engine.extract_from_summary(
        "我是开发者，住在北京",
        "test_user",
        source_id,
    );

    assert!(!cards.is_empty());
    // All cards should have the source memory ID
    assert!(cards.iter().all(|c| c.source_memory_id == Some(source_id)));
}

/// Test configuration-based extraction.
#[test]
fn test_configurable_extraction() {
    let config = ExtractionConfig {
        patterns: vec![],
        min_confidence: 0.5,
        extract_on_store: true,
    };

    let engine = ExtractionEngine::new(config).unwrap();
    let cards = engine.extract("test", "user");

    // With no patterns, should return empty
    assert!(cards.is_empty());
}

/// Test question pattern priority.
#[test]
fn test_question_pattern_priority() {
    let mut detector = QuestionTypeDetector::with_defaults();

    // Add a high-priority custom pattern
    detector.add_pattern(
        nanors_memory::QuestionPattern::new(nanors_memory::QuestionType::WhatKind, r"test-custom")
            .with_priority(200),
    );

    assert_eq!(detector.detect("test-custom"), QuestionType::WhatKind);
    // Should be first pattern (highest priority)
    assert_eq!(detector.patterns()[0].priority, 200);
}

/// Test query expander configuration.
#[test]
fn test_configurable_expander() {
    let config = QueryExpanderConfig {
        stopwords: vec!["test".to_string()],
        enabled: true,
        min_or_tokens: 3,
    };

    let expander = QueryExpander::new(config);

    // With high min_or_tokens, shouldn't expand short queries
    let result = expander.expand_or("short query");
    assert!(result.is_none());

    // Stopword should be filtered
    assert!(expander.is_stopword("test"));
}

/// Test disabled expander.
#[test]
fn test_disabled_expander() {
    let config = QueryExpanderConfig {
        stopwords: vec![],
        enabled: false,
        min_or_tokens: 2,
    };

    let expander = QueryExpander::new(config);
    let results = expander.expand("test query");

    assert!(results.is_empty());
}

/// Test extraction confidence scoring.
#[test]
fn test_extraction_confidence() {
    let engine = ExtractionEngine::with_defaults().unwrap();

    let cards = engine.extract("我是安卓玩机用户", "test_user");
    assert!(!cards.is_empty());

    // All cards should have confidence scores
    assert!(cards.iter().all(|c| c.confidence.is_some()));

    // Confidence should be between 0.0 and 1.0
    for card in &cards {
        if let Some(conf) = card.confidence {
            assert!((0.0..=1.0).contains(&conf));
        }
    }
}

/// Test memory card builder methods.
#[test]
fn test_memory_card_builder() {
    let card = MemoryCard::new(
        "test_user".to_string(),
        nanors_memory::CardKind::Fact,
        "user".to_string(),
        "location".to_string(),
        "北京".to_string(),
    )
    .with_polarity(nanors_memory::Polarity::Neutral)
    .with_confidence(0.9)
    .with_source_memory(uuid::Uuid::now_v7())
    .with_version_relation(nanors_memory::VersionRelation::Sets);

    assert_eq!(card.entity, "user");
    assert_eq!(card.slot, "location");
    assert_eq!(card.value, "北京");
    assert_eq!(card.polarity, Some(nanors_memory::Polarity::Neutral));
    assert_eq!(card.confidence, Some(0.9));
    assert!(card.source_memory_id.is_some());
    assert_eq!(card.version_relation, nanors_memory::VersionRelation::Sets);
}

/// Test memory card version key generation.
#[test]
fn test_memory_card_version_key() {
    let card = MemoryCard::new(
        "test_user".to_string(),
        nanors_memory::CardKind::Fact,
        "user".to_string(),
        "user_type".to_string(),
        "developer".to_string(),
    );

    assert_eq!(card.default_version_key(), "user:user_type");
    assert_eq!(card.version_key, Some("user:user_type".to_string()));
}

/// Test memory card matching.
#[test]
fn test_memory_card_matching() {
    let card = MemoryCard::new(
        "test_user".to_string(),
        nanors_memory::CardKind::Fact,
        "user".to_string(),
        "location".to_string(),
        "北京".to_string(),
    );

    assert!(card.matches("user", "location"));
    assert!(!card.matches("user", "user_type"));
    assert!(!card.matches("phone", "location"));
}

/// Test card kind conversion.
#[test]
fn test_card_kind_conversion() {
    assert_eq!(nanors_memory::CardKind::Fact.as_str(), "fact");
    assert_eq!(nanors_memory::CardKind::Preference.as_str(), "preference");
    assert_eq!(nanors_memory::CardKind::Event.as_str(), "event");

    assert_eq!(
        nanors_memory::CardKind::from_str("fact"),
        nanors_memory::CardKind::Fact
    );
    assert_eq!(
        nanors_memory::CardKind::from_str("unknown"),
        nanors_memory::CardKind::Fact // Default fallback
    );
}

/// Test polarity conversion.
#[test]
fn test_polarity_conversion() {
    assert_eq!(nanors_memory::Polarity::Positive.as_str(), "positive");
    assert_eq!(nanors_memory::Polarity::Negative.as_str(), "negative");
    assert_eq!(nanors_memory::Polarity::Neutral.as_str(), "neutral");

    assert_eq!(
        nanors_memory::Polarity::from_str("positive"),
        Some(nanors_memory::Polarity::Positive)
    );
    assert_eq!(nanors_memory::Polarity::from_str("unknown"), None);
}

/// Test version relation conversion.
#[test]
fn test_version_relation_conversion() {
    assert_eq!(nanors_memory::VersionRelation::Sets.as_str(), "sets");
    assert_eq!(nanors_memory::VersionRelation::Updates.as_str(), "updates");

    assert_eq!(
        nanors_memory::VersionRelation::from_str("sets"),
        nanors_memory::VersionRelation::Sets
    );
}

/// Test question type conversion.
#[test]
fn test_question_type_conversion() {
    assert_eq!(QuestionType::WhatKind.as_str(), "what_kind");
    assert_eq!(QuestionType::Recency.as_str(), "recency");

    assert_eq!(QuestionType::from_str("what_kind"), QuestionType::WhatKind);
    assert_eq!(QuestionType::from_str("unknown"), QuestionType::Generic);
}

/// Test expansion type conversion.
#[test]
fn test_expansion_type_conversion() {
    assert_eq!(nanors_memory::ExpansionType::OrQuery.as_str(), "or_query");
    assert_eq!(nanors_memory::ExpansionType::Stopwords.as_str(), "stopwords");
}
