//! Keyword-triggered memory versioning module
//! This module provides rule-based memory versioning without LLM

use chrono::{DateTime, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::memory::{MemoryItem, MemoryType};

/// Fact type for versioned memories
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FactType {
    /// User's residential address
    Address,
    /// User's nickname/preferred name
    Nickname,
    /// User's workplace/company
    Workplace,
    /// User's phone number
    PhoneNumber,
    /// User's email address
    Email,
    /// Generic fact update
    Generic,
}

impl FactType {
    /// Get the database string representation
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Address => "address",
            Self::Nickname => "nickname",
            Self::Workplace => "workplace",
            Self::PhoneNumber => "phone_number",
            Self::Email => "email",
            Self::Generic => "generic",
        }
    }

    /// Parse from database string
    #[must_use]
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "address" => Some(Self::Address),
            "nickname" => Some(Self::Nickname),
            "workplace" => Some(Self::Workplace),
            "phone_number" => Some(Self::PhoneNumber),
            "email" => Some(Self::Email),
            "generic" => Some(Self::Generic),
            _ => None,
        }
    }
}

/// Keyword pattern for detecting fact updates
#[derive(Debug, Clone)]
pub struct KeywordPattern {
    /// Fact type this pattern belongs to
    pub fact_type: FactType,
    /// Regex pattern to match
    pub pattern: &'static str,
    /// Priority (higher = checked first)
    pub priority: i32,
}

/// Keyword library for detecting fact updates
pub struct KeywordLibrary {
    /// Compiled regex patterns for statements (storage)
    statement_patterns: Vec<(Regex, FactType, i32)>,
    /// Compiled regex patterns for queries (retrieval)
    query_patterns: Vec<(Regex, FactType, i32)>,
}

impl KeywordLibrary {
    /// Create a new keyword library with default patterns
    #[must_use]
    pub fn new() -> Self {
        let (statement_patterns, query_patterns) = Self::default_patterns();

        let compiled_statements = statement_patterns
            .iter()
            .filter_map(|(fact_type, pattern, priority)| {
                Some((Regex::new(pattern).ok()?, *fact_type, *priority))
            })
            .collect();

        let compiled_queries = query_patterns
            .iter()
            .filter_map(|(fact_type, pattern, priority)| {
                Some((Regex::new(pattern).ok()?, *fact_type, *priority))
            })
            .collect();

        Self {
            statement_patterns: compiled_statements,
            query_patterns: compiled_queries,
        }
    }

