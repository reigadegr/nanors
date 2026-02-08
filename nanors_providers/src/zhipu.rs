use async_trait::async_trait;
use nanors_core::{ChatMessage, LLMResponse, LLMProvider};
use reqwest::Client;
use serde_json::json;
use tracing::info;

pub struct ZhipuProvider {
    client: Client,
    api_key: String,
    base_url: String,
}

impl ZhipuProvider {
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
}

#[async_trait]
impl LLMProvider for ZhipuProvider {
    async fn chat(&self, messages: &[ChatMessage], model: &str) -> anyhow::Result<LLMResponse> {
        let request = json!({
            "model": model,
            "messages": messages,
        });

        info!("Sending request to Zhipu API: model={}", model);

        let response = self.client
            .post(format!("{}/chat/completions", self.base_url))
            .bearer_auth(&self.api_key)
            .json(&request)
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
            completion_tokens: u32::try_from(u["completion_tokens"].as_u64().unwrap_or(0)).unwrap_or(0),
            total_tokens: u32::try_from(u["total_tokens"].as_u64().unwrap_or(0)).unwrap_or(0),
        });

        info!("Received response from Zhipu API");
        Ok(LLMResponse { content, usage })
    }

    fn get_default_model(&self) -> &'static str {
        "glm-4-flash"
    }
}
