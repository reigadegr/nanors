//! Memory card types for structured memory extraction and storage.
//!
//! Memory cards are atomic, structured representations of memories extracted
//! from conversation content. Unlike raw chunks which are text fragments,
//! cards are semantic units with identity, value, temporality, provenance,
//! and versioning information.
//!
//! Ported from memvid with adaptations for database storage (Uuid, `DateTime<Utc>`).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use uuid::Uuid;

/// The kind of memory being stored.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum MemoryKind {
    /// Factual information: "User works at Anthropic"
    #[default]
    Fact = 0,
    /// User preference: "User prefers dark mode"
    Preference = 1,
    /// Discrete event: "User moved to San Francisco on 2024-03-15"
    Event = 2,
    /// Background/profile information: "User is a software engineer"
    Profile = 3,
    /// Relationship between entities: "User's manager is Alice"
    Relationship = 4,
    /// Goal or intent: "User wants to learn Rust"
    Goal = 5,
    /// Other/custom kind
    Other = 6,
}

impl MemoryKind {
    /// Returns the string representation of this kind.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Fact => "fact",
            Self::Preference => "preference",
            Self::Event => "event",
            Self::Profile => "profile",
            Self::Relationship => "relationship",
            Self::Goal => "goal",
            Self::Other => "other",
        }
    }
}

impl FromStr for MemoryKind {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "fact" => Ok(Self::Fact),
            "preference" => Ok(Self::Preference),
            "event" => Ok(Self::Event),
            "profile" => Ok(Self::Profile),
            "relationship" => Ok(Self::Relationship),
            "goal" => Ok(Self::Goal),
            _ => Ok(Self::Other),
        }
    }
}

/// How this card relates to prior versions of the same slot.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum VersionRelation {
    /// First time this slot is being set.
    #[default]
    Sets = 0,
    /// Replaces a previous value entirely.
    Updates = 1,
    /// Adds to existing value (e.g., list of hobbies).
    Extends = 2,
    /// Negates/removes a previous value.
    Retracts = 3,
}

impl VersionRelation {
    /// Returns the string representation.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Sets => "sets",
            Self::Updates => "updates",
            Self::Extends => "extends",
            Self::Retracts => "retracts",
        }
    }
}

impl FromStr for VersionRelation {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "updates" => Ok(Self::Updates),
            "extends" => Ok(Self::Extends),
            "retracts" => Ok(Self::Retracts),
            _ => Ok(Self::Sets),
        }
    }
}

/// Polarity for preferences and boolean facts.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum Polarity {
    /// "likes", "prefers", "wants"
    Positive = 0,
    /// "dislikes", "avoids", "doesn't want"
    Negative = 1,
    /// Factual, no sentiment
    #[default]
    Neutral = 2,
}

impl Polarity {
    /// Returns the string representation.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Positive => "positive",
            Self::Negative => "negative",
            Self::Neutral => "neutral",
        }
    }
}

impl FromStr for Polarity {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "positive" => Ok(Self::Positive),
            "negative" => Ok(Self::Negative),
            "neutral" => Ok(Self::Neutral),
            _ => Err(anyhow::anyhow!("invalid polarity: {s}")),
        }
    }
}

