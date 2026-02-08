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

pub mod adaptive;
pub mod compression;
pub mod sufficiency;

pub use adaptive::{
    AdaptiveConfig, AdaptiveResult, AdaptiveStats, CutoffStrategy, find_adaptive_cutoff,
};
pub use compression::{
    CategoryCompressor, LLMAbstractor, build_category_summary_prompt, build_short_id,
    extract_references,
};
pub use sufficiency::{SufficiencyChecker, SufficiencyResult};
