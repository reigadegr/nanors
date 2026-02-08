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
use tracing::debug;

use crate::{ChatMessage, LLMProvider, Role};

/// Default system prompt for sufficiency checking
pub const DEFAULT_SUFFICIENCY_SYSTEM_PROMPT: &str = r"
# Task Objective
Determine whether the current retrieved content is sufficient to answer the user's query.

# Rules
Return RETRIEVE if more information is needed, otherwise return NO_RETRIEVE.

# Output Format
<decision>RETRIEVE or NO_RETRIEVE</decision>
";

/// Default user prompt template for sufficiency checking
pub const DEFAULT_SUFFICIENCY_USER_PROMPT: &str = r"
User Query: {query}

Retrieved Content:
{retrieved_content}
";

/// Result of sufficiency check
#[derive(Debug, Clone)]
pub struct SufficiencyResult {
    /// Whether more retrieval is needed
    pub needs_more: bool,
    /// Potentially rewritten query for better retrieval
    pub rewritten_query: String,
}

/// Parse a sufficiency check response to extract the decision
///
/// # Arguments
/// * `response` - The LLM response text
///
/// # Returns
/// * Tuple of (needs_more, rewritten_query)
fn parse_sufficiency_response(response: &str) -> (bool, String) {
    let response_lower = response.to_lowercase();

    // Look for the decision tag
    if response_lower.contains("<decision>") {
        if let Some(start) = response_lower.find("<decision>") {
            if let Some(end) = response_lower.find("</decision>") {
                let decision = &response_lower[start + 10..end];
                // Check for NO_RETRIEVE first (more specific)
                if decision.contains("no_retrieve") || decision.contains("no retrieve") {
                    return (false, String::new());
                }
                let needs_more = decision.contains("retrieve");
                return (needs_more, String::new());
            }
        }
    }

    // Fallback: check for keywords
    // Check for NO_RETRIEVE first (more specific)
    if response_lower.contains("no_retrieve") || response_lower.contains("no retrieve") {
        return (false, String::new());
    }
    let needs_more = response_lower.contains("retrieve");

    (needs_more, String::new())
}

/// Trait for checking if retrieved content is sufficient to answer a query
#[async_trait]
pub trait SufficiencyChecker: Send + Sync {
    /// Check if the retrieved content is sufficient
    ///
    /// # Arguments
    /// * `query` - The user's original query
    /// * `retrieved_content` - The content retrieved so far
    ///
    /// # Returns
    /// * `SufficiencyResult` - Whether more retrieval is needed and potentially rewritten query
    async fn check(
        &self,
        query: &str,
        retrieved_content: &str,
    ) -> anyhow::Result<SufficiencyResult>;
}

/// LLM-based sufficiency checker
pub struct LLMSufficiencyChecker<P>
where
    P: LLMProvider + Send + Sync,
{
    provider: P,
    model: String,
    system_prompt: String,
    user_prompt_template: String,
}

impl<P> LLMSufficiencyChecker<P>
where
    P: LLMProvider + Send + Sync,
{
    /// Create a new LLM-based sufficiency checker
    #[must_use]
    pub fn new(provider: P) -> Self {
        Self {
            provider,
            model: "glm-4-flash".to_string(),
            system_prompt: DEFAULT_SUFFICIENCY_SYSTEM_PROMPT.to_string(),
            user_prompt_template: DEFAULT_SUFFICIENCY_USER_PROMPT.to_string(),
        }
    }

    /// Set the model to use for sufficiency checking
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

    /// Set a custom user prompt template
    ///
    /// The template should contain {query} and {retrieved_content} placeholders
    #[must_use]
    pub fn with_user_prompt_template(mut self, template: String) -> Self {
        self.user_prompt_template = template;
        self
    }

    /// Parse the LLM response to extract the decision
    fn parse_response(&self, response: &str) -> (bool, String) {
        parse_sufficiency_response(response)
    }
}

#[async_trait]
impl<P> SufficiencyChecker for LLMSufficiencyChecker<P>
where
    P: LLMProvider + Send + Sync,
{
    async fn check(
        &self,
        query: &str,
        retrieved_content: &str,
    ) -> anyhow::Result<SufficiencyResult> {
        // If no content was retrieved, we definitely need more
        if retrieved_content.trim().is_empty() {
            debug!("No retrieved content, indicating need for more retrieval");
            return Ok(SufficiencyResult {
                needs_more: true,
                rewritten_query: query.to_string(),
            });
        }

        let user_prompt = self
            .user_prompt_template
            .replace("{query}", query)
            .replace("{retrieved_content}", retrieved_content);

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

        debug!("Sufficiency check response: {}", response.content);

        let (needs_more, rewritten_query) = self.parse_response(&response.content);

        debug!("Sufficiency check result: needs_more={}", needs_more);

        Ok(SufficiencyResult {
            needs_more,
            rewritten_query: if rewritten_query.is_empty() {
                query.to_string()
            } else {
                rewritten_query
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_response_retrieve() {
        let response = "<decision>RETRIEVE</decision>";
        let (needs_more, _) = parse_sufficiency_response(response);
        assert!(needs_more);
    }

    #[test]
    fn test_parse_response_no_retrieve() {
        let response = "<decision>NO_RETRIEVE</decision>";
        let (needs_more, _) = parse_sufficiency_response(response);
        assert!(!needs_more);
    }

    #[test]
    fn test_parse_response_fallback() {
        let response = "We need to RETRIEVE more information";
        let (needs_more, _) = parse_sufficiency_response(response);
        assert!(needs_more);
    }

    #[test]
    fn test_parse_response_fallback_negative() {
        let response = "We have NO_RETRIEVE, this is sufficient";
        let (needs_more, _) = parse_sufficiency_response(response);
        assert!(!needs_more);
    }
}
