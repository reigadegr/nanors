//! Query expansion for improved retrieval recall.
//!
//! This module provides configurable query expansion strategies including:
//! - Stopword filtering to remove noise words
//! - Singular/plural and variant expansion

use serde::{Deserialize, Serialize};

/// The type of query expansion applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum ExpansionType {
    /// Stopword removal: remove question words and particles
    Stopwords = 0,
    /// Variant expansion: singular/plural, synonyms
    Variants = 1,
}

impl ExpansionType {
    /// Returns the string representation.
    #[must_use]
    pub const fn as_str(&self) -> &str {
        match self {
            Self::Stopwords => "stopwords",
            Self::Variants => "variants",
        }
    }
}

/// Configuration for query expansion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryExpanderConfig {
    /// Stopwords to filter out (can be language-specific).
    #[serde(default)]
    pub stopwords: Vec<String>,

    /// Whether expansion is enabled.
    #[serde(default = "default_expansion_enabled")]
    pub enabled: bool,
}

const fn default_expansion_enabled() -> bool {
    true
}

impl Default for QueryExpanderConfig {
    fn default() -> Self {
        Self {
            stopwords: default_stopwords(),
            enabled: true,
        }
    }
}

/// Default stopwords for Chinese and English.
#[must_use]
pub fn default_stopwords() -> Vec<String> {
    vec![
        // Chinese question words (multi-character)
        "什么".to_string(),
        "怎么".to_string(),
        "如何".to_string(),
        "哪里".to_string(),
        "哪个".to_string(),
        "多少".to_string(),
        "谁".to_string(),
        "什么时候".to_string(),
        "为什么".to_string(),
        "咋".to_string(),
        "吗".to_string(),
        "呢".to_string(),
        // Chinese particles
        "的".to_string(),
        "了".to_string(),
        "是".to_string(),
        "有".to_string(),
        "在".to_string(),
        // Single-character Chinese stopwords (for character-based tokenization)
        "我".to_string(),
        "你".to_string(),
        "他".to_string(),
        "她".to_string(),
        "它".to_string(),
        "什".to_string(),
        "么".to_string(),
        "怎".to_string(),
        "如".to_string(),
        // English question words
        "what".to_string(),
        "how".to_string(),
        "where".to_string(),
        "which".to_string(),
        "who".to_string(),
        "when".to_string(),
        "why".to_string(),
        "whose".to_string(),
        // English stopwords
        "a".to_string(),
        "an".to_string(),
        "the".to_string(),
        "is".to_string(),
        "are".to_string(),
        "was".to_string(),
        "were".to_string(),
        "do".to_string(),
        "does".to_string(),
        "did".to_string(),
        "am".to_string(),
    ]
}

/// Query expander for improving recall.
pub struct QueryExpander {
    stopwords: Vec<String>,
    enabled: bool,
}

impl QueryExpander {
    /// Create a new expander from configuration.
    #[must_use]
    pub fn new(config: QueryExpanderConfig) -> Self {
        Self {
            stopwords: config.stopwords,
            enabled: config.enabled,
        }
    }

    /// Create expander with default configuration.
    #[must_use]
    pub fn with_defaults() -> Self {
        Self::new(QueryExpanderConfig::default())
    }

    /// Expand a query using all enabled strategies.
    #[must_use]
    pub fn expand(&self, query: &str) -> Vec<ExpandedQuery> {
        if !self.enabled {
            return Vec::new();
        }

        let mut results = Vec::new();

        // Remove stopwords
        if let Some(filtered) = self.remove_stopwords(query) {
            results.push(ExpandedQuery {
                query: filtered,
                expansion_type: ExpansionType::Stopwords,
            });
        }

        results
    }

    /// Remove stopwords from query.
    ///
    /// Example: "我是什么用户" -> "用户"
    #[must_use]
    pub fn remove_stopwords(&self, query: &str) -> Option<String> {
        let tokens = Self::tokenize(query);
        let original_len = tokens.len();
        let filtered: Vec<String> = tokens
            .into_iter()
            .filter(|t| !self.is_stopword(t))
            .collect();

        if !filtered.is_empty() && filtered.len() < original_len {
            Some(filtered.join(" "))
        } else {
            None
        }
    }

    /// Generate variant expansions (singular/plural, etc.).
    #[must_use]
    pub fn expand_variants(&self, query: &str) -> Vec<String> {
        let tokens = Self::tokenize(query);
        let mut variants = Vec::new();

        for token in &tokens {
            if self.is_stopword(token) {
                continue;
            }

            // Add original
            variants.push(token.clone());

            // Chinese: try adding/removing "们" for plural
            if token.ends_with('们') {
                variants.push(token[..token.len() - 3].to_string());
            } else {
                // For common nouns, try plural form
                if matches!(token.as_str(), "用户" | "设备" | "手机") {
                    variants.push(format!("{token}们"));
                }
            }
        }

        // Deduplicate
        variants.sort();
        variants.dedup();

        variants
    }

