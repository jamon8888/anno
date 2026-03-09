//! Binary embeddings for fast candidate blocking via Hamming distance.
//!
//! Research note: binary embeddings are for blocking, not primary retrieval.
//! The sign-rank limitation means they cannot represent all similarity functions.
//! Use as a pre-filter only.

// =============================================================================

/// Binary hash for fast approximate nearest neighbor search.
///
/// # Research Background
///
/// Binary embeddings enable sub-linear search via Hamming distance. Key insight
/// from our research synthesis: **binary embeddings are for blocking, not primary
/// retrieval**. The sign-rank limitation means they cannot represent all similarity
/// relationships, but they excel at fast candidate filtering.
///
/// # Two-Stage Retrieval Pattern
///
/// ```text
/// Query → [Binary Hash] → Hamming Filter (fast) → Candidates
///                                                      ↓
///                                              [Dense Similarity]
///                                                      ↓
///                                               Final Results
/// ```
///
/// # Example
///
/// ```rust
/// use anno::backends::inference::BinaryHash;
///
/// // Create hashes from embeddings
/// let hash1 = BinaryHash::from_embedding(&[0.1, -0.2, 0.3, -0.4, 0.5, -0.6, 0.7, -0.8]);
/// let hash2 = BinaryHash::from_embedding(&[0.15, -0.25, 0.35, -0.45, 0.55, -0.65, 0.75, -0.85]);
///
/// // Similar embeddings → low Hamming distance
/// assert!(hash1.hamming_distance(&hash2) < 2);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BinaryHash {
    /// Packed bits (each u64 holds 64 bits)
    pub bits: Vec<u64>,
    /// Original dimension (number of bits)
    pub dim: usize,
}

impl BinaryHash {
    /// Create from a dense embedding using sign function.
    ///
    /// Each positive value → 1, each negative/zero value → 0.
    #[must_use]
    pub fn from_embedding(embedding: &[f32]) -> Self {
        let dim = embedding.len();
        let num_u64s = dim.div_ceil(64);
        let mut bits = vec![0u64; num_u64s];

        for (i, &val) in embedding.iter().enumerate() {
            if val > 0.0 {
                let word_idx = i / 64;
                let bit_idx = i % 64;
                bits[word_idx] |= 1u64 << bit_idx;
            }
        }

        Self { bits, dim }
    }

    /// Create from a dense f64 embedding.
    #[must_use]
    pub fn from_embedding_f64(embedding: &[f64]) -> Self {
        let dim = embedding.len();
        let num_u64s = dim.div_ceil(64);
        let mut bits = vec![0u64; num_u64s];

        for (i, &val) in embedding.iter().enumerate() {
            if val > 0.0 {
                let word_idx = i / 64;
                let bit_idx = i % 64;
                bits[word_idx] |= 1u64 << bit_idx;
            }
        }

        Self { bits, dim }
    }

    /// Compute Hamming distance (number of differing bits).
    ///
    /// Uses POPCNT instruction when available for hardware acceleration.
    #[must_use]
    pub fn hamming_distance(&self, other: &Self) -> u32 {
        self.bits
            .iter()
            .zip(other.bits.iter())
            .map(|(a, b)| (a ^ b).count_ones())
            .sum()
    }

    /// Compute normalized Hamming distance (0.0 to 1.0).
    #[must_use]
    pub fn hamming_distance_normalized(&self, other: &Self) -> f64 {
        if self.dim == 0 {
            return 0.0;
        }
        self.hamming_distance(other) as f64 / self.dim as f64
    }

    /// Convert Hamming distance to approximate cosine similarity.
    ///
    /// Based on the relationship: cos(θ) ≈ 1 - 2 * (hamming_distance / dim)
    /// This is an approximation valid for random hyperplane hashing.
    #[must_use]
    pub fn approximate_cosine(&self, other: &Self) -> f64 {
        1.0 - 2.0 * self.hamming_distance_normalized(other)
    }
}

