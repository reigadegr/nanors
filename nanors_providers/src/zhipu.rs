use async_trait::async_trait;
use nanors_core::{
    ChatMessage, ContentBlock, LLMProvider, LLMResponse, LLMToolResponse, MessageContent, Role,
};
use reqwest::Client;
use serde_json::json;
use tracing::{info, warn};

use crate::retry::retry_with_backoff;

#[derive(Clone)]
pub struct ZhipuProvider {
    client: Client,
    api_key: String,
    base_url: String,
}

impl ZhipuProvider {
    /// Convert f64 to f32 for embedding values
    /// Precision loss is acceptable for ML embeddings
    #[expect(clippy::cast_possible_truncation, reason = "ML embeddings use f32")]
    const fn f64_to_f32(x: f64) -> f32 {
        x as f32
    }

    pub fn new(api_key: String) -> Self {
        info!("Creating ZhipuProvider");
        Self {
            client: Client::new(),
            api_key,
            base_url: "https://open.bigmodel.cn/api/paas/v4".to_string(),
        }
    }

    /// Handle HTTP response with proper error logging
    async fn handle_http_response(
        response: reqwest::Response,
    ) -> anyhow::Result<serde_json::Value> {
        let status = response.status();

        if !status.is_success() {
            let error_response = response.json::<serde_json::Value>().await.ok();

            if let Some(error_body) = error_response {
                warn!(
                    "HTTP error {status}: {}",
                    serde_json::to_string_pretty(&error_body)
                        .unwrap_or_else(|_| "Unable to format".to_string())
                );
            } else {
                warn!("HTTP error {status}");
            }
            return Err(anyhow::anyhow!("HTTP error: {status}"));
        }

        response
            .json::<serde_json::Value>()
            .await
            .map_err(Into::into)
    }

    /// Convert `ChatMessage` to Zhipu API format
    fn convert_message_to_zhipu(msg: &ChatMessage) -> serde_json::Value {
        // Handle Role::Tool with ToolResult - Zhipu API requires separate format
        if msg.role == Role::Tool {
            if let MessageContent::Blocks(blocks) = &msg.content {
                for block in blocks {
                    if let ContentBlock::ToolResult {
                        tool_use_id,
                        content,
                        ..
                    } = block
                    {
                        return json!({
                            "role": "tool",
                            "tool_call_id": tool_use_id,
                            "content": content,
                        });
                    }
                }
            }
        }

        match &msg.content {
            MessageContent::Text(text) => json!({
                "role": role_to_zhipu(&msg.role),
                "content": text,
            }),
            MessageContent::Blocks(blocks) => {
                // Zhipu API format: separate tool_calls and content fields
                let mut text_parts = Vec::new();
                let mut tool_calls = Vec::new();

                for block in blocks {
                    match block {
                        ContentBlock::Text { text } if !text.is_empty() => {
                            text_parts.push(text.as_str());
                        }
                        ContentBlock::ToolUse { id, name, input } => {
                            // Convert input to JSON string for Zhipu API
                            let arguments =
                                serde_json::to_string(input).unwrap_or_else(|_| "{}".to_string());

                            tool_calls.push(json!({
                                "id": id,
                                "type": "function",
                                "function": {
                                    "name": name,
                                    "arguments": arguments,
                                }
                            }));
                        }
                        ContentBlock::Text { .. } | ContentBlock::ToolResult { .. } => {
                            // Skip empty text blocks and ToolResult blocks
                            // (ToolResult should only appear in Role::Tool messages,
                            // which are handled above)
                        }
                    }
                }

                let mut message = json!({
                    "role": role_to_zhipu(&msg.role),
                });

                // Add content if there's text
                if !text_parts.is_empty() {
                    message["content"] = json!(text_parts.join("\n"));
                } else if tool_calls.is_empty() {
                    // No content and no tool calls - add empty content
                    message["content"] = json!("");
                }

                // Add tool_calls field if present (Zhipu specific format)
                if !tool_calls.is_empty() {
                    message["tool_calls"] = json!(tool_calls);
                }

                message
            }
        }
    }

