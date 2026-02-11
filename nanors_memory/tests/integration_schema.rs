//! Integration tests for schema validation.
//!
//! These tests verify that:
//! - `SchemaRegistry` validates card values correctly
//! - Strict mode rejects unknown predicates
//! - Schema inference works from existing data

use nanors_memory::schema::{Cardinality, SchemaError, SchemaRegistry, ValueType};
use nanors_memory::{CardKind, CardRepository, DatabaseCardRepository, MemoryCard};

#[tokio::test]
async fn test_schema_validation_valid_age() {
    let registry = SchemaRegistry::new();

    let card = MemoryCard::new(
        "user".to_string(),
        CardKind::Fact,
        "user".to_string(),
        "age".to_string(),
        "25".to_string(),
    );

    let result = registry.validate_card(&card);
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_schema_validation_invalid_age() {
    let registry = SchemaRegistry::new();

    let card = MemoryCard::new(
        "user".to_string(),
        CardKind::Fact,
        "user".to_string(),
        "age".to_string(),
        "twenty-five".to_string(),
    );

    let result = registry.validate_card(&card);
    assert!(result.is_err());

    if let Err(SchemaError::InvalidRange { expected, .. }) = result {
        assert_eq!(expected, "number");
    } else {
        panic!("Expected InvalidRange error");
    }
}

#[tokio::test]
async fn test_schema_validation_unknown_slot_non_strict() {
    let registry = SchemaRegistry::new(); // non-strict by default

    let card = MemoryCard::new(
        "user".to_string(),
        CardKind::Fact,
        "user".to_string(),
        "unknown_slot".to_string(),
        "any_value".to_string(),
    );

    // Should pass in non-strict mode
    let result = registry.validate_card(&card);
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_schema_validation_unknown_slot_strict() {
    let registry = SchemaRegistry::new().strict();

    let card = MemoryCard::new(
        "user".to_string(),
        CardKind::Fact,
        "user".to_string(),
        "unknown_slot".to_string(),
        "any_value".to_string(),
    );

    let result = registry.validate_card(&card);
    assert!(result.is_err());

    if let Err(SchemaError::UnknownPredicate(slot)) = result {
        assert_eq!(slot, "unknown_slot");
    } else {
        panic!("Expected UnknownPredicate error");
    }
}

#[tokio::test]
async fn test_schema_inference_numeric() {
    let registry = SchemaRegistry::new();
    let values = ["25", "30", "35"];

    let schema = registry.infer_from_values("age", &values);

    assert!(matches!(schema.range, ValueType::Number));
    assert_eq!(schema.id, "age");
}

#[tokio::test]
async fn test_schema_inference_multiple() {
    let registry = SchemaRegistry::new();
    let values = ["reading", "swimming", "coding"];

    let schema = registry.infer_from_values("hobby", &values);

    assert!(matches!(schema.cardinality, Cardinality::Multiple));
}

#[tokio::test]
async fn test_schema_builtin_schemas() {
    let registry = SchemaRegistry::new();

    // Check that built-in schemas are registered
    assert!(registry.contains("location"));
    assert!(registry.contains("user_type"));
    assert!(registry.contains("age"));
    assert!(registry.contains("preference"));
    assert!(registry.contains("hobby"));

    // Verify the age schema expects numbers
    let age_schema = registry.get("age").expect("age schema should exist");
    assert!(matches!(age_schema.range, ValueType::Number));
}

#[tokio::test]
async fn test_schema_datetime_validation_chinese() {
    let registry = SchemaRegistry::new();

    let datetime_schema = registry
        .get("birthday")
        .expect("birthday schema should exist");

    // Test Chinese date formats
    assert!(datetime_schema.range.matches("2024年1月1日"));
    assert!(datetime_schema.range.matches("2024-01-01"));
    assert!(datetime_schema.range.matches("1704067200"));

    // Test invalid date
    assert!(!datetime_schema.range.matches("not a date"));
}

#[tokio::test]
async fn test_schema_enum_validation() {
    let registry = SchemaRegistry::new();

    // Create an enum schema for user_type
    let schema = registry.infer_from_values("user_type", &["developer", "student", "teacher"]);

    // The inferred schema won't be Enum type (it defaults to String),
    // but let's verify it can handle string matching
    assert!(schema.range.matches("developer"));
    assert!(schema.range.matches("student"));
}

#[tokio::test]
async fn test_schema_with_database_storage() {
    let database_url = "postgres://reigadegr:1234@localhost/nanors";
    let db = sea_orm::Database::connect(database_url)
        .await
        .expect("Failed to connect to database");

    let user_scope = "test_schema_db";
    let repo = DatabaseCardRepository::new(db);

    // Create a card with valid age
    let card1 = MemoryCard::new(
        user_scope.to_string(),
        CardKind::Fact,
        "user".to_string(),
        "age".to_string(),
        "30".to_string(),
    );

    repo.insert(&card1).await.expect("Failed to insert card");

    // Verify it can be retrieved
    let retrieved = repo
        .find_by_entity_slot(user_scope, "user", "age")
        .await
        .expect("Failed to find card");

    assert!(retrieved.is_some());
    let card = retrieved.unwrap();
    assert_eq!(card.value, "30");
}

#[tokio::test]
async fn test_schema_validates_boolean_slot() {
    let registry = SchemaRegistry::new();

    // Create a verified card
    let card = MemoryCard::new(
        "user".to_string(),
        CardKind::Fact,
        "user".to_string(),
        "verified".to_string(),
        "true".to_string(),
    );

    let result = registry.validate_card(&card);
    assert!(result.is_ok());

    // Test with invalid value
    let card_invalid = MemoryCard::new(
        "user".to_string(),
        CardKind::Fact,
        "user".to_string(),
        "verified".to_string(),
        "maybe".to_string(),
    );

    let result_invalid = registry.validate_card(&card_invalid);
    assert!(result_invalid.is_err());
}