/// Blocker using binary embeddings for fast candidate filtering.
///
/// # Usage Pattern
///
/// 1. Pre-compute binary hashes for all entities in your KB
/// 2. At query time, hash the query embedding
/// 3. Find candidates within Hamming distance threshold
/// 4. Run dense similarity only on candidates
///
/// # Example
///
/// ```rust
/// use anno::backends::inference::{BinaryBlocker, BinaryHash};
///
/// let mut blocker = BinaryBlocker::new(8); // 8-bit Hamming threshold
///
/// // Add entities to the index
/// let hash1 = BinaryHash::from_embedding(&vec![0.1; 768]);
/// let hash2 = BinaryHash::from_embedding(&vec![-0.1; 768]);
/// blocker.add(0, hash1);
/// blocker.add(1, hash2);
///
/// // Query
/// let query = BinaryHash::from_embedding(&vec![0.1; 768]);
/// let candidates = blocker.query(&query);
/// assert!(candidates.contains(&0)); // Similar to hash1
/// ```
#[derive(Debug, Clone)]
pub struct BinaryBlocker {
    /// Hamming distance threshold for candidates
    pub threshold: u32,
    /// Index of hashes by ID
    index: Vec<(usize, BinaryHash)>,
}

impl BinaryBlocker {
    /// Create a new blocker with the given threshold.
    #[must_use]
    pub fn new(threshold: u32) -> Self {
        Self {
            threshold,
            index: Vec::new(),
        }
    }

    /// Add an entity to the index.
    pub fn add(&mut self, id: usize, hash: BinaryHash) {
        self.index.push((id, hash));
    }

    /// Add multiple entities.
    pub fn add_batch(&mut self, entries: impl IntoIterator<Item = (usize, BinaryHash)>) {
        self.index.extend(entries);
    }

    /// Find candidate IDs within Hamming distance threshold.
    #[must_use]
    pub fn query(&self, query: &BinaryHash) -> Vec<usize> {
        self.index
            .iter()
            .filter(|(_, hash)| hash.hamming_distance(query) <= self.threshold)
            .map(|(id, _)| *id)
            .collect()
    }

    /// Find candidates with their distances.
    #[must_use]
    pub fn query_with_distance(&self, query: &BinaryHash) -> Vec<(usize, u32)> {
        self.index
            .iter()
            .map(|(id, hash)| (*id, hash.hamming_distance(query)))
            .filter(|(_, dist)| *dist <= self.threshold)
            .collect()
    }

    /// Number of entries in the index.
    #[must_use]
    pub fn len(&self) -> usize {
        self.index.len()
    }

    /// Check if index is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    /// Clear the index.
    pub fn clear(&mut self) {
        self.index.clear();
    }
}

