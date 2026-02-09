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

/// Chinese question keywords that indicate a question rather than an answer.
/// When these appear in memory, and the query also contains them, the memory
/// should be penalized to avoid retrieving questions instead of answers.
const QUESTION_KEYWORDS: &[&str] = &[
    "哪",
    "什么",
    "谁",
    "多少",
    "怎么",
    "如何",
    "为什么",
    "为啥",
    "吗",
    "是不是",
    "呢",
];

/// Detect if text contains Chinese question keywords.
///
/// Returns the count of unique question keywords found in the text.
/// This is used to penalize memories that are questions when the user's
/// query is also a question.
#[must_use]
pub fn count_question_keywords(text: &str) -> usize {
    QUESTION_KEYWORDS
        .iter()
        .filter(|keyword| text.contains(**keyword))
        .count()
}

/// Compute question penalty for memory retrieval.
///
/// When both the query and the memory contain question keywords, we heavily penalize
/// the memory to avoid returning questions instead of answers.
///
/// Penalty formula: `1.0 - (query_question_count * memory_question_count) * 0.50`
///
/// This ensures that:
/// - If neither contains question keywords, no penalty (factor = 1.0)
/// - If only one contains question keywords, no penalty (factor = 1.0)
/// - If both contain question keywords, 50% penalty per matching pair
///
/// # Examples
/// - Query "我住哪" (1 keyword) vs Memory "我住西城区" (0 keywords): factor = 1.0
/// - Query "我住哪" (1 keyword) vs Memory "你住哪里" (1 keyword): factor = 0.50
/// - Query "我住在哪里呢" (2 keywords) vs Memory "你住哪" (1 keyword): factor = 0.0
///
/// The heavy penalty ensures that answers (which typically don't contain question
/// keywords) rank much higher than questions when the user asks a question.
#[must_use]
pub fn question_penalty(query_text: &str, memory_text: &str) -> f64 {
    let query_count = count_question_keywords(query_text);
    let memory_count = count_question_keywords(memory_text);

    // Only apply penalty if both contain question keywords
    if query_count == 0 || memory_count == 0 {
        return 1.0;
    }

    // Heavy penalty: 50% reduction per matching pair of question keywords
    // This ensures answers rank much higher than questions
    let penalty = query_count * memory_count * 50;
    (1.0 - penalty.min(100) as f64 / 100.0).max(0.0)
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

    #[test]
    fn count_question_keywords_counts_unique_keywords() {
        assert_eq!(count_question_keywords("我住哪"), 1);
        assert_eq!(count_question_keywords("你住在哪里呢"), 2); // 哪, 呢
        assert_eq!(count_question_keywords("这是什么"), 1); // 什么
        assert_eq!(count_question_keywords("谁有多少钱"), 2); // 谁, 多少
        assert_eq!(count_question_keywords("我住西城区"), 0);
        assert_eq!(count_question_keywords("我喜欢吃苹果"), 0);
    }

    #[test]
    fn question_penalty_no_penalty_when_no_keywords() {
        // Neither query nor memory has question keywords
        let penalty = question_penalty("我住西城区", "User: 我住朝阳区");
        assert!((penalty - 1.0).abs() < 1e-9);
    }

    #[test]
    fn question_penalty_no_penalty_when_only_query_has_keywords() {
        // Query has question keyword, memory doesn't
        let penalty = question_penalty("我住哪", "User: 我住西城区");
        assert!((penalty - 1.0).abs() < 1e-9);
    }

    #[test]
    fn question_penalty_no_penalty_when_only_memory_has_keywords() {
        // Memory has question keyword, query doesn't
        let penalty = question_penalty("我住西城区", "User: 你住哪里");
        assert!((penalty - 1.0).abs() < 1e-9);
    }

    #[test]
    fn question_penalty_applies_when_both_have_keywords() {
        // Both have 1 question keyword each - 50% penalty
        let penalty = question_penalty("我住哪", "User: 你住哪里");
        assert!((penalty - 0.50).abs() < 1e-9); // 1.0 - 1*1*0.50 = 0.50
    }

    #[test]
    fn question_penalty_multiple_keywords_in_both() {
        // Query has 2 keywords, memory has 1 keyword - 100% penalty (capped)
        let penalty = question_penalty("我住在哪里呢", "User: 你住哪");
        assert!((penalty - 0.0).abs() < 1e-9); // 1.0 - 2*1*0.50 = 0.0 (capped at 0)
    }

    #[test]
    fn question_penalty_capped_at_100_percent() {
        // Even with many keywords, penalty is capped at 100%
        let penalty = question_penalty("这是什么为什么怎么如何", "User: 哪谁多少吗呢啊");
        assert!((penalty - 0.0).abs() < 1e-9); // Capped at 100% penalty
    }
}
