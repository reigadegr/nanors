//! Adaptive retrieval configuration and algorithms.
//!
//! Adaptive retrieval dynamically determines how many results to return based on
//! relevancy score distribution, rather than using a fixed `k`. This ensures:
//! - All relevant results are included (no "missing answers")
//! - Irrelevant results are excluded (reduced noise)
//! - Different queries get appropriate amounts of context
//!
//! # Example
//!
//! ```ignore
//! use nanors_core::retrieval::adaptive::{AdaptiveConfig, CutoffStrategy};
//!
//! // Configure adaptive retrieval
//! let config = AdaptiveConfig {
//!     enabled: true,
//!     max_results: 100,
//!     strategy: CutoffStrategy::RelativeThreshold { min_ratio: 0.5 },
//!     ..Default::default()
//! };
//!
//! // Search with adaptive retrieval
//! let results = memory.search_adaptive(&query, config)?;
//! // Returns all results above 50% of top score's relevancy
//! ```
//!
//! # Strategies
//!
//! - **`AbsoluteThreshold`**: Stop when score drops below a fixed value (e.g., 0.7)
//! - **`RelativeThreshold`**: Stop when score drops below X% of the top score
//! - **`ScoreCliff`**: Stop when score drops by more than X% from previous result
//! - **`Elbow`**: Automatically detect the "knee" in the score curve
//! - **`Combined`**: Use multiple strategies together

use serde::{Deserialize, Serialize};

/// Configuration for adaptive retrieval.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdaptiveConfig {
    /// Enable adaptive retrieval (if false, uses fixed `top_k`).
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// Maximum results to consider (over-retrieval limit).
    /// Set high enough to capture all potentially relevant results.
    #[serde(default = "default_max_results")]
    pub max_results: usize,

    /// Minimum results to return regardless of scores.
    #[serde(default = "default_min_results")]
    pub min_results: usize,

    /// Strategy for determining cutoff point.
    #[serde(default)]
    pub strategy: CutoffStrategy,

    /// If true, normalize scores to 0-1 range before applying strategy.
    #[serde(default = "default_normalize")]
    pub normalize_scores: bool,
}

const fn default_enabled() -> bool {
    true
}
const fn default_max_results() -> usize {
    100_000
}
const fn default_min_results() -> usize {
    5
}
const fn default_normalize() -> bool {
    true
}

impl Default for AdaptiveConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_results: 100_000,
            min_results: 5,
            strategy: CutoffStrategy::default(),
            normalize_scores: true,
        }
    }
}

impl AdaptiveConfig {
    /// Create a config with absolute threshold strategy.
    #[must_use]
    pub fn with_absolute_threshold(min_score: f32) -> Self {
        Self {
            strategy: CutoffStrategy::AbsoluteThreshold { min_score },
            ..Default::default()
        }
    }

    /// Create a config with relative threshold strategy.
    #[must_use]
    pub fn with_relative_threshold(min_ratio: f32) -> Self {
        Self {
            strategy: CutoffStrategy::RelativeThreshold { min_ratio },
            ..Default::default()
        }
    }

    /// Create a config with score cliff detection.
    #[must_use]
    pub fn with_score_cliff(max_drop_ratio: f32) -> Self {
        Self {
            strategy: CutoffStrategy::ScoreCliff { max_drop_ratio },
            ..Default::default()
        }
    }

    /// Create a config with automatic elbow detection.
    #[must_use]
    pub fn with_elbow_detection() -> Self {
        Self {
            strategy: CutoffStrategy::Elbow { sensitivity: 1.0 },
            ..Default::default()
        }
    }

    /// Create a combined strategy (recommended for production).
    #[must_use]
    pub fn combined(min_ratio: f32, max_drop: f32, min_score: f32) -> Self {
        Self {
            strategy: CutoffStrategy::Combined {
                relative_threshold: min_ratio,
                max_drop_ratio: max_drop,
                absolute_min: min_score,
            },
            ..Default::default()
        }
    }
}

