//! Integration tests for enrichment tracking.
//!
//! These tests verify that:
//! - Enrichment records are persisted to the database
//! - Incremental processing avoids duplicate work
//! - Cache loading from database works correctly

use nanors_core::MemoryType;
use nanors_entities::memory_items;
use nanors_memory::{CardRepository, DatabaseCardRepository, EnrichmentManifest, ExtractionEngine};
use sea_orm::{ActiveModelTrait, Set};
use uuid::Uuid;

/// Helper to create a test memory item in the database.
async fn create_test_memory(
    db: &sea_orm::DatabaseConnection,
    user_scope: &str,
    summary: &str,
) -> anyhow::Result<Uuid> {
    let memory_id = Uuid::now_v7();
    let model = memory_items::ActiveModel {
        id: Set(memory_id),
        user_scope: Set(user_scope.to_string()),
        memory_type: Set(MemoryType::Semantic.to_string()),
        summary: Set(summary.to_string()),
        embedding: Set(None),
        happened_at: Set(chrono::Utc::now().into()),
        extra: Set(None),
        content_hash: Set(format!("hash_{summary}")),
        reinforcement_count: Set(0),
        created_at: Set(chrono::Utc::now().into()),
        updated_at: Set(chrono::Utc::now().into()),
    };

    model.insert(db).await?;
    Ok(memory_id)
}

#[tokio::test]
async fn test_enrichment_manifest_persistence() {
    let database_url = "postgres://reigadegr:1234@localhost/nanors";
    let db = sea_orm::Database::connect(database_url)
        .await
        .expect("Failed to connect to database");

    // Create manifest with database backing
    let manifest = EnrichmentManifest::with_database(db.clone());
    let user_scope = "test_enrichment_user";

    // Create a memory item first (for foreign key constraint)
    let memory_id = create_test_memory(&db, user_scope, "Test memory for enrichment")
        .await
        .expect("Failed to create memory");

    // Initially, memory needs enrichment
    assert!(manifest.needs_enrichment(user_scope, memory_id, "rules", "1.0.0"));

    // Record enrichment
    let params = nanors_memory::EnrichmentParams {
        user_scope: user_scope.to_string(),
        memory_id,
        engine_kind: "rules".to_string(),
        engine_version: "1.0.0".to_string(),
        card_ids: vec![],
        success: true,
        error_message: None,
    };
    manifest
        .record_enrichment(params)
        .await
        .expect("Failed to record enrichment");

    // Now it should not need enrichment
    assert!(!manifest.needs_enrichment(user_scope, memory_id, "rules", "1.0.0"));

    // But different version still needs enrichment
    assert!(manifest.needs_enrichment(user_scope, memory_id, "rules", "2.0.0"));
}

#[tokio::test]
async fn test_enrichment_cache_loading() {
    let database_url = "postgres://reigadegr:1234@localhost/nanors";
    let db = sea_orm::Database::connect(database_url)
        .await
        .expect("Failed to connect to database");

    // First manifest - write enrichment records
    let manifest1 = EnrichmentManifest::with_database(db.clone());
    let user_scope = "test_cache_loading";
    let memory_id = create_test_memory(&db, user_scope, "Test memory for cache loading")
        .await
        .expect("Failed to create memory");

    let params = nanors_memory::EnrichmentParams {
        user_scope: user_scope.to_string(),
        memory_id,
        engine_kind: "llm:qwen".to_string(),
        engine_version: "1.0.0".to_string(),
        card_ids: vec![Uuid::now_v7()],
        success: true,
        error_message: None,
    };
    manifest1
        .record_enrichment(params)
        .await
        .expect("Failed to record enrichment");

    // Second manifest - should load from database
    let manifest2 = EnrichmentManifest::with_database(db.clone());

    // Initially not in cache
    assert!(manifest2.get_stamps(user_scope, memory_id).is_none());

    // Load from database
    let loaded = manifest2
        .load_from_database(user_scope)
        .await
        .expect("Failed to load from database");

    assert!(loaded >= 1);

    // Now should be in cache
    let stamps = manifest2.get_stamps(user_scope, memory_id);
    assert!(stamps.is_some());
    #[expect(clippy::expect_used, reason = "Test failure should panic with context")]
    {
        assert_eq!(stamps.expect("stamps should exist").len(), 1);
    }
}

#[tokio::test]
async fn test_get_unenriched_memories_integration() {
    let database_url = "postgres://reigadegr:1234@localhost/nanors";
    let db = sea_orm::Database::connect(database_url)
        .await
        .expect("Failed to connect to database");

    let manifest = EnrichmentManifest::with_database(db.clone());
    let user_scope = "test_unenriched";

    let memory1 = create_test_memory(&db, user_scope, "Memory 1")
        .await
        .expect("Failed to create memory");
    let memory2 = create_test_memory(&db, user_scope, "Memory 2")
        .await
        .expect("Failed to create memory");
    let memory3 = create_test_memory(&db, user_scope, "Memory 3")
        .await
        .expect("Failed to create memory");

    // Only enrich memory1
    let params = nanors_memory::EnrichmentParams {
        user_scope: user_scope.to_string(),
        memory_id: memory1,
        engine_kind: "rules".to_string(),
        engine_version: "1.0.0".to_string(),
        card_ids: vec![],
        success: true,
        error_message: None,
    };
    manifest
        .record_enrichment(params)
        .await
        .expect("Failed to record enrichment");

    let all_memories = vec![memory1, memory2, memory3];
    let unenriched = manifest.get_unenriched_memories(user_scope, &all_memories, "rules", "1.0.0");

    // Should only return memory2 and memory3
    assert_eq!(unenriched.len(), 2);
    assert!(!unenriched.contains(&memory1));
    assert!(unenriched.contains(&memory2));
    assert!(unenriched.contains(&memory3));
}

