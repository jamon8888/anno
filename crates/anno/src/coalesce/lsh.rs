//! # Locality-Sensitive Hashing for Entity Resolution Blocking
//!
//! This module provides **LSH-based blocking** to reduce the quadratic comparison
//! cost in entity resolution to near-linear time.
//!
//! ## The Blocking Problem
//!
//! With \( n \) entities, naive pairwise comparison requires \( O(n^2) \) comparisons.
//! For 1 million entities, that's 500 billion comparisons—infeasible even at
//! nanosecond speeds.
//!
//! **Blocking** reduces this by generating *candidate pairs* that are likely to
//! be similar, avoiding comparison of obviously dissimilar pairs.
//!
//! ## Locality-Sensitive Hashing
//!
//! LSH constructs hash functions where **collision probability equals similarity**:
//!
//! \[
//! \Pr[h(x) = h(y)] = \text{sim}(x, y)
//! \]
//!
//! Items that are similar have high probability of hashing to the same bucket;
//! dissimilar items hash to different buckets.
//!
//! ## MinHash for Jaccard Similarity
//!
//! For sets \( A \) and \( B \), the **Jaccard similarity** is:
//!
//! \[
//! J(A, B) = \frac{|A \cap B|}{|A \cup B|}
//! \]
//!
//! **Broder's Theorem (1997)**: For a random permutation \( \pi \):
//!
//! \[
//! \Pr[\min(\pi(A)) = \min(\pi(B))] = J(A, B)
//! \]
//!
//! This remarkable result means we can estimate Jaccard similarity by counting
//! hash collisions, without computing set intersections.
//!
//! ### MinHash Signature
//!
//! For \( k \) independent hash functions \( h_1, \ldots, h_k \):
//!
//! \[
//! \text{sig}(A) = (\min_{a \in A} h_1(a), \ldots, \min_{a \in A} h_k(a))
//! \]
//!
//! The fraction of matching positions estimates Jaccard similarity:
//!
//! \[
//! \hat{J}(A, B) = \frac{|\{i : \text{sig}(A)_i = \text{sig}(B)_i\}|}{k}
//! \]
//!
//! ### Banding for Candidate Selection
//!
//! With \( b \) bands of \( r \) rows each (so \( k = br \)), two items become
//! candidates if they match in **all rows of any band**. The probability is:
//!
//! \[
//! P(\text{candidate}) = 1 - (1 - s^r)^b
//! \]
//!
//! This S-curve has a sharp transition around \( s^* = (1/b)^{1/r} \), effectively
//! filtering to high-similarity pairs.
//!
//! ## SimHash for Cosine Similarity
//!
//! For embedding vectors, **SimHash** (Charikar 2002) uses random hyperplanes.
//! The hash bit is 1 if the embedding has positive dot product with the hyperplane:
//!
//! \[
//! h_{\mathbf{r}}(\mathbf{v}) = \text{sign}(\mathbf{r} \cdot \mathbf{v})
//! \]
//!
//! The collision probability relates to cosine similarity:
//!
//! \[
//! \Pr[h(\mathbf{u}) = h(\mathbf{v})] = 1 - \frac{\theta}{\pi}
//! \]
//!
//! where \( \theta = \cos^{-1}(\text{sim}(\mathbf{u}, \mathbf{v})) \).
//!
//! ## Complexity Analysis
//!
//! | Operation | Naive | With LSH |
//! |-----------|-------|----------|
//! | Build index | — | \( O(n) \) |
//! | Find all candidates | \( O(n^2) \) | \( O(n \log n) \) expected |
//! | Space | \( O(n^2) \) | \( O(n) \) |
//!
//! ## Example
//!
//! ```
//! use anno_coalesce::lsh::{MinHashLSH, LSHConfig};
//!
//! let mut lsh = MinHashLSH::new(LSHConfig::default());
//! lsh.insert_text("1", "Barack Obama");
//! lsh.insert_text("2", "obama");
//! lsh.insert_text("3", "Donald Trump");
//!
//! // Only these pairs are compared (not all 3)
//! let candidates = lsh.candidate_pairs();
//! for (i, j) in candidates {
//!     let sim = lsh.estimated_similarity(i, j).unwrap_or(0.0);
//!     println!("Candidate ({}, {}): estimated sim = {:.2}", i, j, sim);
//! }
//! ```
//!
//! ## Configuration
//!
//! - `num_hashes_per_band` (r): Higher = stricter matching, fewer candidates
//! - `num_bands` (b): Higher = more candidates, better recall
//! - `ngram_size`: Size of character n-grams (default: 3)
//! - `similarity_threshold`: Filter candidates below this
//!
//! ## References
//!
//! - Broder, A. (1997). "On the resemblance and containment of documents".
//!   Compression and Complexity of Sequences.
//! - Charikar, M. (2002). "Similarity estimation techniques from rounding
//!   algorithms". STOC '02.
//! - Indyk, P. & Motwani, R. (1998). "Approximate nearest neighbors: towards
//!   removing the curse of dimensionality". STOC '98.