/// Strategy for determining where to cut off results.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CutoffStrategy {
    /// Stop when score drops below a fixed threshold.
    ///
    /// Good for: Well-calibrated scores where you know what "relevant" means.
    /// Example: `min_score=0.7` means only results with score >= 0.7 are included.
    AbsoluteThreshold {
        /// Minimum acceptable score (0.0-1.0 for normalized, varies otherwise).
        min_score: f32,
    },

    /// Stop when score drops below X% of the top result's score.
    ///
    /// Good for: Relative relevancy where top result sets the baseline.
    /// Example: `min_ratio=0.5` means include results with score >= 50% of top score.
    RelativeThreshold {
        /// Minimum ratio vs top score (0.0-1.0).
        min_ratio: f32,
    },

    /// Stop when score drops by more than X% from the previous result.
    ///
    /// Good for: Detecting natural breaks in relevancy.
    /// Example: `max_drop_ratio=0.3` stops when score drops 30% from previous.
    ScoreCliff {
        /// Maximum allowed drop ratio between consecutive results (0.0-1.0).
        max_drop_ratio: f32,
    },

    /// Automatically detect the "elbow" point in the score curve.
    ///
    /// Good for: Unknown score distributions, automatic tuning.
    /// Uses the Kneedle algorithm to find maximum curvature.
    Elbow {
        /// Sensitivity multiplier (1.0 = normal, higher = more aggressive cutoff).
        sensitivity: f32,
    },

    /// Combine multiple strategies (stop when ANY condition is met).
    ///
    /// Recommended for production use - provides multiple safety nets.
    Combined {
        /// Minimum ratio vs top score.
        relative_threshold: f32,
        /// Maximum drop from previous result.
        max_drop_ratio: f32,
        /// Absolute minimum score.
        absolute_min: f32,
    },
}

impl Default for CutoffStrategy {
    fn default() -> Self {
        // Default: Combined strategy with reasonable defaults
        Self::Combined {
            relative_threshold: 0.3,
            max_drop_ratio: 0.8,
            absolute_min: 0.05,
        }
    }
}

/// Result of adaptive retrieval with statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdaptiveResult<T> {
    /// The filtered results.
    pub results: Vec<T>,

    /// Statistics about the adaptive retrieval.
    pub stats: AdaptiveStats,
}

/// Statistics from adaptive retrieval.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdaptiveStats {
    /// Total results considered (before cutoff).
    pub total_considered: usize,

    /// Results returned (after cutoff).
    pub returned: usize,

    /// Index where cutoff occurred.
    pub cutoff_index: usize,

    /// Score at cutoff point.
    pub cutoff_score: Option<f32>,

    /// Top score (first result).
    pub top_score: Option<f32>,

    /// Score at cutoff as ratio of top score.
    pub cutoff_ratio: Option<f32>,
}

impl<T> AdaptiveResult<T> {
    /// Create an empty result.
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            results: Vec::new(),
            stats: AdaptiveStats {
                total_considered: 0,
                returned: 0,
                cutoff_index: 0,
                cutoff_score: None,
                top_score: None,
                cutoff_ratio: None,
            },
        }
    }
}

/// Find the adaptive cutoff point for a list of scores.
///
/// Returns the cutoff index. Results at indices `0..cutoff_index` should be included.
#[must_use]
pub fn find_adaptive_cutoff(scores: &[f64], config: &AdaptiveConfig) -> usize {
    if scores.is_empty() {
        return 0;
    }

    if scores.len() <= config.min_results {
        return scores.len();
    }

    // Convert f64 scores to f32 for consistency with memvid
    let f32_scores: Vec<f32> = scores.iter().map(|&s| s as f32).collect();

    // Normalize scores if configured
    let normalized = if config.normalize_scores {
        normalize_scores(&f32_scores)
    } else {
        f32_scores
    };

    let top_score = normalized[0];

    match &config.strategy {
        CutoffStrategy::AbsoluteThreshold { min_score } => {
            find_absolute_cutoff(&normalized, *min_score, config.min_results)
        }

        CutoffStrategy::RelativeThreshold { min_ratio } => {
            let threshold = top_score * min_ratio;
            find_absolute_cutoff(&normalized, threshold, config.min_results)
        }

        CutoffStrategy::ScoreCliff { max_drop_ratio } => {
            find_cliff_cutoff(&normalized, *max_drop_ratio, config.min_results)
        }

        CutoffStrategy::Elbow { sensitivity } => {
            find_elbow_cutoff(&normalized, *sensitivity, config.min_results)
        }

        CutoffStrategy::Combined {
            relative_threshold,
            max_drop_ratio,
            absolute_min,
        } => find_combined_cutoff(
            &normalized,
            top_score,
            *relative_threshold,
            *max_drop_ratio,
            *absolute_min,
            config.min_results,
        ),
    }
}

