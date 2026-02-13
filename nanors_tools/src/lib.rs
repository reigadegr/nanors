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

/// Tool trait (internal use only)
#[async_trait]
trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn definition(&self) -> ToolDefinition;
    async fn execute(&self, input: serde_json::Value) -> ToolResult;
}

/// Static dispatch tool enum
///
/// Uses static dispatch instead of dynamic dispatch (trait objects)
/// for zero-cost abstraction and better performance.
/// Static dispatch tool enum
///
/// Uses static dispatch instead of dynamic dispatch (trait objects)
/// for zero-cost abstraction and better performance.
pub enum StaticTool {
    Bash(BashTool),
    ReadFile(ReadFileTool),
    ApplyPatch(ApplyPatchTool),
    Glob(GlobTool),
    Grep(GrepTool),
    WebFetch(WebFetchTool),
}

impl StaticTool {
    /// Fast name matching (compile-time optimization)
    pub fn name_str(&self) -> &str {
        match self {
            Self::Bash(_) => "bash",
            Self::ReadFile(_) => "read_file",
            Self::ApplyPatch(_) => "apply_patch",
            Self::Glob(_) => "glob",
            Self::Grep(_) => "grep",
            Self::WebFetch(_) => "web_fetch",
        }
    }

    /// Get tool definition (static dispatch)
    pub fn definition(&self) -> ToolDefinition {
        match self {
            Self::Bash(t) => t.definition(),
            Self::ReadFile(t) => t.definition(),
            Self::ApplyPatch(t) => t.definition(),
            Self::Glob(t) => t.definition(),
            Self::Grep(t) => t.definition(),
            Self::WebFetch(t) => t.definition(),
        }
    }

    /// Execute tool (static dispatch, can be inlined)
    pub async fn execute(&self, input: serde_json::Value) -> ToolResult {
        match self {
            Self::Bash(t) => t.execute(input).await,
            Self::ReadFile(t) => t.execute(input).await,
            Self::ApplyPatch(t) => t.execute(input).await,
            Self::Glob(t) => t.execute(input).await,
            Self::Grep(t) => t.execute(input).await,
            Self::WebFetch(t) => t.execute(input).await,
        }
    }
}

/// Static dispatch tool registry
///
/// Uses static dispatch via enum matching instead of dynamic dispatch
/// through trait objects. This eliminates vtable lookup overhead and
/// enables compiler optimizations like inlining.
pub struct StaticToolRegistry {
    tools: Vec<StaticTool>,
}

impl StaticToolRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self { tools: Vec::new() }
    }

    /// Create registry with all default tools
    ///
    /// # Panics
    /// Panics if HTTP client creation for `WebFetchTool` fails.
    #[must_use]
    pub fn with_default_tools(working_dir: &str) -> Self {
        Self {
            tools: vec![
                StaticTool::Bash(BashTool::new(working_dir)),
                StaticTool::ReadFile(ReadFileTool::new(working_dir)),
                StaticTool::ApplyPatch(ApplyPatchTool::new(working_dir)),
                StaticTool::Glob(GlobTool::new(working_dir)),
                StaticTool::Grep(GrepTool::new(working_dir)),
                StaticTool::WebFetch(
                    WebFetchTool::new(WebFetchConfig::default())
                        .expect("Failed to create WebFetchTool"),
                ),
            ],
        }
    }

    #[must_use]
    pub fn definitions(&self) -> Vec<ToolDefinition> {
        self.tools.iter().map(|t| t.definition()).collect()
    }

    pub async fn execute(&self, name: &str, input: serde_json::Value) -> ToolResult {
        let started = Instant::now();

        let result = match self.tools.iter().find(|t| t.name_str() == name) {
            Some(tool) => tool.execute(input).await,
            None => {
                return ToolResult::error(format!("Unknown tool: {name}"))
                    .with_error_type("unknown_tool");
            }
        };

        let mut result = result;
        result.duration_ms = Some(started.elapsed().as_millis());
        result.bytes = result.content.len();
        if result.is_error && result.error_type.is_none() {
            result.error_type = Some("tool_error".to_string());
        }
        if result.status_code.is_none() {
            result.status_code = Some(i32::from(result.is_error));
        }
        result
    }
}

impl Default for StaticToolRegistry {
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

/// Static dispatch tests
#[cfg(test)]
mod static_dispatch_tests {
    use super::*;

    #[tokio::test]
    async fn static_dispatch_bash() {
        let registry = StaticToolRegistry::with_default_tools(".");
        let result = registry
            .execute("bash", json!({"command": "echo hello"}))
            .await;
        assert!(!result.is_error);
        assert!(result.content.contains("hello"));
    }

    #[tokio::test]
    async fn static_dispatch_read_file() {
        let registry = StaticToolRegistry::with_default_tools(".");
        let result = registry
            .execute("read_file", json!({"path": "src/lib.rs"}))
            .await;
        if result.is_error {
            panic!("read_file failed: {}", result.content);
        }
        assert!(!result.is_error);
    }

    #[tokio::test]
    async fn static_dispatch_glob() {
        let registry = StaticToolRegistry::with_default_tools(".");
        let result = registry
            .execute("glob", json!({"pattern": "**/*.rs"}))
            .await;
        assert!(!result.is_error);
    }

    #[tokio::test]
    async fn static_dispatch_grep() {
        let registry = StaticToolRegistry::with_default_tools(".");
        let result = registry
            .execute("grep", json!({"pattern": "test", "path": "."}))
            .await;
        assert!(!result.is_error);
    }

    #[tokio::test]
    async fn static_dispatch_unknown_tool() {
        let registry = StaticToolRegistry::with_default_tools(".");
        let result = registry.execute("unknown", json!({})).await;
        assert!(result.is_error);
        assert_eq!(result.error_type.as_deref(), Some("unknown_tool"));
    }

    #[test]
    fn static_tool_name_str() {
        let bash = StaticTool::Bash(BashTool::new("."));
        assert_eq!(bash.name_str(), "bash");
        let read_file = StaticTool::ReadFile(ReadFileTool::new("."));
        assert_eq!(read_file.name_str(), "read_file");
        let apply_patch = StaticTool::ApplyPatch(ApplyPatchTool::new("."));
        assert_eq!(apply_patch.name_str(), "apply_patch");
        let glob = StaticTool::Glob(GlobTool::new("."));
        assert_eq!(glob.name_str(), "glob");
        let grep = StaticTool::Grep(GrepTool::new("."));
        assert_eq!(grep.name_str(), "grep");
        let web_fetch = StaticTool::WebFetch(
            WebFetchTool::new(WebFetchConfig::default()).expect("Failed to create WebFetchTool"),
        );
        assert_eq!(web_fetch.name_str(), "web_fetch");
    }

    #[test]
    fn static_registry_definitions() {
        let registry = StaticToolRegistry::with_default_tools(".");
        let defs = registry.definitions();
        assert_eq!(defs.len(), 6);
        let names: Vec<_> = defs.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"bash"));
        assert!(names.contains(&"read_file"));
        assert!(names.contains(&"apply_patch"));
        assert!(names.contains(&"glob"));
        assert!(names.contains(&"grep"));
        assert!(names.contains(&"web_fetch"));
    }
}