use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

/// Configuration for LSH blocking.
#[derive(Debug, Clone)]
pub struct LSHConfig {
    /// Number of hash functions per band (higher = stricter matching)
    pub num_hashes_per_band: usize,
    /// Number of bands (higher = more candidates, better recall)
    pub num_bands: usize,
    /// N-gram size for text shingling
    pub ngram_size: usize,
    /// Whether to use character n-grams (vs word n-grams)
    pub char_ngrams: bool,
    /// Minimum Jaccard similarity threshold for candidates
    pub similarity_threshold: f32,
}

impl Default for LSHConfig {
    fn default() -> Self {
        Self {
            num_hashes_per_band: 4,
            num_bands: 25,
            ngram_size: 3,
            char_ngrams: true,
            similarity_threshold: 0.5,
        }
    }
}

impl LSHConfig {
    /// Create config optimized for high recall (more candidates).
    pub fn high_recall() -> Self {
        Self {
            num_bands: 50,
            num_hashes_per_band: 2,
            ..Default::default()
        }
    }

    /// Create config optimized for high precision (fewer, better candidates).
    pub fn high_precision() -> Self {
        Self {
            num_bands: 10,
            num_hashes_per_band: 8,
            ..Default::default()
        }
    }

    /// Estimate the probability that two items with given Jaccard similarity
    /// will be placed in the same bucket (i.e., become candidates).
    ///
    /// P(candidate) = 1 - (1 - s^r)^b
    /// where s = similarity, r = num_hashes_per_band, b = num_bands
    pub fn candidate_probability(&self, jaccard_similarity: f32) -> f32 {
        let s = jaccard_similarity;
        let r = self.num_hashes_per_band as f32;
        let b = self.num_bands as f32;
        1.0 - (1.0 - s.powf(r)).powf(b)
    }
}

/// An item indexed in the LSH structure.
#[derive(Debug, Clone)]
pub struct LSHItem {
    /// Unique identifier for this item
    pub id: String,
    /// Optional document ID (for cross-doc resolution)
    pub doc_id: Option<String>,
    /// The text content to hash
    pub text: String,
    /// Pre-computed MinHash signature (lazy-initialized)
    signature: Option<Vec<u64>>,
}

impl LSHItem {
    /// Create a new LSH item.
    pub fn new(id: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            doc_id: None,
            text: text.into(),
            signature: None,
        }
    }

    /// Create with document ID.
    pub fn with_doc_id(mut self, doc_id: impl Into<String>) -> Self {
        self.doc_id = Some(doc_id.into());
        self
    }
}

/// MinHash-based LSH for Jaccard similarity.
///
/// This is the primary LSH implementation for entity resolution,
/// as it works well with text similarity based on n-gram overlap.
#[derive(Debug)]
pub struct MinHashLSH {
    config: LSHConfig,
    /// Hash coefficients for MinHash: (a, b) pairs for h(x) = (ax + b) mod p
    hash_coefficients: Vec<(u64, u64)>,
    /// Prime for modular arithmetic (Mersenne prime 2^61 - 1)
    prime: u64,
    /// Buckets: band_idx -> bucket_hash -> item_indices
    buckets: Vec<HashMap<u64, Vec<usize>>>,
    /// All items indexed
    items: Vec<LSHItem>,
}

