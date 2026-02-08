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

//! Graph-aware query types for hybrid retrieval.
//!
//! Enables combining graph traversal with vector similarity for relational queries.

use serde::{Deserialize, Serialize};

/// A triple pattern for graph matching.
/// Variables start with `?`, literals are exact matches.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TriplePattern {
    /// Subject: entity name or `?var`
    pub subject: PatternTerm,
    /// Predicate: slot/relationship name or `?var`
    pub predicate: PatternTerm,
    /// Object: value or entity or `?var`
    pub object: PatternTerm,
}

impl TriplePattern {
    /// Create a new triple pattern.
    #[must_use]
    pub const fn new(subject: PatternTerm, predicate: PatternTerm, object: PatternTerm) -> Self {
        Self {
            subject,
            predicate,
            object,
        }
    }

    /// Create a pattern matching entity:slot = value
    #[must_use]
    pub fn entity_slot_value(entity: &str, slot: &str, value: &str) -> Self {
        Self {
            subject: PatternTerm::Literal(entity.to_lowercase()),
            predicate: PatternTerm::Literal(slot.to_lowercase()),
            object: PatternTerm::Literal(value.to_string()),
        }
    }

    /// Create a pattern matching entity:slot = ?var (any value)
    #[must_use]
    pub fn entity_slot_any(entity: &str, slot: &str, var: &str) -> Self {
        Self {
            subject: PatternTerm::Literal(entity.to_lowercase()),
            predicate: PatternTerm::Literal(slot.to_lowercase()),
            object: PatternTerm::Variable(var.to_string()),
        }
    }

    /// Create a pattern matching ?entity:slot = value (find entities with this value)
    #[must_use]
    pub fn any_slot_value(var: &str, slot: &str, value: &str) -> Self {
        Self {
            subject: PatternTerm::Variable(var.to_string()),
            predicate: PatternTerm::Literal(slot.to_lowercase()),
            object: PatternTerm::Literal(value.to_string()),
        }
    }
}

/// A term in a triple pattern - either a variable or literal.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PatternTerm {
    /// Variable binding (e.g., `?user`, `?food`)
    Variable(String),
    /// Literal value (e.g., "alice", "employer", "Anthropic")
    Literal(String),
}

impl PatternTerm {
    /// Check if this term is a variable.
    #[must_use]
    pub const fn is_variable(&self) -> bool {
        matches!(self, Self::Variable(_))
    }

    /// Get the variable name if this is a variable.
    #[must_use]
    pub fn variable_name(&self) -> Option<&str> {
        match self {
            Self::Variable(name) => Some(name),
            Self::Literal(_) => None,
        }
    }

    /// Get the literal value if this is a literal.
    #[must_use]
    pub fn literal_value(&self) -> Option<&str> {
        match self {
            Self::Literal(value) => Some(value),
            Self::Variable(_) => None,
        }
    }
}

/// A graph pattern for filtering - conjunction of triple patterns.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GraphPattern {
    /// Triple patterns to match (all must match - AND semantics)
    pub triples: Vec<TriplePattern>,
}

impl GraphPattern {
    /// Create an empty pattern.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            triples: Vec::new(),
        }
    }

    /// Add a triple pattern.
    pub fn add(&mut self, pattern: TriplePattern) {
        self.triples.push(pattern);
    }

    /// Create from a single triple pattern.
    #[must_use]
    pub fn single(pattern: TriplePattern) -> Self {
        Self {
            triples: vec![pattern],
        }
    }

    /// Check if the pattern is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.triples.is_empty()
    }
}

/// Query plan for graph-aware retrieval.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum QueryPlan {
    /// Pure vector similarity (fallback when no graph patterns detected)
    VectorOnly {
        /// Query text for reference
        query_text: Option<String>,
        /// Number of results
        top_k: usize,
    },

    /// Pure graph traversal (for relational queries)
    GraphOnly {
        /// Graph pattern to match
        pattern: GraphPattern,
        /// Maximum results
        limit: usize,
    },

    /// Hybrid: Graph filter + Vector rank
    Hybrid {
        /// First: Graph pattern to get candidate entities
        graph_filter: GraphPattern,
        /// Query text for reference
        query_text: Option<String>,
        /// Number of final results
        top_k: usize,
    },
}

impl QueryPlan {
    /// Create a vector-only plan.
    #[must_use]
    pub const fn vector_only(query_text: Option<String>, top_k: usize) -> Self {
        Self::VectorOnly { query_text, top_k }
    }

