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

use async_trait::async_trait;
use std::collections::HashSet;
use std::sync::OnceLock;
use tracing::debug;
use uuid::Uuid;

use crate::{ChatMessage, LLMProvider, Role};

/// Reference pattern for extracting item references from text
/// Format: [`ref:ITEM_ID`] or [ref:ID1,ID2,ID3]
static REF_PATTERN: OnceLock<regex::Regex> = OnceLock::new();

/// Get the reference pattern regex
#[expect(
    clippy::expect_used,
    reason = "Static regex pattern validated at compile time"
)]
fn ref_pattern() -> &'static regex::Regex {
    REF_PATTERN.get_or_init(|| {
        regex::Regex::new(r"\[ref:([a-zA-Z0-9_,\-]+)\]")
            .expect("Static regex pattern is guaranteed to be valid")
    })
}

/// Extract item references from text in the format [`ref:ITEM_ID`]
///
/// # Arguments
/// * `text` - Text containing references
///
/// # Returns
/// * Vector of unique item IDs in the order they appear
#[must_use]
pub fn extract_references(text: &str) -> Vec<String> {
    let mut ids = Vec::new();
    let mut seen = HashSet::new();

    for cap in ref_pattern().captures_iter(text) {
        if let Some(ids_str) = cap.get(1) {
            for id in ids_str.as_str().split(',') {
                let id = id.trim();
                if seen.insert(id.to_string()) {
                    ids.push(id.to_string());
                }
            }
        }
    }

    ids
}

/// Build a short ID from a UUID (first 6 characters without hyphens)
///
/// # Arguments
/// * `uuid` - Full UUID
///
/// # Returns
/// * Short ID string (6 characters)
#[must_use]
pub fn build_short_id(uuid: &Uuid) -> String {
    let uuid_str = uuid.to_string().replace('-', "");
    uuid_str[..uuid_str.len().min(6)].to_string()
}

/// Build a prompt for category summary compression
///
/// # Arguments
/// * `category_name` - Name of the category
/// * `current_summary` - Current summary text
/// * `new_items` - New items to merge (`item_id`, summary) pairs
/// * `target_length` - Target length in tokens
///
/// # Returns
/// * Prompt string for the LLM
#[must_use]
pub fn build_category_summary_prompt(
    category_name: &str,
    current_summary: &str,
    new_items: &[(Uuid, String)],
    target_length: usize,
) -> String {
    let items_text = new_items
        .iter()
        .map(|(id, summary)| format!("- [{}] {}", build_short_id(id), summary))
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        r"# Task
Merge new memory items into the existing category summary.

# Category
{category_name}

# Current Summary
{current_summary}

# New Items to Merge
{items_text}

# Requirements
- Maximum {target_length} tokens
- Include [ref:SHORT_ID] citations for merged items
- Output only the updated summary in markdown format
",
        category_name = category_name,
        current_summary = if current_summary.is_empty() {
            "(empty)"
        } else {
            current_summary
        },
        items_text = if items_text.is_empty() {
            "(none)"
        } else {
            &items_text
        },
        target_length = target_length
    )
}

/// Result of category compression
#[derive(Debug, Clone)]
pub struct CompressionResult {
    /// The compressed summary text
    pub summary: String,
    /// IDs of items referenced in the summary
    pub referenced_item_ids: Vec<String>,
}

/// Trait for compressing category summaries
#[async_trait]
pub trait CategoryCompressor: Send + Sync {
    /// Compress a category summary by merging new items
    ///
    /// # Arguments
    /// * `category_name` - Name of the category
    /// * `current_summary` - Current summary text
    /// * `new_items` - New items to merge (`item_id`, summary) pairs
    /// * `target_length` - Target length in tokens
    ///
    /// # Returns
    /// * Compression result with new summary and referenced IDs
    async fn compress_category_summary(
        &self,
        category_name: &str,
        current_summary: &str,
        new_items: &[(Uuid, String)],
        target_length: usize,
    ) -> anyhow::Result<CompressionResult>;
}

/// Default system prompt for category summarization
const DEFAULT_SUMMARIZATION_SYSTEM_PROMPT: &str = r"
# Task
You are a memory summarization expert. Your job is to merge new memory items into an existing category summary while maintaining coherence and staying within the token limit.

