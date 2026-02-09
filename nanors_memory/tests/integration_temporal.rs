//! Integration tests for temporal (time travel) queries.
//!
//! These tests verify that:
//! - `get_at_time()` queries return correct historical values
//! - Timeline shows value changes in chronological order
//! - Effective timestamp priority works correctly

use nanors_memory::{
    CardKind, CardRepository, DatabaseCardRepository, DatabaseCardRepositoryTemporal, MemoryCard,
    TimeTravelQuery, VersionRelation,
};

#[tokio::test]
async fn test_get_at_time_returns_most_recent() {
    let database_url = "postgres://reigadegr:1234@localhost/nanors";
    let db = sea_orm::Database::connect(database_url)
        .await
        .expect("Failed to connect to database");

    let user_scope = "test_get_at_time";

    // Create cards with different timestamps
    let now = chrono::Utc::now();
    let repo = DatabaseCardRepository::new(db.clone());
    let temporal_repo = DatabaseCardRepositoryTemporal::new(db);

    // First card: location = "Beijing" 3 days ago
    let card1 = MemoryCard::new(
        user_scope.to_string(),
        CardKind::Fact,
        "user".to_string(),
        "location".to_string(),
        "Beijing".to_string(),
    )
    .with_event_date(now - chrono::Duration::days(10));

    repo.insert(&card1).await.expect("Failed to insert card1");

    // Second card: location = "Shanghai" 1 day ago
    let card2 = MemoryCard::new(
        user_scope.to_string(),
        CardKind::Fact,
        "user".to_string(),
        "location".to_string(),
        "Shanghai".to_string(),
    )
    .with_event_date(now - chrono::Duration::days(5));

    repo.insert(&card2).await.expect("Failed to insert card2");

    // Query for value 7 days ago - should return Beijing
    let result_old = TimeTravelQuery::get_at_time(
        &temporal_repo,
        user_scope,
        "user",
        "location",
        now - chrono::Duration::days(7),
    )
    .await;

    assert!(result_old.is_some());
    assert!(result_old.unwrap().value.contains("Beijing"));

    // Query for current value - should return Shanghai (most recent)
    let result_current =
        TimeTravelQuery::get_current(&temporal_repo, user_scope, "user", "location").await;

    assert!(result_current.is_some());
    assert!(result_current.unwrap().value.contains("Shanghai"));
}

#[tokio::test]
async fn test_get_timeline_chronological_order() {
    let database_url = "postgres://reigadegr:1234@localhost/nanors";
    let db = sea_orm::Database::connect(database_url)
        .await
        .expect("Failed to connect to database");

    // Use unique scope with timestamp
    let user_scope = format!("test_get_timeline_{}", chrono::Utc::now().timestamp());
    let now = chrono::Utc::now();

    let repo = DatabaseCardRepository::new(db.clone());
    let temporal_repo = DatabaseCardRepositoryTemporal::new(db);

    // Create cards in random order
    let card2 = MemoryCard::new(
        user_scope.clone(),
        CardKind::Fact,
        "user".to_string(),
        "location".to_string(),
        "Shanghai".to_string(),
    )
    .with_event_date(now - chrono::Duration::days(5));

    let card1 = MemoryCard::new(
        user_scope.clone(),
        CardKind::Fact,
        "user".to_string(),
        "location".to_string(),
        "Beijing".to_string(),
    )
    .with_event_date(now - chrono::Duration::days(10));

    let card3 = MemoryCard::new(
        user_scope.clone(),
        CardKind::Fact,
        "user".to_string(),
        "location".to_string(),
        "Shenzhen".to_string(),
    )
    .with_event_date(now - chrono::Duration::days(2));

    repo.insert(&card1).await.expect("Failed to insert card1");
    repo.insert(&card2).await.expect("Failed to insert card2");
    repo.insert(&card3).await.expect("Failed to insert card3");

    // Get timeline - should be in chronological order
    let timeline =
        TimeTravelQuery::get_timeline(&temporal_repo, &user_scope, "user", "location").await;

    assert_eq!(timeline.len(), 3);
    assert!(timeline[0].value.contains("Beijing"));
    assert!(timeline[1].value.contains("Shanghai"));
    assert!(timeline[2].value.contains("Shenzhen"));
}

#[tokio::test]
async fn test_retracted_cards_excluded() {
    let database_url = "postgres://reigadegr:1234@localhost/nanors";
    let db = sea_orm::Database::connect(database_url)
        .await
        .expect("Failed to connect to database");

    let user_scope = "test_retracted";
    let now = chrono::Utc::now();

    let repo = DatabaseCardRepository::new(db.clone());
    let temporal_repo = DatabaseCardRepositoryTemporal::new(db);

    // Original card
    let card1 = MemoryCard::new(
        user_scope.to_string(),
        CardKind::Fact,
        "user".to_string(),
        "location".to_string(),
        "Beijing".to_string(),
    )
    .with_event_date(now - chrono::Duration::days(10))
    .with_version_relation(VersionRelation::Sets);

    // Retracted card
    let card2 = MemoryCard::new(
        user_scope.to_string(),
        CardKind::Fact,
        "user".to_string(),
        "location".to_string(),
        String::new(), // Empty value indicates retraction
    )
    .with_event_date(now - chrono::Duration::days(5))
    .with_version_relation(VersionRelation::Retracts);

    // New value after retraction
    let card3 = MemoryCard::new(
        user_scope.to_string(),
        CardKind::Fact,
        "user".to_string(),
        "location".to_string(),
        "Shanghai".to_string(),
    )
    .with_event_date(now - chrono::Duration::days(2))
    .with_version_relation(VersionRelation::Sets);

    repo.insert(&card1).await.expect("Failed to insert card1");
    repo.insert(&card2).await.expect("Failed to insert card2");
    repo.insert(&card3).await.expect("Failed to insert card3");

    // Query for value 7 days ago - should skip retracted card and return Beijing
    let result_old = TimeTravelQuery::get_at_time(
        &temporal_repo,
        user_scope,
        "user",
        "location",
        now - chrono::Duration::days(7),
    )
    .await;

    assert!(result_old.is_some());
    assert!(result_old.unwrap().value.contains("Beijing"));

    // Query for current value - should return Shanghai (after retraction)
    let result_current =
        TimeTravelQuery::get_current(&temporal_repo, user_scope, "user", "location").await;

    assert!(result_current.is_some());
    assert!(result_current.unwrap().value.contains("Shanghai"));
}

