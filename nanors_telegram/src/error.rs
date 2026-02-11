#![deny(
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

use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Telegram API error: {0}")]
    Telegram(#[from] teloxide::RequestError),

    #[error("AI provider error: {0}")]
    Provider(anyhow::Error),

    #[error("Memory manager error: {0}")]
    Memory(anyhow::Error),

    #[error("Session not found for chat_id: {0}")]
    SessionNotFound(i64),

    #[error("Unauthorized access from chat_id: {0}")]
    Unauthorized(i64),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
