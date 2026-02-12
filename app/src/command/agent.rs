use nanors_config::Config;
use nanors_core::{AgentConfig, AgentLoop};
use nanors_memory::MemoryManager;
use nanors_providers::ZhipuProvider;
use std::sync::Arc;
use tracing::info;
use uuid::Uuid;

/// Input parameters for the Agent command strategy.
///
/// This struct encapsulates all parameters needed to execute the Agent command,
/// following the type-state pattern for compile-time correctness.
#[derive(Debug, Clone)]
pub struct AgentInput {
    /// Optional single message to send (non-interactive mode)
    pub message: Option<String>,
    /// Optional model override
    pub model: Option<String>,
}

/// Strategy for executing the Agent command.
///
/// This strategy handles the core agent functionality:
/// - Loading configuration
/// - Initializing provider and session manager
/// - Setting up memory if enabled
/// - Running in interactive or single-message mode
///
/// # Design
/// - Zero-allocation: No heap allocation beyond what business logic requires
/// - Static dispatch: All method calls are monomorphized
/// - Stateless: Strategy holds no internal state, all input via `AgentInput`
#[derive(Debug, Clone, Copy)]
pub struct AgentStrategy;

impl super::CommandStrategy for AgentStrategy {
    type Input = AgentInput;

    async fn execute(&self, input: Self::Input) -> anyhow::Result<()> {
        let config = Config::load()?;

        let provider = ZhipuProvider::new(config.providers.zhipu.api_key.clone());

        info!("Connecting to database");
        let memory_manager = Arc::new(MemoryManager::new(&config.database.url).await?);

        let agent_config = AgentConfig {
            model: input
                .model
                .unwrap_or_else(|| config.agents.defaults.model.clone()),
            max_tokens: config.agents.defaults.max_tokens,
            temperature: config.agents.defaults.temperature,
        };

        // Create agent with MemoryManager as both session and memory storage
        let agent = AgentLoop::new(provider, memory_manager.clone(), agent_config);

        let agent = super::setup_memory_storage(&config, agent, memory_manager);

        match input.message {
            Some(msg) => {
                let session_id = Uuid::now_v7();
                let response = agent.process_message(&session_id, &msg).await?;
                println!("{response}");
            }
            None => {
                agent.run_interactive().await?;
            }
        }

        Ok(())
    }
}