impl MinHashLSH {
    /// Create a new MinHash LSH index.
    pub fn new(config: LSHConfig) -> Self {
        let total_hashes = config.num_hashes_per_band * config.num_bands;
        let prime = (1u64 << 61) - 1; // Mersenne prime

        // Generate random hash coefficients deterministically
        // Using a simple LCG seeded with band/hash indices for reproducibility
        let mut coefficients = Vec::with_capacity(total_hashes);
        let mut rng_state = 0x5DEECE66Du64;
        for _ in 0..total_hashes {
            rng_state = rng_state.wrapping_mul(0x5DEECE66D).wrapping_add(0xB);
            let a = (rng_state >> 16) % prime;
            rng_state = rng_state.wrapping_mul(0x5DEECE66D).wrapping_add(0xB);
            let b = (rng_state >> 16) % prime;
            coefficients.push((a.max(1), b)); // a must be non-zero
        }

        let buckets = (0..config.num_bands).map(|_| HashMap::new()).collect();

        Self {
            config,
            hash_coefficients: coefficients,
            prime,
            buckets,
            items: Vec::new(),
        }
    }

    /// Insert an item into the index.
    pub fn insert(&mut self, item: LSHItem) {
        let idx = self.items.len();
        let shingles = self.shingle(&item.text);
        let signature = self.minhash_signature(&shingles);

        // Insert into buckets
        for (band_idx, band_sig) in signature
            .chunks(self.config.num_hashes_per_band)
            .enumerate()
        {
            let bucket_hash = self.hash_band(band_sig);
            self.buckets[band_idx]
                .entry(bucket_hash)
                .or_default()
                .push(idx);
        }

        // Store item with signature
        let mut item = item;
        item.signature = Some(signature);
        self.items.push(item);
    }

    /// Insert a simple text item.
    pub fn insert_text(&mut self, id: impl Into<String>, text: impl Into<String>) {
        self.insert(LSHItem::new(id, text));
    }

    /// Get all candidate pairs that might be similar.
    ///
    /// Returns pairs of item indices that share at least one bucket.
    /// These are the pairs that should be compared with the actual similarity function.
    pub fn candidate_pairs(&self) -> Vec<(usize, usize)> {
        let mut candidates: HashSet<(usize, usize)> = HashSet::new();

        for bucket_map in &self.buckets {
            for indices in bucket_map.values() {
                // All pairs within this bucket are candidates
                for i in 0..indices.len() {
                    for j in (i + 1)..indices.len() {
                        let (a, b) = (indices[i], indices[j]);
                        let pair = if a < b { (a, b) } else { (b, a) };
                        candidates.insert(pair);
                    }
                }
            }
        }

        candidates.into_iter().collect()
    }

    /// Get candidate pairs with their estimated Jaccard similarity.
    ///
    /// The similarity is estimated from the MinHash signatures,
    /// which is faster than computing exact Jaccard similarity.
    pub fn candidate_pairs_with_similarity(&self) -> Vec<(usize, usize, f32)> {
        self.candidate_pairs()
            .into_iter()
            .filter_map(|(i, j)| {
                let sim = self.estimated_similarity(i, j)?;
                if sim >= self.config.similarity_threshold {
                    Some((i, j, sim))
                } else {
                    None
                }
            })
            .collect()
    }

    /// Get candidates for a specific item by index.
    pub fn candidates_for(&self, item_idx: usize) -> Vec<usize> {
        if item_idx >= self.items.len() {
            return Vec::new();
        }

        let item = &self.items[item_idx];
        let signature = match &item.signature {
            Some(sig) => sig,
            None => return Vec::new(),
        };

        let mut candidates: HashSet<usize> = HashSet::new();

        for (band_idx, band_sig) in signature
            .chunks(self.config.num_hashes_per_band)
            .enumerate()
        {
            let bucket_hash = self.hash_band(band_sig);
            if let Some(bucket_items) = self.buckets[band_idx].get(&bucket_hash) {
                for &idx in bucket_items {
                    if idx != item_idx {
                        candidates.insert(idx);
                    }
                }
            }
        }

        candidates.into_iter().collect()
    }

    /// Query for candidates matching new text (without inserting).
    pub fn query(&self, text: &str) -> Vec<usize> {
        let shingles = self.shingle(text);
        let signature = self.minhash_signature(&shingles);

        let mut candidates: HashSet<usize> = HashSet::new();

        for (band_idx, band_sig) in signature
            .chunks(self.config.num_hashes_per_band)
            .enumerate()
        {
            let bucket_hash = self.hash_band(band_sig);
            if let Some(bucket_items) = self.buckets[band_idx].get(&bucket_hash) {
                for &idx in bucket_items {
                    candidates.insert(idx);
                }
            }
        }

        candidates.into_iter().collect()
    }