impl std::fmt::Display for Polarity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::fmt::Display for MemoryKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// A structured memory unit extracted from conversation content.
///
/// Memory cards represent atomic facts, preferences, events, or other
/// information extracted from raw text. They support:
/// - **Identity**: What entity/slot this card describes
/// - **Value**: The actual information
/// - **Temporality**: When this was true (event time) vs when recorded (document time)
/// - **Provenance**: Which memory item it came from, which engine extracted it
/// - **Versioning**: How this card relates to prior knowledge
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryCard {
    /// Unique identifier.
    pub id: Uuid,

    /// User scope for isolation.
    pub user_scope: String,

    /// Associated memory item (source).
    pub memory_item_id: Option<Uuid>,

    /// What kind of memory this represents.
    pub kind: MemoryKind,

    /// The entity this memory is about (e.g., "user", "user.team", "project.memvid").
    pub entity: String,

    /// The attribute/slot being described (e.g., "employer", "`favorite_food`", "location").
    pub slot: String,

    /// The actual value (always stored as string, can be JSON for complex values).
    pub value: String,

    /// Sentiment/polarity for preferences.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub polarity: Option<Polarity>,

    /// When the event/fact occurred (not when it was recorded).
    /// For events: the event date.
    /// For facts: when this became true (if known).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_date: Option<DateTime<Utc>>,

    /// When this information was recorded (from the source document/conversation).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document_date: Option<DateTime<Utc>>,

    /// Versioning: key to group related cards (usually entity:slot).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version_key: Option<String>,

    /// How this relates to prior versions.
    #[serde(default)]
    pub version_relation: VersionRelation,

    /// URI of the source (for provenance).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_uri: Option<String>,

    /// Which engine produced this card.
    pub engine: String,

    /// Version of the engine.
    pub engine_version: String,

    /// Confidence score (0.0-1.0) if from probabilistic engine.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,

    /// When this card was created.
    pub created_at: DateTime<Utc>,

    /// When this card was last updated.
    pub updated_at: DateTime<Utc>,
}

impl MemoryCard {
    /// Generate the default version key from entity and slot.
    #[must_use]
    pub fn default_version_key(&self) -> String {
        format!("{}:{}", self.entity, self.slot)
    }

    /// Check if this card supersedes another based on version relation and timestamps.
    #[must_use]
    pub fn supersedes(&self, other: &Self) -> bool {
        // Must have same version key to supersede
        let self_key = self
            .version_key
            .as_ref()
            .map_or_else(|| self.default_version_key(), std::clone::Clone::clone);
        let other_key = other
            .version_key
            .as_ref()
            .map_or_else(|| other.default_version_key(), std::clone::Clone::clone);

        if self_key != other_key {
            return false;
        }

        match self.version_relation {
            VersionRelation::Updates | VersionRelation::Retracts => {
                // Compare by event_date if available, else document_date
                let self_time = self.event_date.or(self.document_date);
                let other_time = other.event_date.or(other.document_date);
                match (self_time, other_time) {
                    (Some(st), Some(ot)) => st > ot,
                    (Some(_), None) => true,
                    (None, Some(_) | None) => false,
                }
            }
            VersionRelation::Sets | VersionRelation::Extends => false,
        }
    }

    /// Get the effective timestamp for temporal ordering.
    #[must_use]
    pub fn effective_timestamp(&self) -> DateTime<Utc> {
        self.event_date
            .or(self.document_date)
            .unwrap_or(self.created_at)
    }

    /// Check if this card is a retraction.
    #[must_use]
    pub const fn is_retracted(&self) -> bool {
        matches!(self.version_relation, VersionRelation::Retracts)
    }
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
    event_date: Option<DateTime<Utc>>,
    document_date: Option<DateTime<Utc>>,
    version_key: Option<String>,
    version_relation: VersionRelation,
    source_uri: Option<String>,
    engine: Option<String>,
    engine_version: Option<String>,
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
            event_date: None,
            document_date: None,
            version_key: None,
            version_relation: VersionRelation::Sets,
            source_uri: None,
            engine: None,
            engine_version: None,
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

    /// Set kind to Fact.
    #[must_use]
    pub const fn fact(self) -> Self {
        self.kind(MemoryKind::Fact)
    }

    /// Set kind to Preference.
    #[must_use]
    pub const fn preference(self) -> Self {
        self.kind(MemoryKind::Preference)
    }

    /// Set kind to Event.
    #[must_use]
    pub const fn event(self) -> Self {
        self.kind(MemoryKind::Event)
    }

