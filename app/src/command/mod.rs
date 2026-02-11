//! Static strategy pattern for CLI commands.
//!
//! This module implements a zero-allocation, static dispatch strategy pattern
//! inspired by the `MetricAdapter` pattern. Each command is a separate strategy
//! with its own type, enabling compile-time optimization and zero runtime overhead.

use nanors_config::Config;
use nanors_core::AgentLoop;
use nanors_memory::MemoryManager;
use nanors_providers::ZhipuProvider;
use std::sync::Arc;
use tracing::info;

mod agent;
mod chat;
mod info;
mod init;
mod telegram;
mod version;

/// Set up memory storage (semantic retrieval) for agent.
///
/// `MemoryManager` already provides `SessionStorage`, so we only need to add
/// memory retrieval capabilities through Arc<dyn MemoryItemRepo>.
fn setup_memory_storage(
    config: &Config,
    agent: AgentLoop<ZhipuProvider, Arc<MemoryManager>>,
    memory_manager: Arc<MemoryManager>,
) -> AgentLoop<ZhipuProvider, Arc<MemoryManager>> {
    info!("Memory feature enabled, setting up memory retrieval");
    let user_scope = config.memory.default_user_scope.clone();

    let retrieval_config = config.memory.retrieval.clone();

    info!(
        "Retrieval config: items_top_k={}, context_target_length={}",
        retrieval_config.items_top_k, retrieval_config.context_target_length
    );

    // Cast to Arc<dyn MemoryItemRepo> for with_memory
    let memory_repo: Arc<dyn nanors_core::MemoryItemRepo> = memory_manager;

    agent
        .with_memory(memory_repo, user_scope)
        .with_retrieval_config(retrieval_config)
}

pub use agent::{AgentInput, AgentStrategy};
pub use chat::{ChatInput, ChatStrategy};
pub use info::InfoStrategy;
pub use init::InitStrategy;
pub use telegram::{TelegramInput, TelegramStrategy};
pub use version::VersionStrategy;

/// Core trait defining the contract for all command strategies.
///
/// # Design Principles
/// - **Zero allocation**: No heap allocation required
/// - **Static dispatch**: All calls are monomorphized at compile time
/// - **Type safety**: Each strategy defines its own input type via associated type
/// - **Extensibility**: Adding new commands requires only implementing this trait
///
/// # Example
/// ```rust
/// struct MyStrategy;
///
/// impl CommandStrategy for MyStrategy {
///     type Input = MyInput;
///
///     async fn execute(&self, input: Self::Input) -> anyhow::Result<()> {
///         // Command logic here
///         Ok(())
///     }
/// }
/// ```
pub trait CommandStrategy: Send + Sync + 'static {
    /// The input type this strategy accepts.
    ///
    /// Each strategy can define its own input type, enabling type-safe
    /// parameter passing without runtime casting or boxing.
    type Input;

    /// Execute the command with the given input.
    ///
    /// This is the core method where each strategy implements its command logic.
    /// The method is async but uses static dispatch - no dynamic trait objects.
    ///
    /// # Errors
    /// Returns an error if command execution fails.
    async fn execute(&self, input: Self::Input) -> anyhow::Result<()>;
}
