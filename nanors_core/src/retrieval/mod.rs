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