# Rules
1. Preserve the most important information from both the current summary and new items
2. Use [ref:SHORT_ID] citations to reference specific items
3. Output ONLY the updated summary in markdown format
4. Stay within the specified token limit
5. Maintain a coherent narrative flow
";

/// LLM-based category compressor
pub struct LLMAbstractor<P>
where
    P: LLMProvider + Send + Sync,
{
    provider: P,
    model: String,
    system_prompt: String,
}

impl<P> LLMAbstractor<P>
where
    P: LLMProvider + Send + Sync,
{
    /// Create a new LLM-based category compressor
    #[must_use]
    pub fn new(provider: P) -> Self {
        Self {
            provider,
            model: "glm-4-flash".to_string(),
            system_prompt: DEFAULT_SUMMARIZATION_SYSTEM_PROMPT.to_string(),
        }
    }

    /// Set the model to use for compression
    #[must_use]
    pub fn with_model(mut self, model: String) -> Self {
        self.model = model;
        self
    }

    /// Set a custom system prompt
    #[must_use]
    pub fn with_system_prompt(mut self, prompt: String) -> Self {
        self.system_prompt = prompt;
        self
    }

    /// Parse the response to extract the summary and references
    fn parse_response(response: &str) -> CompressionResult {
        let referenced_item_ids = extract_references(response);
        debug!(
            "Extracted {} references from compression response",
            referenced_item_ids.len()
        );

        CompressionResult {
            summary: response.to_string(),
            referenced_item_ids,
        }
    }
}

#[async_trait]
impl<P> CategoryCompressor for LLMAbstractor<P>
where
    P: LLMProvider + Send + Sync,
{
    async fn compress_category_summary(
        &self,
        category_name: &str,
        current_summary: &str,
        new_items: &[(Uuid, String)],
        target_length: usize,
    ) -> anyhow::Result<CompressionResult> {
        let user_prompt =
            build_category_summary_prompt(category_name, current_summary, new_items, target_length);

        let messages = vec![
            ChatMessage {
                role: Role::System,
                content: self.system_prompt.clone(),
            },
            ChatMessage {
                role: Role::User,
                content: user_prompt,
            },
        ];

        let response = self.provider.chat(&messages, &self.model).await?;

        debug!(
            "Category compression response for '{}': {} chars",
            category_name,
            response.content.len()
        );

        Ok(Self::parse_response(&response.content))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_references_single() {
        let text = "Some text [ref:abc123] more text";
        let ids = extract_references(text);
        assert_eq!(ids, vec!["abc123"]);
    }

    #[test]
    fn test_extract_references_multiple() {
        let text = "Text [ref:abc123] and [ref:def456] more";
        let ids = extract_references(text);
        assert_eq!(ids, vec!["abc123", "def456"]);
    }

    #[test]
    fn test_extract_references_comma_separated() {
        let text = "Text [ref:abc123,def456,ghi789] more";
        let ids = extract_references(text);
        assert_eq!(ids, vec!["abc123", "def456", "ghi789"]);
    }

    #[test]
    fn test_extract_references_duplicates() {
        let text = "Text [ref:abc123] and [ref:abc123] more";
        let ids = extract_references(text);
        assert_eq!(ids, vec!["abc123"]); // No duplicates
    }

    #[test]
    fn test_build_short_id() {
        let uuid = Uuid::now_v7();
        let short_id = build_short_id(&uuid);
        assert_eq!(short_id.len(), 6);
    }

    #[test]
    fn test_build_category_summary_prompt() {
        let uuid1 = Uuid::now_v7();
        let uuid2 = Uuid::now_v7();
        let new_items = vec![
            (uuid1, "First item".to_string()),
            (uuid2, "Second item".to_string()),
        ];

        let prompt =
            build_category_summary_prompt("Test Category", "Existing summary", &new_items, 400);

        assert!(prompt.contains("Test Category"));
        assert!(prompt.contains("Existing summary"));
        assert!(prompt.contains("First item"));
        assert!(prompt.contains("Second item"));
        assert!(prompt.contains("400"));
    }
}
