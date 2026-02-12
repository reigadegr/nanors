//! Utility functions for content hashing and deduplication.

use sha2::{Digest, Sha256};

/// Default system prompt for agents with memory support.
pub const DEFAULT_SYSTEM_PROMPT_WITH_MEMORY: &str = "You are a helpful AI assistant with memory of past conversations. Provide clear, concise responses.";

/// Default system prompt for agents without memory.
pub const DEFAULT_SYSTEM_PROMPT: &str = "You are a helpful AI assistant.";

/// Compute a SHA-256 content hash for deduplication.
///
/// Mirrors memU's `compute_content_hash`: concatenates memory type and
/// summary, then returns the hex-encoded SHA-256 digest.
#[must_use]
pub fn content_hash(memory_type: &str, summary: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(memory_type.as_bytes());
    hasher.update(b":");
    hasher.update(summary.as_bytes());
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_hash() {
        let h1 = content_hash("episodic", "had coffee");
        let h2 = content_hash("episodic", "had coffee");
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 64); // SHA-256 hex length
    }

    #[test]
    fn different_inputs_different_hashes() {
        let h1 = content_hash("episodic", "had coffee");
        let h2 = content_hash("semantic", "had coffee");
        assert_ne!(h1, h2);
    }
}
