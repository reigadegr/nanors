//! Extraction engine for structured memory cards.
//!
//! The engine applies configured regex patterns to extract entity/slot/value
//! triples from text and stores them in the database.

use async_trait::async_trait;
use chrono::Utc;
use nanors_entities::memory_cards;
use nanors_entities::memory_cards::{Entity as MemoryCardEntity, Model as MemoryCardModel};
use regex::Regex;
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};
use std::str::FromStr;
use uuid::Uuid;

use crate::extraction::cards::{CardKind, MemoryCard, Polarity, VersionRelation};
use crate::extraction::patterns::{ExtractionPattern, PatternDef};
use crate::schema::SchemaRegistry;

/// Configuration for the extraction engine.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExtractionConfig {
    /// Extraction patterns to apply.
    pub patterns: Vec<PatternDef>,

    /// Minimum confidence threshold for storing cards.
    pub min_confidence: f32,

    /// Whether to extract cards when storing memories.
    pub extract_on_store: bool,
}

impl Default for ExtractionConfig {
    fn default() -> Self {
        Self {
            patterns: crate::extraction::patterns::default_patterns(),
            min_confidence: 0.3,
            extract_on_store: true,
        }
    }
}

/// Extraction engine for creating structured memory cards from text.
pub struct ExtractionEngine {
    /// Compiled extraction patterns.
    patterns: Vec<ExtractionPattern>,
    /// Configuration.
    config: ExtractionConfig,
}

impl ExtractionEngine {
    /// Create a new extraction engine from configuration.
    ///
    /// # Errors
    /// Returns an error if pattern compilation fails.
    pub fn new(config: ExtractionConfig) -> Result<Self, String> {
        let patterns = config
            .patterns
            .iter()
            .map(super::patterns::PatternDef::build)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Failed to compile patterns: {e}"))?;

        Ok(Self { patterns, config })
    }

    /// Create an extraction engine with default patterns.
    ///
    /// # Errors
    /// Returns an error if default pattern compilation fails.
    pub fn with_defaults() -> Result<Self, String> {
        Self::new(ExtractionConfig::default())
    }

    /// Extract cards from text for a given user scope.
    #[must_use]
    pub fn extract(&self, text: &str, user_scope: &str) -> Vec<MemoryCard> {
        let mut cards = Vec::new();

        for pattern in &self.patterns {
            if let Some(card) = Self::apply_pattern(pattern, text, user_scope) {
                // Filter by confidence if applicable
                if let Some(conf) = card.confidence {
                    if conf < self.config.min_confidence {
                        continue;
                    }
                }
                cards.push(card);
            }
        }

        cards
    }

    /// Extract cards from a memory item summary.
    #[must_use]
    pub fn extract_from_summary(
        &self,
        summary: &str,
        user_scope: &str,
        source_memory_id: Uuid,
    ) -> Vec<MemoryCard> {
        let mut cards = self.extract(summary, user_scope);

        // Add source memory id to all cards
        for card in &mut cards {
            card.source_memory_id = Some(source_memory_id);
        }

        cards
    }

    /// Apply a single pattern to text.
    fn apply_pattern(
        pattern: &ExtractionPattern,
        text: &str,
        user_scope: &str,
    ) -> Option<MemoryCard> {
        let re = Regex::new(&pattern.pattern).ok()?;

        let caps = re.captures(text)?;

        // Build the card
        let mut card = MemoryCard::new(
            user_scope.to_string(),
            pattern.kind,
            Self::expand_template(&pattern.entity, &caps),
            Self::expand_template(&pattern.slot, &caps),
            Self::expand_template(&pattern.value, &caps),
        );

        // Set polarity if present
        if let Some(polarity) = pattern.polarity {
            card.polarity = Some(polarity);
        }

        // Set confidence based on pattern specificity
        let confidence = Self::calculate_confidence(pattern, text);
        card.confidence = Some(confidence);

        Some(card)
    }

    /// Expand a template string with capture groups.
    fn expand_template(template: &str, caps: &regex::Captures) -> String {
        let mut result = template.to_string();

        // Expand $1, $2, etc. up to $9
        for i in 1..=9 {
            let placeholder = format!("${i}");
            if let Some(matched) = caps.get(i) {
                result = result.replace(&placeholder, matched.as_str());
            }
        }

        result.trim().to_string()
    }

    /// Calculate confidence score for a pattern match.
    fn calculate_confidence(pattern: &ExtractionPattern, text: &str) -> f32 {
        let mut confidence = 0.5_f32;

        // Boost confidence for longer matches (more specific)
        if let Ok(re) = Regex::new(&pattern.pattern) {
            if let Some(m) = re.find(text) {
                confidence += (m.len() as f32 / text.len() as f32) * 0.3;
            }
        }

        // Boost for entity/slot specificity
        if !pattern.entity.contains('$') {
            confidence += 0.1;
        }
        if !pattern.slot.contains('$') {
            confidence += 0.1;
        }

        confidence.min(1.0)
    }
}

