use async_trait::async_trait;
use serde_json::json;
use std::path::{Path, PathBuf};
use tracing::info;

use crate::path_guard;
use crate::{Tool, ToolDefinition, ToolResult, WorkingDirIsolation, schema_object};

pub struct ApplyPatchTool {
    working_dir: PathBuf,
    working_dir_isolation: WorkingDirIsolation,
}

impl ApplyPatchTool {
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
impl Tool for ApplyPatchTool {
    fn name(&self) -> &'static str {
        "apply_patch"
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "apply_patch".into(),
            description: "Apply a unified diff patch to files. The diff format uses standard unified diff with paths relative to base_path. \
            First use 'bash' tool with 'pwd' and 'ls' to confirm your current location and file structure. \
            Then generate diff paths relative to the confirmed base_path. \
            Format: --- a/relative/path/to/file\n+++ b/relative/path/to/file\n@@ -line,count +line,count @@\n-old line\n+new line".into(),
            input_schema: schema_object(
                json!({
                    "diff_content": {
                        "type": "string",
                        "description": "The unified diff content to apply. Paths should be relative to base_path. Format: --- a/file.txt\n+++ b/file.txt\n@@ -1,1 +1,1 @@\n-old\n+new"
                    },
                    "base_path": {
                        "type": "string",
                        "description": "Base directory for resolving relative paths in the diff (default: current working directory)"
                    }
                }),
                &["diff_content"],
            ),
        }
    }

    async fn execute(&self, input: serde_json::Value) -> ToolResult {
        let Some(diff_content) = input.get("diff_content").and_then(|v| v.as_str()) else {
            return ToolResult::error("Missing 'diff_content' parameter");
        };

        let base_path_str = input
            .get("base_path")
            .and_then(|v| v.as_str())
            .unwrap_or(".");

        let working_dir =
            crate::resolve_tool_working_dir(&self.working_dir, self.working_dir_isolation, &input);
        let base_dir = working_dir.join(base_path_str);

        info!("Applying patch in: {}", base_dir.display());

        match apply_unified_diff(diff_content, &base_dir) {
            Ok(messages) => {
                if messages.is_empty() {
                    ToolResult::success("Patch applied successfully (no changes made)")
                } else {
                    ToolResult::success(format!(
                        "Patch applied successfully:\n{}",
                        messages.join("\n")
                    ))
                }
            }
            Err(e) => ToolResult::error(format!("Failed to apply patch: {e}")),
        }
    }
}

fn apply_unified_diff(diff_content: &str, base_dir: &Path) -> anyhow::Result<Vec<String>> {
    let lines: Vec<&str> = diff_content.lines().collect();
    let mut messages = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];

        // Look for unified diff header: --- a/path
        if line.starts_with("--- ") {
            let _old_path = extract_path(line, "a/")?;
            i += 1;

            if i >= lines.len() {
                return Err(anyhow::anyhow!("Unexpected end of diff after --- line"));
            }

            let new_path = extract_path(lines[i], "b/")?;
            i += 1;

            if i >= lines.len() || !lines[i].starts_with("@@ ") {
                return Err(anyhow::anyhow!("Expected @@ hunk header after file paths"));
            }

            // Parse hunk header: @@ -old_start,old_count +new_start,new_count @@
            let hunk_info = parse_hunk_header(lines[i])?;
            i += 1;

            // Apply the patch
            let result = apply_patch_to_file(base_dir, &new_path, &lines, &mut i, &hunk_info)?;
            messages.push(result);
        } else {
            i += 1;
        }
    }

    Ok(messages)
}

fn extract_path(line: &str, prefix: &str) -> anyhow::Result<String> {
    let parts: Vec<&str> = line.splitn(2, ' ').collect();
    if parts.len() < 2 {
        return Err(anyhow::anyhow!("Invalid diff line: {line}"));
    }
    let path = parts[1];
    if let Some(stripped) = path.strip_prefix(prefix) {
        Ok(stripped.to_string())
    } else {
        Ok(path.to_string())
    }
}

#[derive(Debug)]
struct HunkInfo {
    old_start: usize,
    old_count: usize,
    new_count: usize,
}

