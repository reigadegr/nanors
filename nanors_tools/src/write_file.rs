use async_trait::async_trait;
use serde_json::json;
use std::path::PathBuf;
use tracing::info;

use crate::path_guard;
use crate::{Tool, ToolDefinition, ToolResult, WorkingDirIsolation, schema_object};

pub struct WriteFileTool {
    working_dir: PathBuf,
    working_dir_isolation: WorkingDirIsolation,
}

impl WriteFileTool {
    #[must_use]
    pub fn new(working_dir: &str) -> Self {
        Self::new_with_isolation(working_dir, WorkingDirIsolation::Shared)
    }

    #[must_use]
    pub fn new_with_isolation(
        working_dir: &str,
        working_dir_isolation: WorkingDirIsolation,
    ) -> Self {
        Self {
            working_dir: PathBuf::from(working_dir),
            working_dir_isolation,
        }
    }
}

#[async_trait]
impl Tool for WriteFileTool {
    fn name(&self) -> &'static str {
        "write_file"
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "write_file".into(),
            description: "Write content to a file. Creates the file and any parent directories if they don't exist. Overwrites existing content.".into(),
            input_schema: schema_object(
                json!({
                    "path": {
                        "type": "string",
                        "description": "The file path to write to"
                    },
                    "content": {
                        "type": "string",
                        "description": "The content to write"
                    }
                }),
                &["path", "content"],
            ),
        }
    }

    async fn execute(&self, input: serde_json::Value) -> ToolResult {
        let Some(path) = input.get("path").and_then(|v| v.as_str()) else {
            return ToolResult::error("Missing 'path' parameter");
        };
        let working_dir =
            crate::resolve_tool_working_dir(&self.working_dir, self.working_dir_isolation, &input);
        let resolved_path = crate::resolve_tool_path(&working_dir, path);
        let resolved_path_str = resolved_path.to_string_lossy().to_string();

        if let Err(msg) = path_guard::check_path(&resolved_path_str) {
            return ToolResult::error(msg);
        }

        let Some(content) = input.get("content").and_then(|v| v.as_str()) else {
            return ToolResult::error("Missing 'content' parameter");
        };

        info!("Writing file: {}", resolved_path.display());

        if let Some(parent) = resolved_path.parent() {
            if let Err(e) = tokio::fs::create_dir_all(parent).await {
                return ToolResult::error(format!("Failed to create directories: {e}"));
            }
        }

        match tokio::fs::write(&resolved_path, content).await {
            Ok(()) => {
                ToolResult::success(format!("Successfully wrote to {}", resolved_path.display()))
            }
            Err(e) => ToolResult::error(format!("Failed to write file: {e}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn test_write_file_success() {
        let dir = std::env::temp_dir().join(format!("nanors_wf_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("out.txt");

        let tool = WriteFileTool::new(".");
        let result = tool
            .execute(json!({"path": file.to_str().unwrap(), "content": "hello world"}))
            .await;
        assert!(!result.is_error);
        assert!(result.content.contains("Successfully wrote"));

        let content = std::fs::read_to_string(&file).unwrap();
        assert_eq!(content, "hello world");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_write_file_missing_params() {
        let tool = WriteFileTool::new(".");

        let result = tool.execute(json!({"content": "hello"})).await;
        assert!(result.is_error);
        assert!(result.content.contains("Missing 'path'"));

        let result = tool.execute(json!({"path": "/tmp/x"})).await;
        assert!(result.is_error);
        assert!(result.content.contains("Missing 'content'"));
    }
}