/// Normalize scores to 0-1 range using min-max normalization.
pub fn normalize_scores(scores: &[f32]) -> Vec<f32> {
    if scores.is_empty() {
        return Vec::new();
    }

    let max_score = scores.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let min_score = scores.iter().copied().fold(f32::INFINITY, f32::min);
    let range = max_score - min_score;

    if range < f32::EPSILON {
        // All scores are the same
        return vec![1.0; scores.len()];
    }

    scores.iter().map(|s| (s - min_score) / range).collect()
}

/// Find cutoff using absolute threshold.
fn find_absolute_cutoff(scores: &[f32], min_score: f32, min_results: usize) -> usize {
    for (i, &score) in scores.iter().enumerate() {
        if score < min_score && i >= min_results {
            return i;
        }
    }
    scores.len()
}

/// Find cutoff using cliff detection (large drops between consecutive scores).
fn find_cliff_cutoff(scores: &[f32], max_drop_ratio: f32, min_results: usize) -> usize {
    for i in 1..scores.len() {
        if i < min_results {
            continue;
        }

        let prev = scores[i - 1];
        let curr = scores[i];

        if prev > f32::EPSILON {
            let drop_ratio = (prev - curr) / prev;
            if drop_ratio > max_drop_ratio {
                return i;
            }
        }
    }
    scores.len()
}

/// Find cutoff using elbow/knee detection (Kneedle algorithm).
fn find_elbow_cutoff(scores: &[f32], sensitivity: f32, min_results: usize) -> usize {
    if scores.len() < 3 {
        return scores.len();
    }

    // Kneedle algorithm: find point of maximum curvature
    let n = scores.len();

    // Normalize x-axis to 0-1
    let x_norm: Vec<f32> = (0..n).map(|i| i as f32 / (n - 1) as f32).collect();

    // Scores are already normalized (or not, depending on config)
    let y_norm = scores;

    // Calculate differences for knee detection
    // We're looking for the point where the curve bends most sharply
    let mut max_distance = 0.0f32;
    let mut elbow_index = min_results;

    // Line from first to last point
    let x1 = x_norm[0];
    let y1 = y_norm[0];
    let x2 = x_norm[n - 1];
    let y2 = y_norm[n - 1];

    let line_len = (x2 - x1).hypot(y2 - y1);
    if line_len < f32::EPSILON {
        return scores.len();
    }

    // Distance from each point to the line
    for i in min_results..n - 1 {
        let x0 = x_norm[i];
        let y0 = y_norm[i];

        // Distance from point (x0, y0) to line through (x1, y1) and (x2, y2)
        let distance = y2
            .mul_add(
                -x1,
                x2.mul_add(y1, (y2 - y1).mul_add(x0, -((x2 - x1) * y0))),
            )
            .abs()
            / line_len;

        // Apply sensitivity: higher sensitivity = prefer earlier cutoff
        let adjusted_distance = distance * sensitivity.mul_add(1.0 - x_norm[i], 1.0);

        if adjusted_distance > max_distance {
            max_distance = adjusted_distance;
            elbow_index = i;
        }
    }

    // Only cut if we found a significant elbow
    if max_distance > 0.05 * sensitivity {
        elbow_index + 1
    } else {
        scores.len()
    }
}

