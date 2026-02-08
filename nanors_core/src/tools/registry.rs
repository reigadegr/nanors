use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::info;

use super::Tool;

pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
}

impl ToolRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    pub fn register(&mut self, tool: Arc<dyn Tool>) {
        info!("Registering tool: {}", tool.name());
        self.tools.insert(tool.name().to_string(), tool);
    }

    pub async fn execute(&self, name: &str, args: serde_json::Value) -> anyhow::Result<String> {
        let tool = self.tools.get(name)
            .ok_or_else(|| anyhow::anyhow!("Tool not found: {name}"))?;
        tool.execute(args).await
    }

    #[must_use]
    pub fn get_definitions(&self) -> Vec<serde_json::Value> {
        self.tools.values()
            .map(|t| json!({
                "type": "function",
                "function": {
                    "name": t.name(),
                    "description": t.description(),
                    "parameters": t.parameters()
                }
            }))
            .collect()
    }

    #[must_use]
    pub fn list(&self) -> Vec<String> {
        self.tools.keys().cloned().collect()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}
