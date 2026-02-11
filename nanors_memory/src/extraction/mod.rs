//! Structured memory extraction module.
//!
//! This module provides configurable pattern-based extraction of entity/slot/value
//! triples from text, enabling fast O(1) lookups for common queries.

pub mod cards;
pub mod engine;
pub mod patterns;

// Pattern configuration
