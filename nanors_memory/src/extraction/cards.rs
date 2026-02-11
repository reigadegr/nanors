//! Structured memory card types.
//!
//! Memory cards represent atomic facts extracted from text as entity/slot/value triples.
//! This module defines the types used for structured memory extraction and storage.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use uuid::Uuid;

/// The kind of memory card.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
#[derive(Default)]
pub enum CardKind {
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
}

impl CardKind {
    /// Returns the string representation of this kind.
    #[must_use]
    pub const fn as_str(&self) -> &str {
        match self {
            Self::Fact => "fact",
            Self::Preference => "preference",
            Self::Event => "event",
            Self::Profile => "profile",
            Self::Relationship => "relationship",
            Self::Goal => "goal",
        }
    }
}

impl FromStr for CardKind {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "preference" => Ok(Self::Preference),
            "event" => Ok(Self::Event),
            "profile" => Ok(Self::Profile),
            "relationship" => Ok(Self::Relationship),
            "goal" => Ok(Self::Goal),
            "fact" => Ok(Self::Fact),
            _ => Err("unknown card kind"),
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
    pub const fn as_str(&self) -> &str {
        match self {
            Self::Sets => "sets",
            Self::Updates => "updates",
            Self::Extends => "extends",
            Self::Retracts => "retracts",
        }
    }
}

impl FromStr for VersionRelation {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "updates" => Ok(Self::Updates),
            "extends" => Ok(Self::Extends),
            "retracts" => Ok(Self::Retracts),
            "sets" => Ok(Self::Sets),
            _ => Err("unknown version relation"),
        }
    }
}

/// Polarity for preferences and boolean facts.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
#[derive(Default)]
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
    pub const fn as_str(&self) -> &str {
        match self {
            Self::Positive => "positive",
            Self::Negative => "negative",
            Self::Neutral => "neutral",
        }
    }
}

impl FromStr for Polarity {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "positive" => Ok(Self::Positive),
            "negative" => Ok(Self::Negative),
            "neutral" => Ok(Self::Neutral),
            _ => Err("unknown polarity"),
        }
    }
}

/// A structured memory card extracted from text.
///
/// Cards represent atomic facts as entity/slot/value triples for fast O(1) lookup.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryCard {
    /// Unique identifier.
    pub id: Uuid,

    /// The kind of memory card.
    pub kind: CardKind,

    /// The entity this card describes (e.g., "user", "user.phone").
    pub entity: String,

    /// The attribute/slot being described (e.g., "`user_type`", "location").
    pub slot: String,

    /// The actual value (e.g., "`android_enthusiast`", "北京").
    pub value: String,

    /// Sentiment/polarity for preferences.
    pub polarity: Option<Polarity>,

    /// When the event/fact occurred (not when recorded).
    pub event_date: Option<DateTime<Utc>>,

    /// When this information was recorded.
    pub document_date: Option<DateTime<Utc>>,

    /// Key to group related cards (usually "entity:slot").
    pub version_key: Option<String>,

    /// How this relates to prior versions.
    pub version_relation: VersionRelation,

    /// Reference to the source memory item.
    pub source_memory_id: Option<Uuid>,

    /// Which engine extracted this card.
    pub engine: String,

    /// Version of the engine.
    pub engine_version: String,

    /// Confidence score (0.0-1.0).
    pub confidence: Option<f32>,

    /// When this card was created.
    pub created_at: DateTime<Utc>,
}

impl MemoryCard {
    /// Generate the default version key from entity and slot.
    #[must_use]
    pub fn default_version_key(&self) -> String {
        format!("{}:{}", self.entity, self.slot)
    }

    /// Create a new memory card with minimal fields.
    #[must_use]
    pub fn new(kind: CardKind, entity: String, slot: String, value: String) -> Self {
        let now = Utc::now();
        let entity_key = format!("{entity}:{slot}");
        Self {
            id: Uuid::now_v7(),
            kind,
            entity,
            slot,
            value,
            polarity: None,
            event_date: None,
            document_date: None,
            version_key: Some(entity_key),
            version_relation: VersionRelation::Sets,
            source_memory_id: None,
            engine: "rules".to_string(),
            engine_version: "1.0.0".to_string(),
            confidence: None,
            created_at: now,
        }
    }

    /// Set polarity for this card.
    #[must_use]
    pub const fn with_polarity(mut self, polarity: Polarity) -> Self {
        self.polarity = Some(polarity);
        self
    }

    /// Set the source memory id.
    #[must_use]
    pub const fn with_source_memory(mut self, source_id: Uuid) -> Self {
        self.source_memory_id = Some(source_id);
        self
    }

    /// Set confidence score.
    #[must_use]
    pub const fn with_confidence(mut self, confidence: f32) -> Self {
        self.confidence = Some(confidence.clamp(0.0, 1.0));
        self
    }

    /// Set the `created_at` timestamp.
    #[must_use]
    pub const fn with_created_at(mut self, created_at: DateTime<Utc>) -> Self {
        self.created_at = created_at;
        self
    }

    /// Set the `event_date` timestamp.
    #[must_use]
    pub const fn with_event_date(mut self, event_date: DateTime<Utc>) -> Self {
        self.event_date = Some(event_date);
        self
    }