fn parse_hunk_header(line: &str) -> anyhow::Result<HunkInfo> {
    // Format: @@ -old_start,old_count +new_start,new_count @@
    let content = line
        .strip_prefix("@@ ")
        .and_then(|s| s.strip_suffix(" @@"))
        .ok_or_else(|| anyhow::anyhow!("Invalid hunk header: {line}"))?;

    let parts: Vec<&str> = content.split(' ').collect();
    if parts.len() < 2 {
        return Err(anyhow::anyhow!("Invalid hunk header format: {line}"));
    }

    let old_part = parts[0]
        .strip_prefix('-')
        .ok_or_else(|| anyhow::anyhow!("Missing old line count"))?;
    let new_part = parts[1]
        .strip_prefix('+')
        .ok_or_else(|| anyhow::anyhow!("Missing new line count"))?;

    let old_coords: Vec<usize> = old_part
        .split(',')
        .map(|s| {
            s.parse::<usize>()
                .map_err(|_| anyhow::anyhow!("Invalid old line numbers"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let new_coords: Vec<usize> = new_part
        .split(',')
        .map(|s| {
            s.parse::<usize>()
                .map_err(|_| anyhow::anyhow!("Invalid new line numbers"))
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(HunkInfo {
        old_start: old_coords[0],
        old_count: *old_coords.get(1).unwrap_or(&1),
        new_count: *new_coords.get(1).unwrap_or(&1),
    })
}

fn apply_patch_to_file(
    base_dir: &Path,
    file_path: &str,
    diff_lines: &[&str],
    line_idx: &mut usize,
    hunk_info: &HunkInfo,
) -> anyhow::Result<String> {
    let full_path = base_dir.join(file_path);
    let full_path_str = full_path.to_string_lossy().to_string();

    if let Err(msg) = path_guard::check_path(&full_path_str) {
        return Err(anyhow::anyhow!("Path check failed: {msg}"));
    }

    let original_content = std::fs::read_to_string(&full_path)
        .map_err(|e| anyhow::anyhow!("Failed to read file: {e}"))?;

    let has_trailing_newline = original_content.ends_with('\n');
    let original_lines: Vec<&str> = original_content.lines().collect();
    let mut new_lines: Vec<String> = original_lines.iter().map(|&s| s.to_string()).collect();
    let mut line_offset = 0isize;

    // Process diff lines until we hit next file header or end
    let mut old_line_idx = hunk_info.old_start.saturating_sub(1);
    let mut remaining_old = hunk_info.old_count;
    let mut remaining_new = hunk_info.new_count;

    while *line_idx < diff_lines.len()
        && (remaining_old > 0 || remaining_new > 0)
        && !diff_lines[*line_idx].starts_with("--- ")
    {
        let line = diff_lines[*line_idx];

        if line.starts_with(' ') || line.is_empty() {
            // Context line - verify it matches
            let context_line = line.strip_prefix(' ').unwrap_or(line);
            if old_line_idx < original_lines.len() && original_lines[old_line_idx] != context_line {
                return Err(anyhow::anyhow!(
                    "Context mismatch at line {}: expected '{}', got '{}'",
                    old_line_idx + 1,
                    context_line,
                    original_lines[old_line_idx]
                ));
            }
            old_line_idx += 1;
            remaining_old = remaining_old.saturating_sub(1);
            remaining_new = remaining_new.saturating_sub(1);
        } else if line.starts_with('-') {
            // Remove line
            if old_line_idx < original_lines.len() {
                let remove_idx = old_line_idx
                    .checked_add_signed(line_offset)
                    .unwrap_or(old_line_idx);
                if remove_idx < new_lines.len() {
                    new_lines.remove(remove_idx);
                    line_offset -= 1;
                }
            }
            old_line_idx += 1;
            remaining_old = remaining_old.saturating_sub(1);
        } else if line.starts_with('+') {
            // Add line
            let new_line = line.strip_prefix('+').unwrap_or(line);
            let insert_idx = old_line_idx
                .checked_add_signed(line_offset)
                .unwrap_or(old_line_idx);
            if insert_idx <= new_lines.len() {
                new_lines.insert(insert_idx, new_line.to_string());
                line_offset += 1;
            }
            remaining_new = remaining_new.saturating_sub(1);
        } else if line.starts_with('\\') {
            // Special line (e.g., \ No newline at end of file) - skip
        } else if line.starts_with("@@ ") {
            // New hunk - apply it to the already modified content
            let new_hunk_info = parse_hunk_header(line)?;
            *line_idx += 1;
            apply_hunk_to_lines(&mut new_lines, &new_hunk_info, diff_lines, line_idx)?;
            break;
        }

        *line_idx += 1;
    }

    let mut new_content = new_lines.join("\n");
    if has_trailing_newline {
        new_content.push('\n');
    }
    std::fs::write(&full_path, new_content)
        .map_err(|e| anyhow::anyhow!("Failed to write file: {e}"))?;

    Ok(format!("  Updated: {file_path}"))
}

fn apply_hunk_to_lines(
    lines: &mut Vec<String>,
    hunk_info: &HunkInfo,
    diff_lines: &[&str],
    line_idx: &mut usize,
) -> anyhow::Result<()> {
    let mut line_offset = 0isize;
    let mut old_line_idx = hunk_info.old_start.saturating_sub(1);
    let mut remaining_old = hunk_info.old_count;
    let mut remaining_new = hunk_info.new_count;

    while *line_idx < diff_lines.len()
        && (remaining_old > 0 || remaining_new > 0)
        && !diff_lines[*line_idx].starts_with("--- ")
        && !diff_lines[*line_idx].starts_with("@@ ")
    {
        let line = diff_lines[*line_idx];

        if line.starts_with(' ') || line.is_empty() {
            let context_line = line.strip_prefix(' ').unwrap_or(line);
            if old_line_idx < lines.len() && lines[old_line_idx] != context_line {
                return Err(anyhow::anyhow!(
                    "Context mismatch at line {}",
                    old_line_idx + 1
                ));
            }
            old_line_idx += 1;
            remaining_old = remaining_old.saturating_sub(1);
            remaining_new = remaining_new.saturating_sub(1);
        } else if line.starts_with('-') {
            let remove_idx = old_line_idx
                .checked_add_signed(line_offset)
                .unwrap_or(old_line_idx);
            if remove_idx < lines.len() {
                lines.remove(remove_idx);
                line_offset -= 1;
            }
            old_line_idx += 1;
            remaining_old = remaining_old.saturating_sub(1);
        } else if line.starts_with('+') {
            let new_line = line.strip_prefix('+').unwrap_or(line);
            let insert_idx = old_line_idx
                .checked_add_signed(line_offset)
                .unwrap_or(old_line_idx);
            if insert_idx <= lines.len() {
                lines.insert(insert_idx, new_line.to_string());
                line_offset += 1;
            }
            remaining_new = remaining_new.saturating_sub(1);
        }

        *line_idx += 1;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn test_apply_patch_simple() {
        let dir = std::env::temp_dir().join(format!("nanors_patch_{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&dir).unwrap();

        let file = dir.join("test.txt");
        std::fs::write(&file, "hello world\nfoo bar\n").unwrap();

        let tool = ApplyPatchTool::new(".");
        let diff = r"--- a/test.txt
+++ b/test.txt
@@ -1,2 +1,2 @@
 hello world
-foo bar
+baz qux
";

        let result = tool
            .execute(json!({
                "diff_content": diff,
                "base_path": dir.to_str().unwrap()
            }))
            .await;

        assert!(!result.is_error, "{:?}", result.content);
        let content = std::fs::read_to_string(&file).unwrap();
        assert_eq!(content, "hello world\nbaz qux\n");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_apply_patch_add_line() {
        let dir = std::env::temp_dir().join(format!("nanors_patch_{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&dir).unwrap();

        let file = dir.join("test.txt");
        std::fs::write(&file, "line1\nline3\n").unwrap();

        let tool = ApplyPatchTool::new(".");
        let diff = r"--- a/test.txt
+++ b/test.txt
@@ -1,2 +1,3 @@
 line1
+line2
 line3
";

        let result = tool
            .execute(json!({
                "diff_content": diff,
                "base_path": dir.to_str().unwrap()
            }))
            .await;

        assert!(!result.is_error, "{:?}", result.content);
        let content = std::fs::read_to_string(&file).unwrap();
        assert_eq!(content, "line1\nline2\nline3\n");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_apply_patch_delete_line() {
        let dir = std::env::temp_dir().join(format!("nanors_patch_{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&dir).unwrap();

        let file = dir.join("test.txt");
        std::fs::write(&file, "line1\nline2\nline3\n").unwrap();

        let tool = ApplyPatchTool::new(".");
        let diff = r"--- a/test.txt
+++ b/test.txt
@@ -1,3 +1,2 @@
 line1
-line2
 line3
";

        let result = tool
            .execute(json!({
                "diff_content": diff,
                "base_path": dir.to_str().unwrap()
            }))
            .await;

        assert!(!result.is_error, "{:?}", result.content);
        let content = std::fs::read_to_string(&file).unwrap();
        assert_eq!(content, "line1\nline3\n");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
