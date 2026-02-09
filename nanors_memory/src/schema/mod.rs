//! Schema validation for memory cards.
//!
//! This module provides type validation and schema checking for structured
//! memory cards, ensuring data quality and consistency.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Value type for memory card slots.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ValueType {
    #[default]
    String,
    Number,
    DateTime,
    Boolean,
    #[serde(rename = "enum")]
    Enum {
        values: Vec<String>,
    },
    Any,
}

impl ValueType {
    /// Get the string representation for error messages.
    #[must_use]
    pub const fn as_str(&self) -> &str {
        match self {
            Self::String => "string",
            Self::Number => "number",
            Self::DateTime => "datetime",
            Self::Boolean => "boolean",
            Self::Enum { .. } => "enum",
            Self::Any => "any",
        }
    }

    /// Check if a value matches this type.
    #[must_use]
    pub fn matches(&self, value: &str) -> bool {
        match self {
            Self::String | Self::Any => true,
            Self::Number => value.parse::<f64>().is_ok(),
            Self::DateTime => {
                value.parse::<i64>().is_ok()
                    || value.contains('T')
                    || value.contains('-')
                    || value.contains('年')
                    || value.contains('月')
                    || value.contains('日')
            }
            Self::Boolean => matches!(
                value.to_lowercase().as_str(),
                "true" | "false" | "yes" | "no" | "1" | "0" | "是" | "否" | "对" | "错"
            ),
            Self::Enum { values } => values.iter().any(|v| v.eq_ignore_ascii_case(value)),
        }
    }
}

/// Cardinality for slots.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Cardinality {
    #[default]
    Single,
    Multiple,
}

/// Schema for a predicate (slot).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredicateSchema {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub range: ValueType,
    #[serde(default)]
    pub cardinality: Cardinality,
    #[serde(default)]
    pub builtin: bool,
}

impl PredicateSchema {
    /// Create a new predicate schema.
    #[must_use]
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            description: None,
            range: ValueType::String,
            cardinality: Cardinality::Single,
            builtin: false,
        }
    }

    /// Set the value type for this predicate.
    #[must_use]
    pub fn with_range(mut self, range: ValueType) -> Self {
        self.range = range;
        self
    }

    /// Set cardinality to multiple.
    #[must_use]
    pub const fn multiple(mut self) -> Self {
        self.cardinality = Cardinality::Multiple;
        self
    }

    /// Mark as a built-in schema.
    #[must_use]
    pub const fn builtin(mut self) -> Self {
        self.builtin = true;
        self
    }

    /// Validate a value against this schema.
    ///
    /// # Errors
    /// Returns `SchemaError` if the value doesn't match the expected type.
    pub fn validate_value(&self, value: &str) -> Result<(), SchemaError> {
        if !self.range.matches(value) {
            return Err(SchemaError::InvalidRange {
                predicate: self.id.clone(),
                expected: self.range.as_str().to_string(),
                got: value.to_string(),
            });
        }
        Ok(())
    }
}

/// Schema validation error.
#[derive(Debug, Clone)]
pub enum SchemaError {
    InvalidRange {
        predicate: String,
        expected: String,
        got: String,
    },
    UnknownPredicate(String),
}

impl std::fmt::Display for SchemaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRange {
                predicate,
                expected,
                got,
            } => write!(
                f,
                "invalid value for '{predicate}': expected {expected}, got '{got}'"
            ),
            Self::UnknownPredicate(p) => write!(f, "unknown predicate: '{p}'"),
        }
    }
}

impl std::error::Error for SchemaError {}

/// Schema registry with built-in predicates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaRegistry {
    schemas: HashMap<String, PredicateSchema>,
    #[serde(default)]
    strict: bool,
}

impl SchemaRegistry {
    /// Create a new schema registry with default built-in schemas.
    #[must_use]
    pub fn new() -> Self {
        let mut registry = Self {
            schemas: HashMap::new(),
            strict: false,
        };
        registry.register_builtin_schemas();
        registry
    }

    /// Enable strict mode where unknown predicates are rejected.
    #[must_use]
    pub const fn strict(mut self) -> Self {
        self.strict = true;
        self
    }

    /// Register a predicate schema.
    pub fn register(&mut self, schema: PredicateSchema) {
        self.schemas.insert(schema.id.clone(), schema);
    }

    /// Get a predicate schema by ID.
    #[must_use]
    pub fn get(&self, predicate: &str) -> Option<&PredicateSchema> {
        self.schemas.get(predicate)
    }

    /// Check if a predicate is registered.
    #[must_use]
    pub fn contains(&self, predicate: &str) -> bool {
        self.schemas.contains_key(predicate)
    }

    /// Validate a card against the schema.
    ///
    /// # Errors
    /// Returns `SchemaError` if:
    /// - The card's slot has a registered schema that the value doesn't match
    /// - `strict` mode is enabled and the slot is unknown
    pub fn validate_card(&self, card: &crate::MemoryCard) -> Result<(), SchemaError> {
        if let Some(schema) = self.schemas.get(&card.slot) {
            schema.validate_value(&card.value)?;
        } else if self.strict {
            return Err(SchemaError::UnknownPredicate(card.slot.clone()));
        }
        Ok(())
    }

