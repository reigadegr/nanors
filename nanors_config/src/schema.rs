use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Config {
    pub agents: AgentsConfig,
    pub providers: ProvidersConfig,
    #[serde(default = "default_database_config")]
    pub database: DatabaseConfig,
    #[serde(default = "default_memory_config")]
    pub memory: MemoryConfig,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DatabaseConfig {
    #[serde(default = "default_database_url")]
    pub url: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct MemoryConfig {
    #[serde(default = "default_memory_enabled")]
    pub enabled: bool,
    #[serde(default = "default_user_scope")]
    pub default_user_scope: String,
    #[serde(default = "default_retrieval_config")]
    pub retrieval: RetrievalConfig,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct RetrievalConfig {
    #[serde(default = "default_categories_enabled")]
    pub categories_enabled: bool,
    #[serde(default = "default_categories_top_k")]
    pub categories_top_k: usize,
    #[serde(default = "default_items_top_k")]
    pub items_top_k: usize,
    #[serde(default = "default_resources_enabled")]
    pub resources_enabled: bool,
    #[serde(default = "default_resources_top_k")]
    pub resources_top_k: usize,
    #[serde(default = "default_context_target_length")]
    pub context_target_length: usize,
}

fn default_database_url() -> String {
    "mysql://username:password@localhost:3306/nanors".to_string()
}

const fn default_categories_enabled() -> bool {
    true
}

const fn default_categories_top_k() -> usize {
    3
}

const fn default_items_top_k() -> usize {
    5
}

const fn default_resources_enabled() -> bool {
    true
}

const fn default_resources_top_k() -> usize {
    2
}

const fn default_context_target_length() -> usize {
    2000
}

const fn default_retrieval_config() -> RetrievalConfig {
    RetrievalConfig {
        categories_enabled: default_categories_enabled(),
        categories_top_k: default_categories_top_k(),
        items_top_k: default_items_top_k(),
        resources_enabled: default_resources_enabled(),
        resources_top_k: default_resources_top_k(),
        context_target_length: default_context_target_length(),
    }
}

fn default_database_config() -> DatabaseConfig {
    DatabaseConfig {
        url: default_database_url(),
    }
}

const fn default_memory_enabled() -> bool {
    false
}

fn default_user_scope() -> String {
    "default".to_string()
}

fn default_memory_config() -> MemoryConfig {
    MemoryConfig {
        enabled: default_memory_enabled(),
        default_user_scope: default_user_scope(),
        retrieval: default_retrieval_config(),
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

        let config = Self {
            agents: AgentsConfig {
                defaults: AgentDefaults {
                    model: "glm-4.7-flash".to_string(),
                    max_tokens: 8192,
                    temperature: 0.7,
                },
            },
            providers: ProvidersConfig {
                zhipu: ProviderConfig {
                    api_key: "your-zhipu-api-key-here".to_string(),
                },
            },
            database: DatabaseConfig {
                url: "mysql://username:password@localhost:3306/nanors".to_string(),
            },
            memory: MemoryConfig {
                enabled: false,
                default_user_scope: "default".to_string(),
                retrieval: RetrievalConfig {
                    categories_enabled: true,
                    categories_top_k: 3,
                    items_top_k: 5,
                    resources_enabled: true,
                    resources_top_k: 2,
                    context_target_length: 2000,
                },
            },
        };

        let content = serde_json::to_string_pretty(&config)?;
        std::fs::write(&config_path, content)?;

        println!("Created config file at: {}", config_path.display());
        println!("Please edit it and add your Zhipu API key.");
        Ok(())
    }
}
