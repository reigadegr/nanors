//! Temporal (time travel) queries for memory cards.
//!
//! This module provides functionality to query memory state at specific
//! points in time, enabling "time travel" to see what values were effective
//! at any moment in the past.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use nanors_entities::memory_cards;
use nanors_entities::memory_cards::{Entity as MemoryCardEntity, Model as MemoryCardModel};
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
use uuid::Uuid;

use crate::extraction::cards::{MemoryCard, VersionRelation};

/// Time travel query for memory cards.
///
/// Based on m12-lifecycle: understanding resource lifecycle and state changes over time.
pub struct TimeTravelQuery;

impl TimeTravelQuery {
    /// Get the value of an entity:slot at a specific point in time.
    ///
    /// # Algorithm
    /// 1. Get all cards for entity:slot
    /// 2. Filter by `effective_timestamp` <= `query_time`
    /// 3. Sort by `effective_timestamp` DESC (most recent first)
    /// 4. Find first non-retracted card
    ///
    /// # Arguments
    /// * `repo` - Card repository to query
    /// * `user_scope` - User namespace
    /// * `entity` - Entity name (e.g., "user")
    /// * `slot` - Slot name (e.g., "location")
    /// * `query_time` - Point in time to query
    ///
    /// # Returns
    /// The most recent non-retracted card at the given time, or None if no card existed.
    pub async fn get_at_time(
        repo: &dyn CardRepositoryTemporal,
        user_scope: &str,
        entity: &str,
        slot: &str,
        query_time: DateTime<Utc>,
    ) -> Option<MemoryCard> {
        // Get all cards for this entity:slot
        let mut cards = repo
            .find_by_entity_slot_all(user_scope, entity, slot)
            .await
            .ok()?;

        // Filter: only cards that existed at query_time
        cards.retain(|card| Self::effective_timestamp(card) <= query_time);

        // Sort by effective_timestamp DESC (most recent first)
        cards.sort_by(|a, b| {
            let a_time = Self::effective_timestamp(a);
            let b_time = Self::effective_timestamp(b);
            b_time.cmp(&a_time)
        });

        // Find first non-retracted card
        cards
            .into_iter()
            .find(|card| card.version_relation != VersionRelation::Retracts)
    }

    /// Get the timeline of values for an entity:slot.
    ///
    /// Returns all cards in chronological order, showing how the value
    /// changed over time.
    ///
    /// # Arguments
    /// * `repo` - Card repository to query
    /// * `user_scope` - User namespace
    /// * `entity` - Entity name (e.g., "user")
    /// * `slot` - Slot name (e.g., "location")
    ///
    /// # Returns
    /// Timeline entries in chronological order.
    pub async fn get_timeline(
        repo: &dyn CardRepositoryTemporal,
        user_scope: &str,
        entity: &str,
        slot: &str,
    ) -> Vec<TimelineEntry> {
        let Ok(cards) = repo.find_by_entity_slot_all(user_scope, entity, slot).await else {
            return Vec::new();
        };

        let mut entries: Vec<_> = cards
            .into_iter()
            .map(|card| TimelineEntry {
                timestamp: Self::effective_timestamp(&card),
                value: card.value.clone(),
                version_relation: card.version_relation,
                created_at: card.created_at,
                card_id: card.id,
            })
            .collect();

        entries.sort_by_key(|e| e.timestamp);
        entries
    }

    /// Get the effective timestamp for a card.
    ///
    /// Priority: `event_date` > `document_date` > `created_at`
    ///
    /// This is used to determine when a card "became effective"
    /// for time travel queries.
    fn effective_timestamp(card: &MemoryCard) -> DateTime<Utc> {
        card.event_date
            .or(card.document_date)
            .unwrap_or(card.created_at)
    }

    /// Get the current (most recent) value for an entity:slot.
    ///
    /// This is equivalent to `get_at_time` with `Utc::now()`.
    ///
    /// # Arguments
    /// * `repo` - Card repository to query
    /// * `user_scope` - User namespace
    /// * `entity` - Entity name
    /// * `slot` - Slot name
    ///
    /// # Returns
    /// The current value, or None if no card exists.
    pub async fn get_current(
        repo: &dyn CardRepositoryTemporal,
        user_scope: &str,
        entity: &str,
        slot: &str,
    ) -> Option<MemoryCard> {
        Self::get_at_time(repo, user_scope, entity, slot, Utc::now()).await
    }
}

/// Timeline entry showing a value change.
#[derive(Debug, Clone)]
pub struct TimelineEntry {
    pub timestamp: DateTime<Utc>,
    pub value: String,
    pub version_relation: VersionRelation,
    pub created_at: DateTime<Utc>,
    pub card_id: Uuid,
}

/// Extension trait for temporal queries on card repositories.
#[async_trait]
pub trait CardRepositoryTemporal: Send + Sync {
    /// Get all cards for entity:slot (including history).
    ///
    /// Unlike `find_by_entity_slot` which only returns the current card,
    /// this returns all cards including historical versions.
    async fn find_by_entity_slot_all(
        &self,
        user_scope: &str,
        entity: &str,
        slot: &str,
    ) -> anyhow::Result<Vec<MemoryCard>>;
}

/// Database implementation of temporal card queries.
pub struct DatabaseCardRepositoryTemporal {
    db: DatabaseConnection,
}

