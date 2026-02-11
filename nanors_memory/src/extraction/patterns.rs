//! Configurable extraction patterns for structured memory.
//!
//! This module provides pattern definitions that can be loaded from configuration
//! rather than hardcoded, making the system domain-agnostic.

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

use crate::extraction::cards::{CardKind, Polarity};

/// Error type for pattern building.
#[derive(Debug)]
pub enum BuildError {
    /// The regex pattern is invalid.
    Regex(String),

    /// The card kind is invalid.
    Kind(String),

    /// The polarity is invalid.
    Polarity(String),
}

impl std::fmt::Display for BuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Regex(e) => write!(f, "invalid regex: {e}"),
            Self::Kind(k) => write!(f, "invalid card kind: {k}"),
            Self::Polarity(p) => write!(f, "invalid polarity: {p}"),
        }
    }
}

impl std::error::Error for BuildError {}

impl From<regex::Error> for BuildError {
    fn from(err: regex::Error) -> Self {
        Self::Regex(err.to_string())
    }
}

// Re-export ExtractionPattern from cards for convenience
pub use crate::extraction::cards::ExtractionPattern;

/// Definition of a single extraction pattern.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternDef {
    /// Unique identifier for this pattern.
    pub id: String,

    /// Human-readable name.
    pub name: String,

    /// Regex pattern to match text.
    pub pattern: String,

    /// The kind of card to create.
    pub kind: String,

    /// Entity template (supports $1, $2... for capture groups).
    pub entity: String,

    /// Slot template.
    pub slot: String,

    /// Value template.
    pub value: String,

    /// Polarity for preferences (optional).
    pub polarity: Option<String>,
}

impl PatternDef {
    /// Convert to an `ExtractionPattern`.
    ///
    /// # Errors
    /// Returns an error if the regex pattern is invalid or if the kind/polarity is invalid.
    pub fn build(&self) -> Result<ExtractionPattern, BuildError> {
        // Validate regex
        Regex::new(&self.pattern).map_err(|e| BuildError::Regex(e.to_string()))?;

        let kind = CardKind::from_str(&self.kind).map_err(|e| BuildError::Kind(e.to_string()))?;
        let polarity = self
            .polarity
            .as_ref()
            .map(|p| Polarity::from_str(p))
            .transpose()
            .map_err(|e| BuildError::Polarity(e.to_string()))?;

        polarity.map_or_else(
            || {
                Ok(ExtractionPattern::new(
                    &self.name,
                    &self.pattern,
                    kind,
                    &self.entity,
                    &self.slot,
                    &self.value,
                ))
            },
            |pol| {
                Ok(ExtractionPattern::preference(
                    &self.name,
                    &self.pattern,
                    &self.entity,
                    &self.slot,
                    &self.value,
                    pol,
                ))
            },
        )
    }
}

/// Default pattern set for common Chinese and English statements.
///
/// This provides a sensible default that can be overridden via configuration.
#[must_use]
pub fn default_patterns() -> Vec<PatternDef> {
    let mut patterns = Vec::new();
    patterns.extend(user_identity_patterns());
    patterns.extend(location_patterns());
    patterns.extend(device_patterns());
    patterns.extend(preference_patterns());
    patterns.extend(work_education_patterns());
    patterns.extend(relationship_patterns());
    patterns.extend(english_patterns());
    patterns
}

/// User identity extraction patterns.
fn user_identity_patterns() -> Vec<PatternDef> {
    vec![
        PatternDef {
            id: "user_identity_statement".to_string(),
            name: "user_identity".to_string(),
            pattern: r"(?i)我(?:是|算)(?:一个)?(.{1,20})(?:用户|玩机党|开发者|学生|工程师|设计师|产品经理)".to_string(),
            kind: "profile".to_string(),
            entity: "user".to_string(),
            slot: "user_type".to_string(),
            value: "$1用户".to_string(),
            polarity: None,
        },
        PatternDef {
            id: "user_identity_simple".to_string(),
            name: "user_identity_simple".to_string(),
            pattern: r"(?i)我(?:是|属于)(.{1,30})".to_string(),
            kind: "profile".to_string(),
            entity: "user".to_string(),
            slot: "identity".to_string(),
            value: "$1".to_string(),
            polarity: None,
        },
    ]
}

