//! Enrichment manifest for tracking incremental processing.
//!
//! Based on m02-resource (Arc<`RwLock`<T>> for shared state) and
//! m07-concurrency (thread-safe read/write access patterns).

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use nanors_entities::enrichment_records;
use nanors_entities::enrichment_records::Entity as EnrichmentEntity;
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use uuid::Uuid;

/// Type alias for the enrichment cache.
type EnrichmentCache = Arc<RwLock<HashMap<String, HashMap<Uuid, Vec<EngineStamp>>>>>;

/// Error type for enrichment operations.
#[derive(Debug)]
pub enum EnrichmentError {
    /// Lock was poisoned (another thread panicked while holding the lock).
    LockPoisoned,
}

impl std::fmt::Display for EnrichmentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LockPoisoned => write!(f, "enrichment cache lock is poisoned"),
        }
    }
}

impl std::error::Error for EnrichmentError {}

/// Parameters for recording enrichment.
#[derive(Debug, Clone)]
pub struct EnrichmentParams {
    /// User namespace
    pub user_scope: String,
    /// Memory item ID
    pub memory_id: Uuid,
    /// Engine type identifier
    pub engine_kind: String,
    /// Engine version
    pub engine_version: String,
    /// Cards produced from enrichment
    pub card_ids: Vec<Uuid>,
    /// Whether enrichment succeeded
    pub success: bool,
    /// Error message if failed
    pub error_message: Option<String>,
}

/// Engine stamp tracking which engine processed which memory.
///
/// This represents a single enrichment run on a memory item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineStamp {
    /// Engine type identifier (e.g., "rules", "llm:qwen").
    pub engine_kind: String,
    /// Engine version (e.g., "1.0.0").
    pub engine_version: String,
    /// When enrichment occurred.
    pub enriched_at: DateTime<Utc>,
    /// Memory card IDs produced from this enrichment.
    pub card_ids: Vec<Uuid>,
    /// Whether the enrichment succeeded.
    pub success: bool,
    /// Error message if enrichment failed.
    pub error_message: Option<String>,
}

/// Enrichment record for a single memory item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrichmentRecord {
    /// The memory that was enriched.
    pub memory_id: Uuid,
    /// Stamps from all engines that processed this memory.
    pub stamps: Vec<EngineStamp>,
}

/// Thread-safe enrichment manifest.
///
/// Uses Arc<`RwLock`<T>> pattern (m02-resource) for:
/// - Multiple readers (check if enriched)
/// - Single writer (record enrichment)
///
/// The in-memory cache is backed by the database for persistence.
#[derive(Debug, Clone)]
pub struct EnrichmentManifest {
    /// Database connection for persistence.
    db: Option<DatabaseConnection>,
    /// In-memory cache of enrichment records (`user_scope` -> `memory_id` -> stamps).
    /// Uses Arc<`RwLock`<T>> for thread-safe access (m07-concurrency).
    cache: EnrichmentCache,
}