impl DatabaseCardRepositoryTemporal {
    /// Create a new temporal repository.
    #[must_use]
    pub const fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

#[async_trait]
impl CardRepositoryTemporal for DatabaseCardRepositoryTemporal {
    async fn find_by_entity_slot_all(
        &self,
        user_scope: &str,
        entity: &str,
        slot: &str,
    ) -> anyhow::Result<Vec<MemoryCard>> {
        let results = MemoryCardEntity::find()
            .filter(memory_cards::Column::UserScope.eq(user_scope))
            .filter(memory_cards::Column::Entity.eq(entity))
            .filter(memory_cards::Column::Slot.eq(slot))
            .all(&self.db)
            .await?;

        // Convert models to cards, handling any parsing errors
        let mut cards = Vec::with_capacity(results.len());
        for model in results {
            cards.push(Self::model_to_card(model)?);
        }
        Ok(cards)
    }
}

impl DatabaseCardRepositoryTemporal {
    fn model_to_card(model: MemoryCardModel) -> anyhow::Result<MemoryCard> {
        use crate::extraction::cards::{CardKind, MemoryCard, Polarity, VersionRelation};
        use std::str::FromStr;

        Ok(MemoryCard {
            id: model.id,
            user_scope: model.user_scope,
            kind: CardKind::from_str(&model.kind)
                .map_err(|_| anyhow::anyhow!("invalid card kind: {}", model.kind))?,
            entity: model.entity,
            slot: model.slot,
            value: model.value,
            polarity: model
                .polarity
                .as_ref()
                .and_then(|p| Polarity::from_str(p).ok()),
            event_date: model.event_date.map(std::convert::Into::into),
            document_date: model.document_date.map(std::convert::Into::into),
            version_key: model.version_key,
            version_relation: VersionRelation::from_str(&model.version_relation).map_err(|_| {
                anyhow::anyhow!("invalid version relation: {}", model.version_relation)
            })?,
            source_memory_id: model.source_memory_id,
            engine: model.engine,
            engine_version: model.engine_version,
            confidence: model.confidence.map(|f| f as f32),
            created_at: model.created_at.into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extraction::cards::{CardKind, MemoryCard, VersionRelation};

    fn make_card(
        user_scope: &str,
        entity: &str,
        slot: &str,
        value: &str,
        created_at: DateTime<Utc>,
    ) -> MemoryCard {
        MemoryCard::new(
            user_scope.to_string(),
            CardKind::Fact,
            entity.to_string(),
            slot.to_string(),
            value.to_string(),
        )
        .with_created_at(created_at)
    }

    #[test]
    fn test_effective_timestamp_priority() {
        let now = Utc::now();

        // Card with event_date
        let card1 = make_card("user", "user", "location", "A", now)
            .with_event_date(now - chrono::Duration::days(10));
        assert_eq!(
            TimeTravelQuery::effective_timestamp(&card1),
            now - chrono::Duration::days(10)
        );

        // Card with document_date (no event_date)
        let card2 = make_card("user", "user", "location", "B", now)
            .with_document_date(now - chrono::Duration::days(5));
        assert_eq!(
            TimeTravelQuery::effective_timestamp(&card2),
            now - chrono::Duration::days(5)
        );

        // Card with only created_at
        let card3 = make_card("user", "user", "location", "C", now);
        assert_eq!(TimeTravelQuery::effective_timestamp(&card3), now);
    }

    #[test]
    fn test_effective_timestamp_event_priority() {
        let now = Utc::now();

        // event_date takes priority over document_date
        let card = make_card("user", "user", "location", "A", now)
            .with_event_date(now - chrono::Duration::days(10))
            .with_document_date(now - chrono::Duration::days(5));

        assert_eq!(
            TimeTravelQuery::effective_timestamp(&card),
            now - chrono::Duration::days(10)
        );
    }

    #[tokio::test]
    async fn test_get_current() {
        struct MockRepo {
            cards: Vec<MemoryCard>,
        }

        #[async_trait]
        impl CardRepositoryTemporal for MockRepo {
            async fn find_by_entity_slot_all(
                &self,
                _user_scope: &str,
                _entity: &str,
                _slot: &str,
            ) -> anyhow::Result<Vec<MemoryCard>> {
                Ok(self.cards.clone())
            }
        }

        let now = Utc::now();
        let cards = vec![
            make_card("user", "user", "location", "Beijing", now),
            make_card("user", "user", "location", "Shanghai", now),
        ];

        let repo = MockRepo { cards };
        let result = TimeTravelQuery::get_current(&repo, "user", "user", "location").await;

        assert!(result.is_some());
        // Should return the first card when timestamps are equal
        #[expect(clippy::expect_used, reason = "Test failure should panic with context")]
        {
            assert_eq!(result.expect("result should exist").value, "Beijing");
        }
    }

    #[test]
    fn test_timeline_entry_structure() {
        let now = Utc::now();
        let card = make_card("user", "user", "location", "Beijing", now);

        let entry = TimelineEntry {
            timestamp: TimeTravelQuery::effective_timestamp(&card),
            value: card.value.clone(),
            version_relation: card.version_relation,
            created_at: card.created_at,
            card_id: card.id,
        };

        assert_eq!(entry.value, "Beijing");
        assert_eq!(entry.version_relation, VersionRelation::Sets);
    }
}
