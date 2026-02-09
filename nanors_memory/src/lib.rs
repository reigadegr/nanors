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
mod enrichment;
mod extraction;
mod manager;
mod query;
pub mod rerank;
mod schema;
mod scoring;
mod temporal;

// Re-export SessionStorage so MemoryManager can be used as session storage
pub use nanors_core::SessionStorage;

pub use dedup::content_hash;
pub use enrichment::{
    DatabaseEnrichmentRepository, EngineStamp, EnrichmentManifest, EnrichmentParams,
    EnrichmentRecord, EnrichmentRepository,
};
pub use extraction::{
    CardKind, CardRepository, DatabaseCardRepository, ExtractionConfig, ExtractionEngine,
    MemoryCard, Polarity, VersionRelation,
};
pub use manager::MemoryManager;
pub use query::{QueryExpander, QuestionType, QuestionTypeDetector};
pub use rerank::{NoOpReranker, Reranker, RuleBasedReranker};
pub use schema::{Cardinality, PredicateSchema, SchemaError, SchemaRegistry, ValueType};
pub use scoring::{compute_salience, cosine_similarity};
pub use temporal::{
    CardRepositoryTemporal, DatabaseCardRepositoryTemporal, TimeTravelQuery, TimelineEntry,
};