    /// Set kind to Profile.
    #[must_use]
    pub const fn profile(self) -> Self {
        self.kind(MemoryKind::Profile)
    }

    /// Set kind to Relationship.
    #[must_use]
    pub const fn relationship(self) -> Self {
        self.kind(MemoryKind::Relationship)
    }

    /// Set kind to Goal.
    #[must_use]
    pub const fn goal(self) -> Self {
        self.kind(MemoryKind::Goal)
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

    /// Set polarity to Positive.
    #[must_use]
    pub const fn positive(self) -> Self {
        self.polarity(Polarity::Positive)
    }

    /// Set polarity to Negative.
    #[must_use]
    pub const fn negative(self) -> Self {
        self.polarity(Polarity::Negative)
    }

    /// Set the event date.
    #[must_use]
    pub const fn event_date(mut self, ts: DateTime<Utc>) -> Self {
        self.event_date = Some(ts);
        self
    }

    /// Set the document date.
    #[must_use]
    pub const fn document_date(mut self, ts: DateTime<Utc>) -> Self {
        self.document_date = Some(ts);
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

    /// Set the source URI.
    #[must_use]
    pub fn source_uri(mut self, uri: impl Into<String>) -> Self {
        self.source_uri = Some(uri.into());
        self
    }

    /// Set the engine name and version.
    #[must_use]
    pub fn engine(mut self, name: impl Into<String>, version: impl Into<String>) -> Self {
        self.engine = Some(name.into());
        self.engine_version = Some(version.into());
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
    pub fn build(self, id: Uuid) -> Result<MemoryCard, MemoryCardBuilderError> {
        let user_scope = self
            .user_scope
            .ok_or(MemoryCardBuilderError::MissingField("user_scope"))?;
        let kind = self
            .kind
            .ok_or(MemoryCardBuilderError::MissingField("kind"))?;
        let entity = self
            .entity
            .ok_or(MemoryCardBuilderError::MissingField("entity"))?;
        let slot = self
            .slot
            .ok_or(MemoryCardBuilderError::MissingField("slot"))?;
        let value = self
            .value
            .ok_or(MemoryCardBuilderError::MissingField("value"))?;
        let engine = self
            .engine
            .ok_or(MemoryCardBuilderError::MissingField("engine"))?;
        let engine_version = self
            .engine_version
            .ok_or(MemoryCardBuilderError::MissingField("engine_version"))?;

        let now = Utc::now();

        Ok(MemoryCard {
            id,
            user_scope,
            memory_item_id: self.memory_item_id,
            kind,
            entity: entity.to_lowercase(),
            slot: slot.to_lowercase(),
            value,
            polarity: self.polarity,
            event_date: self.event_date,
            document_date: self.document_date,
            version_key: self.version_key,
            version_relation: self.version_relation,
            source_uri: self.source_uri,
            engine,
            engine_version,
            confidence: self.confidence,
            created_at: now,
            updated_at: now,
        })
    }
}

/// Error type for `MemoryCardBuilder`.
#[derive(Debug, Clone)]
pub enum MemoryCardBuilderError {
    /// A required field is missing.
    MissingField(&'static str),
}

impl std::fmt::Display for MemoryCardBuilderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingField(field) => write!(f, "missing required field: {field}"),
        }
    }
}

impl std::error::Error for MemoryCardBuilderError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_card_builder() {
        let id = Uuid::now_v7();
        #[expect(
            clippy::expect_used,
            reason = "test: builder with all required fields must succeed"
        )]
        let card = MemoryCardBuilder::new()
            .user_scope("test_user")
            .fact()
            .entity("user")
            .slot("employer")
            .value("Anthropic")
            .engine("rules-v1", "1.0.0")
            .build(id)
            .expect("builder with all required fields must succeed");

        assert_eq!(card.id, id);
        assert_eq!(card.kind, MemoryKind::Fact);
        assert_eq!(card.entity, "user");
        assert_eq!(card.slot, "employer");
        assert_eq!(card.value, "Anthropic");
        assert_eq!(card.engine, "rules-v1");
    }

    #[test]
    fn test_preference_with_polarity() {
        let id = Uuid::now_v7();
        #[expect(
            clippy::expect_used,
            reason = "test: preference builder with polarity must succeed"
        )]
        let card = MemoryCardBuilder::new()
            .user_scope("test_user")
            .preference()
            .entity("user")
            .slot("beverage")
            .value("coffee")
            .positive()
            .engine("rules-v1", "1.0.0")
            .build(id)
            .expect("preference builder with polarity must succeed");

        assert_eq!(card.kind, MemoryKind::Preference);
        assert_eq!(card.polarity, Some(Polarity::Positive));
    }

    #[test]
    fn test_version_key_default() {
        let id = Uuid::now_v7();
        #[expect(
            clippy::expect_used,
            reason = "test: builder with all required fields must succeed"
        )]
        let card = MemoryCardBuilder::new()
            .user_scope("test_user")
            .fact()
            .entity("user")
            .slot("location")
            .value("San Francisco")
            .engine("rules-v1", "1.0.0")
            .build(id)
            .expect("builder with all required fields must succeed");

        assert_eq!(card.default_version_key(), "user:location");
    }

    #[test]
    fn test_supersedes() {
        let id1 = Uuid::now_v7();
        let id2 = Uuid::now_v7();
        let now = Utc::now();
        let earlier = now - chrono::Duration::days(1);

        #[expect(clippy::expect_used, reason = "test: old card builder must succeed")]
        let old_card = MemoryCardBuilder::new()
            .user_scope("test_user")
            .fact()
            .entity("user")
            .slot("location")
            .value("New York")
            .document_date(earlier)
            .engine("rules-v1", "1.0.0")
            .build(id1)
            .expect("old card builder must succeed");

        #[expect(clippy::expect_used, reason = "test: new card builder must succeed")]
        let new_card = MemoryCardBuilder::new()
            .user_scope("test_user")
            .fact()
            .entity("user")
            .slot("location")
            .value("San Francisco")
            .document_date(now)
            .updates()
            .engine("rules-v1", "1.0.0")
            .build(id2)
            .expect("new card builder must succeed");

        assert!(new_card.supersedes(&old_card));

        // Sets doesn't supersede
        #[expect(clippy::expect_used, reason = "test: sets card builder must succeed")]
        let sets_card = MemoryCardBuilder::new()
            .user_scope("test_user")
            .fact()
            .entity("user")
            .slot("location")
            .value("San Francisco")
            .document_date(now)
            .engine("rules-v1", "1.0.0")
            .build(id2)
            .expect("sets card builder must succeed");
        assert!(!sets_card.supersedes(&old_card));
    }

    #[test]
    fn test_builder_missing_field() {
        let id = Uuid::now_v7();
        let result = MemoryCardBuilder::new()
            .user_scope("test_user")
            .fact()
            .entity("user")
            // missing slot, value, engine
            .build(id);

        assert!(result.is_err());
    }

    #[test]
    fn test_memory_kind_from_str() {
        #[expect(
            clippy::expect_used,
            reason = "test: valid input must parse successfully"
        )]
        let kind = MemoryKind::from_str("fact").expect("valid input must parse successfully");
        assert_eq!(kind, MemoryKind::Fact);

        #[expect(
            clippy::expect_used,
            reason = "test: valid input must parse successfully"
        )]
        let kind = MemoryKind::from_str("PREFERENCE").expect("valid input must parse successfully");
        assert_eq!(kind, MemoryKind::Preference);

        #[expect(
            clippy::expect_used,
            reason = "test: valid input must parse successfully"
        )]
        let kind =
            MemoryKind::from_str("custom_type").expect("valid input must parse successfully");
        assert_eq!(kind, MemoryKind::Other);
    }
}
