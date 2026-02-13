use async_trait::async_trait;
use serde_json::json;
use std::path::{Path, PathBuf};
use tracing::info;

use crate::path_guard;
use crate::{Tool, ToolDefinition, ToolResult, WorkingDirIsolation, schema_object};

pub struct GrepTool {
    working_dir: PathBuf,
    working_dir_isolation: WorkingDirIsolation,
}

impl GrepTool {
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
impl Tool for GrepTool {
    fn name(&self) -> &'static str {
        "grep"
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "grep".into(),
            description: "Search file contents using a regex pattern. Returns matching lines with file paths and line numbers.".into(),
            input_schema: schema_object(
                json!({
                    "pattern": {
                        "type": "string",
                        "description": "Regex pattern to search for"
                    },
                    "path": {
                        "type": "string",
                        "description": "File or directory to search in (default: current directory)"
                    },
                    "glob": {
                        "type": "string",
                        "description": "Glob pattern to filter files (e.g., '*.rs')"
                    }
                }),
                &["pattern"],
            ),
        }
    }

    async fn execute(&self, input: serde_json::Value) -> ToolResult {
        let Some(pattern) = input.get("pattern").and_then(|v| v.as_str()) else {
            return ToolResult::error("Missing 'pattern' parameter");
        };
        let path = input.get("path").and_then(|v| v.as_str()).unwrap_or(".");
        let working_dir =
            crate::resolve_tool_working_dir(&self.working_dir, self.working_dir_isolation, &input);
        let resolved_path = crate::resolve_tool_path(&working_dir, path);
        let resolved_path_str = resolved_path.to_string_lossy().to_string();
        if let Err(msg) = path_guard::check_path(&resolved_path_str) {
            return ToolResult::error(msg);
        }
        let file_glob = input.get("glob").and_then(|v| v.as_str());

        info!("Grep: {} in {}", pattern, resolved_path.display());

        let re = match regex::Regex::new(pattern) {
            Ok(r) => r,
            Err(e) => return ToolResult::error(format!("Invalid regex: {e}")),
        };

        let mut results = Vec::new();
        let mut file_count = 0;

        if let Err(e) = grep_recursive(
            &resolved_path,
            file_glob,
            &re,
            &mut results,
            &mut file_count,
        ) {
            return ToolResult::error(format!("Search error: {e}"));
        }

        if results.is_empty() {
            ToolResult::success("No matches found.")
        } else {
            if results.len() > 500 {
                results.truncate(500);
                results.push("... (results truncated)".to_string());
            }
            ToolResult::success(results.join("\n"))
        }
    }
}

fn grep_recursive(
    path: &Path,
    file_glob: Option<&str>,
    re: &regex::Regex,
    results: &mut Vec<String>,
    file_count: &mut usize,
) -> std::io::Result<()> {
    let metadata = std::fs::metadata(path)?;

    if metadata.is_file() {
        grep_file(path, re, results);
    } else if metadata.is_dir() {
        let glob_pattern = file_glob.and_then(|g| glob::Pattern::new(g).ok());

        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let entry_path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();

            // Skip hidden directories and common non-code dirs
            if name.starts_with('.') || name == "node_modules" || name == "target" {
                continue;
            }

            if entry_path.is_dir() {
                grep_recursive(&entry_path, file_glob, re, results, file_count)?;
            } else if entry_path.is_file() {
                if path_guard::is_blocked(&entry_path) {
                    continue;
                }
                if let Some(ref pat) = glob_pattern {
                    if !pat.matches(&name) {
                        continue;
                    }
                }
                *file_count += 1;
                if *file_count > 10000 {
                    return Ok(());
                }
                grep_file(&entry_path, re, results);
            }
        }
    }
    Ok(())
}

fn grep_file(path: &Path, re: &regex::Regex, results: &mut Vec<String>) {
    let Ok(content) = std::fs::read_to_string(path) else {
        return; // Skip binary / unreadable files
    };

    for (line_num, line) in content.lines().enumerate() {
        if re.is_match(line) {
            results.push(format!("{}:{}: {}", path.display(), line_num + 1, line));
            if results.len() >= 500 {
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn setup_grep_dir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("nanors_grep_{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("hello.rs"),
            "fn main() {\n    println!(\"hello\");\n}\n",
        )
        .unwrap();
        std::fs::write(dir.join("world.txt"), "hello world\ngoodbye world\n").unwrap();
        dir
    }

    #[tokio::test]
    async fn test_grep_finds_matches() {
        let dir = setup_grep_dir();
        let tool = GrepTool::new(".");
        let result = tool
            .execute(json!({"pattern": "hello", "path": dir.to_str().unwrap()}))
            .await;
        assert!(!result.is_error);
        assert!(result.content.contains("hello"));
        // Should have file:line format
        assert!(result.content.contains(':'));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_grep_no_matches() {
        let dir = setup_grep_dir();
        let tool = GrepTool::new(".");
        let result = tool
            .execute(json!({"pattern": "zzzzzzz", "path": dir.to_str().unwrap()}))
            .await;
        assert!(!result.is_error);
        assert!(result.content.contains("No matches"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_grep_invalid_regex() {
        let tool = GrepTool::new(".");
        let result = tool
            .execute(json!({"pattern": "[invalid", "path": "."}))
            .await;
        assert!(result.is_error);
        assert!(result.content.contains("Invalid regex"));
    }

    #[tokio::test]
    async fn test_grep_missing_pattern() {
        let tool = GrepTool::new(".");
        let result = tool.execute(json!({})).await;
        assert!(result.is_error);
        assert!(result.content.contains("Missing 'pattern'"));
    }
}