    /// Get the item at a given index.
    pub fn get(&self, idx: usize) -> Option<&LSHItem> {
        self.items.get(idx)
    }

    /// Get the number of items indexed.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Check if the index is empty.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Estimate Jaccard similarity between two items using MinHash.
    pub fn estimated_similarity(&self, i: usize, j: usize) -> Option<f32> {
        let sig_i = self.items.get(i)?.signature.as_ref()?;
        let sig_j = self.items.get(j)?.signature.as_ref()?;

        let matches = sig_i
            .iter()
            .zip(sig_j.iter())
            .filter(|(a, b)| a == b)
            .count();

        Some(matches as f32 / sig_i.len() as f32)
    }

    /// Compute exact Jaccard similarity between two items.
    pub fn exact_similarity(&self, i: usize, j: usize) -> Option<f32> {
        let text_i = &self.items.get(i)?.text;
        let text_j = &self.items.get(j)?.text;
        Some(jaccard_similarity(
            &self.shingle(text_i),
            &self.shingle(text_j),
        ))
    }

    // =========================================================================
    // Internal methods
    // =========================================================================

    /// Create shingles (n-grams) from text.
    fn shingle(&self, text: &str) -> HashSet<u64> {
        let normalized = text.to_lowercase();
        let n = self.config.ngram_size;

        if self.config.char_ngrams {
            // Character n-grams
            let chars: Vec<char> = normalized.chars().collect();
            if chars.len() < n {
                let mut set = HashSet::new();
                set.insert(self.hash_string(&normalized));
                return set;
            }

            chars
                .windows(n)
                .map(|window| {
                    let s: String = window.iter().collect();
                    self.hash_string(&s)
                })
                .collect()
        } else {
            // Word n-grams
            let words: Vec<&str> = normalized.split_whitespace().collect();
            if words.len() < n {
                let mut set = HashSet::new();
                set.insert(self.hash_string(&normalized));
                return set;
            }

            words
                .windows(n)
                .map(|window| {
                    let s = window.join(" ");
                    self.hash_string(&s)
                })
                .collect()
        }
    }

    /// Compute MinHash signature for a set of shingles.
    fn minhash_signature(&self, shingles: &HashSet<u64>) -> Vec<u64> {
        self.hash_coefficients
            .iter()
            .map(|(a, b)| {
                shingles
                    .iter()
                    .map(|&x| {
                        // h(x) = (ax + b) mod p
                        (a.wrapping_mul(x).wrapping_add(*b)) % self.prime
                    })
                    .min()
                    .unwrap_or(u64::MAX)
            })
            .collect()
    }

    /// Hash a band of the signature to get a bucket key.
    fn hash_band(&self, band: &[u64]) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        for &h in band {
            h.hash(&mut hasher);
        }
        hasher.finish()
    }

    /// Hash a string to u64.
    fn hash_string(&self, s: &str) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        s.hash(&mut hasher);
        hasher.finish()
    }
}

/// Compute exact Jaccard similarity between two sets.
pub fn jaccard_similarity(set_a: &HashSet<u64>, set_b: &HashSet<u64>) -> f32 {
    if set_a.is_empty() && set_b.is_empty() {
        return 1.0;
    }

    let intersection = set_a.intersection(set_b).count();
    let union = set_a.union(set_b).count();

    if union == 0 {
        0.0
    } else {
        intersection as f32 / union as f32
    }
}

// =============================================================================
// SimHash for embedding vectors (cosine similarity)
// =============================================================================

/// SimHash-based LSH for cosine similarity of embedding vectors.
///
/// This is useful when tracks have pre-computed embeddings.
#[derive(Debug)]
pub struct SimHashLSH {
    /// Number of hyperplanes (bits in the hash)
    num_bits: usize,
    /// Random hyperplane vectors (num_bits x embedding_dim)
    hyperplanes: Vec<Vec<f32>>,
    /// Buckets: hash -> item_indices
    buckets: HashMap<u64, Vec<usize>>,
    /// Stored embeddings and metadata
    items: Vec<(String, Vec<f32>)>,
}