    /// Get default keyword patterns
    /// Returns (statement_patterns, query_patterns)
    fn default_patterns() -> (
        Vec<(FactType, &'static str, i32)>,
        Vec<(FactType, &'static str, i32)>,
    ) {
        let statements = vec![
            // Address update patterns (highest priority)
            (
                FactType::Address,
                r"(?:搬家到|搬到|现住|居住在|住址改为).{1,20}",
                100,
            ),
            (FactType::Address, r"我住(?:在|是)?.{1,30}", 90),
            (FactType::Address, r"地址(?:是|改为).{1,50}", 80),
            // Nickname update patterns
            (FactType::Nickname, r"(?:改名为|叫我|昵称是).{1,20}", 100),
            (FactType::Nickname, r"我是(?:叫)?.{1,20}", 80),
            // Workplace update patterns
            (
                FactType::Workplace,
                r"(?:入职|就职于|工作单位改为|在.{1,20}工作)",
                100,
            ),
            (FactType::Workplace, r"公司是?.{1,30}", 80),
            // Phone number patterns
            (FactType::PhoneNumber, r"手机(?:号)?(?:是|改为).{11}", 100),
            (FactType::PhoneNumber, r"联系电话.{11}", 90),
            // Email patterns
            (FactType::Email, r"邮箱(?:是|改为).{1,50}", 100),
            // Generic update patterns (lowest priority)
            (FactType::Generic, r"(?:更换为|更新为|现在是).{1,50}", 50),
        ];

        let queries = vec![
            // Address query patterns
            (
                FactType::Address,
                r"(?:现在)?我住(?:哪里|哪儿|在哪|哪)|我现在住哪",
                70,
            ),
            (FactType::Address, r"地址(?:是|在哪|是什么)", 60),
            // Nickname query patterns
            (
                FactType::Nickname,
                r"(?:我叫|我的名字是|我的名字叫|我的昵称)",
                70,
            ),
            (FactType::Nickname, r"我叫什么", 60),
            // Workplace query patterns
            (
                FactType::Workplace,
                r"(?:我在哪工作|我在哪上班|我在哪就职)",
                70,
            ),
            (FactType::Workplace, r"我的公司(?:是|是什么)", 60),
            // Phone number query patterns
            (FactType::PhoneNumber, r"我的手机(?:号)?(?:是|是什么)", 70),
            (FactType::PhoneNumber, r"我的电话(?:号)?(?:是|是什么)", 60),
            // Email query patterns
            (FactType::Email, r"我的邮箱(?:是|是什么)", 70),
        ];

        (statements, queries)
    }

    /// Match input text against keyword patterns (for retrieval)
    /// Returns the detected fact type, if any
    #[must_use]
    pub fn match_fact_type(&self, text: &str) -> Option<FactType> {
        let text_lower = text.to_lowercase();

        // Check statement patterns first (higher priority)
        let mut sorted_statements = self.statement_patterns.clone();
        sorted_statements.sort_by(|a, b| b.2.cmp(&a.2));

        for (regex, fact_type, _priority) in &sorted_statements {
            if regex.is_match(&text_lower) {
                return Some(*fact_type);
            }
        }

        // Then check query patterns
        let mut sorted_queries = self.query_patterns.clone();
        sorted_queries.sort_by(|a, b| b.2.cmp(&a.2));

        for (regex, fact_type, _priority) in &sorted_queries {
            if regex.is_match(&text_lower) {
                return Some(*fact_type);
            }
        }

        None
    }

    /// Match input text against statement patterns only (for storage)
    /// Returns the detected fact type, if any
    #[must_use]
    pub fn match_statement_fact_type(&self, text: &str) -> Option<FactType> {
        let text_lower = text.to_lowercase();

        // Only check statement patterns
        let mut sorted_statements = self.statement_patterns.clone();
        sorted_statements.sort_by(|a, b| b.2.cmp(&a.2));

        for (regex, fact_type, _priority) in &sorted_statements {
            if regex.is_match(&text_lower) {
                return Some(*fact_type);
            }
        }

        None
    }

    /// Add a custom keyword pattern (adds to statement patterns)
    pub fn add_pattern(&mut self, fact_type: FactType, pattern: &'static str, priority: i32) {
        if let Ok(regex) = Regex::new(pattern) {
            self.statement_patterns.push((regex, fact_type, priority));
        }
    }
}

impl Default for KeywordLibrary {
    fn default() -> Self {
        Self::new()
    }
}

/// Versioning action to take
#[derive(Debug, Clone)]
pub enum VersioningAction {
    /// No versioning needed (assistant response or no fact detected)
    NoVersioning,
    /// Create new version for existing fact
    CreateVersion {
        fact_type: FactType,
        parent_id: Uuid,
    },
    /// Insert as new fact
    NewFact { fact_type: FactType },
    /// Insert as non-fact memory
    NonFact,
}

/// Result of versioning analysis
#[derive(Debug, Clone)]
pub struct VersioningResult {
    pub action: VersioningAction,
    pub fact_type: Option<FactType>,
}

/// Memory versioner for keyword-triggered versioning
pub struct MemoryVersioner {
    keyword_library: KeywordLibrary,
}

impl MemoryVersioner {
    /// Create a new memory versioner
    #[must_use]
    pub fn new() -> Self {
        Self {
            keyword_library: KeywordLibrary::new(),
        }
    }

    /// Create a new memory versioner with custom keyword library
    #[must_use]
    pub const fn with_library(keyword_library: KeywordLibrary) -> Self {
        Self { keyword_library }
    }

    /// Analyze a memory item and determine versioning action
    #[must_use]
    pub fn analyze(&self, item: &MemoryItem) -> VersioningResult {
        // Check if this is an assistant response
        if item.summary.starts_with("Assistant:") {
            return VersioningResult {
                action: VersioningAction::NoVersioning,
                fact_type: None,
            };
        }

        // Try to match fact type from user input (statement patterns only)
        if let Some(fact_type) = self
            .keyword_library
            .match_statement_fact_type(&item.summary)
        {
            VersioningResult {
                action: VersioningAction::NewFact {
                    fact_type: fact_type,
                },
                fact_type: Some(fact_type),
            }
        } else {
            // No fact detected, store as non-fact memory
            VersioningResult {
                action: VersioningAction::NonFact,
                fact_type: None,
            }
        }
    }

    /// Set keyword library
    pub fn set_keyword_library(&mut self, library: KeywordLibrary) {
        self.keyword_library = library;
    }

    /// Get reference to keyword library
    #[must_use]
    pub const fn keyword_library(&self) -> &KeywordLibrary {
        &self.keyword_library
    }
}

impl Default for MemoryVersioner {
    fn default() -> Self {
        Self::new()
    }
}

/// Memory item with versioning fields
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionedMemoryItem {
    pub id: Uuid,
    pub user_scope: String,
    pub resource_id: Option<Uuid>,
    pub memory_type: MemoryType,
    pub summary: String,
    pub embedding: Option<Vec<f32>>,
    pub happened_at: DateTime<Utc>,
    pub extra: Option<serde_json::Value>,
    pub content_hash: String,
    pub reinforcement_count: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub version: i32,
    pub parent_version_id: Option<Uuid>,
    pub version_relation: Option<String>,
    pub fact_type: Option<String>,
    pub is_active: bool,
    pub parent_id: Option<Uuid>,
}

impl From<MemoryItem> for VersionedMemoryItem {
    fn from(item: MemoryItem) -> Self {
        Self {
            id: item.id,
            user_scope: item.user_scope,
            resource_id: item.resource_id,
            memory_type: item.memory_type,
            summary: item.summary,
            embedding: item.embedding,
            happened_at: item.happened_at,
            extra: item.extra,
            content_hash: item.content_hash,
            reinforcement_count: item.reinforcement_count,
            created_at: item.created_at,
            updated_at: item.updated_at,
            version: item.version,
            parent_version_id: item.parent_version_id,
            version_relation: item.version_relation,
            fact_type: None,
            is_active: true,
            parent_id: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keyword_library_address_detection() {
        let library = KeywordLibrary::new();

        assert_eq!(
            library.match_fact_type("我搬家到了东城区"),
            Some(FactType::Address)
        );
        assert_eq!(
            library.match_fact_type("我现住朝阳区"),
            Some(FactType::Address)
        );
        assert_eq!(
            library.match_fact_type("住址改为海淀区"),
            Some(FactType::Address)
        );
    }

    #[test]
    fn test_keyword_library_nickname_detection() {
        let library = KeywordLibrary::new();

        assert_eq!(
            library.match_fact_type("叫我小明"),
            Some(FactType::Nickname)
        );
        assert_eq!(
            library.match_fact_type("昵称是大伟"),
            Some(FactType::Nickname)
        );
    }

    #[test]
    fn test_keyword_library_workplace_detection() {
        let library = KeywordLibrary::new();

        assert_eq!(
            library.match_fact_type("我入职了字节跳动"),
            Some(FactType::Workplace)
        );
        assert_eq!(
            library.match_fact_type("就职于腾讯公司"),
            Some(FactType::Workplace)
        );
    }

    #[test]
    fn test_keyword_library_no_match() {
        let library = KeywordLibrary::new();

        assert_eq!(library.match_fact_type("今天天气很好"), None);
        assert_eq!(library.match_fact_type("你好"), None);
    }

    #[test]
    fn test_keyword_library_address_query_detection() {
        let library = KeywordLibrary::new();

        assert_eq!(library.match_fact_type("我住哪里"), Some(FactType::Address));
        assert_eq!(
            library.match_fact_type("我现在住哪"),
            Some(FactType::Address)
        );
    }

    #[test]
    fn test_keyword_library_nickname_query_detection() {
        let library = KeywordLibrary::new();

        assert_eq!(
            library.match_fact_type("我叫什么"),
            Some(FactType::Nickname)
        );
        assert_eq!(
            library.match_fact_type("我的名字叫"),
            Some(FactType::Nickname)
        );
        assert_eq!(
            library.match_fact_type("我的昵称"),
            Some(FactType::Nickname)
        );
    }

    #[test]
    fn test_keyword_library_workplace_query_detection() {
        let library = KeywordLibrary::new();

        assert_eq!(
            library.match_fact_type("我在哪工作"),
            Some(FactType::Workplace)
        );
        assert_eq!(
            library.match_fact_type("我的公司是"),
            Some(FactType::Workplace)
        );
    }

    #[test]
    fn test_memory_versioner_analyze_assistant() {
        let versioner = MemoryVersioner::new();
        let item = MemoryItem {
            id: Uuid::now_v7(),
            user_scope: "test".to_string(),
            resource_id: None,
            memory_type: MemoryType::Episodic,
            summary: "Assistant: 好的，我记住了".to_string(),
            embedding: None,
            happened_at: Utc::now(),
            extra: None,
            content_hash: "abc123".to_string(),
            reinforcement_count: 0,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            version: 1,
            parent_version_id: None,
            version_relation: None,
            fact_type: None,
            is_active: true,
            parent_id: None,
        };

        let result = versioner.analyze(&item);
        assert!(matches!(result.action, VersioningAction::NoVersioning));
        assert!(result.fact_type.is_none());
    }

    #[test]
    fn test_memory_versioner_analyze_user_address() {
        let versioner = MemoryVersioner::new();
        let item = MemoryItem {
            id: Uuid::now_v7(),
            user_scope: "test".to_string(),
            resource_id: None,
            memory_type: MemoryType::Episodic,
            summary: "User: 我搬家到了东城区".to_string(),
            embedding: None,
            happened_at: Utc::now(),
            extra: None,
            content_hash: "abc123".to_string(),
            reinforcement_count: 0,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            version: 1,
            parent_version_id: None,
            version_relation: None,
            fact_type: None,
            is_active: true,
            parent_id: None,
        };

        let result = versioner.analyze(&item);
        assert!(matches!(
            result.action,
            VersioningAction::NewFact {
                fact_type: FactType::Address
            }
        ));
        assert_eq!(result.fact_type, Some(FactType::Address));
    }

    #[test]
    fn test_memory_versioner_analyze_non_fact() {
        let versioner = MemoryVersioner::new();
        let item = MemoryItem {
            id: Uuid::now_v7(),
            user_scope: "test".to_string(),
            resource_id: None,
            memory_type: MemoryType::Episodic,
            summary: "User: 今天天气很好".to_string(),
            embedding: None,
            happened_at: Utc::now(),
            extra: None,
            content_hash: "abc123".to_string(),
            reinforcement_count: 0,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            version: 1,
            parent_version_id: None,
            version_relation: None,
            fact_type: None,
            is_active: true,
            parent_id: None,
        };

        let result = versioner.analyze(&item);
        assert!(matches!(result.action, VersioningAction::NonFact));
        assert!(result.fact_type.is_none());
    }
}
