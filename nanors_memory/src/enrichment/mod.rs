//! Enrichment tracking for incremental processing.
//!
//! This module provides types and functionality for tracking which memory items
//! have been processed by which enrichment engines, enabling incremental processing
//! and avoiding duplicate work (especially expensive LLM calls).

pub mod manifest;

pub use manifest::{
    DatabaseEnrichmentRepository, EngineStamp, EnrichmentManifest, EnrichmentParams,
    EnrichmentRecord, EnrichmentRepository,
};