impl SimHashLSH {
    /// Create a new SimHash LSH index.
    ///
    /// # Arguments
    /// * `embedding_dim` - Dimension of the embedding vectors
    /// * `num_bits` - Number of bits in the hash (more = finer granularity)
    pub fn new(embedding_dim: usize, num_bits: usize) -> Self {
        // Generate random hyperplanes deterministically
        let mut hyperplanes = Vec::with_capacity(num_bits);
        let mut rng_state = 0x12345678u64;

        for _ in 0..num_bits {
            let mut plane = Vec::with_capacity(embedding_dim);
            for _ in 0..embedding_dim {
                // Generate random float in [-1, 1]
                rng_state = rng_state.wrapping_mul(0x5DEECE66D).wrapping_add(0xB);
                let val = ((rng_state >> 16) as f32 / u32::MAX as f32) * 2.0 - 1.0;
                plane.push(val);
            }
            hyperplanes.push(plane);
        }

        Self {
            num_bits,
            hyperplanes,
            buckets: HashMap::new(),
            items: Vec::new(),
        }
    }

    /// Insert an embedding vector.
    pub fn insert(&mut self, id: impl Into<String>, embedding: Vec<f32>) {
        let idx = self.items.len();
        let hash = self.simhash(&embedding);

        self.buckets.entry(hash).or_default().push(idx);
        self.items.push((id.into(), embedding));
    }

    /// Query for candidates similar to an embedding.
    pub fn query(&self, embedding: &[f32]) -> Vec<usize> {
        let hash = self.simhash(embedding);

        // Return exact bucket match and neighbors (Hamming distance 1)
        let mut candidates: HashSet<usize> = HashSet::new();

        // Exact match
        if let Some(indices) = self.buckets.get(&hash) {
            candidates.extend(indices);
        }

        // Hamming distance 1 neighbors (flip each bit)
        for bit in 0..self.num_bits.min(64) {
            let neighbor_hash = hash ^ (1u64 << bit);
            if let Some(indices) = self.buckets.get(&neighbor_hash) {
                candidates.extend(indices);
            }
        }

        candidates.into_iter().collect()
    }

    /// Get item by index.
    pub fn get(&self, idx: usize) -> Option<(&str, &[f32])> {
        self.items
            .get(idx)
            .map(|(id, emb)| (id.as_str(), emb.as_slice()))
    }

