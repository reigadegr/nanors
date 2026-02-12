use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// Import RetrievalConfig from nanors_core to avoid duplication
use nanors_core::agent::RetrievalConfig;

/// Configuration directory name (relative to home directory)
const CONFIG_DIR_NAME: &str = ".nanors";

/// Configuration file name
const CONFIG_FILE_NAME: &str = "config.json";

#[derive(Debug, Default, Deserialize, Serialize, Clone)]
pub struct Config {
    #[serde(default)]
    pub agents: AgentsConfig,
    #[serde(default)]
    pub providers: ProvidersConfig,
    #[serde(default)]
    pub database: DatabaseConfig,
    #[serde(default)]
    pub memory: MemoryConfig,
    #[serde(default)]
    pub telegram: TelegramConfig,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DatabaseConfig {
    #[serde(default = "DatabaseConfig::default_url")]
    pub url: String,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            url: Self::default_url(),
        }
    }
}

impl DatabaseConfig {
    fn default_url() -> String {
        "postgresql://reigadegr:1234@localhost:5432/nanors".to_string()
    }
}

#[derive(Debug, Default, Deserialize, Serialize, Clone)]
pub struct MemoryConfig {
    #[serde(default)]
    pub retrieval: RetrievalConfig,
}

#[derive(Debug, Default, Deserialize, Serialize, Clone)]
pub struct TelegramConfig {
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub allow_from: Vec<String>,
}

#[derive(Debug, Default, Deserialize, Serialize, Clone)]
pub struct AgentsConfig {
    #[serde(default)]
    pub defaults: AgentDefaults,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AgentDefaults {
    pub model: String,
    pub max_tokens: usize,
    pub temperature: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub history_limit: Option<usize>,
}

impl Default for AgentDefaults {
    fn default() -> Self {
        Self {
            model: "glm-4-flash".to_string(),
            max_tokens: 8192,
            temperature: 0.7,
            system_prompt: Some(
                "You are a helpful AI assistant with memory of past conversations. Provide clear, concise responses."
                    .to_string(),
            ),
            history_limit: Some(20),
        }
    }
}

#[derive(Debug, Default, Deserialize, Serialize, Clone)]
pub struct ProvidersConfig {
    #[serde(default)]
    pub zhipu: ProviderConfig,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ProviderConfig {
    pub api_key: String,
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            api_key: "your-zhipu-api-key-here".to_string(),
        }
    }
}