/// Recommended two-stage retrieval using binary blocking + dense reranking.
///
/// # Research Context
///
/// This implements the pattern identified in our research synthesis:
/// - Stage 1: Binary blocking for O(n) candidate filtering
/// - Stage 2: Dense similarity for accurate ranking
///
/// The key insight is that binary embeddings have fundamental limitations
/// (sign-rank theorem) but excel at fast filtering.
///
/// # Arguments
///
/// * `query_embedding` - Dense query embedding
/// * `candidate_embeddings` - Dense embeddings of all candidates
/// * `binary_threshold` - Hamming distance threshold for blocking
/// * `top_k` - Number of final results to return
///
/// # Returns
///
/// Vector of (candidate_index, similarity_score) pairs, sorted by score descending.
#[must_use]
pub fn two_stage_retrieval(
    query_embedding: &[f32],
    candidate_embeddings: &[Vec<f32>],
    binary_threshold: u32,
    top_k: usize,
) -> Vec<(usize, f32)> {
    // Stage 1: Binary blocking
    let query_hash = BinaryHash::from_embedding(query_embedding);

    let candidate_hashes: Vec<BinaryHash> = candidate_embeddings
        .iter()
        .map(|e| BinaryHash::from_embedding(e))
        .collect();

    let mut blocker = BinaryBlocker::new(binary_threshold);
    for (i, hash) in candidate_hashes.into_iter().enumerate() {
        blocker.add(i, hash);
    }

    let candidates = blocker.query(&query_hash);

    // Stage 2: Dense similarity on candidates only
    // Performance: Pre-allocate scored vec with known size
    let mut scored: Vec<(usize, f32)> = Vec::with_capacity(candidates.len());
    scored.extend(candidates.into_iter().map(|idx| {
        let sim = innr::cosine(query_embedding, &candidate_embeddings[idx]);
        (idx, sim)
    }));

    // Performance: Use unstable sort (we don't need stable sort here)
    // Sort by similarity descending
    scored.sort_unstable_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(top_k);
    scored
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── BinaryHash: sign quantization ────────────────────────────────

    #[test]
    fn from_embedding_sign_quantization() {
        // positive → 1, negative → 0, zero → 0
        let h = BinaryHash::from_embedding(&[1.0, -1.0, 0.5, 0.0, -0.5, 0.0, 0.1, -0.1]);
        assert_eq!(h.dim, 8);
        // bits[0]: bit0=1, bit1=0, bit2=1, bit3=0, bit4=0, bit5=0, bit6=1, bit7=0
        //        = 0b0100_0101 = 0x45
        assert_eq!(h.bits[0] & 0xFF, 0b0100_0101);
    }

    #[test]
    fn from_embedding_f64_matches_f32() {
        let f32_vals: Vec<f32> = vec![0.1, -0.2, 0.3, -0.4, 0.5, -0.6, 0.7, -0.8];
        let f64_vals: Vec<f64> = f32_vals.iter().map(|&v| v as f64).collect();
        let h32 = BinaryHash::from_embedding(&f32_vals);
        let h64 = BinaryHash::from_embedding_f64(&f64_vals);
        assert_eq!(h32.bits, h64.bits);
        assert_eq!(h32.dim, h64.dim);
    }

    // ── Edge cases ───────────────────────────────────────────────────

    #[test]
    fn zero_vector_all_bits_zero() {
        let h = BinaryHash::from_embedding(&[0.0; 128]);
        assert!(h.bits.iter().all(|&w| w == 0));
        assert_eq!(h.dim, 128);
    }

    #[test]
    fn all_positive_all_bits_set() {
        let h = BinaryHash::from_embedding(&[1.0; 64]);
        assert_eq!(h.bits.len(), 1);
        assert_eq!(h.bits[0], u64::MAX);
    }

    #[test]
    fn all_negative_all_bits_zero() {
        let h = BinaryHash::from_embedding(&[-1.0; 64]);
        assert_eq!(h.bits.len(), 1);
        assert_eq!(h.bits[0], 0);
    }

    #[test]
    fn empty_embedding() {
        let h = BinaryHash::from_embedding(&[]);
        assert_eq!(h.dim, 0);
        assert!(h.bits.is_empty());
    }

    #[test]
    fn non_multiple_of_64_dim() {
        // 65 dims → 2 u64 words, only bit 0 of second word used
        let mut vals = vec![-1.0f32; 65];
        vals[64] = 1.0; // last element positive
        let h = BinaryHash::from_embedding(&vals);
        assert_eq!(h.bits.len(), 2);
        assert_eq!(h.bits[0], 0);
        assert_eq!(h.bits[1], 1); // only bit 0 set
    }

    // ── Hamming distance ─────────────────────────────────────────────

    #[test]
    fn hamming_distance_identical() {
        let h = BinaryHash::from_embedding(&[1.0, -1.0, 0.5, -0.5]);
        assert_eq!(h.hamming_distance(&h), 0);
    }

    #[test]
    fn hamming_distance_known() {
        // All positive vs all negative → every bit differs
        let a = BinaryHash::from_embedding(&[1.0; 64]);
        let b = BinaryHash::from_embedding(&[-1.0; 64]);
        assert_eq!(a.hamming_distance(&b), 64);
    }

    #[test]
    fn hamming_distance_single_flip() {
        let a = BinaryHash::from_embedding(&[1.0, 1.0, 1.0, 1.0]);
        let b = BinaryHash::from_embedding(&[1.0, 1.0, -1.0, 1.0]); // bit 2 flipped
        assert_eq!(a.hamming_distance(&b), 1);
    }

    #[test]
    fn hamming_distance_symmetry() {
        let a = BinaryHash::from_embedding(&[0.1, -0.2, 0.3, -0.4, 0.5]);
        let b = BinaryHash::from_embedding(&[-0.1, 0.2, 0.3, 0.4, -0.5]);
        assert_eq!(a.hamming_distance(&b), b.hamming_distance(&a));
    }

    #[test]
    fn hamming_distance_empty() {
        let a = BinaryHash::from_embedding(&[]);
        let b = BinaryHash::from_embedding(&[]);
        assert_eq!(a.hamming_distance(&b), 0);
    }

    // ── Normalized distance & approximate cosine ─────────────────────

    #[test]
    fn normalized_distance_range() {
        let a = BinaryHash::from_embedding(&[1.0, -1.0, 1.0, -1.0]);
        let b = BinaryHash::from_embedding(&[-1.0, 1.0, 1.0, -1.0]);
        let norm = a.hamming_distance_normalized(&b);
        assert!((0.0..=1.0).contains(&norm));
        // 2 out of 4 bits differ → 0.5
        assert!((norm - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn normalized_distance_empty_is_zero() {
        let a = BinaryHash::from_embedding(&[]);
        let b = BinaryHash::from_embedding(&[]);
        assert_eq!(a.hamming_distance_normalized(&b), 0.0);
    }

    #[test]
    fn approximate_cosine_identical() {
        let h = BinaryHash::from_embedding(&[1.0, -1.0, 0.5, -0.5]);
        assert!((h.approximate_cosine(&h) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn approximate_cosine_opposite() {
        let a = BinaryHash::from_embedding(&[1.0; 64]);
        let b = BinaryHash::from_embedding(&[-1.0; 64]);
        assert!((a.approximate_cosine(&b) - (-1.0)).abs() < f64::EPSILON);
    }

    #[test]
    fn approximate_cosine_symmetry() {
        let a = BinaryHash::from_embedding(&[0.1, -0.3, 0.5, -0.7, 0.9]);
        let b = BinaryHash::from_embedding(&[-0.2, 0.4, 0.5, -0.6, -0.8]);
        assert!((a.approximate_cosine(&b) - b.approximate_cosine(&a)).abs() < f64::EPSILON);
    }

    // ── BinaryBlocker ────────────────────────────────────────────────

    #[test]
    fn blocker_query_finds_nearby() {
        let mut blocker = BinaryBlocker::new(2);
        let h1 = BinaryHash::from_embedding(&[1.0, 1.0, 1.0, 1.0]);
        let h2 = BinaryHash::from_embedding(&[-1.0, -1.0, -1.0, -1.0]);
        blocker.add(0, h1);
        blocker.add(1, h2);

        let query = BinaryHash::from_embedding(&[1.0, 1.0, 1.0, -1.0]); // 1 bit from h1
        let results = blocker.query(&query);
        assert!(results.contains(&0));
        assert!(!results.contains(&1)); // 3 bits differ, > threshold 2
    }

    #[test]
    fn blocker_query_with_distance() {
        let mut blocker = BinaryBlocker::new(10);
        let h = BinaryHash::from_embedding(&[1.0; 8]);
        blocker.add(42, h);

        let query = BinaryHash::from_embedding(&[1.0, 1.0, 1.0, -1.0, -1.0, -1.0, -1.0, -1.0]);
        let results = blocker.query_with_distance(&query);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0], (42, 5)); // bits 3..8 differ
    }

    #[test]
    fn blocker_add_batch() {
        let mut blocker = BinaryBlocker::new(0);
        let entries = vec![
            (0, BinaryHash::from_embedding(&[1.0; 8])),
            (1, BinaryHash::from_embedding(&[-1.0; 8])),
        ];
        blocker.add_batch(entries);
        assert_eq!(blocker.len(), 2);
        assert!(!blocker.is_empty());
    }

    #[test]
    fn blocker_clear() {
        let mut blocker = BinaryBlocker::new(5);
        blocker.add(0, BinaryHash::from_embedding(&[1.0; 8]));
        assert_eq!(blocker.len(), 1);
        blocker.clear();
        assert!(blocker.is_empty());
    }

    // ── two_stage_retrieval ──────────────────────────────────────────

    #[test]
    fn two_stage_retrieval_basic() {
        let query = vec![1.0; 8];
        let candidates = vec![
            vec![1.0; 8],                                     // identical → should rank first
            vec![-1.0; 8], // opposite → should be filtered or rank last
            vec![1.0, 1.0, 1.0, 1.0, -1.0, -1.0, -1.0, -1.0], // partial match
        ];
        let results = two_stage_retrieval(&query, &candidates, 8, 3);
        assert!(!results.is_empty());
        // First result should be the identical vector
        assert_eq!(results[0].0, 0);
        assert!((results[0].1 - 1.0).abs() < 1e-6);
    }

    #[test]
    fn two_stage_retrieval_respects_top_k() {
        let query = vec![1.0; 8];
        let candidates: Vec<Vec<f32>> = (0..10).map(|_| vec![1.0; 8]).collect();
        let results = two_stage_retrieval(&query, &candidates, 64, 3);
        assert!(results.len() <= 3);
    }
}
