#![warn(
    clippy::all,
    clippy::nursery,
    clippy::pedantic,
    clippy::style,
    clippy::complexity,
    clippy::perf,
    clippy::correctness,
    clippy::suspicious,
    clippy::unwrap_used,
    clippy::expect_used
)]
#![allow(
    clippy::similar_names,
    clippy::missing_safety_doc,
    clippy::missing_panics_doc,
    clippy::missing_errors_doc
)]

//! Multi-turn conversation support with persistent sessions.
//!
//! This module provides conversational AI capabilities that maintain context
//! across multiple turns of dialogue, unlike the single-turn `AgentLoop`.
//!
//! # Key Features
//! - Persistent session IDs across conversation turns
//! - Configurable message history window
//! - Optional memory retrieval integration
//! - Conversation summarization for long sessions

mod history;
mod manager;
mod session;

pub use history::{HistoryConfig, HistoryManager, HistoryWindow};
pub use manager::{ConversationConfig, ConversationManager, TurnContext};
pub use session::ConversationSession;