#[tokio::test]
async fn test_effective_timestamp_priority() {
    let database_url = "postgres://reigadegr:1234@localhost/nanors";
    let db = sea_orm::Database::connect(database_url)
        .await
        .expect("Failed to connect to database");

    let user_scope = "test_timestamp_priority";
    let now = chrono::Utc::now();

    let repo = DatabaseCardRepository::new(db.clone());
    let temporal_repo = DatabaseCardRepositoryTemporal::new(db);

    // Card with event_date in the past
    let card1 = MemoryCard::new(
        user_scope.to_string(),
        CardKind::Fact,
        "user".to_string(),
        "location".to_string(),
        "EventValue".to_string(),
    )
    .with_event_date(now - chrono::Duration::days(10))
    .with_document_date(now - chrono::Duration::days(5)); // document_date is more recent

    // Insert the card into the database
    repo.insert(&card1).await.expect("Failed to insert card");

    // Query at the document_date time - should use event_date for filtering
    let query_time = now - chrono::Duration::days(7);
    let result =
        TimeTravelQuery::get_at_time(&temporal_repo, user_scope, "user", "location", query_time)
            .await;

    // The card should be visible because its event_date (10 days ago) is before query_time (7 days ago)
    // even though its document_date (5 days ago) is after query_time
    assert!(result.is_some());
    assert!(result.unwrap().value.contains("EventValue"));
}

#[tokio::test]
async fn test_empty_timeline() {
    let database_url = "postgres://reigadegr:1234@localhost/nanors";
    let db = sea_orm::Database::connect(database_url)
        .await
        .expect("Failed to connect to database");

    let temporal_repo = DatabaseCardRepositoryTemporal::new(db);

    // Query non-existent entity:slot
    let timeline =
        TimeTravelQuery::get_timeline(&temporal_repo, "nonexistent_user", "user", "location").await;

    assert!(timeline.is_empty());
}

#[tokio::test]
async fn test_version_relation_updates() {
    let database_url = "postgres://reigadegr:1234@localhost/nanors";
    let db = sea_orm::Database::connect(database_url)
        .await
        .expect("Failed to connect to database");

    // Use unique scope with timestamp
    let user_scope = format!("test_version_updates_{}", chrono::Utc::now().timestamp());
    let now = chrono::Utc::now();

    let repo = DatabaseCardRepository::new(db.clone());
    let temporal_repo = DatabaseCardRepositoryTemporal::new(db);

    // Original value
    let card1 = MemoryCard::new(
        user_scope.clone(),
        CardKind::Fact,
        "user".to_string(),
        "location".to_string(),
        "Beijing".to_string(),
    )
    .with_event_date(now - chrono::Duration::days(10))
    .with_version_relation(VersionRelation::Sets);

    // Updated value
    let card2 = MemoryCard::new(
        user_scope.clone(),
        CardKind::Fact,
        "user".to_string(),
        "location".to_string(),
        "Shanghai".to_string(),
    )
    .with_event_date(now - chrono::Duration::days(5))
    .with_version_relation(VersionRelation::Updates);

    repo.insert(&card1).await.expect("Failed to insert card1");
    repo.insert(&card2).await.expect("Failed to insert card2");

    let timeline =
        TimeTravelQuery::get_timeline(&temporal_repo, &user_scope, "user", "location").await;

    assert_eq!(timeline.len(), 2);
    assert_eq!(timeline[0].version_relation, VersionRelation::Sets);
    assert_eq!(timeline[1].version_relation, VersionRelation::Updates);
}

#[tokio::test]
async fn test_current_vs_specific_time() {
    let database_url = "postgres://reigadegr:1234@localhost/nanors";
    let db = sea_orm::Database::connect(database_url)
        .await
        .expect("Failed to connect to database");

    let user_scope = "test_current_vs_time";
    let now = chrono::Utc::now();

    let repo = DatabaseCardRepository::new(db.clone());
    let temporal_repo = DatabaseCardRepositoryTemporal::new(db);

    // Create a card with a past event_date
    let card = MemoryCard::new(
        user_scope.to_string(),
        CardKind::Fact,
        "user".to_string(),
        "location".to_string(),
        "Beijing".to_string(),
    )
    .with_event_date(now - chrono::Duration::days(10));

    repo.insert(&card).await.expect("Failed to insert card");

    // get_current should find the card
    let current =
        TimeTravelQuery::get_current(&temporal_repo, user_scope, "user", "location").await;
    assert!(current.is_some());
    assert!(current.unwrap().value.contains("Beijing"));

    // Query for a time before the card existed
    let past = TimeTravelQuery::get_at_time(
        &temporal_repo,
        user_scope,
        "user",
        "location",
        now - chrono::Duration::days(20),
    )
    .await;

    assert!(past.is_none());
}
