use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Config {
    pub agents: AgentsConfig,
    pub providers: ProvidersConfig,
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
            .join(".nanobot");

        let config_path = config_dir.join("config.json");

        if !config_path.exists() {
            anyhow::bail!("Config file not found at: {}. Please create a config file with your Zhipu API key.", config_path.display());
        }

        let content = std::fs::read_to_string(&config_path)?;
        let config: Self = serde_json::from_str(&content)?;

        Ok(config)
    }

    pub fn ensure_config_dir() -> anyhow::Result<PathBuf> {
        let config_dir = dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("Cannot find home directory"))?
            .join(".nanobot");

        std::fs::create_dir_all(&config_dir)?;
        Ok(config_dir)
    }

    pub fn create_example_config() -> anyhow::Result<()> {
        let config_dir = Self::ensure_config_dir()?;
        let config_path = config_dir.join("config.json.example");

        let example = Self {
            agents: AgentsConfig {
                defaults: AgentDefaults {
                    model: "glm-4-flash".to_string(),
                    max_tokens: 8192,
                    temperature: 0.7,
                },
            },
            providers: ProvidersConfig {
                zhipu: ProviderConfig {
                    api_key: "your-zhipu-api-key-here".to_string(),
                },
            },
        };

        let content = serde_json::to_string_pretty(&example)?;
        std::fs::write(&config_path, content)?;

        println!("Created example config at: {}", config_path.display());
        println!("Please edit it and rename to config.json");
        Ok(())
    }
}