/// Location extraction patterns.
fn location_patterns() -> Vec<PatternDef> {
    vec![
        PatternDef {
            id: "location_live_in".to_string(),
            name: "location_live".to_string(),
            pattern: r"(?i)我(?:住|居住|生活在|在)(.{1,50})(?:省|市|区|县|镇|村)?".to_string(),
            kind: "fact".to_string(),
            entity: "user".to_string(),
            slot: "location".to_string(),
            value: "$1".to_string(),
            polarity: None,
        },
        PatternDef {
            id: "location_moved_to".to_string(),
            name: "location_moved".to_string(),
            pattern: r"(?i)我(?:搬|搬迁|迁移)(?:到|去了|去)(.{1,50})".to_string(),
            kind: "event".to_string(),
            entity: "user".to_string(),
            slot: "location".to_string(),
            value: "$1".to_string(),
            polarity: None,
        },
    ]
}

/// Device extraction patterns.
fn device_patterns() -> Vec<PatternDef> {
    vec![
        PatternDef {
            id: "device_ownership".to_string(),
            name: "device_own".to_string(),
            pattern:
                r"(?i)(?:我|我的)(.{1,10})(?:手机|电脑|设备|平板|笔记本)(?:是|：|:)?\s*(.{1,30})"
                    .to_string(),
            kind: "fact".to_string(),
            entity: "user".to_string(),
            slot: "device_$1".to_string(),
            value: "$2".to_string(),
            polarity: None,
        },
        PatternDef {
            id: "phone_model".to_string(),
            name: "phone_model".to_string(),
            pattern:
                r"(?i)我(?:的)?(?:手机|电话|机子)(?:是|：|:)?\s*([a-zA-Z0-9\u4e00-\u9fff]{1,30})"
                    .to_string(),
            kind: "fact".to_string(),
            entity: "user.phone".to_string(),
            slot: "model".to_string(),
            value: "$1".to_string(),
            polarity: None,
        },
    ]
}

/// Preference extraction patterns.
fn preference_patterns() -> Vec<PatternDef> {
    vec![
        PatternDef {
            id: "preference_like".to_string(),
            name: "preference_like".to_string(),
            pattern: r"(?i)我(?:喜欢|爱|偏爱|偏好)(.{1,50})".to_string(),
            kind: "preference".to_string(),
            entity: "user".to_string(),
            slot: "preference".to_string(),
            value: "$1".to_string(),
            polarity: Some("positive".to_string()),
        },
        PatternDef {
            id: "preference_dislike".to_string(),
            name: "preference_dislike".to_string(),
            pattern: r"(?i)我(?:讨厌|不喜欢|厌恶|反感|讨厌)(.{1,50})".to_string(),
            kind: "preference".to_string(),
            entity: "user".to_string(),
            slot: "preference".to_string(),
            value: "$1".to_string(),
            polarity: Some("negative".to_string()),
        },
    ]
}

/// Work and education extraction patterns.
fn work_education_patterns() -> Vec<PatternDef> {
    vec![
        PatternDef {
            id: "work_company".to_string(),
            name: "work_company".to_string(),
            pattern: r"(?i)我(?:在)?(?:就职|工作|任职)(?:于|在)?(.{1,50})(?:公司|厂|局|所|部)?"
                .to_string(),
            kind: "fact".to_string(),
            entity: "user".to_string(),
            slot: "workplace".to_string(),
            value: "$1".to_string(),
            polarity: None,
        },
        PatternDef {
            id: "education_school".to_string(),
            name: "education_school".to_string(),
            pattern: r"(?i)我(?:就读|毕业于|在)(.{1,50})(?:大学|学院|学校|中学|小学)?".to_string(),
            kind: "profile".to_string(),
            entity: "user".to_string(),
            slot: "education".to_string(),
            value: "$1".to_string(),
            polarity: None,
        },
    ]
}