#[tokio::test]
async fn test_extraction_with_enrichment_tracking() {
    let database_url = "postgres://reigadegr:1234@localhost/nanors";
    let db = sea_orm::Database::connect(database_url)
        .await
        .expect("Failed to connect to database");

    let manifest = EnrichmentManifest::with_database(db.clone());
    let card_repo = DatabaseCardRepository::new(db.clone());
    let engine = ExtractionEngine::with_defaults().expect("Failed to create engine");

    let user_scope = "test_extraction_tracking";
    let memory_id = create_test_memory(&db, user_scope, "Test memory for extraction")
        .await
        .expect("Failed to create memory");

    let text = "我住在北京";

    // Check if enrichment is needed
    let needs_enrichment = manifest.needs_enrichment(user_scope, memory_id, "rules", "1.0.0");
    assert!(needs_enrichment);

    // Extract cards
    let cards = engine.extract(text, user_scope);

    // Debug: print extracted cards
    println!("Extracted {} cards:", cards.len());
    for card in &cards {
        println!(
            "  entity={}, slot={}, value={}",
            card.entity, card.slot, card.value
        );
    }

    // Store cards
    for card in &cards {
        card_repo.insert(card).await.expect("Failed to insert card");
    }

    // Record enrichment
    let card_ids: Vec<Uuid> = cards.iter().map(|c| c.id).collect();
    let params = nanors_memory::EnrichmentParams {
        user_scope: user_scope.to_string(),
        memory_id,
        engine_kind: "rules".to_string(),
        engine_version: "1.0.0".to_string(),
        card_ids,
        success: true,
        error_message: None,
    };
    manifest
        .record_enrichment(params)
        .await
        .expect("Failed to record enrichment");

    // Should not need enrichment now
    assert!(!manifest.needs_enrichment(user_scope, memory_id, "rules", "1.0.0"));

    // Verify cards can be retrieved
    let location_card = card_repo
        .find_by_entity_slot(user_scope, "user", "location")
        .await
        .expect("Failed to find location card");

    assert!(location_card.is_some());
    let card = location_card.unwrap();
    assert!(card.value.contains("北京"));
}

#[tokio::test]
async fn test_failed_enrichment_prevents_retry() {
    let database_url = "postgres://reigadegr:1234@localhost/nanors";
    let db = sea_orm::Database::connect(database_url)
        .await
        .expect("Failed to connect to database");

    let manifest = EnrichmentManifest::with_database(db.clone());
    let user_scope = "test_failed_enrichment";
    let memory_id = create_test_memory(&db, user_scope, "Test memory for failed enrichment")
        .await
        .expect("Failed to create memory");

    // Initially needs enrichment
    assert!(manifest.needs_enrichment(user_scope, memory_id, "rules", "1.0.0"));

    // Record FAILED enrichment
    let params = nanors_memory::EnrichmentParams {
        user_scope: user_scope.to_string(),
        memory_id,
        engine_kind: "rules".to_string(),
        engine_version: "1.0.0".to_string(),
        card_ids: vec![],
        success: false,
        error_message: Some("Pattern match failed".to_string()),
    };
    manifest
        .record_enrichment(params)
        .await
        .expect("Failed to record enrichment");

    // Should NOT need enrichment (failed attempt still counts)
    // This prevents infinite retry loops
    assert!(!manifest.needs_enrichment(user_scope, memory_id, "rules", "1.0.0"));
}

#[tokio::test]
async fn test_multiple_engine_tracking() {
    let database_url = "postgres://reigadegr:1234@localhost/nanors";
    let db = sea_orm::Database::connect(database_url)
        .await
        .expect("Failed to connect to database");

    let manifest = EnrichmentManifest::with_database(db.clone());
    let user_scope = "test_multiple_engines";
    let memory_id = create_test_memory(&db, user_scope, "Test memory for multiple engines")
        .await
        .expect("Failed to create memory");

    // Enrich with rules engine
    let params1 = nanors_memory::EnrichmentParams {
        user_scope: user_scope.to_string(),
        memory_id,
        engine_kind: "rules".to_string(),
        engine_version: "1.0.0".to_string(),
        card_ids: vec![Uuid::now_v7()],
        success: true,
        error_message: None,
    };
    manifest
        .record_enrichment(params1)
        .await
        .expect("Failed to record enrichment");

    // Rules engine should be enriched
    assert!(!manifest.needs_enrichment(user_scope, memory_id, "rules", "1.0.0"));

    // LLM engine still needs enrichment
    assert!(manifest.needs_enrichment(user_scope, memory_id, "llm:qwen", "1.0.0"));

    // Enrich with LLM engine
    let params2 = nanors_memory::EnrichmentParams {
        user_scope: user_scope.to_string(),
        memory_id,
        engine_kind: "llm:qwen".to_string(),
        engine_version: "1.0.0".to_string(),
        card_ids: vec![Uuid::now_v7()],
        success: true,
        error_message: None,
    };
    manifest
        .record_enrichment(params2)
        .await
        .expect("Failed to record enrichment");

    // Both should be enriched now
    assert!(!manifest.needs_enrichment(user_scope, memory_id, "rules", "1.0.0"));
    assert!(!manifest.needs_enrichment(user_scope, memory_id, "llm:qwen", "1.0.0"));

    // Verify we can get all stamps
    let stamps = manifest.get_stamps(user_scope, memory_id);
    assert!(stamps.is_some());
    #[expect(clippy::expect_used, reason = "Test failure should panic with context")]
    {
        assert_eq!(stamps.expect("stamps should exist").len(), 2);
    }
}
