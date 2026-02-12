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
    clippy::missing_errors_doc,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation
)]

mod convert;
mod dedup;
pub mod extraction;
mod manager;
pub mod query;
pub mod rerank;
pub mod schema;
mod scoring;
mod session;

// Re-export SessionStorage so MemoryManager can be used as session storage
pub use nanors_core::SessionStorage;

pub use dedup::content_hash;
pub use extraction::cards::{CardKind, MemoryCard, Polarity, VersionRelation};
pub use extraction::engine::{DatabaseCardRepository, ExtractionConfig, ExtractionEngine};
pub use extraction::patterns::default_patterns;
pub use manager::MemoryManager;
pub use query::detector::{QuestionPattern, QuestionType, QuestionTypeDetector};
pub use schema::{Cardinality, SchemaError, SchemaRegistry, ValueType};