impl Config {
    /// Returns the configuration directory path.
    fn config_dir() -> anyhow::Result<PathBuf> {
        Ok(dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("Cannot find home directory"))?
            .join(CONFIG_DIR_NAME))
    }

    /// Returns the configuration file path.
    pub fn config_path() -> anyhow::Result<PathBuf> {
        Ok(Self::config_dir()?.join(CONFIG_FILE_NAME))
    }

    /// Returns the configuration directory path, creating it if necessary.
    pub fn ensure_config_dir() -> anyhow::Result<PathBuf> {
        let config_dir = Self::config_dir()?;
        std::fs::create_dir_all(&config_dir)?;
        Ok(config_dir)
    }

    /// Loads the configuration from the config file.
    pub fn load() -> anyhow::Result<Self> {
        let config_path = Self::config_path()?;

        if !config_path.exists() {
            anyhow::bail!(
                "Config file not found at: {}. Please run 'nanors init' to create config.",
                config_path.display()
            );
        }

        let content = std::fs::read_to_string(&config_path)?;
        let config: Self = serde_json::from_str(&content)?;

        tracing::info!("Loaded config from {}", config_path.display());

        Ok(config)
    }

    /// Creates a new configuration file with default values.
    pub fn create_config() -> anyhow::Result<()> {
        Self::ensure_config_dir()?;
        let config_path = Self::config_path()?;

        if config_path.exists() {
            anyhow::bail!(
                "Config file already exists at: {}. Please edit it directly.",
                config_path.display()
            );
        }

        let config = Self::default();
        let config_json = serde_json::to_string_pretty(&config)?;

        std::fs::write(&config_path, config_json)?;

        println!("✅ Created config file at: {}", config_path.display());
        println!();
        println!("📝 Next steps:");
        println!("   1. Edit the config file and add your Zhipu API key");
        println!("   2. Ensure IvorySQL/PostgreSQL is running at the specified URL");
        println!("   3. Run 'nanors chat' to start a conversation");
        println!();
        println!("🔧 Configuration options:");
        println!("   - model: AI model to use (glm-4-flash, glm-4-plus, glm-4-0520, etc.)");
        println!("   - history_limit: Number of messages to keep in context (for chat command)");
        println!();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_serializes_correctly() {
        let config = Config::default();

        // 验证 agents 配置
        assert_eq!(config.agents.defaults.model, "glm-4-flash");
        assert_eq!(config.agents.defaults.max_tokens, 8192);
        // 浮点数使用近似比较
        assert!((config.agents.defaults.temperature - 0.7).abs() < f32::EPSILON);
        assert_eq!(
            config.agents.defaults.system_prompt,
            Some("You are a helpful AI assistant with memory of past conversations. Provide clear, concise responses.".to_string())
        );
        assert_eq!(config.agents.defaults.history_limit, Some(20));

        // 验证 providers 配置
        assert_eq!(config.providers.zhipu.api_key, "your-zhipu-api-key-here");

        // 验证 database 配置
        assert_eq!(
            config.database.url,
            "postgresql://reigadegr:1234@localhost:5432/nanors"
        );

        // 验证 telegram 配置
        assert_eq!(config.telegram.token, "");
        assert!(config.telegram.allow_from.is_empty());
    }

    #[test]
    fn test_config_serialization_roundtrip() {
        let original = Config::default();

        // 序列化为 JSON
        let json = serde_json::to_string_pretty(&original).expect("Failed to serialize config");

        // 反序列化回 Config
        let deserialized: Config =
            serde_json::from_str(&json).expect("Failed to deserialize config");

        // 验证所有字段一致
        assert_eq!(
            original.agents.defaults.model,
            deserialized.agents.defaults.model
        );
        assert_eq!(
            original.agents.defaults.max_tokens,
            deserialized.agents.defaults.max_tokens
        );
        // 浮点数使用近似比较
        assert!(
            (original.agents.defaults.temperature - deserialized.agents.defaults.temperature).abs()
                < f32::EPSILON
        );
        assert_eq!(
            original.agents.defaults.system_prompt,
            deserialized.agents.defaults.system_prompt
        );
        assert_eq!(
            original.agents.defaults.history_limit,
            deserialized.agents.defaults.history_limit
        );
        assert_eq!(
            original.providers.zhipu.api_key,
            deserialized.providers.zhipu.api_key
        );
        assert_eq!(original.database.url, deserialized.database.url);
        assert_eq!(original.telegram.token, deserialized.telegram.token);
        assert_eq!(
            original.telegram.allow_from,
            deserialized.telegram.allow_from
        );
    }

    #[test]
    fn test_config_json_is_valid() {
        let config = Config::default();
        let json = serde_json::to_string_pretty(&config).expect("Failed to serialize config");

        // 验证 JSON 格式正确（可以再次解析）
        let _: Config = serde_json::from_str(&json).expect("Generated JSON is invalid");

        // 验证 JSON 包含预期的键
        assert!(json.contains("\"agents\""));
        assert!(json.contains("\"providers\""));
        assert!(json.contains("\"database\""));
        assert!(json.contains("\"memory\""));
        assert!(json.contains("\"telegram\""));
        assert!(json.contains("\"glm-4-flash\""));
        assert!(json.contains("\"your-zhipu-api-key-here\""));
    }

    #[test]
    fn test_default_impl_for_all_configs() {
        // 验证所有配置结构体都有正确的 Default 实现
        let agent_defaults = AgentDefaults::default();
        assert_eq!(agent_defaults.model, "glm-4-flash");
        assert_eq!(agent_defaults.max_tokens, 8192);

        let agents = AgentsConfig::default();
        assert_eq!(agents.defaults.model, "glm-4-flash");

        let provider = ProviderConfig::default();
        assert_eq!(provider.api_key, "your-zhipu-api-key-here");

        let providers = ProvidersConfig::default();
        assert_eq!(providers.zhipu.api_key, "your-zhipu-api-key-here");

        let database = DatabaseConfig::default();
        assert_eq!(
            database.url,
            "postgresql://reigadegr:1234@localhost:5432/nanors"
        );

        let telegram = TelegramConfig::default();
        assert_eq!(telegram.token, "");
        assert!(telegram.allow_from.is_empty());

        let memory = MemoryConfig::default();
        // RetrievalConfig 有自己的默认值
        assert_eq!(memory.retrieval.items_top_k, 5);

        let config = Config::default();
        assert_eq!(config.agents.defaults.model, "glm-4-flash");
    }
}
