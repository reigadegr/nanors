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

mod convert;
mod dedup;
mod manager;
mod scoring;

pub use dedup::content_hash;
pub use manager::MemoryManager;
pub use scoring::{compute_salience, cosine_similarity};
