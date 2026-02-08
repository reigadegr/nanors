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
        "mysql://username:password@localhost:3306/nanors".to_string()
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
    /// Semantic similarity threshold (0.0-1.0) for memory versioning.
    /// When storing a new memory, if an existing memory has similarity
    /// above this threshold, a new version is created instead of a new memory.
    #[serde(default = "MemoryConfig::default_semantic_similarity_threshold")]
    pub semantic_similarity_threshold: f64,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            default_user_scope: Self::default_user_scope(),
            retrieval: RetrievalConfig::default(),
            semantic_similarity_threshold: Self::default_semantic_similarity_threshold(),
        }
    }
}

impl MemoryConfig {
    fn default_user_scope() -> String {
        "default".to_string()
    }

    const fn default_semantic_similarity_threshold() -> f64 {
        0.75
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
                semantic_similarity_threshold: 0.75,
                retrieval: RetrievalConfig {
                    categories_enabled: true,
                    categories_top_k: 3,
                    items_top_k: 5,
                    resources_enabled: true,
                    resources_top_k: 2,
                    context_target_length: 2000,
                    sufficiency_check_enabled: false,
                    enable_category_compression: false,
                    category_summary_target_length: 400,
                    adaptive_items: nanors_core::retrieval::AdaptiveConfig::default(),
                    adaptive_categories: nanors_core::retrieval::AdaptiveConfig::default(),
                    adaptive_resources: nanors_core::retrieval::AdaptiveConfig::default(),
                    semantic_similarity_threshold: 0.75,
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