/// Relationship extraction patterns.
fn relationship_patterns() -> Vec<PatternDef> {
    vec![PatternDef {
        id: "relationship_family".to_string(),
        name: "relationship_family".to_string(),
        pattern: r"(?i)我(?:的)?(.{1,10})(?:是|叫)(.{1,20})".to_string(),
        kind: "relationship".to_string(),
        entity: "user".to_string(),
        slot: "family_$1".to_string(),
        value: "$2".to_string(),
        polarity: None,
    }]
}

/// English language extraction patterns.
fn english_patterns() -> Vec<PatternDef> {
    vec![
        PatternDef {
            id: "en_identity".to_string(),
            name: "en_identity".to_string(),
            pattern: r"(?i)I am a? (.{1,30})".to_string(),
            kind: "profile".to_string(),
            entity: "user".to_string(),
            slot: "identity".to_string(),
            value: "$1".to_string(),
            polarity: None,
        },
        PatternDef {
            id: "en_location".to_string(),
            name: "en_location".to_string(),
            pattern: r"(?i)I live in (.{1,50})".to_string(),
            kind: "fact".to_string(),
            entity: "user".to_string(),
            slot: "location".to_string(),
            value: "$1".to_string(),
            polarity: None,
        },
        PatternDef {
            id: "en_work".to_string(),
            name: "en_work".to_string(),
            pattern: r"(?i)I work at (.{1,50})".to_string(),
            kind: "fact".to_string(),
            entity: "user".to_string(),
            slot: "workplace".to_string(),
            value: "$1".to_string(),
            polarity: None,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[expect(clippy::expect_used, reason = "Test failure should panic with context")]
    fn test_pattern_def_build() {
        let def = PatternDef {
            id: "test".to_string(),
            name: "test_pattern".to_string(),
            pattern: r"(?i)test\s+(.+)".to_string(),
            kind: "fact".to_string(),
            entity: "test_entity".to_string(),
            slot: "test_slot".to_string(),
            value: "$1".to_string(),
            polarity: None,
        };

        let pattern = def.build().expect("valid pattern should build");
        assert_eq!(pattern.name, "test_pattern");
        assert_eq!(pattern.kind, CardKind::Fact);
    }

    #[test]
    #[expect(clippy::expect_used, reason = "Test failure should panic with context")]
    fn test_pattern_def_with_polarity() {
        let def = PatternDef {
            id: "test".to_string(),
            name: "test_preference".to_string(),
            pattern: r"like\s+(.+)".to_string(),
            kind: "preference".to_string(),
            entity: "user".to_string(),
            slot: "preference".to_string(),
            value: "$1".to_string(),
            polarity: Some("positive".to_string()),
        };

        let pattern = def.build().expect("valid pattern should build");
        assert_eq!(pattern.kind, CardKind::Preference);
        assert_eq!(pattern.polarity, Some(Polarity::Positive));
    }

    #[test]
    #[expect(clippy::expect_used, reason = "Test failure should panic with context")]
    fn test_pattern_def_serialization() {
        let def = PatternDef {
            id: "test".to_string(),
            name: "test".to_string(),
            pattern: r"test".to_string(),
            kind: "fact".to_string(),
            entity: "e".to_string(),
            slot: "s".to_string(),
            value: "v".to_string(),
            polarity: None,
        };

        // Test serialization
        let json = serde_json::to_string(&def).expect("pattern should serialize");
        let deserialized: PatternDef =
            serde_json::from_str(&json).expect("valid JSON should deserialize");

        assert_eq!(deserialized.id, def.id);
        assert_eq!(deserialized.pattern, def.pattern);
    }
}
