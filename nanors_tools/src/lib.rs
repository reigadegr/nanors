pub mod apply_patch;
pub mod bash;
pub mod command_runner;
pub mod glob;
pub mod grep;
pub mod path_guard;
pub mod read_file;
pub mod web_fetch;

// Re-export tool types for convenience
pub use apply_patch::ApplyPatchTool;
pub use bash::BashTool;
pub use glob::GlobTool;
pub use grep::GrepTool;
pub use read_file::ReadFileTool;
pub use web_fetch::{WebFetchConfig, WebFetchTool};

use std::path::{Path, PathBuf};
use std::time::Instant;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::json;

/// Working directory isolation mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkingDirIsolation {
    /// All sessions share the same working directory
    Shared,
    /// Each chat/session has its own isolated directory
    Chat,
}

/// Tool definition for LLM tool calling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// Result of tool execution
#[derive(Debug, Clone)]
pub struct ToolResult {
    pub content: String,
    pub is_error: bool,
    pub status_code: Option<i32>,
    pub bytes: usize,
    pub duration_ms: Option<u128>,
    pub error_type: Option<String>,
}

impl ToolResult {
    pub fn success(content: impl Into<String>) -> Self {
        let content = content.into();
        let bytes = content.len();
        Self {
            content,
            is_error: false,
            status_code: Some(0),
            bytes,
            duration_ms: None,
            error_type: None,
        }
    }

    pub fn error(content: impl Into<String>) -> Self {
        let content = content.into();
        let bytes = content.len();
        Self {
            content,
            is_error: true,
            status_code: Some(1),
            bytes,
            duration_ms: None,
            error_type: Some("tool_error".to_string()),
        }
    }

    #[must_use]
    pub fn with_status_code(mut self, status_code: i32) -> Self {
        self.status_code = Some(status_code);
        self
    }

    #[must_use]
    pub fn with_error_type(mut self, error_type: impl Into<String>) -> Self {
        self.error_type = Some(error_type.into());
        self
    }
}

/// Tool trait
#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn definition(&self) -> ToolDefinition;
    async fn execute(&self, input: serde_json::Value) -> ToolResult;
}

/// Tool registry
pub struct ToolRegistry {
    tools: Vec<Box<dyn Tool>>,
}

