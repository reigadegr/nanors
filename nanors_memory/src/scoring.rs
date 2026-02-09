use chrono::{DateTime, Utc};
use std::collections::HashSet;

/// Compute keyword overlap score between two strings using character-level bigrams.
///
/// This is a simple approximation of text similarity that helps match questions
/// with answers when vector similarity alone is insufficient.
///
/// Returns 0.0 if either string is empty.
#[must_use]
pub fn keyword_overlap(a: &str, b: &str) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }

    // Extract character bigrams from both strings
    let bigrams_a: HashSet<String> = a
        .chars()
        .collect::<Vec<char>>()
        .windows(2)
        .map(|w| w.iter().collect())
        .collect();

    let bigrams_b: HashSet<String> = b
        .chars()
        .collect::<Vec<char>>()
        .windows(2)
        .map(|w| w.iter().collect())
        .collect();

    if bigrams_a.is_empty() || bigrams_b.is_empty() {
        return 0.0;
    }

    // Jaccard similarity: |A ∩ B| / |A ∪ B|
    let intersection = bigrams_a.intersection(&bigrams_b).count();
    let union = bigrams_a.union(&bigrams_b).count();

    if union == 0 {
        return 0.0;
    }

    intersection as f64 / union as f64
}

/// Compute hybrid similarity score combining vector and keyword similarity.
///
/// Formula: `vector_sim * 0.7 + keyword_overlap * 0.3`
///
/// This gives 70% weight to semantic vector similarity and 30% to keyword overlap.
/// The keyword overlap helps match Q&A pairs where the semantic meaning differs
/// but key terms are shared (e.g., "What is my phone?" vs "My phone is `OnePlus` 13").
#[must_use]
pub fn hybrid_similarity(vector_sim: f64, keyword_overlap: f64) -> f64 {
    vector_sim.mul_add(0.7, keyword_overlap * 0.3)
}

/// Compute cosine similarity between two embedding vectors.
///
/// Returns 0.0 if either vector has zero magnitude.
#[must_use]
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let mut dot = 0.0_f64;
    let mut mag_a = 0.0_f64;
    let mut mag_b = 0.0_f64;

    for (x, y) in a.iter().zip(b.iter()) {
        let x = f64::from(*x);
        let y = f64::from(*y);
        dot += x * y;
        mag_a += x * x;
        mag_b += y * y;
    }

    let denom = mag_a.sqrt() * mag_b.sqrt();
    if denom < f64::EPSILON {
        return 0.0;
    }

    dot / denom
}

/// Compute salience score for a memory item.
///
/// Formula: `similarity * (1 + ln(1 + reinforcement_count)) * 1/ln(2 + hours_ago)`
///
/// This mirrors memU's scoring strategy where more recent and more
/// reinforced memories rank higher. The minimum reinforcement factor is 1.0
/// to ensure that even unreinforced memories can rank based on similarity.
/// Using `+2` in the recency denominator prevents division by zero when `hours_ago` is 0.
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn compute_salience(
    similarity: f64,
    reinforcement_count: i32,
    happened_at: DateTime<Utc>,
    now: DateTime<Utc>,
) -> f64 {
    #[allow(clippy::cast_possible_truncation)]
    let hours_ago = (now - happened_at).num_seconds().max(1) as f64 / 3600.0;
    // Use 1.0 + ln(1 + reinforcement_count) to ensure non-zero factor for unreinforced memories
    let reinforcement = 1.0 + f64::from(reinforcement_count).ln_1p();
    // Use ln(2 + hours_ago) to prevent division by zero when hours_ago is 0
    let recency = 1.0 / (hours_ago + 1.0).ln_1p();

    similarity * reinforcement * recency
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_vectors_similarity_one() {
        let v = [1.0_f32, 2.0, 3.0];
        let sim = cosine_similarity(&v, &v);
        assert!((sim - 1.0).abs() < 1e-9);
    }

    #[test]
    fn orthogonal_vectors_similarity_zero() {
        let a = [1.0_f32, 0.0];
        let b = [0.0_f32, 1.0];
        let sim = cosine_similarity(&a, &b);
        assert!(sim.abs() < 1e-9);
    }

    #[test]
    fn empty_vectors_return_zero() {
        let sim = cosine_similarity(&[], &[]);
        assert!((sim).abs() < 1e-9);
    }

    #[test]
    fn salience_recent_higher() {
        let now = Utc::now();
        let recent = now - chrono::Duration::hours(1);
        let old = now - chrono::Duration::hours(100);

        let s_recent = compute_salience(0.9, 3, recent, now);
        let s_old = compute_salience(0.9, 3, old, now);
        assert!(s_recent > s_old);
    }
}
