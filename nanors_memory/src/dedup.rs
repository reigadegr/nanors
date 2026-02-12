//! Content hashing utilities.

//! Re-export `content_hash` from `nanors_core` to avoid duplication.
pub use nanors_core::content_hash;

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