/// Repository for storing and retrieving memory cards.
#[async_trait]
pub trait CardRepository: Send + Sync {
    /// Insert a new memory card.
    async fn insert(&self, card: &MemoryCard) -> anyhow::Result<Uuid>;

    /// Find a card by user scope, entity, and slot.
    async fn find_by_entity_slot(
        &self,
        user_scope: &str,
        entity: &str,
        slot: &str,
    ) -> anyhow::Result<Option<MemoryCard>>;

    /// Find all cards for a user scope.
    async fn find_by_scope(&self, user_scope: &str) -> anyhow::Result<Vec<MemoryCard>>;

    /// Find cards by source memory id.
    async fn find_by_source_memory(
        &self,
        source_memory_id: &Uuid,
    ) -> anyhow::Result<Vec<MemoryCard>>;

    /// Update or insert a card (upsert by `version_key`).
    async fn upsert_card(&self, card: &MemoryCard) -> anyhow::Result<Uuid>;

    /// Find the current value for an entity/slot (latest by `updated_at`).
    async fn get_current_value(
        &self,
        user_scope: &str,
        entity: &str,
        slot: &str,
    ) -> anyhow::Result<Option<String>>;
}

/// Database implementation of card repository.
pub struct DatabaseCardRepository {
    db: DatabaseConnection,
    schema_registry: SchemaRegistry,
}

impl DatabaseCardRepository {
    /// Create a new database-backed card repository.
    #[must_use]
    pub fn new(db: DatabaseConnection) -> Self {
        Self {
            db,
            schema_registry: SchemaRegistry::new(),
        }
    }

    /// Get a reference to the schema registry.
    #[must_use]
    pub const fn schema_registry(&self) -> &SchemaRegistry {
        &self.schema_registry
    }
}

#[async_trait]
impl CardRepository for DatabaseCardRepository {
    async fn insert(&self, card: &MemoryCard) -> anyhow::Result<Uuid> {
        // Validate card against schema before storing
        self.schema_registry
            .validate_card(card)
            .map_err(|e| anyhow::anyhow!("Schema validation failed: {e}"))?;

        let model = memory_cards::ActiveModel {
            id: Set(card.id),
            user_scope: Set(card.user_scope.clone()),
            kind: Set(card.kind.as_str().to_string()),
            entity: Set(card.entity.clone()),
            slot: Set(card.slot.clone()),
            value: Set(card.value.clone()),
            polarity: Set(card.polarity.map(|p| p.as_str().to_string())),
            event_date: Set(card.event_date.map(std::convert::Into::into)),
            document_date: Set(card.document_date.map(std::convert::Into::into)),
            version_key: Set(card.version_key.clone()),
            version_relation: Set(card.version_relation.as_str().to_string()),
            source_memory_id: Set(card.source_memory_id),
            engine: Set(card.engine.clone()),
            engine_version: Set(card.engine_version.clone()),
            confidence: Set(card.confidence.map(f64::from)),
            extra: Set(None),
            created_at: Set(card.created_at.into()),
            updated_at: Set(card.created_at.into()),
        };

        let result = model.insert(&self.db).await?;
        Ok(result.id)
    }

    async fn find_by_entity_slot(
        &self,
        user_scope: &str,
        entity: &str,
        slot: &str,
    ) -> anyhow::Result<Option<MemoryCard>> {
        let result = MemoryCardEntity::find()
            .filter(memory_cards::Column::UserScope.eq(user_scope))
            .filter(memory_cards::Column::Entity.eq(entity))
            .filter(memory_cards::Column::Slot.eq(slot))
            .one(&self.db)
            .await?;

        match result {
            Some(model) => Ok(Some(Self::model_to_card(model)?)),
            None => Ok(None),
        }
    }

    async fn find_by_scope(&self, user_scope: &str) -> anyhow::Result<Vec<MemoryCard>> {
        let results = MemoryCardEntity::find()
            .filter(memory_cards::Column::UserScope.eq(user_scope))
            .all(&self.db)
            .await?;

        // Convert models to cards, handling any parsing errors
        let mut cards = Vec::with_capacity(results.len());
        for model in results {
            cards.push(Self::model_to_card(model)?);
        }
        Ok(cards)
    }

    async fn find_by_source_memory(
        &self,
        source_memory_id: &Uuid,
    ) -> anyhow::Result<Vec<MemoryCard>> {
        let results = MemoryCardEntity::find()
            .filter(memory_cards::Column::SourceMemoryId.eq(*source_memory_id))
            .all(&self.db)
            .await?;

        // Convert models to cards, handling any parsing errors
        let mut cards = Vec::with_capacity(results.len());
        for model in results {
            cards.push(Self::model_to_card(model)?);
        }
        Ok(cards)
    }

