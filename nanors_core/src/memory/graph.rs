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

//! Graph result types for structured memory matching.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Result of matching a graph pattern.
///
/// This type is kept for potential future graph-based retrieval features.
/// For now, it serves as a placeholder for structured memory match results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphMatchResult {
    /// The matched entity
    pub entity: String,
    /// The memory item ID that contains this information
    pub memory_item_id: Option<Uuid>,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_graph_match_result() {
        let mut result = GraphMatchResult::new("user".to_string(), 0.95);
        result.bind("slot", "employer");
        result.bind("value", "Anthropic");

        assert_eq!(result.entity, "user");
        assert_eq!(result.confidence, 0.95);
        assert_eq!(result.bindings.len(), 2);
    }
}