    /// Convert `ToolDefinition` to Zhipu API format
    fn convert_tool_to_zhipu(tool: &nanors_tools::ToolDefinition) -> serde_json::Value {
        json!({
            "type": "function",
            "function": {
                "name": tool.name,
                "description": tool.description,
                "parameters": tool.input_schema,
            }
        })
    }

    /// Extract content blocks from Zhipu response
    fn extract_content_blocks(
        response: &serde_json::Value,
    ) -> anyhow::Result<(
        Vec<ContentBlock>,
        Option<String>,
        Option<nanors_core::Usage>,
    )> {
        let choice = response["choices"]
            .get(0)
            .ok_or_else(|| anyhow::anyhow!("No choices in response"))?;

        let message = choice
            .get("message")
            .ok_or_else(|| anyhow::anyhow!("No message in choice"))?;

        let stop_reason = choice
            .get("finish_reason")
            .and_then(|v| v.as_str())
            .map(String::from);

        // Parse usage
        let usage = response.get("usage").map(|u| nanors_core::Usage {
            prompt_tokens: u32::try_from(u["prompt_tokens"].as_u64().unwrap_or(0)).unwrap_or(0),
            completion_tokens: u32::try_from(u["completion_tokens"].as_u64().unwrap_or(0))
                .unwrap_or(0),
            total_tokens: u32::try_from(u["total_tokens"].as_u64().unwrap_or(0)).unwrap_or(0),
        });

        let mut blocks = Vec::new();

        // First, extract text content if present
        if let Some(text) = message.get("content").and_then(|v| v.as_str()) {
            if !text.is_empty() {
                blocks.push(ContentBlock::Text {
                    text: text.to_string(),
                });
            }
        }

        // Then, extract tool calls if present (Zhipu specific format)
        if let Some(tool_calls) = message.get("tool_calls").and_then(|v| v.as_array()) {
            for tool_call in tool_calls {
                let id = tool_call["id"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing id in tool_call"))?
                    .to_string();

                let function = &tool_call["function"];
                let name = function["name"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing name in function"))?
                    .to_string();

                // arguments is a JSON string in Zhipu API, need to parse it
                let arguments_str = function["arguments"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing arguments in function"))?;

                let input: serde_json::Value = serde_json::from_str(arguments_str)
                    .map_err(|e| anyhow::anyhow!("Failed to parse arguments: {e}"))?;

                blocks.push(ContentBlock::ToolUse { id, name, input });
            }
        }

        Ok((blocks, stop_reason, usage))
    }
}

const fn role_to_zhipu(role: &Role) -> &str {
    match role {
        Role::User => "user",
        Role::Assistant => "assistant",
        Role::System => "system",
        Role::Tool => "tool",
    }
}

#[async_trait]
impl LLMProvider for ZhipuProvider {
    async fn chat(&self, messages: &[ChatMessage], model: &str) -> anyhow::Result<LLMResponse> {
        let zhipu_messages: Vec<serde_json::Value> = messages
            .iter()
            .map(Self::convert_message_to_zhipu)
            .collect();

        let request = json!({
            "model": model,
            "messages": zhipu_messages,
        });

        info!("Sending chat request to Zhipu API: model={}", model);

        // Retry with exponential backoff: 2s, 4s, 6s, 8s, then 10s x 3
        let base_delays: [u64; 4] = [2, 4, 6, 8];
        let final_retries = 3;

        let response =
            retry_with_backoff(|| self.try_send_chat(&request), &base_delays, final_retries)
                .await?;

        info!("Received response from Zhipu API");
        Ok(response)
    }

    fn get_default_model(&self) -> &'static str {
        "glm-4-flash"
    }

    async fn embed(&self, text: &str) -> anyhow::Result<Vec<f32>> {
        let response = self
            .client
            .post(format!("{}/embeddings", self.base_url))
            .bearer_auth(&self.api_key)
            .json(&json!({
                "model": "embedding-2",
                "input": text,
            }))
            .send()
            .await?;

        let response = Self::handle_http_response(response).await?;

        let embedding = response["data"][0]["embedding"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("Invalid response format: missing embedding"))?
            .iter()
            .map(|v| {
                v.as_f64()
                    .map(Self::f64_to_f32)
                    .ok_or_else(|| anyhow::anyhow!("Invalid embedding value"))
            })
            .collect::<Result<Vec<f32>, _>>()?;

        Ok(embedding)
    }

    /// Chat with tools support - Zhipu GLM-4 supports function calling
    async fn chat_with_tools(
        &self,
        messages: &[ChatMessage],
        model: &str,
        tools: Option<Vec<nanors_tools::ToolDefinition>>,
    ) -> anyhow::Result<LLMToolResponse> {
        let zhipu_messages: Vec<serde_json::Value> = messages
            .iter()
            .map(Self::convert_message_to_zhipu)
            .collect();

        let mut request = json!({
            "model": model,
            "messages": zhipu_messages,
        });

        // Add tools if provided
        let tool_count = tools.as_ref().map_or(0, std::vec::Vec::len);
        if let Some(tools) = tools {
            if !tools.is_empty() {
                let zhipu_tools: Vec<serde_json::Value> =
                    tools.iter().map(Self::convert_tool_to_zhipu).collect();
                request["tools"] = json!(zhipu_tools);
            }
        }

        info!(
            "Sending tool-enabled request to Zhipu API: model={}, tools={}",
            model, tool_count
        );

        // Retry with exponential backoff
        let base_delays: [u64; 4] = [2, 4, 6, 8];
        let final_retries = 3;

        let response = retry_with_backoff(
            || self.try_send_chat_with_tools(&request),
            &base_delays,
            final_retries,
        )
        .await?;

        info!("Received tool-enabled response from Zhipu API");
        Ok(response)
    }
}

impl ZhipuProvider {
    /// Helper method to send a single chat request
    async fn try_send_chat(&self, request: &serde_json::Value) -> anyhow::Result<LLMResponse> {
        let response = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .bearer_auth(&self.api_key)
            .json(request)
            .send()
            .await?;

        let response = Self::handle_http_response(response).await?;

        let content = response["choices"][0]["message"]["content"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Invalid response format: missing content"))?
            .to_string();

        let usage = response["usage"].as_object().map(|u| nanors_core::Usage {
            prompt_tokens: u32::try_from(u["prompt_tokens"].as_u64().unwrap_or(0)).unwrap_or(0),
            completion_tokens: u32::try_from(u["completion_tokens"].as_u64().unwrap_or(0))
                .unwrap_or(0),
            total_tokens: u32::try_from(u["total_tokens"].as_u64().unwrap_or(0)).unwrap_or(0),
        });

        Ok(LLMResponse { content, usage })
    }

    /// Helper method to send a chat request with tools
    async fn try_send_chat_with_tools(
        &self,
        request: &serde_json::Value,
    ) -> anyhow::Result<LLMToolResponse> {
        let request_body = serde_json::to_string_pretty(request)?;
        info!("Request body length: {} bytes", request_body.len());

        // Save request body to file for debugging
        if let Err(e) = std::fs::write("/tmp/zhipu_request_debug.json", &request_body) {
            warn!("Failed to write debug request file: {e}");
        }

        let response = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .bearer_auth(&self.api_key)
            .header("Content-Type", "application/json")
            .body(request_body)
            .send()
            .await?;

        let response_json = Self::handle_http_response(response).await?;

        let (content, stop_reason, usage) = Self::extract_content_blocks(&response_json)?;

        Ok(LLMToolResponse {
            content,
            stop_reason,
            usage,
        })
    }
}
