use nanors_core::AgentLoop;
use nanors_tools::{
    BashTool, EditFileTool, GlobTool, GrepTool, ReadFileTool, ToolRegistry, WriteFileTool,
};
use uuid::Uuid;

use super::{build_agent_config, init_common_components};

/// Input parameters for Agent command strategy.
///
/// This struct encapsulates all parameters needed to execute the Agent command,
/// following type-state pattern for compile-time correctness.
#[derive(Debug, Clone)]
pub struct AgentInput {
    /// Optional single message to send (non-interactive mode)
    pub message: Option<String>,
    /// Optional model override
    pub model: Option<String>,
    /// Working directory for tools
    pub working_dir: Option<String>,
    /// Enable tool calling
    pub enable_tools: bool,
}

/// Strategy for executing Agent command.
///
/// This strategy handles core agent functionality:
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
        let common = init_common_components().await?;

        let agent_config = build_agent_config(&common.config, input.model.clone());

        // Create agent with MemoryManager as both session and memory storage
        let agent = AgentLoop::new(common.provider, common.memory_manager.clone(), agent_config);

        let agent = super::setup_memory_storage(&common.config, agent, common.memory_manager);

        // Register tools if enabled
        let agent = if input.enable_tools {
            let working_dir = input.working_dir.unwrap_or_else(|| ".".to_string());

            let mut registry = ToolRegistry::new();

            // Register core tools
            registry.add_tool(Box::new(BashTool::new(&working_dir)));
            registry.add_tool(Box::new(ReadFileTool::new(&working_dir)));
            registry.add_tool(Box::new(WriteFileTool::new(&working_dir)));
            registry.add_tool(Box::new(EditFileTool::new(&working_dir)));
            registry.add_tool(Box::new(GlobTool::new(&working_dir)));
            registry.add_tool(Box::new(GrepTool::new(&working_dir)));

            eprintln!("🔧 Tool calling enabled with 6 tools");

            agent.with_tools(registry)
        } else {
            agent
        };

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
