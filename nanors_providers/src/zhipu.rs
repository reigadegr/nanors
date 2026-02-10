use async_trait::async_trait;
use nanors_core::{ChatMessage, LLMProvider, LLMResponse};
use reqwest::Client;
use serde_json::json;
use tracing::info;

use crate::retry::retry_with_backoff;

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

    #[must_use]
    pub fn with_base_url(mut self, base_url: String) -> Self {
        self.base_url = base_url;
        self
    }

    /// Helper method to send a single request
    async fn try_send(&self, request: &serde_json::Value) -> anyhow::Result<LLMResponse> {
        let response = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .bearer_auth(&self.api_key)
            .json(request)
            .send()
            .await?
            .error_for_status()?
            .json::<serde_json::Value>()
            .await?;

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
}

#[async_trait]
impl LLMProvider for ZhipuProvider {
    async fn chat(&self, messages: &[ChatMessage], model: &str) -> anyhow::Result<LLMResponse> {
        let request = json!({
            "model": model,
            "messages": messages,
        });

        info!("Sending request to Zhipu API: model={}", model);

        // Retry with exponential backoff: 2s, 4s, 6s, 8s, then 10s x 3
        let base_delays: [u64; 4] = [2, 4, 6, 8];
        let final_retries = 3;

        let response =
            retry_with_backoff(|| self.try_send(&request), &base_delays, final_retries).await?;

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
            .await?
            .error_for_status()?
            .json::<serde_json::Value>()
            .await?;

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
}