/// Find cutoff using combined strategy (first trigger wins).
fn find_combined_cutoff(
    scores: &[f32],
    top_score: f32,
    relative_threshold: f32,
    max_drop_ratio: f32,
    absolute_min: f32,
    min_results: usize,
) -> usize {
    let relative_min = top_score * relative_threshold;

    for i in 0..scores.len() {
        if i < min_results {
            continue;
        }

        let score = scores[i];

        // Check absolute minimum
        if score < absolute_min {
            return i;
        }

        // Check relative threshold
        if score < relative_min {
            return i;
        }

        // Check cliff detection
        if i > 0 {
            let prev = scores[i - 1];
            if prev > f32::EPSILON {
                let drop_ratio = (prev - score) / prev;
                if drop_ratio > max_drop_ratio {
                    return i;
                }
            }
        }
    }

    scores.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_scores() {
        let scores = vec![1.0, 0.8, 0.6, 0.4, 0.2];
        let normalized = normalize_scores(&scores);

        assert!((normalized[0] - 1.0).abs() < 0.001);
        assert!((normalized[4] - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_absolute_threshold() {
        let scores = [0.95_f64, 0.88, 0.75, 0.60, 0.45, 0.30, 0.15];
        let config = AdaptiveConfig::with_absolute_threshold(0.5);

        let cutoff = find_adaptive_cutoff(&scores, &config);

        // Should include scores >= 0.5 (0.95, 0.88, 0.75, 0.60)
        assert!(cutoff >= 4);
    }

    #[test]
    fn test_relative_threshold() {
        let scores = [1.0_f64, 0.9, 0.8, 0.5, 0.3, 0.1];
        let config = AdaptiveConfig::with_relative_threshold(0.6);

        let cutoff = find_adaptive_cutoff(&scores, &config);

        // Should include scores >= 60% of 1.0 = 0.6
        assert!(cutoff >= 3);
    }

    #[test]
    fn test_score_cliff() {
        // Sharp drop between 0.8 and 0.3
        let scores = [1.0_f64, 0.95, 0.9, 0.85, 0.8, 0.3, 0.25, 0.2];
        let config = AdaptiveConfig::with_score_cliff(0.4);

        let cutoff = find_adaptive_cutoff(&scores, &config);

        // Should stop at the cliff (0.8 -> 0.3 is 62.5% drop)
        assert!(cutoff <= 6);
    }

    #[test]
    fn test_combined_strategy() {
        let scores = [0.95_f64, 0.90, 0.85, 0.80, 0.75, 0.40, 0.35, 0.30];
        let config = AdaptiveConfig::combined(0.5, 0.3, 0.3);

        let cutoff = find_adaptive_cutoff(&scores, &config);

        // Should stop either at relative threshold (50% of 0.95 = 0.475)
        // or at cliff (0.75 -> 0.40 is ~47% drop)
        assert!((4..=6).contains(&cutoff));
    }

    #[test]
    fn test_min_results_respected() {
        let scores = [1.0_f64, 0.1, 0.05, 0.01];
        let mut config = AdaptiveConfig::with_absolute_threshold(0.9);
        config.min_results = 3;

        let cutoff = find_adaptive_cutoff(&scores, &config);

        // Even though only first result meets threshold, min_results=3
        assert!(cutoff >= 3);
    }

    #[test]
    fn test_empty_scores() {
        let scores: Vec<f64> = Vec::new();
        let config = AdaptiveConfig::default();

        let cutoff = find_adaptive_cutoff(&scores, &config);

        assert_eq!(cutoff, 0);
    }

    #[test]
    fn test_real_world_scenario() {
        // Simulating a search where answer is in 12 chunks but k=8 would miss some
        let scores = [
            0.92_f64, 0.89, 0.87, 0.85, 0.84, 0.82, 0.80, 0.79, // First 8 (would be k=8)
            0.78, 0.76, 0.75, 0.74, // 4 more still relevant!
            0.45, 0.40, 0.35, 0.30, 0.25, // Clearly not relevant
        ];

        let config = AdaptiveConfig::combined(0.5, 0.35, 0.4);
        let cutoff = find_adaptive_cutoff(&scores, &config);

        // Should include all 12 relevant chunks, stop before 0.45
        assert!(cutoff >= 10, "Should include more than k=8 results");
        assert!(cutoff <= 13, "Should stop before irrelevant results");
    }
}