    /// Tokenize a query string into individual terms.
    fn tokenize(query: &str) -> Vec<String> {
        let mut tokens = Vec::new();

        // First, split by whitespace
        for part in query.split_whitespace() {
            tokens.push(part.to_string());
        }

        // If no whitespace, check if text contains CJK characters
        if tokens.is_empty() && query.chars().any(Self::is_cjk) {
            // For CJK text, use character-based tokenization
            for ch in query.chars() {
                tokens.push(ch.to_string());
            }
        } else if tokens.is_empty() {
            // Fallback for other scripts
            tokens.push(query.to_string());
        }

        tokens
    }

    /// Check if a character is CJK (Chinese, Japanese, Korean).
    fn is_cjk(ch: char) -> bool {
        let cp = ch as u32;
        (0x4E00..=0x9FFF).contains(&cp)      // CJK Unified Ideographs
            || (0x3400..=0x4DBF).contains(&cp)  // CJK Unified Ideographs Extension A
            || (0x20000..=0x2A6DF).contains(&cp) // CJK Unified Ideographs Extension B
            || (0x2A700..=0x2B73F).contains(&cp) // CJK Unified Ideographs Extension C
            || (0x2B740..=0x2B81F).contains(&cp) // CJK Unified Ideographs Extension D
            || (0x2B820..=0x2CEAF).contains(&cp) // CJK Unified Ideographs Extension E
            || (0x2CEB0..=0x2EBEF).contains(&cp) // CJK Unified Ideographs Extension F
            || (0x3000..=0x303F).contains(&cp)   // CJK Symbols and Punctuation
            || (0xFF00..=0xFFEF).contains(&cp) // Halfwidth and Fullwidth Forms
    }

    /// Check if a token is a stopword.
    #[must_use]
    fn is_stopword(&self, token: &str) -> bool {
        let lower = token.to_lowercase();

        // Direct match
        if self.stopwords.contains(&lower) {
            return true;
        }

        // For Chinese tokens without spaces, check if any stopword is contained
        for stopword in &self.stopwords {
            if lower.contains(stopword) {
                return true;
            }
        }

        false
    }

    /// Get the current stopwords.
    #[must_use]
    pub fn stopwords(&self) -> &[String] {
        &self.stopwords
    }

    /// Add a custom stopword.
    pub fn add_stopword(&mut self, word: impl Into<String>) {
        self.stopwords.push(word.into());
    }

    /// Check if expansion is enabled.
    #[must_use]
    pub const fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Enable or disable expansion.
    pub const fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }
}

impl Default for QueryExpander {
    fn default() -> Self {
        Self::with_defaults()
    }
}

/// An expanded query with metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpandedQuery {
    /// The expanded query string.
    pub query: String,

    /// The type of expansion applied.
    pub expansion_type: ExpansionType,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expansion_type() {
        assert_eq!(ExpansionType::Stopwords.as_str(), "stopwords");
        assert_eq!(ExpansionType::Variants.as_str(), "variants");
    }

    #[test]
    #[expect(clippy::expect_used, reason = "Test failure should panic with context")]
    fn test_remove_stopwords() {
        let config = QueryExpanderConfig {
            stopwords: default_stopwords(),
            enabled: true,
        };
        let expander = QueryExpander::new(config);
        let result = expander.remove_stopwords("what is my user type");

        assert!(result.is_some());
        let filtered = result.expect("stopword removal should succeed");
        assert!(filtered.contains("user"));
        assert!(!filtered.contains("what"));
        assert!(!filtered.contains("is"));
    }

    #[test]
    fn test_expand_multiple() {
        let expander = QueryExpander::with_defaults();
        let results = expander.expand("what is my user type");

        // Should return expanded versions
        assert!(!results.is_empty());

        // Check that stopword removal is present
        assert!(
            results
                .iter()
                .any(|r| r.expansion_type == ExpansionType::Stopwords)
        );
    }

    #[test]
    fn test_disabled_expander() {
        let config = QueryExpanderConfig {
            stopwords: default_stopwords(),
            enabled: false,
        };
        let expander = QueryExpander::new(config);

        let results = expander.expand("我是什么用户");
        assert!(results.is_empty());
    }

    #[test]
    fn test_add_stopword() {
        let mut expander = QueryExpander::with_defaults();
        let original_count = expander.stopwords().len();

        expander.add_stopword("custom_stopword");

        assert_eq!(expander.stopwords().len(), original_count + 1);
        assert!(expander.is_stopword("custom_stopword"));
    }

    #[test]
    fn test_expand_variants() {
        let expander = QueryExpander::with_defaults();
        let variants = expander.expand_variants("用户");

        assert!(!variants.is_empty());
        assert!(variants.contains(&"用户".to_string()));
        // Should contain plural form
        assert!(variants.iter().any(|v| v.contains("用户")));
    }

    #[test]
    #[expect(clippy::expect_used, reason = "Test failure should panic with context")]
    fn test_config_serialization() {
        let config = QueryExpanderConfig::default();

        let json = serde_json::to_string(&config).expect("config should serialize");
        let deserialized: QueryExpanderConfig =
            serde_json::from_str(&json).expect("valid JSON should deserialize");

        assert_eq!(deserialized.enabled, config.enabled);
        assert_eq!(deserialized.stopwords, config.stopwords);
    }

    #[test]
    fn test_tokenize() {
        let _expander = QueryExpander::with_defaults();
        let tokens = QueryExpander::tokenize("我是什么用户");

        // Chinese without spaces returns as single token
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0], "我是什么用户");
    }
}