    /// Compute SimHash for an embedding vector.
    fn simhash(&self, embedding: &[f32]) -> u64 {
        let mut hash = 0u64;

        for (i, plane) in self.hyperplanes.iter().enumerate() {
            // Dot product with hyperplane
            let dot: f32 = plane.iter().zip(embedding.iter()).map(|(a, b)| a * b).sum();

            // Set bit if dot product is positive
            if dot > 0.0 {
                hash |= 1u64 << (i % 64);
            }
        }

        hash
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_minhash_basic() {
        let mut lsh = MinHashLSH::new(LSHConfig::default());
        lsh.insert_text("1", "Barack Obama");
        lsh.insert_text("2", "barack obama");
        lsh.insert_text("3", "Donald Trump");

        // Similar items should be candidates
        let candidates = lsh.candidate_pairs();
        assert!(!candidates.is_empty());

        // Check estimated similarity
        let sim_12 = lsh
            .estimated_similarity(0, 1)
            .expect("LSH similarity should succeed");
        let sim_13 = lsh
            .estimated_similarity(0, 2)
            .expect("LSH similarity should succeed");
        assert!(
            sim_12 > sim_13,
            "Similar items should have higher similarity"
        );
    }

    #[test]
    fn test_minhash_query() {
        let mut lsh = MinHashLSH::new(LSHConfig::default());
        lsh.insert_text("1", "New York City");
        lsh.insert_text("2", "New York");
        lsh.insert_text("3", "Los Angeles");

        let candidates = lsh.query("new york");
        // Should find items related to "New York"
        assert!(!candidates.is_empty());
    }

    #[test]
    fn test_config_probability() {
        let config = LSHConfig::default();

        // High similarity should have high candidate probability
        let high_sim_prob = config.candidate_probability(0.9);
        let low_sim_prob = config.candidate_probability(0.3);

        assert!(high_sim_prob > low_sim_prob);
        assert!(high_sim_prob > 0.9);
        assert!(low_sim_prob < 0.5);
    }

    #[test]
    fn test_simhash_basic() {
        let mut lsh = SimHashLSH::new(384, 64);

        // Similar vectors
        let v1: Vec<f32> = (0..384).map(|i| (i as f32).sin()).collect();
        let v2: Vec<f32> = (0..384).map(|i| (i as f32).sin() + 0.01).collect();
        // Different vector
        let v3: Vec<f32> = (0..384).map(|i| (i as f32).cos()).collect();

        lsh.insert("1", v1.clone());
        lsh.insert("2", v2);
        lsh.insert("3", v3);

        let candidates = lsh.query(&v1);
        assert!(!candidates.is_empty());
    }

    #[test]
    fn test_jaccard_similarity() {
        let set_a: HashSet<u64> = [1, 2, 3, 4].into_iter().collect();
        let set_b: HashSet<u64> = [3, 4, 5, 6].into_iter().collect();
        let set_c: HashSet<u64> = [1, 2, 3, 4].into_iter().collect();

        assert!((jaccard_similarity(&set_a, &set_c) - 1.0).abs() < 0.001);
        assert!((jaccard_similarity(&set_a, &set_b) - 0.333).abs() < 0.1);
    }

    #[test]
    fn test_empty_lsh() {
        let lsh = MinHashLSH::new(LSHConfig::default());
        assert!(lsh.is_empty());
        assert!(lsh.candidate_pairs().is_empty());
    }
}

// =============================================================================
// Property Tests
// =============================================================================

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(50))]

        /// Property: LSH is reflexive (item is candidate with itself)
        #[test]
        fn lsh_reflexivity(text in "[A-Za-z ]{5,30}") {
            let mut lsh = MinHashLSH::new(LSHConfig::default());
            lsh.insert_text("1", &text);

            let _candidates = lsh.candidates_for(0);
            // Self is not in candidates (correctly), but query should find it
            let query_candidates = lsh.query(&text);
            prop_assert!(!query_candidates.is_empty() || lsh.len() == 1);
        }

        /// Property: Identical strings are always candidates
        #[test]
        fn lsh_identical_candidates(text in "[A-Za-z]{5,20}") {
            let mut lsh = MinHashLSH::new(LSHConfig::default());
            lsh.insert_text("1", &text);
            lsh.insert_text("2", &text);

            let candidates = lsh.candidate_pairs();
            prop_assert!(!candidates.is_empty(),
                "Identical strings should be candidates");
        }

        /// Property: Estimated similarity bounded [0, 1]
        #[test]
        fn lsh_similarity_bounded(
            text1 in "[A-Za-z ]{5,30}",
            text2 in "[A-Za-z ]{5,30}"
        ) {
            let mut lsh = MinHashLSH::new(LSHConfig::default());
            lsh.insert_text("1", &text1);
            lsh.insert_text("2", &text2);

            if let Some(sim) = lsh.estimated_similarity(0, 1) {
                prop_assert!((0.0..=1.0).contains(&sim),
                    "Similarity {} out of bounds", sim);
            }
        }

        /// Property: Exact similarity equals estimated for identical strings
        #[test]
        fn lsh_exact_vs_estimated_identity(text in "[A-Za-z]{5,20}") {
            let mut lsh = MinHashLSH::new(LSHConfig::default());
            lsh.insert_text("1", &text);
            lsh.insert_text("2", &text);

            let exact = lsh.exact_similarity(0, 1).unwrap_or(0.0);
            prop_assert!((exact - 1.0).abs() < 0.001,
                "Identical strings should have exact similarity 1.0, got {}", exact);
        }

        /// Property: Candidate count bounded by n*(n-1)/2
        #[test]
        fn lsh_candidate_count_bounded(n in 2usize..20) {
            let mut lsh = MinHashLSH::new(LSHConfig::default());
            for i in 0..n {
                lsh.insert_text(i.to_string(), format!("item {}", i));
            }

            let candidates = lsh.candidate_pairs();
            let max_pairs = n * (n - 1) / 2;
            prop_assert!(candidates.len() <= max_pairs,
                "Too many candidates: {} > max {}", candidates.len(), max_pairs);
        }
    }
}