    /// Infer a schema from existing values.
    #[must_use]
    pub fn infer_from_values(&self, slot: &str, values: &[&str]) -> PredicateSchema {
        let all_numeric = !values.is_empty() && values.iter().all(|v| v.parse::<f64>().is_ok());
        let all_datetime = !values.is_empty()
            && values
                .iter()
                .all(|v| v.parse::<i64>().is_ok() || v.contains('T') || v.contains('-'));
        let all_boolean = !values.is_empty()
            && values.iter().all(|v| {
                matches!(
                    v.to_lowercase().as_str(),
                    "true" | "false" | "yes" | "no" | "1" | "0"
                )
            });

        let mut schema = PredicateSchema::new(slot, slot);
        if all_numeric {
            schema.range = ValueType::Number;
        } else if all_datetime {
            schema.range = ValueType::DateTime;
        } else if all_boolean {
            schema.range = ValueType::Boolean;
        }

        // Check cardinality
        let unique_values: std::collections::HashSet<_> = values.iter().collect();
        if unique_values.len() > 1 {
            schema.cardinality = Cardinality::Multiple;
        }

        schema
    }

    /// Register built-in schemas for common predicates.
    fn register_builtin_schemas(&mut self) {
        // Location predicates
        self.register(PredicateSchema::new("location", "Location").builtin());
        self.register(PredicateSchema::new("user_type", "User Type").builtin());

        // Personal info with type validation
        self.register(
            PredicateSchema::new("age", "Age")
                .with_range(ValueType::Number)
                .builtin(),
        );
        self.register(
            PredicateSchema::new("birthday", "Birthday")
                .with_range(ValueType::DateTime)
                .builtin(),
        );

        // Preferences (multiple allowed)
        self.register(
            PredicateSchema::new("preference", "Preference")
                .multiple()
                .builtin(),
        );
        self.register(PredicateSchema::new("hobby", "Hobby").multiple().builtin());

        // Boolean predicates
        self.register(
            PredicateSchema::new("verified", "Verified")
                .with_range(ValueType::Boolean)
                .builtin(),
        );
    }
}

impl Default for SchemaRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extraction::cards::{CardKind, MemoryCard};

    #[test]
    fn test_value_type_string_matches() {
        assert!(ValueType::String.matches("any text"));
        assert!(ValueType::Any.matches("any text"));
    }

    #[test]
    fn test_value_type_number_matches() {
        assert!(ValueType::Number.matches("123"));
        assert!(ValueType::Number.matches("12.34"));
        assert!(ValueType::Number.matches("-5"));
        assert!(!ValueType::Number.matches("abc"));
    }

    #[test]
    fn test_value_type_datetime_matches() {
        assert!(ValueType::DateTime.matches("2024-01-01T00:00:00Z"));
        assert!(ValueType::DateTime.matches("1704067200"));
        assert!(ValueType::DateTime.matches("2024年1月1日"));
        assert!(!ValueType::DateTime.matches("not a date"));
    }

    #[test]
    fn test_value_type_boolean_matches() {
        assert!(ValueType::Boolean.matches("true"));
        assert!(ValueType::Boolean.matches("false"));
        assert!(ValueType::Boolean.matches("yes"));
        assert!(ValueType::Boolean.matches("no"));
        assert!(ValueType::Boolean.matches("1"));
        assert!(ValueType::Boolean.matches("0"));
        assert!(!ValueType::Boolean.matches("maybe"));
    }

    #[test]
    fn test_value_type_enum_matches() {
        let enum_type = ValueType::Enum {
            values: vec!["red".to_string(), "green".to_string(), "blue".to_string()],
        };
        assert!(enum_type.matches("red"));
        assert!(enum_type.matches("Red"));
        assert!(enum_type.matches("RED"));
        assert!(!enum_type.matches("yellow"));
    }

    #[test]
    #[expect(clippy::expect_used, reason = "Test failure should panic with context")]
    fn test_predicate_schema_validation() {
        let schema = PredicateSchema::new("age", "Age").with_range(ValueType::Number);

        // Valid values
        schema
            .validate_value("25")
            .expect("valid number should pass");
        schema.validate_value("0").expect("zero should pass");
        schema.validate_value("-5").expect("negative should pass");

        // Invalid value
        let result = schema.validate_value("twenty-five");
        assert!(result.is_err());
    }

    #[test]
    fn test_schema_registry_default() {
        let registry = SchemaRegistry::new();

        // Built-in schemas should be registered
        assert!(registry.contains("location"));
        assert!(registry.contains("user_type"));
        assert!(registry.contains("age"));

        // Unknown schema
        assert!(!registry.contains("unknown_slot"));
    }

    #[test]
    fn test_schema_strict_mode() {
        let registry = SchemaRegistry::new().strict();

        let card = MemoryCard::new(
            "user".to_string(),
            CardKind::Fact,
            "user".to_string(),
            "unknown_slot".to_string(),
            "some_value".to_string(),
        );

        let result = registry.validate_card(&card);
        assert!(result.is_err());

        if let Err(SchemaError::UnknownPredicate(slot)) = result {
            assert_eq!(slot, "unknown_slot");
        } else {
            panic!("Expected UnknownPredicate error");
        }
    }

    #[test]
    fn test_infer_from_values_numeric() {
        let registry = SchemaRegistry::new();
        let values = ["25", "30", "35"];

        let schema = registry.infer_from_values("age", &values);

        assert!(matches!(schema.range, ValueType::Number));
    }

    #[test]
    fn test_infer_from_values_multiple() {
        let registry = SchemaRegistry::new();
        let values = ["reading", "swimming", "coding"];

        let schema = registry.infer_from_values("hobby", &values);

        assert!(matches!(schema.cardinality, Cardinality::Multiple));
    }

    #[test]
    fn test_error_display() {
        let error = SchemaError::InvalidRange {
            predicate: "age".to_string(),
            expected: "number".to_string(),
            got: "twenty-five".to_string(),
        };

        let display = format!("{error}");
        assert!(display.contains("age"));
        assert!(display.contains("number"));
        assert!(display.contains("twenty-five"));
    }
}
