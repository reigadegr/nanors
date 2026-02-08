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
    clippy::missing_errors_doc
)]

//! Graph repository traits for memory card operations.

use async_trait::async_trait;
use uuid::Uuid;

use super::graph::{
    GraphMatchResult, GraphPattern, MemoryCard, MemoryKind, Polarity, VersionRelation,
};

/// Repository for memory card operations.
#[async_trait]
pub trait MemoryCardRepo: Send + Sync {
    /// Insert a new memory card.
    async fn insert_card(&self, card: &MemoryCard) -> anyhow::Result<()>;

    /// Get the current value for an entity's slot.
    async fn get_current(
        &self,
        user_scope: &str,
        entity: &str,
        slot: &str,
    ) -> anyhow::Result<Option<MemoryCard>>;

    /// Get all cards for an entity.
    async fn get_entity_cards(
        &self,
        user_scope: &str,
        entity: &str,
    ) -> anyhow::Result<Vec<MemoryCard>>;

    /// Query cards matching a graph pattern.
    async fn query_pattern(
        &self,
        user_scope: &str,
        pattern: &GraphPattern,
    ) -> anyhow::Result<Vec<GraphMatchResult>>;

    /// Find entities with a specific slot value.
    async fn find_by_slot_value(
        &self,
        user_scope: &str,
        slot: &str,
        value: &str,
    ) -> anyhow::Result<Vec<String>>;

    /// Update a card (creates new version).
    async fn update_card(&self, card: &MemoryCard) -> anyhow::Result<()>;

    /// List all version keys for a user scope.
    async fn list_version_keys(&self, user_scope: &str) -> anyhow::Result<Vec<String>>;
}

/// Builder for constructing `MemoryCards`.
#[derive(Debug, Default)]
pub struct MemoryCardBuilder {
    user_scope: Option<String>,
    memory_item_id: Option<Uuid>,
    kind: Option<MemoryKind>,
    entity: Option<String>,
    slot: Option<String>,
    value: Option<String>,
    polarity: Option<Polarity>,
    version_key: Option<String>,
    version_relation: VersionRelation,
    confidence: Option<f32>,
}

impl MemoryCardBuilder {
    /// Create a new builder.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            user_scope: None,
            memory_item_id: None,
            kind: None,
            entity: None,
            slot: None,
            value: None,
            polarity: None,
            version_key: None,
            version_relation: VersionRelation::Sets,
            confidence: None,
        }
    }

    /// Set the user scope.
    #[must_use]
    pub fn user_scope(mut self, scope: impl Into<String>) -> Self {
        self.user_scope = Some(scope.into());
        self
    }

    /// Set the memory item ID.
    #[must_use]
    pub const fn memory_item_id(mut self, id: Uuid) -> Self {
        self.memory_item_id = Some(id);
        self
    }

    /// Set the memory kind.
    #[must_use]
    pub const fn kind(mut self, kind: MemoryKind) -> Self {
        self.kind = Some(kind);
        self
    }

    /// Set the entity.
    #[must_use]
    pub fn entity(mut self, entity: impl Into<String>) -> Self {
        self.entity = Some(entity.into());
        self
    }

    /// Set the slot.
    #[must_use]
    pub fn slot(mut self, slot: impl Into<String>) -> Self {
        self.slot = Some(slot.into());
        self
    }

    /// Set the value.
    #[must_use]
    pub fn value(mut self, value: impl Into<String>) -> Self {
        self.value = Some(value.into());
        self
    }

    /// Set the polarity.
    #[must_use]
    pub const fn polarity(mut self, polarity: Polarity) -> Self {
        self.polarity = Some(polarity);
        self
    }

    /// Set the version key explicitly.
    #[must_use]
    pub fn version_key(mut self, key: impl Into<String>) -> Self {
        self.version_key = Some(key.into());
        self
    }

    /// Set version relation to Updates.
    #[must_use]
    pub const fn updates(mut self) -> Self {
        self.version_relation = VersionRelation::Updates;
        self
    }

    /// Set version relation to Extends.
    #[must_use]
    pub const fn extends(mut self) -> Self {
        self.version_relation = VersionRelation::Extends;
        self
    }

    /// Set version relation to Retracts.
    #[must_use]
    pub const fn retracts(mut self) -> Self {
        self.version_relation = VersionRelation::Retracts;
        self
    }

    /// Set the confidence score.
    #[must_use]
    pub const fn confidence(mut self, conf: f32) -> Self {
        self.confidence = Some(conf.clamp(0.0, 1.0));
        self
    }

    /// Build the `MemoryCard`.
    ///
    /// # Errors
    /// Returns an error if required fields are missing.
    pub fn build(self, id: Uuid) -> anyhow::Result<MemoryCard> {
        let user_scope = self
            .user_scope
            .ok_or_else(|| anyhow::anyhow!("missing user_scope"))?;
        let kind = self.kind.unwrap_or_default();
        let entity = self
            .entity
            .ok_or_else(|| anyhow::anyhow!("missing entity"))?;
        let slot = self.slot.ok_or_else(|| anyhow::anyhow!("missing slot"))?;
        let value = self.value.ok_or_else(|| anyhow::anyhow!("missing value"))?;

        let now = chrono::Utc::now();

        Ok(MemoryCard {
            id,
            user_scope,
            memory_item_id: self.memory_item_id,
            kind,
            entity: entity.to_lowercase(),
            slot: slot.to_lowercase(),
            value,
            polarity: self.polarity,
            version_key: self.version_key,
            version_relation: self.version_relation,
            confidence: self.confidence,
            created_at: now,
            updated_at: now,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_card_builder() {
        let id = Uuid::now_v7();
        let card = MemoryCardBuilder::new()
            .user_scope("test_user")
            .entity("Alice")
            .slot("employer")
            .value("Anthropic")
            .build(id)
            .unwrap();

        assert_eq!(card.user_scope, "test_user");
        assert_eq!(card.entity, "alice");
        assert_eq!(card.slot, "employer");
        assert_eq!(card.value, "Anthropic");
    }

    #[test]
    fn test_memory_card_builder_missing_fields() {
        let id = Uuid::now_v7();
        let result = MemoryCardBuilder::new()
            .user_scope("test_user")
            .entity("Alice")
            // missing slot and value
            .build(id);

        assert!(result.is_err());
    }
}
