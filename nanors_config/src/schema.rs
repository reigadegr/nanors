use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// Import RetrievalConfig from nanors_core to avoid duplication
use nanors_core::agent::RetrievalConfig;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Config {
    pub agents: AgentsConfig,
    pub providers: ProvidersConfig,
    #[serde(default)]
    pub database: DatabaseConfig,
    #[serde(default)]
    pub memory: MemoryConfig,
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

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct MemoryConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "MemoryConfig::default_user_scope")]
    pub default_user_scope: String,
    #[serde(default)]
    pub retrieval: RetrievalConfig,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            default_user_scope: Self::default_user_scope(),
            retrieval: RetrievalConfig::default(),
        }
    }
}

impl MemoryConfig {
    fn default_user_scope() -> String {
        "default".to_string()
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AgentsConfig {
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

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ProvidersConfig {
    pub zhipu: ProviderConfig,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ProviderConfig {
    pub api_key: String,
}

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        let config_dir = dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("Cannot find home directory"))?
            .join("nanors");

        let config_path = config_dir.join("config.json");

        if !config_path.exists() {
            anyhow::bail!(
                "Config file not found at: {}. Please run 'nanors init' to create config.",
                config_path.display()
            );
        }

        let content = std::fs::read_to_string(&config_path)?;
        let config: Self = serde_json::from_str(&content)?;

        Ok(config)
    }

    pub fn ensure_config_dir() -> anyhow::Result<PathBuf> {
        let config_dir = dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("Cannot find home directory"))?
            .join("nanors");

        std::fs::create_dir_all(&config_dir)?;
        Ok(config_dir)
    }

    pub fn create_config() -> anyhow::Result<()> {
        let config_dir = Self::ensure_config_dir()?;
        let config_path = config_dir.join("config.json");

        // 检查是否已存在
        if config_path.exists() {
            anyhow::bail!(
                "Config file already exists at: {}. Please edit it directly.",
                config_path.display()
            );
        }

        // 使用模板生成配置文件
        let config_template = r#"{
  "agents": {
    "defaults": {
      "model": "glm-4-flash",
      "max_tokens": 8192,
      "temperature": 0.7,
      "system_prompt": "You are a helpful AI assistant with memory of past conversations. Provide clear, concise responses.",
      "history_limit": 20
    }
  },
  "providers": {
    "zhipu": {
      "api_key": "your-zhipu-api-key-here"
    }
  },
  "database": {
    "url": "postgresql://reigadegr:1234@localhost:5432/nanors"
  },
  "memory": {
    "enabled": true,
    "default_user_scope": "default",
    "retrieval": {
      "items_top_k": 10,
      "context_target_length": 2000,
      "adaptive": {
        "enabled": true,
        "min_items": 5,
        "max_items": 50
      }
    }
  }
}"#;

        std::fs::write(&config_path, config_template)?;

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
        println!("   - memory.enabled: Enable long-term memory features");
        println!();
        Ok(())
    }
}
