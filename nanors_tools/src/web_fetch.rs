#![deny(
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

use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

use super::{Tool, ToolDefinition, ToolResult, schema_object};

/// Web fetch tool configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebFetchConfig {
    /// Request timeout (seconds)
    #[serde(default = "WebFetchConfig::default_timeout")]
    pub timeout: u64,

    /// User-Agent header
    #[serde(default = "WebFetchConfig::default_user_agent")]
    pub user_agent: String,

    /// Maximum response size (bytes)
    #[serde(default = "WebFetchConfig::default_max_size")]
    pub max_size: usize,
}

impl WebFetchConfig {
    const fn default_timeout() -> u64 {
        10
    }

    fn default_user_agent() -> String {
        "Mozilla/5.0 (compatible; nanors/1.0)".to_string()
    }

    const fn default_max_size() -> usize {
        1_000_000 // 1MB
    }
}

impl Default for WebFetchConfig {
    fn default() -> Self {
        Self {
            timeout: Self::default_timeout(),
            user_agent: Self::default_user_agent(),
            max_size: Self::default_max_size(),
        }
    }
}

/// Web fetch tool
pub struct WebFetchTool {
    client: Client,
    config: WebFetchConfig,
}

impl WebFetchTool {
    pub fn new(config: WebFetchConfig) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout))
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self { client, config })
    }
}

#[async_trait]
impl Tool for WebFetchTool {
    fn name(&self) -> &'static str {
        "web_fetch"
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: self.name().to_string(),
            description: "Fetch a web page and convert to simplified text. \
                Supports http and https URLs."
                .to_string(),
            input_schema: schema_object(
                serde_json::json!({
                    "url": {
                        "type": "string",
                        "description": "The URL to fetch (http or https only)"
                    }
                }),
                &["url"],
            ),
        }
    }

    async fn execute(&self, input: serde_json::Value) -> ToolResult {
        // Extract URL parameter
        let Some(url) = input.get("url").and_then(|v| v.as_str()) else {
            return ToolResult::error("Missing required parameter: url");
        };

        // Validate URL
        let parsed = match url::Url::parse(url) {
            Ok(u) => u,
            Err(e) => {
                return ToolResult::error(format!("Invalid URL: {e}"))
                    .with_error_type("invalid_url");
            }
        };

        // Only support HTTP/HTTPS
        if !matches!(parsed.scheme(), "http" | "https") {
            return ToolResult::error("Only http and https URLs are supported")
                .with_error_type("unsupported_scheme");
        }

        // Send request
        let response = match self
            .client
            .get(url)
            .header("User-Agent", &self.config.user_agent)
            .header("Accept", "text/html, text/markdown, text/plain")
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                return ToolResult::error(format!("HTTP request failed: {e}"))
                    .with_error_type("http_error");
            }
        };

        let status = response.status();
        let headers = response.headers().clone();

        // Get content type
        let content_type = headers
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("application/octet-stream");

        // Limit read size
        let bytes = match response.bytes().await {
            Ok(b) => b,
            Err(e) => {
                return ToolResult::error(format!("Failed to read response: {e}"))
                    .with_error_type("read_error");
            }
        };

        if bytes.len() > self.config.max_size {
            return ToolResult::error(format!(
                "Response too large: {} bytes (max: {})",
                bytes.len(),
                self.config.max_size
            ))
            .with_error_type("size_exceeded");
        }

        // Convert content
        let content = if content_type.contains("html") {
            html_to_text(&bytes)
        } else {
            String::from_utf8_lossy(&bytes).to_string()
        };

        // Truncate long content
        let content = if content.len() > 10_000 {
            format!("{}\n\n... (truncated at 10000 chars)", &content[..10_000])
        } else {
            content
        };

        ToolResult::success(content).with_status_code(i32::from(status.as_u16()))
    }
}

/// Convert HTML to plain text (simplified)
fn html_to_text(bytes: &[u8]) -> String {
    let html = String::from_utf8_lossy(bytes);

    // Remove script and style tags
    let html = remove_tag(&html, "script");
    let html = remove_tag(&html, "style");

    // Simple text extraction
    let text = html
        .replace("<br>", "\n")
        .replace("<br/>", "\n")
        .replace("<br />", "\n")
        .replace("</p>", "\n\n")
        .replace("</div>", "\n")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");

    text.chars()
        .collect::<Vec<_>>()
        .chunks(100)
        .map(|chunk| chunk.iter().collect())
        .collect::<Vec<String>>()
        .join("\n")
}

/// Remove an HTML tag and its content from the string
fn remove_tag(html: &str, tag: &str) -> String {
    let start = format!("<{tag}>");
    let end_start = format!("</{tag}>");
    let end_self = format!("<{tag} ");

    let mut result = html.to_string();
    let mut pos = 0;

    while pos < result.len() {
        if let Some(idx) = result[pos..].find(&start) {
            let start_pos = pos + idx;
            // Find closing tag and remove everything between start and end
            if let Some(end_idx) = result[start_pos..].find(&end_start) {
                result.replace_range(start_pos..(start_pos + end_idx + end_start.len()), " ");
            } else if let Some(self_idx) = result[start_pos..].find(&end_self) {
                if let Some(closer) = result[start_pos + self_idx..].find('>') {
                    result.replace_range(start_pos..=(start_pos + self_idx + closer), " ");
                }
            }
            pos += 1;
        } else {
            break;
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_web_fetch_config_default() {
        let config = WebFetchConfig::default();
        assert_eq!(config.timeout, 10);
        assert_eq!(config.max_size, 1_000_000);
        assert!(config.user_agent.contains("nanors"));
    }

    #[test]
    fn test_web_fetch_tool_new() {
        let tool = WebFetchTool::new(WebFetchConfig::default());
        assert!(tool.is_ok());
    }

    #[test]
    fn test_web_fetch_definition() {
        let Ok(tool) = WebFetchTool::new(WebFetchConfig::default()) else {
            panic!("Failed to create WebFetchTool");
        };
        let def = tool.definition();
        assert_eq!(def.name, "web_fetch");
        assert!(def.description.contains("http"));
    }

    #[test]
    fn test_html_to_text() {
        let html = r"<html><body><h1>Title</h1><p>Hello <script>var x = 1;</script>world</p></body></html>";
        let text = html_to_text(html.as_bytes());
        assert!(!text.contains("script"));
        assert!(!text.contains("var x"));
        assert!(text.contains("Title") || text.contains("Hello") || text.contains("world"));
    }
}