impl ToolRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self { tools: Vec::new() }
    }

    /// Create a new registry with all default tools registered.
    ///
    /// # Panics
    /// Panics if HTTP client creation for `WebFetchTool` fails.
    #[must_use]
    pub fn with_default_tools(working_dir: &str) -> Self {
        let mut registry = Self::new();
        registry.add_tool(Box::new(BashTool::new(working_dir)));
        registry.add_tool(Box::new(ReadFileTool::new(working_dir)));
        registry.add_tool(Box::new(ApplyPatchTool::new(working_dir)));
        registry.add_tool(Box::new(GlobTool::new(working_dir)));
        registry.add_tool(Box::new(GrepTool::new(working_dir)));
        registry.add_tool(Box::new(
            WebFetchTool::new(WebFetchConfig::default()).expect("Failed to create WebFetchTool"),
        ));
        registry
    }

    pub fn add_tool(&mut self, tool: Box<dyn Tool>) {
        self.tools.push(tool);
    }

    #[must_use]
    pub fn definitions(&self) -> Vec<ToolDefinition> {
        self.tools.iter().map(|t| t.definition()).collect()
    }

    pub async fn execute(&self, name: &str, input: serde_json::Value) -> ToolResult {
        for tool in &self.tools {
            if tool.name() == name {
                let started = Instant::now();
                let mut result = tool.execute(input).await;
                result.duration_ms = Some(started.elapsed().as_millis());
                result.bytes = result.content.len();
                if result.is_error && result.error_type.is_none() {
                    result.error_type = Some("tool_error".to_string());
                }
                if result.status_code.is_none() {
                    result.status_code = Some(i32::from(result.is_error));
                }
                return result;
            }
        }
        ToolResult::error(format!("Unknown tool: {name}")).with_error_type("unknown_tool")
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Resolve tool path
#[must_use]
pub fn resolve_tool_path(working_dir: &Path, path: &str) -> PathBuf {
    let candidate = PathBuf::from(path);
    if candidate.is_absolute() {
        candidate
    } else {
        working_dir.join(candidate)
    }
}

/// Sanitize channel segment for directory names
fn sanitize_channel_segment(channel: &str) -> String {
    let mut out = String::with_capacity(channel.len());
    for c in channel.chars() {
        if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
            out.push(c.to_ascii_lowercase());
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        "unknown".to_string()
    } else {
        out
    }
}

/// Build chat working directory
fn chat_working_dir(base_working_dir: &Path, channel: &str, chat_id: i64) -> PathBuf {
    let chat_segment = if chat_id < 0 {
        format!("neg{}", chat_id.unsigned_abs())
    } else {
        chat_id.to_string()
    };
    base_working_dir
        .join("chat")
        .join(sanitize_channel_segment(channel))
        .join(chat_segment)
}

const AUTH_CONTEXT_KEY: &str = "__nanors_auth";

/// Auth context for tool authorization
#[derive(Debug, Clone)]
pub struct ToolAuthContext {
    pub caller_channel: String,
    pub caller_chat_id: i64,
    pub control_chat_ids: Vec<i64>,
}

impl ToolAuthContext {
    #[must_use]
    pub fn is_control_chat(&self) -> bool {
        self.control_chat_ids.contains(&self.caller_chat_id)
    }

    #[must_use]
    pub fn can_access_chat(&self, target_chat_id: i64) -> bool {
        self.is_control_chat() || self.caller_chat_id == target_chat_id
    }
}

/// Extract auth context from input
#[must_use]
pub fn auth_context_from_input(input: &serde_json::Value) -> Option<ToolAuthContext> {
    let ctx = input.get(AUTH_CONTEXT_KEY)?;
    let caller_channel = ctx
        .get("caller_channel")
        .and_then(|v| v.as_str())
        .unwrap_or("cli")
        .to_string();
    let caller_chat_id = ctx.get("caller_chat_id")?.as_i64()?;
    let control_chat_ids = ctx
        .get("control_chat_ids")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(serde_json::Value::as_i64).collect())
        .unwrap_or_default();
    Some(ToolAuthContext {
        caller_channel,
        caller_chat_id,
        control_chat_ids,
    })
}

/// Resolve tool working directory
#[must_use]
pub fn resolve_tool_working_dir(
    base_working_dir: &Path,
    isolation: WorkingDirIsolation,
    input: &serde_json::Value,
) -> PathBuf {
    let resolved = match isolation {
        WorkingDirIsolation::Shared => base_working_dir.to_path_buf(),
        WorkingDirIsolation::Chat => auth_context_from_input(input).map_or_else(
            || base_working_dir.to_path_buf(),
            |auth| chat_working_dir(base_working_dir, &auth.caller_channel, auth.caller_chat_id),
        ),
    };
    let _ = std::fs::create_dir_all(&resolved);
    resolved
}

/// Helper to build JSON schema
#[must_use]
#[allow(clippy::needless_pass_by_value)]
pub fn schema_object(properties: serde_json::Value, required: &[&str]) -> serde_json::Value {
    json!({
        "type": "object",
        "properties": properties,
        "required": required,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_result_success() {
        let r = ToolResult::success("ok".to_string());
        assert_eq!(r.content, "ok");
        assert!(!r.is_error);
    }

    #[test]
    fn test_tool_result_error() {
        let r = ToolResult::error("fail".to_string());
        assert_eq!(r.content, "fail");
        assert!(r.is_error);
    }

    #[test]
    fn test_schema_object() {
        let schema = schema_object(
            json!({
                "name": {"type": "string"},
                "age": {"type": "integer"}
            }),
            &["name"],
        );
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["name"].is_object());
    }

    #[test]
    fn test_resolve_tool_working_dir_shared() {
        let dir = resolve_tool_working_dir(
            Path::new("/tmp/work"),
            WorkingDirIsolation::Shared,
            &json!({}),
        );
        assert_eq!(dir, PathBuf::from("/tmp/work"));
    }
}
