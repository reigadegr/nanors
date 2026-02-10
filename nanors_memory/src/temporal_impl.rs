use crate::manager::MemoryManager;

impl MemoryManager {
    /// Get the value of an entity:slot at a specific point in time.
    ///
    /// This enables "time travel" to see what values were effective at any moment
    /// in the past.
    ///
    /// # Arguments
    /// * `user_scope` - User namespace
    /// * `entity` - Entity name (e.g., "user")
    /// * `slot` - Slot name (e.g., "location")
    /// * `query_time` - Point in time to query
    ///
    /// # Returns
    /// The most recent non-retracted card at the given time, or None if no card existed.
    pub async fn get_card_at_time(
        &self,
        user_scope: &str,
        entity: &str,
        slot: &str,
        query_time: chrono::DateTime<chrono::Utc>,
    ) -> anyhow::Result<Option<crate::extraction::MemoryCard>> {
        let temporal_repo = crate::temporal::DatabaseCardRepositoryTemporal::new(self.db.clone());
        Ok(crate::temporal::TimeTravelQuery::get_at_time(
            &temporal_repo,
            user_scope,
            entity,
            slot,
            query_time,
        )
        .await)
    }

    /// Get the timeline of values for an entity:slot.
    ///
    /// Returns all cards in chronological order, showing how the value
    /// changed over time.
    ///
    /// # Arguments
    /// * `user_scope` - User namespace
    /// * `entity` - Entity name (e.g., "user")
    /// * `slot` - Slot name (e.g., "location")
    ///
    /// # Returns
    /// Timeline entries in chronological order.
    pub async fn get_card_timeline(
        &self,
        user_scope: &str,
        entity: &str,
        slot: &str,
    ) -> anyhow::Result<Vec<crate::temporal::TimelineEntry>> {
        let temporal_repo = crate::temporal::DatabaseCardRepositoryTemporal::new(self.db.clone());
        Ok(
            crate::temporal::TimeTravelQuery::get_timeline(
                &temporal_repo,
                user_scope,
                entity,
                slot,
            )
            .await,
        )
    }

    /// Get the current (most recent) value for an entity:slot.
    ///
    /// This is equivalent to `get_card_at_time` with `Utc::now()`.
    ///
    /// # Arguments
    /// * `user_scope` - User namespace
    /// * `entity` - Entity name
    /// * `slot` - Slot name
    ///
    /// # Returns
    /// The current value, or None if no card exists.
    pub async fn get_current_card(
        &self,
        user_scope: &str,
        entity: &str,
        slot: &str,
    ) -> anyhow::Result<Option<crate::extraction::MemoryCard>> {
        let temporal_repo = crate::temporal::DatabaseCardRepositoryTemporal::new(self.db.clone());
        Ok(
            crate::temporal::TimeTravelQuery::get_current(&temporal_repo, user_scope, entity, slot)
                .await,
        )
    }
}