    async fn upsert_card(&self, card: &MemoryCard) -> anyhow::Result<Uuid> {
        // Validate card against schema before storing/updating
        self.schema_registry
            .validate_card(card)
            .map_err(|e| anyhow::anyhow!("Schema validation failed: {e}"))?;

        // Check for existing card with same version_key - get the database model directly
        let existing_model = MemoryCardEntity::find()
            .filter(memory_cards::Column::UserScope.eq(&card.user_scope))
            .filter(memory_cards::Column::Entity.eq(&card.entity))
            .filter(memory_cards::Column::Slot.eq(&card.slot))
            .one(&self.db)
            .await?;

        if let Some(existing) = existing_model {
            // Update existing - construct ActiveModel from the database model
            let mut active = memory_cards::ActiveModel::from(existing);
            active.value = Set(card.value.clone());
            active.updated_at = Set(Utc::now().into());
            if let Some(polarity) = card.polarity {
                active.polarity = Set(Some(polarity.as_str().to_string()));
            }
            active.version_relation = Set(card.version_relation.as_str().to_string());

            let updated = active.update(&self.db).await?;
            Ok(updated.id)
        } else {
            // Insert new
            self.insert(card).await
        }
    }

    async fn get_current_value(
        &self,
        user_scope: &str,
        entity: &str,
        slot: &str,
    ) -> anyhow::Result<Option<String>> {
        let result = MemoryCardEntity::find()
            .filter(memory_cards::Column::UserScope.eq(user_scope))
            .filter(memory_cards::Column::Entity.eq(entity))
            .filter(memory_cards::Column::Slot.eq(slot))
            .one(&self.db)
            .await?;

        Ok(result.map(|m| m.value))
    }
}

impl DatabaseCardRepository {
    fn model_to_card(model: MemoryCardModel) -> anyhow::Result<MemoryCard> {
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

    #[test]
    #[expect(clippy::expect_used, reason = "Test failure should panic with context")]
    fn test_extraction_engine_new() {
        let engine = ExtractionEngine::with_defaults().expect("default engine should build");
        assert!(!engine.patterns.is_empty());
    }

    #[test]
    #[expect(clippy::expect_used, reason = "Test failure should panic with context")]
    fn test_extract_user_identity() {
        let engine = ExtractionEngine::with_defaults().expect("default engine should build");
        let cards = engine.extract("我是安卓玩机用户", "test_user");

        assert!(!cards.is_empty());
        let user_type = cards.iter().find(|c| c.slot == "user_type");
        assert!(user_type.is_some());
        assert!(
            user_type
                .expect("user_type card should exist")
                .value
                .contains("用户")
        );
    }

    #[test]
    #[expect(clippy::expect_used, reason = "Test failure should panic with context")]
    fn test_extract_location() {
        let engine = ExtractionEngine::with_defaults().expect("default engine should build");
        let cards = engine.extract("我住在湖南长沙", "test_user");

        let location = cards.iter().find(|c| c.slot == "location");
        assert!(location.is_some());
        assert!(
            location
                .expect("location card should exist")
                .value
                .contains("长沙")
        );
    }

    #[test]
    #[expect(clippy::expect_used, reason = "Test failure should panic with context")]
    fn test_extract_preference() {
        let engine = ExtractionEngine::with_defaults().expect("default engine should build");
        let cards = engine.extract("我喜欢吃辣的食物", "test_user");

        let pref = cards.iter().find(|c| c.slot == "preference");
        assert!(pref.is_some());
        assert_eq!(
            pref.expect("preference card should exist").polarity,
            Some(Polarity::Positive)
        );
    }

    #[test]
    #[expect(clippy::expect_used, reason = "Test failure should panic with context")]
    fn test_extract_from_summary() {
        let engine = ExtractionEngine::with_defaults().expect("default engine should build");
        let source_id = Uuid::now_v7();
        let cards = engine.extract_from_summary("我是开发者，住在北京", "test_user", source_id);

        assert!(!cards.is_empty());
        // All cards should have source_memory_id set
        assert!(cards.iter().all(|c| c.source_memory_id == Some(source_id)));
    }

    #[test]
    #[expect(clippy::expect_used, reason = "Test failure should panic with context")]
    fn test_expand_template() {
        let re = Regex::new(r"我是(.{1,5})用户").expect("valid regex should compile");
        let caps = re.captures("我是安卓用户").expect("regex should match");

        let result = ExtractionEngine::expand_template("身份：$1", &caps);
        assert_eq!(result, "身份：安卓");
    }

    #[test]
    fn test_config_default() {
        let config = ExtractionConfig::default();
        assert!(config.extract_on_store);
        assert!(!config.patterns.is_empty());
    }

    #[test]
    #[expect(clippy::expect_used, reason = "Test failure should panic with context")]
    #[expect(
        clippy::float_cmp,
        reason = "Testing exact equality of serialized config values"
    )]
    fn test_config_serialization() {
        let config = ExtractionConfig::default();

        // Test serialization
        let json = serde_json::to_string(&config).expect("config should serialize");
        let deserialized: ExtractionConfig =
            serde_json::from_str(&json).expect("valid JSON should deserialize");

        assert_eq!(deserialized.min_confidence, config.min_confidence);
        assert_eq!(deserialized.extract_on_store, config.extract_on_store);
    }
}
