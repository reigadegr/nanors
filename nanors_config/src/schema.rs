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
    #[serde(default)]
    pub extraction: ExtractionConfig,
    #[serde(default)]
    pub query: QueryConfig,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            default_user_scope: Self::default_user_scope(),
            retrieval: RetrievalConfig::default(),
            extraction: ExtractionConfig::default(),
            query: QueryConfig::default(),
        }
    }
}

impl MemoryConfig {
    fn default_user_scope() -> String {
        "default".to_string()
    }
}

/// Configuration for structured memory extraction.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ExtractionConfig {
    /// Enable automatic extraction of structured memory cards.
    #[serde(default)]
    pub enabled: bool,
    /// Minimum confidence threshold for storing extracted cards.
    #[serde(default = "ExtractionConfig::default_min_confidence")]
    pub min_confidence: f32,
    /// Extract cards when storing new memories.
    #[serde(default = "ExtractionConfig::default_extract_on_store")]
    pub extract_on_store: bool,
}

impl Default for ExtractionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            min_confidence: Self::default_min_confidence(),
            extract_on_store: Self::default_extract_on_store(),
        }
    }
}

impl ExtractionConfig {
    const fn default_min_confidence() -> f32 {
        0.3
    }

    const fn default_extract_on_store() -> bool {
        true
    }
}

/// Configuration for query analysis and expansion.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct QueryConfig {
    /// Enable question type detection.
    #[serde(default = "QueryConfig::default_detection_enabled")]
    pub detection_enabled: bool,
    /// Enable query expansion for better recall.
    #[serde(default = "QueryConfig::default_expansion_enabled")]
    pub expansion_enabled: bool,
    /// Minimum tokens to apply OR query expansion.
    #[serde(default = "QueryConfig::default_min_or_tokens")]
    pub min_or_tokens: usize,
}

impl Default for QueryConfig {
    fn default() -> Self {
        Self {
            detection_enabled: true,
            expansion_enabled: true,
            min_or_tokens: Self::default_min_or_tokens(),
        }
    }
}

impl QueryConfig {
    const fn default_detection_enabled() -> bool {
        true
    }

    const fn default_expansion_enabled() -> bool {
        true
    }

    const fn default_min_or_tokens() -> usize {
        2
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
                    items_top_k: 10,
                    context_target_length: 2000,
                },
                extraction: ExtractionConfig {
                    enabled: true,
                    min_confidence: 0.5,
                    extract_on_store: true,
                },
                query: QueryConfig {
                    detection_enabled: true,
                    expansion_enabled: true,
                    min_or_tokens: 2,
                },
            },
        };

        let content = serde_json::to_string_pretty(&config)?;
        std::fs::write(&config_path, content)?;

        println!("Created config file at: {}", config_path.display());
        println!("Please edit it and add your Zhipu API key.");
        println!("Configuration includes:");
        println!("  - Structured memory extraction (enabled by default)");
        println!("  - Question type detection (enabled by default)");
        println!("  - Query expansion (enabled by default)");
        Ok(())
    }
}