impl EnrichmentManifest {
    /// Create a new enrichment manifest.
    #[must_use]
    pub fn new() -> Self {
        Self {
            db: None,
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create an enrichment manifest with database backing.
    #[must_use]
    pub fn with_database(db: DatabaseConnection) -> Self {
        Self {
            db: Some(db),
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Check if a memory needs enrichment by a specific engine.
    ///
    /// This is the "read" path - uses `RwLock` read lock.
    ///
    /// Note: Failed enrichments (success: false) still count as "attempted",
    /// so the memory won't be re-enriched with the same engine version. This
    /// prevents infinite retry loops on consistently failing patterns.
    ///
    /// # Arguments
    /// * `user_scope` - User namespace
    /// * `memory_id` - Memory item ID
    /// * `engine_kind` - Engine type identifier
    /// * `engine_version` - Engine version
    ///
    /// # Returns
    /// `true` if the memory has not been processed by this engine version.
    /// Returns `true` on lock error (conservative approach - assume enrichment needed).
    #[must_use]
    pub fn needs_enrichment(
        &self,
        user_scope: &str,
        memory_id: Uuid,
        engine_kind: &str,
        engine_version: &str,
    ) -> bool {
        self.cache.read().map_or(true, |cache| {
            cache.get(user_scope).is_none_or(|memories| {
                memories.get(&memory_id).is_none_or(|stamps| {
                    !stamps.iter().any(|s| {
                        s.engine_kind == engine_kind && s.engine_version == engine_version
                        // Note: We don't check s.success here - failed enrichments still count
                    })
                })
            })
        })
        // Lock poisoned - conservatively return true
    }

    /// Record that enrichment was performed.
    ///
    /// This is the "write" path - uses `RwLock` write lock.
    ///
    /// # Arguments
    /// * `params` - Enrichment parameters containing all necessary information
    ///
    /// # Returns
    /// `Ok(())` on success, `Err` on database or lock error.
    pub async fn record_enrichment(&self, params: EnrichmentParams) -> anyhow::Result<()> {
        let stamp = EngineStamp {
            engine_kind: params.engine_kind.clone(),
            engine_version: params.engine_version.clone(),
            enriched_at: Utc::now(),
            card_ids: params.card_ids.clone(),
            success: params.success,
            error_message: params.error_message,
        };

        // Update in-memory cache
        {
            let mut cache = self
                .cache
                .write()
                .map_err(|_| EnrichmentError::LockPoisoned)?;
            cache
                .entry(params.user_scope.clone())
                .or_default()
                .entry(params.memory_id)
                .or_default()
                .push(stamp.clone());
        }

        // Persist to database if available
        if let Some(db) = &self.db {
            let model = enrichment_records::ActiveModel {
                id: Set(Uuid::now_v7()),
                user_scope: Set(params.user_scope.clone()),
                memory_id: Set(params.memory_id),
                engine_kind: Set(params.engine_kind),
                engine_version: Set(params.engine_version),
                enriched_at: Set(stamp.enriched_at.into()),
                card_ids: Set(Some(params.card_ids)),
                success: Set(stamp.success),
                error_message: Set(stamp.error_message),
                extra: Set(None),
                created_at: Set(Utc::now().into()),
            };

            model.insert(db).await?;
        }

        Ok(())
    }

    /// Get unenriched memory IDs for batch processing.
    ///
    /// # Arguments
    /// * `user_scope` - User namespace
    /// * `all_memory_ids` - All memory IDs to check
    /// * `engine_kind` - Engine type identifier
    /// * `engine_version` - Engine version
    ///
    /// # Returns
    /// Memory IDs that need enrichment.
    #[must_use]
    pub fn get_unenriched_memories(
        &self,
        user_scope: &str,
        all_memory_ids: &[Uuid],
        engine_kind: &str,
        engine_version: &str,
    ) -> Vec<Uuid> {
        all_memory_ids
            .iter()
            .filter(|id| self.needs_enrichment(user_scope, **id, engine_kind, engine_version))
            .copied()
            .collect()
    }

    /// Get all enrichment stamps for a memory.
    ///
    /// # Arguments
    /// * `user_scope` - User namespace
    /// * `memory_id` - Memory item ID
    ///
    /// # Returns
    /// All stamps for this memory, or None if none exist or lock is poisoned.
    #[must_use]
    pub fn get_stamps(&self, user_scope: &str, memory_id: Uuid) -> Option<Vec<EngineStamp>> {
        self.cache
            .read()
            .ok()
            .and_then(|cache| cache.get(user_scope)?.get(&memory_id).cloned())
    }

    /// Clear all cached enrichment records.
    ///
    /// This is useful for testing or when the database state changes externally.
    ///
    /// # Returns
    /// `Ok(())` on success, `Err` if lock is poisoned.
    pub fn clear_cache(&self) -> Result<(), EnrichmentError> {
        self.cache
            .write()
            .map(|mut cache| cache.clear())
            .map_err(|_| EnrichmentError::LockPoisoned)
    }

    /// Load enrichment records from the database into the cache.
    ///
    /// # Arguments
    /// * `user_scope` - User namespace to load records for
    ///
    /// # Returns
    /// Number of records loaded, or error on database or lock failure.
    pub async fn load_from_database(&self, user_scope: &str) -> anyhow::Result<usize> {
        let Some(db) = &self.db else {
            return Ok(0);
        };

        let records = EnrichmentEntity::find()
            .filter(enrichment_records::Column::UserScope.eq(user_scope))
            .all(db)
            .await?;

        let count = records.len();

        // Group stamps by memory_id outside the lock
        let mut stamps_by_memory: std::collections::HashMap<Uuid, Vec<EngineStamp>> =
            std::collections::HashMap::new();
        for record in records {
            stamps_by_memory
                .entry(record.memory_id)
                .or_default()
                .push(EngineStamp {
                    engine_kind: record.engine_kind,
                    engine_version: record.engine_version,
                    enriched_at: record.enriched_at.into(),
                    card_ids: record.card_ids.unwrap_or_default(),
                    success: record.success,
                    error_message: record.error_message,
                });
        }

        // Insert all stamps for each memory_id in one operation
        for (memory_id, stamps) in stamps_by_memory {
            let mut cache = self
                .cache
                .write()
                .map_err(|_| EnrichmentError::LockPoisoned)?;
            cache
                .entry(user_scope.to_string())
                .or_default()
                .entry(memory_id)
                .or_default()
                .extend(stamps);
        }

        Ok(count)
    }
}

impl Default for EnrichmentManifest {
    fn default() -> Self {
        Self::new()
    }
}

/// Repository for enrichment records.
#[async_trait]
pub trait EnrichmentRepository: Send + Sync {
    /// Record enrichment for a memory.
    async fn record_enrichment(&self, params: EnrichmentParams) -> anyhow::Result<()>;

    /// Check if a memory needs enrichment.
    async fn needs_enrichment(
        &self,
        user_scope: &str,
        memory_id: Uuid,
        engine_kind: &str,
        engine_version: &str,
    ) -> bool;

    /// Get enrichment stamps for a memory.
    async fn get_stamps(
        &self,
        user_scope: &str,
        memory_id: Uuid,
    ) -> anyhow::Result<Option<Vec<EngineStamp>>>;

    /// Get all unenriched memory IDs.
    async fn get_unenriched_memories(
        &self,
        user_scope: &str,
        all_memory_ids: &[Uuid],
        engine_kind: &str,
        engine_version: &str,
    ) -> Vec<Uuid>;
}

/// Database implementation of enrichment repository.
pub struct DatabaseEnrichmentRepository {
    db: DatabaseConnection,
    /// In-memory cache using Arc<`RwLock`<T>> pattern.
    manifest: EnrichmentManifest,
}

impl DatabaseEnrichmentRepository {
    /// Create a new database enrichment repository.
    #[must_use]
    pub fn new(db: DatabaseConnection) -> Self {
        let manifest = EnrichmentManifest::with_database(db.clone());
        Self { db, manifest }
    }

    /// Get the manifest for direct access.
    #[must_use]
    pub const fn manifest(&self) -> &EnrichmentManifest {
        &self.manifest
    }

    /// Load enrichment records from database into cache.
    pub async fn load_cache(&self, user_scope: &str) -> anyhow::Result<usize> {
        self.manifest.load_from_database(user_scope).await
    }
}

#[async_trait]
impl EnrichmentRepository for DatabaseEnrichmentRepository {
    async fn record_enrichment(&self, params: EnrichmentParams) -> anyhow::Result<()> {
        self.manifest.record_enrichment(params).await
    }

    async fn needs_enrichment(
        &self,
        user_scope: &str,
        memory_id: Uuid,
        engine_kind: &str,
        engine_version: &str,
    ) -> bool {
        self.manifest
            .needs_enrichment(user_scope, memory_id, engine_kind, engine_version)
    }

    async fn get_stamps(
        &self,
        user_scope: &str,
        memory_id: Uuid,
    ) -> anyhow::Result<Option<Vec<EngineStamp>>> {
        // Check cache first
        if let Some(stamps) = self.manifest.get_stamps(user_scope, memory_id) {
            return Ok(Some(stamps));
        }

        // Query database
        let records = EnrichmentEntity::find()
            .filter(enrichment_records::Column::UserScope.eq(user_scope))
            .filter(enrichment_records::Column::MemoryId.eq(memory_id))
            .all(&self.db)
            .await?;

        if records.is_empty() {
            return Ok(None);
        }

        let stamps: Vec<EngineStamp> = records
            .into_iter()
            .map(|r| EngineStamp {
                engine_kind: r.engine_kind,
                engine_version: r.engine_version,
                enriched_at: r.enriched_at.into(),
                card_ids: r.card_ids.unwrap_or_default(),
                success: r.success,
                error_message: r.error_message,
            })
            .collect();

        Ok(Some(stamps))
    }

    async fn get_unenriched_memories(
        &self,
        user_scope: &str,
        all_memory_ids: &[Uuid],
        engine_kind: &str,
        engine_version: &str,
    ) -> Vec<Uuid> {
        self.manifest.get_unenriched_memories(
            user_scope,
            all_memory_ids,
            engine_kind,
            engine_version,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_needs_enrichment_new_memory() {
        let manifest = EnrichmentManifest::new();
        let memory_id = Uuid::now_v7();

        // New memory should need enrichment
        assert!(manifest.needs_enrichment("test_user", memory_id, "rules", "1.0.0"));
    }

    #[test]
    fn test_needs_enrichment_after_record() {
        let manifest = EnrichmentManifest::new();
        let memory_id = Uuid::now_v7();

        // Record enrichment (synchronously for test)
        {
            #[expect(clippy::expect_used, reason = "Test failure should panic with context")]
            let mut cache = manifest
                .cache
                .write()
                .expect("enrichment cache lock should not be poisoned");
            cache
                .entry("test_user".to_string())
                .or_default()
                .entry(memory_id)
                .or_default()
                .push(EngineStamp {
                    engine_kind: "rules".to_string(),
                    engine_version: "1.0.0".to_string(),
                    enriched_at: Utc::now(),
                    card_ids: vec![],
                    success: true,
                    error_message: None,
                });
        }

        // Should not need enrichment anymore
        assert!(!manifest.needs_enrichment("test_user", memory_id, "rules", "1.0.0"));

        // But different version should need enrichment
        assert!(manifest.needs_enrichment("test_user", memory_id, "rules", "2.0.0"));
    }

    #[test]
    fn test_get_unenriched_memories() {
        let manifest = EnrichmentManifest::new();
        let memory1 = Uuid::now_v7();
        let memory2 = Uuid::now_v7();
        let memory3 = Uuid::now_v7();

        // Record enrichment for memory1 only
        {
            #[expect(clippy::expect_used, reason = "Test failure should panic with context")]
            let mut cache = manifest
                .cache
                .write()
                .expect("enrichment cache lock should not be poisoned");
            cache
                .entry("test_user".to_string())
                .or_default()
                .entry(memory1)
                .or_default()
                .push(EngineStamp {
                    engine_kind: "rules".to_string(),
                    engine_version: "1.0.0".to_string(),
                    enriched_at: Utc::now(),
                    card_ids: vec![],
                    success: true,
                    error_message: None,
                });
        }

        let all_memories = vec![memory1, memory2, memory3];
        let unenriched =
            manifest.get_unenriched_memories("test_user", &all_memories, "rules", "1.0.0");

        // Should only return memory2 and memory3
        assert_eq!(unenriched.len(), 2);
        assert!(!unenriched.contains(&memory1));
        assert!(unenriched.contains(&memory2));
        assert!(unenriched.contains(&memory3));
    }

    #[test]
    fn test_get_stamps() {
        let manifest = EnrichmentManifest::new();
        let memory_id = Uuid::now_v7();

        // No stamps initially
        assert!(manifest.get_stamps("test_user", memory_id).is_none());

        // Add a stamp
        {
            #[expect(clippy::expect_used, reason = "Test failure should panic with context")]
            let mut cache = manifest
                .cache
                .write()
                .expect("enrichment cache lock should not be poisoned");
            cache
                .entry("test_user".to_string())
                .or_default()
                .entry(memory_id)
                .or_default()
                .push(EngineStamp {
                    engine_kind: "rules".to_string(),
                    engine_version: "1.0.0".to_string(),
                    enriched_at: Utc::now(),
                    card_ids: vec![Uuid::now_v7()],
                    success: true,
                    error_message: None,
                });
        }

        // Should return the stamp
        let stamps = manifest.get_stamps("test_user", memory_id);
        assert!(stamps.is_some());
        #[expect(clippy::expect_used, reason = "Test failure should panic with context")]
        {
            let stamps = stamps.expect("stamps should exist");
            assert_eq!(stamps.len(), 1);
        }
    }

    #[test]
    fn test_clear_cache() {
        let manifest = EnrichmentManifest::new();
        let memory_id = Uuid::now_v7();

        // Add data
        {
            #[expect(clippy::expect_used, reason = "Test failure should panic with context")]
            let mut cache = manifest
                .cache
                .write()
                .expect("enrichment cache lock should not be poisoned");
            cache
                .entry("test_user".to_string())
                .or_default()
                .entry(memory_id)
                .or_default()
                .push(EngineStamp {
                    engine_kind: "rules".to_string(),
                    engine_version: "1.0.0".to_string(),
                    enriched_at: Utc::now(),
                    card_ids: vec![],
                    success: true,
                    error_message: None,
                });
        }

        // Verify data exists
        assert!(!manifest.needs_enrichment("test_user", memory_id, "rules", "1.0.0"));

        // Clear cache
        #[expect(clippy::expect_used, reason = "Test failure should panic with context")]
        {
            manifest
                .clear_cache()
                .expect("clear_cache should succeed in test");
        }

        // Data should be gone
        assert!(manifest.needs_enrichment("test_user", memory_id, "rules", "1.0.0"));
    }

    #[test]
    fn test_failed_enrichment_still_counts() {
        let manifest = EnrichmentManifest::new();
        let memory_id = Uuid::now_v7();

        // Record FAILED enrichment
        {
            #[expect(clippy::expect_used, reason = "Test failure should panic with context")]
            let mut cache = manifest
                .cache
                .write()
                .expect("enrichment cache lock should not be poisoned");
            cache
                .entry("test_user".to_string())
                .or_default()
                .entry(memory_id)
                .or_default()
                .push(EngineStamp {
                    engine_kind: "rules".to_string(),
                    engine_version: "1.0.0".to_string(),
                    enriched_at: Utc::now(),
                    card_ids: vec![],
                    success: false,
                    error_message: Some("Pattern match failed".to_string()),
                });
        }

        // Failed enrichment still counts as "attempted" - prevents infinite retry loops
        assert!(!manifest.needs_enrichment("test_user", memory_id, "rules", "1.0.0"));
    }

    #[test]
    fn test_multiple_engines() {
        let manifest = EnrichmentManifest::new();
        let memory_id = Uuid::now_v7();

        // Record rules engine enrichment
        {
            #[expect(clippy::expect_used, reason = "Test failure should panic with context")]
            let mut cache = manifest
                .cache
                .write()
                .expect("enrichment cache lock should not be poisoned");
            cache
                .entry("test_user".to_string())
                .or_default()
                .entry(memory_id)
                .or_default()
                .push(EngineStamp {
                    engine_kind: "rules".to_string(),
                    engine_version: "1.0.0".to_string(),
                    enriched_at: Utc::now(),
                    card_ids: vec![],
                    success: true,
                    error_message: None,
                });
        }

        // Rules engine should be enriched
        assert!(!manifest.needs_enrichment("test_user", memory_id, "rules", "1.0.0"));

        // But LLM engine should still need enrichment
        assert!(manifest.needs_enrichment("test_user", memory_id, "llm:qwen", "1.0.0"));
    }
}