    /// Set the `document_date` timestamp.
    #[must_use]
    pub const fn with_document_date(mut self, document_date: DateTime<Utc>) -> Self {
        self.document_date = Some(document_date);
        self
    }

    /// Set version relation.
    #[must_use]
    pub const fn with_version_relation(mut self, relation: VersionRelation) -> Self {
        self.version_relation = relation;
        self
    }

    /// Check if this card matches an entity/slot query.
    #[must_use]
    pub fn matches(&self, entity: &str, slot: &str) -> bool {
        self.entity == entity && self.slot == slot
    }
}

/// Builder for constructing extraction patterns.
#[derive(Debug, Clone)]
pub struct ExtractionPattern {
    /// Unique name for this pattern.
    pub name: String,

    /// Regex pattern to match.
    pub pattern: String,

    /// The kind of card to create.
    pub kind: CardKind,

    /// Entity template (supports $1, $2 capture groups).
    pub entity: String,

    /// Slot template.
    pub slot: String,

    /// Value template.
    pub value: String,

    /// Polarity for preference cards.
    pub polarity: Option<Polarity>,
}

impl ExtractionPattern {
    /// Create a new extraction pattern.
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        pattern: impl Into<String>,
        kind: CardKind,
        entity: impl Into<String>,
        slot: impl Into<String>,
        value: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            pattern: pattern.into(),
            kind,
            entity: entity.into(),
            slot: slot.into(),
            value: value.into(),
            polarity: None,
        }
    }

    /// Create a preference pattern with polarity.
    #[must_use]
    pub fn preference(
        name: impl Into<String>,
        pattern: impl Into<String>,
        entity: impl Into<String>,
        slot: impl Into<String>,
        value: impl Into<String>,
        polarity: Polarity,
    ) -> Self {
        Self {
            name: name.into(),
            pattern: pattern.into(),
            kind: CardKind::Preference,
            entity: entity.into(),
            slot: slot.into(),
            value: value.into(),
            polarity: Some(polarity),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_card_kind_conversion() {
        assert_eq!(CardKind::Fact.as_str(), "fact");
        #[expect(clippy::expect_used, reason = "Test failure should panic with context")]
        {
            assert_eq!(
                CardKind::from_str("fact").expect("valid kind should parse"),
                CardKind::Fact
            );
        }
        #[expect(clippy::expect_used, reason = "Test failure should panic with context")]
        {
            assert_eq!(
                CardKind::from_str("FACT").expect("valid kind should parse"),
                CardKind::Fact
            );
        }
        assert!(CardKind::from_str("unknown").is_err());
    }

    #[test]
    fn test_polarity_conversion() {
        assert_eq!(Polarity::Positive.as_str(), "positive");
        #[expect(clippy::expect_used, reason = "Test failure should panic with context")]
        {
            assert_eq!(
                Polarity::from_str("positive").expect("valid polarity should parse"),
                Polarity::Positive
            );
        }
        assert!(Polarity::from_str("unknown").is_err());
    }

    #[test]
    fn test_memory_card_new() {
        let card = MemoryCard::new(
            CardKind::Fact,
            "user".to_string(),
            "user_type".to_string(),
            "developer".to_string(),
        );

        assert_eq!(card.entity, "user");
        assert_eq!(card.slot, "user_type");
        assert_eq!(card.value, "developer");
        assert_eq!(card.kind, CardKind::Fact);
    }

    #[test]
    #[expect(clippy::expect_used, reason = "Test failure should panic with context")]
    fn test_memory_card_builder() {
        let card = MemoryCard::new(
            CardKind::Fact,
            "user".to_string(),
            "location".to_string(),
            "北京".to_string(),
        )
        .with_polarity(Polarity::Neutral)
        .with_confidence(0.9)
        .with_source_memory(Uuid::now_v7());

        assert!(card.polarity.is_some());
        assert_eq!(
            card.polarity.expect("polarity should be set"),
            Polarity::Neutral
        );
        assert_eq!(card.confidence, Some(0.9));
        assert!(card.source_memory_id.is_some());
    }

    #[test]
    fn test_memory_card_matches() {
        let card = MemoryCard::new(
            CardKind::Fact,
            "user".to_string(),
            "user_type".to_string(),
            "developer".to_string(),
        );

        assert!(card.matches("user", "user_type"));
        assert!(!card.matches("user", "location"));
        assert!(!card.matches("phone", "user_type"));
    }

    #[test]
    fn test_extraction_pattern_new() {
        let pattern = ExtractionPattern::new(
            "user_type",
            r"(?i)我是(.{1,20})用户",
            CardKind::Profile,
            "user",
            "user_type",
            "$1",
        );

        assert_eq!(pattern.name, "user_type");
        assert_eq!(pattern.kind, CardKind::Profile);
        assert_eq!(pattern.entity, "user");
        assert_eq!(pattern.slot, "user_type");
        assert_eq!(pattern.value, "$1");
        assert!(pattern.polarity.is_none());
    }

    #[test]
    fn test_extraction_pattern_preference() {
        let pattern = ExtractionPattern::preference(
            "likes_food",
            r"(?i)我喜欢(.{1,30})",
            "user",
            "food_preference",
            "$1",
            Polarity::Positive,
        );

        assert_eq!(pattern.kind, CardKind::Preference);
        assert_eq!(pattern.polarity, Some(Polarity::Positive));
    }
}