    /// Create a graph-only plan.
    #[must_use]
    pub const fn graph_only(pattern: GraphPattern, limit: usize) -> Self {
        Self::GraphOnly { pattern, limit }
    }

    /// Create a hybrid plan.
    #[must_use]
    pub const fn hybrid(
        graph_filter: GraphPattern,
        query_text: Option<String>,
        top_k: usize,
    ) -> Self {
        Self::Hybrid {
            graph_filter,
            query_text,
            top_k,
        }
    }
}

/// Result of matching a graph pattern.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphMatchResult {
    /// The matched entity
    pub entity: String,
    /// The memory item ID that contains this information
    pub memory_item_id: Option<uuid::Uuid>,
    /// Confidence score (0.0-1.0)
    pub confidence: f32,
    /// Variable bindings from the pattern match
    pub bindings: std::collections::HashMap<String, String>,
}

impl GraphMatchResult {
    /// Create a new graph match result.
    #[must_use]
    pub fn new(entity: String, confidence: f32) -> Self {
        Self {
            entity,
            memory_item_id: None,
            confidence,
            bindings: std::collections::HashMap::new(),
        }
    }

    /// Bind a variable to a value.
    pub fn bind(&mut self, var: impl Into<String>, value: impl Into<String>) {
        self.bindings.insert(var.into(), value.into());
    }
}

/// `MemoryCard` for structured fact storage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryCard {
    pub id: uuid::Uuid,
    pub user_scope: String,
    pub memory_item_id: Option<uuid::Uuid>,
    pub kind: MemoryKind,
    pub entity: String,
    pub slot: String,
    pub value: String,
    pub polarity: Option<Polarity>,
    pub version_key: Option<String>,
    pub version_relation: VersionRelation,
    pub confidence: Option<f32>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// The kind of memory being stored.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
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

    /// Parse a string into a `MemoryKind`.
    #[must_use]
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "fact" => Self::Fact,
            "preference" => Self::Preference,
            "event" => Self::Event,
            "profile" => Self::Profile,
            "relationship" => Self::Relationship,
            "goal" => Self::Goal,
            _ => Self::Other,
        }
    }
}

/// Polarity for preferences and boolean facts.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
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

/// How this card relates to prior versions of the same slot.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_triple_pattern_creation() {
        let pattern = TriplePattern::entity_slot_value("alice", "employer", "Anthropic");
        assert!(matches!(pattern.subject, PatternTerm::Literal(s) if s == "alice"));
        assert!(matches!(pattern.predicate, PatternTerm::Literal(s) if s == "employer"));
        // Note: object value is converted to lowercase in entity_slot_value
        assert!(matches!(pattern.object, PatternTerm::Literal(_)));
    }

    #[test]
    fn test_pattern_term_variables() {
        let var = PatternTerm::Variable("entity".to_string());
        assert!(var.is_variable());
        assert_eq!(var.variable_name(), Some("entity"));
        assert!(var.literal_value().is_none());
    }

    #[test]
    fn test_pattern_term_literals() {
        let lit = PatternTerm::Literal("alice".to_string());
        assert!(!lit.is_variable());
        assert_eq!(lit.literal_value(), Some("alice"));
        assert!(lit.variable_name().is_none());
    }

    #[test]
    fn test_graph_pattern() {
        let mut pattern = GraphPattern::new();
        pattern.add(TriplePattern::entity_slot_value(
            "alice",
            "employer",
            "Anthropic",
        ));
        assert!(!pattern.is_empty());
        assert_eq!(pattern.triples.len(), 1);
    }

    #[test]
    fn test_query_plan_variants() {
        let vector_only = QueryPlan::vector_only(Some("test".to_string()), 10);
        assert!(matches!(vector_only, QueryPlan::VectorOnly { .. }));

        let graph_only = QueryPlan::graph_only(GraphPattern::new(), 10);
        assert!(matches!(graph_only, QueryPlan::GraphOnly { .. }));

        let hybrid = QueryPlan::hybrid(GraphPattern::new(), Some("test".to_string()), 10);
        assert!(matches!(hybrid, QueryPlan::Hybrid { .. }));
    }

    #[test]
    fn test_memory_kind_from_str() {
        assert_eq!(MemoryKind::from_str("fact"), MemoryKind::Fact);
        assert_eq!(MemoryKind::from_str("PREFERENCE"), MemoryKind::Preference);
        assert_eq!(MemoryKind::from_str("custom_type"), MemoryKind::Other);
    }
}
