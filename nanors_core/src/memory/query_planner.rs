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

//! Query planner for graph-aware retrieval.

use super::graph::{GraphPattern, QueryPlan, TriplePattern};

/// Query planner that analyzes queries and creates execution plans.
#[derive(Debug, Default)]
pub struct QueryPlanner {
    /// Patterns for detecting relational queries
    entity_patterns: Vec<EntityPattern>,
}

/// Pattern for detecting entity-related queries.
#[derive(Debug, Clone)]
struct EntityPattern {
    /// Keywords that trigger this pattern
    keywords: Vec<&'static str>,
    /// Slot to query
    slot: &'static str,
    /// Whether the pattern looks for a specific value
    needs_value: bool,
}

impl QueryPlanner {
    /// Create a new query planner.
    #[must_use]
    pub fn new() -> Self {
        let mut planner = Self::default();
        planner.init_patterns();
        planner
    }

    fn init_patterns(&mut self) {
        // Location patterns
        self.entity_patterns.push(EntityPattern {
            keywords: vec![
                "who lives in",
                "people in",
                "users in",
                "from",
                "located in",
                "based in",
            ],
            slot: "location",
            needs_value: true,
        });

        // Employer/workplace patterns
        self.entity_patterns.push(EntityPattern {
            keywords: vec![
                "who works at",
                "employees of",
                "people at",
                "works for",
                "employed by",
            ],
            slot: "workplace",
            needs_value: true,
        });

        // Preference patterns
        self.entity_patterns.push(EntityPattern {
            keywords: vec![
                "who likes",
                "who loves",
                "fans of",
                "people who like",
                "people who love",
            ],
            slot: "preference",
            needs_value: true,
        });

        // Entity state patterns
        self.entity_patterns.push(EntityPattern {
            keywords: vec!["what is", "where does", "who is", "what does"],
            slot: "",
            needs_value: false,
        });
    }

    /// Analyze a query and produce an execution plan.
    #[must_use]
    pub fn plan(&self, query: &str, top_k: usize) -> QueryPlan {
        let query_lower = query.to_lowercase();

        // Try to detect relational patterns
        if let Some(pattern) = self.detect_pattern(&query_lower, query) {
            if pattern.triples.is_empty() {
                // No specific pattern found, use vector search
                QueryPlan::vector_only(Some(query.to_string()), top_k)
            } else {
                // Found relational pattern - use hybrid search
                QueryPlan::hybrid(pattern, Some(query.to_string()), top_k)
            }
        } else {
            // Default to vector-only search
            QueryPlan::vector_only(Some(query.to_string()), top_k)
        }
    }

    fn detect_pattern(&self, query_lower: &str, _original: &str) -> Option<GraphPattern> {
        let mut pattern = GraphPattern::new();

        for ep in &self.entity_patterns {
            for keyword in &ep.keywords {
                if query_lower.contains(keyword) {
                    // Extract the value after the keyword
                    if let Some(pos) = query_lower.find(keyword) {
                        let after = &query_lower[pos + keyword.len()..];
                        let value = extract_value(after);

                        if !value.is_empty() && ep.needs_value {
                            // Create pattern: ?entity :slot "value"
                            pattern.add(TriplePattern::any_slot_value("entity", ep.slot, &value));
                            return Some(pattern);
                        }
                    }
                }
            }
        }

        // Check for entity-specific queries like "alice's employer" or "what is alice's job"
        if let Some((entity, slot)) = extract_possessive_query(query_lower) {
            pattern.add(TriplePattern::entity_slot_any(&entity, &slot, "value"));
            return Some(pattern);
        }

        Some(pattern)
    }
}

/// Extract a value from text after a keyword.
fn extract_value(text: &str) -> String {
    let trimmed = text.trim();
    // Take words until we hit a common query continuation
    let stop_words = ["and", "or", "who", "what", "that", "?"];
    let mut words = Vec::new();

    for word in trimmed.split_whitespace() {
        let clean = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '-');
        if stop_words.contains(&clean.to_lowercase().as_str()) {
            break;
        }
        if !clean.is_empty() {
            words.push(clean);
        }
        // Stop after a few words
        if words.len() >= 3 {
            break;
        }
    }

    words.join(" ")
}

/// Extract entity and slot from possessive queries like "alice's employer".
fn extract_possessive_query(query: &str) -> Option<(String, String)> {
    // Pattern: "X's Y" or "X's Y is"
    if let Some(pos) = query.find("'s ") {
        let entity = query[..pos].split_whitespace().last()?;
        let after = &query[pos + 3..];
        let slot = after.split_whitespace().next()?;

        // Map common slot aliases
        let slot = match slot {
            "job" | "work" | "employer" | "role" | "company" => "workplace",
            "home" | "city" | "address" => "location",
            "favorite" => "preference",
            "wife" | "husband" | "spouse" | "partner" => "spouse",
            other => other,
        };

        return Some((entity.to_string(), slot.to_string()));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_query_planner_vector_only() {
        let planner = QueryPlanner::new();
        let plan = planner.plan("What is the meaning of life?", 10);

        assert!(matches!(plan, QueryPlan::VectorOnly { .. }));
    }

    #[test]
    fn test_query_planner_works_at() {
        let planner = QueryPlanner::new();
        let plan = planner.plan("Who works at Google?", 10);

        assert!(matches!(plan, QueryPlan::Hybrid { .. }));
        if let QueryPlan::Hybrid { graph_filter, .. } = plan {
            assert!(!graph_filter.triples.is_empty());
        }
    }

    #[test]
    fn test_query_planner_possessive() {
        let planner = QueryPlanner::new();
        let plan = planner.plan("What is Alice's employer?", 10);

        assert!(matches!(plan, QueryPlan::Hybrid { .. }));
        if let QueryPlan::Hybrid { graph_filter, .. } = plan {
            assert!(!graph_filter.triples.is_empty());
        }
    }

    #[test]
    fn test_extract_value() {
        assert_eq!(extract_value("Google and others"), "Google");
        assert_eq!(extract_value("San Francisco"), "San Francisco");
        assert_eq!(extract_value("who?"), "");
        assert_eq!(extract_value("New York City"), "New York City");
    }

    #[test]
    fn test_extract_possessive_query() {
        let Some((entity, slot)) = extract_possessive_query("alice's employer") else {
            panic!("Failed to extract possessive query");
        };
        assert_eq!(entity, "alice");
        assert_eq!(slot, "workplace");

        let Some((entity, slot)) = extract_possessive_query("bob's location") else {
            panic!("Failed to extract possessive query");
        };
        assert_eq!(entity, "bob");
        assert_eq!(slot, "location");
    }
}
