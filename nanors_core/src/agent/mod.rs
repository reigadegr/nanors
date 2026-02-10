mod agent_loop;

pub use agent_loop::{AgentConfig, AgentLoop, RetrievalConfig};

// Re-export adaptive retrieval config for external use
pub use crate::retrieval::adaptive::AdaptiveConfig;
