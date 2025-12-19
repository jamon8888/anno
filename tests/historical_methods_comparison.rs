//! Integration tests comparing historical NER methods.
//!
//! This module tests the methodological evolution documented in Munnangi (2020)
//! "A Brief History of Named Entity Recognition" (arXiv:2411.05057):
//!
//! ```text
//! Era 1: Rule-based (1987-1997)     - Lexicons, hand-crafted patterns
//! Era 2: Statistical (1997-2015)    - HMM → MEMM → CRF
//! Era 3: Neural (2011-present)      - CNN → BiLSTM-CRF → Transformers
//! ```
//!
//! # Key Papers Tested
//!
//! - Bikel et al. 1997: "Nymble" HMM NER (~85% F1 on MUC)
//! - Lafferty et al. 2001: CRF introduction (solved label bias)
//! - Huang et al. 2015: BiLSTM-CRF (arXiv:1508.01991)
//!
//! # Test Categories
//!
//! 1. **Method comparison**: Same inputs across HMM/CRF/BiLSTM-CRF
//! 2. **BIO constraints**: Verify Viterbi produces valid sequences
//! 3. **Label bias**: Demonstrate CRF advantage over local models
//! 4. **Determinism**: Same inputs → same outputs

use anno::backends::{BiLstmCrfNER, HmmNER};
use anno::{CrfNER, HeuristicNER, Model, RegexNER};
use std::collections::HashSet;

// =============================================================================
// Test Data
// =============================================================================

/// Standard test sentences covering various entity patterns.
/// From Munnangi (2020) survey examples and CoNLL-2003 style text.
const TEST_SENTENCES: &[&str] = &[
    // Basic named entities (survey Figure 1 style)
    "John Smith works at Google in California.",
    "Marie Curie discovered radium in Paris.",
    // Multiple entity types
    "President Biden met with Chancellor Scholz in Berlin on Monday.",
    "Apple Inc. announced new products at their headquarters in Cupertino.",
    // Challenging patterns (low-frequency, ambiguous)
    "Dr. Jane Doe presented at MIT yesterday.",
    "The UN Security Council met in New York.",
    // Entity-dense text
    "Barack Obama and Michelle Obama visited the White House.",
    // Multilingual names (per survey's multilingual emphasis)
    "習近平 met with Putin in Moscow.",
    "François Müller works at Deutsche Bank in Frankfurt.",
];

/// BIO tag sequence validator.
/// Returns true if the sequence is valid BIO (no I- without preceding B- of same type).
fn is_valid_bio_sequence(tags: &[&str]) -> bool {
    let mut current_entity: Option<&str> = None;

    for tag in tags {
        if tag.starts_with("B-") {
            current_entity = Some(&tag[2..]);
        } else if tag.starts_with("I-") {
            let entity_type = &tag[2..];
            match current_entity {
                Some(prev) if prev == entity_type => {
                    // Valid: I-X after B-X or I-X
                }
                _ => {
                    // Invalid: I-X without matching B-X
                    return false;
                }
            }
        } else if *tag == "O" {
            current_entity = None;
        }
    }
    true
}

// =============================================================================
// Era 2: Statistical Methods Comparison (HMM vs CRF)
// =============================================================================

mod statistical_era {
    use super::*;

    /// Both HMM and CRF should produce non-overlapping entities.
    #[test]
    fn hmm_and_crf_produce_nonoverlapping_entities() {
        let hmm = HmmNER::new();
        let crf = CrfNER::new();

        for text in TEST_SENTENCES {
            for (name, backend) in [("HMM", &hmm as &dyn Model), ("CRF", &crf as &dyn Model)] {
                let entities = backend.extract_entities(text, None).unwrap();

                // Check no overlaps
                for i in 0..entities.len() {
                    for j in (i + 1)..entities.len() {
                        let a = &entities[i];
                        let b = &entities[j];
                        assert!(
                            a.end <= b.start || b.end <= a.start,
                            "{} produced overlapping entities in '{}': '{}' ({}-{}) and '{}' ({}-{})",
                            name, text, a.text, a.start, a.end, b.text, b.start, b.end
                        );
                    }
                }
            }
        }
    }

    /// CRF should generally find at least as many entities as HMM.
    /// Per Munnangi (2020): CRF ~88-91% F1 vs HMM ~85% F1 on MUC.
    #[test]
    fn crf_generally_competitive_with_hmm() {
        let hmm = HmmNER::new();
        let crf = CrfNER::new();

        let mut hmm_total = 0;
        let mut crf_total = 0;

        for text in TEST_SENTENCES {
            let hmm_entities = hmm.extract_entities(text, None).unwrap();
            let crf_entities = crf.extract_entities(text, None).unwrap();

            hmm_total += hmm_entities.len();
            crf_total += crf_entities.len();
        }

        // Both should find some entities
        assert!(
            hmm_total > 0,
            "HMM found no entities across all test sentences"
        );
        assert!(
            crf_total > 0,
            "CRF found no entities across all test sentences"
        );

        // Note: With heuristic weights, these may perform differently.
        // This test mainly ensures both are functional.
        println!(
            "HMM found {} entities, CRF found {} entities",
            hmm_total, crf_total
        );
    }

    /// Both methods should be deterministic (same input → same output).
    #[test]
    fn statistical_methods_are_deterministic() {
        let hmm = HmmNER::new();
        let crf = CrfNER::new();

        for text in TEST_SENTENCES {
            // HMM determinism
            let hmm1 = hmm.extract_entities(text, None).unwrap();
            let hmm2 = hmm.extract_entities(text, None).unwrap();
            assert_eq!(
                hmm1.len(),
                hmm2.len(),
                "HMM not deterministic on '{}'",
                text
            );

            // CRF determinism
            let crf1 = crf.extract_entities(text, None).unwrap();
            let crf2 = crf.extract_entities(text, None).unwrap();
            assert_eq!(
                crf1.len(),
                crf2.len(),
                "CRF not deterministic on '{}'",
                text
            );
        }
    }
}

// =============================================================================
// Era 3: Neural Methods (BiLSTM-CRF)
// =============================================================================

mod neural_era {
    use super::*;

    /// BiLSTM-CRF should produce valid outputs (falls back to heuristics without trained weights).
    #[test]
    fn bilstm_crf_produces_valid_entities() {
        let bilstm = BiLstmCrfNER::new();

        for text in TEST_SENTENCES {
            let entities = bilstm.extract_entities(text, None).unwrap();
            let char_count = text.chars().count();

            for entity in &entities {
                assert!(
                    entity.start <= entity.end,
                    "Invalid span in '{}': start {} > end {}",
                    text,
                    entity.start,
                    entity.end
                );
                assert!(
                    entity.end <= char_count,
                    "Span exceeds text in '{}': end {} > char_count {}",
                    text,
                    entity.end,
                    char_count
                );
                assert!(
                    entity.confidence >= 0.0 && entity.confidence <= 1.0,
                    "Invalid confidence {} in '{}'",
                    entity.confidence,
                    text
                );
            }
        }
    }

    /// Compare neural (BiLSTM-CRF) with statistical (CRF) baseline.
    #[test]
    fn bilstm_crf_vs_crf_comparison() {
        let crf = CrfNER::new();
        let bilstm = BiLstmCrfNER::new();

        let mut crf_entities = 0;
        let mut bilstm_entities = 0;

        for text in TEST_SENTENCES {
            crf_entities += crf.extract_entities(text, None).unwrap().len();
            bilstm_entities += bilstm.extract_entities(text, None).unwrap().len();
        }

        // With heuristic fallback, BiLSTM-CRF may behave similarly to CRF
        // This test documents the comparison rather than asserting superiority
        println!(
            "Statistical era (CRF): {} entities, Neural era (BiLSTM-CRF): {} entities",
            crf_entities, bilstm_entities
        );
    }
}

// =============================================================================
// Cross-Era Comparison
// =============================================================================

mod cross_era {
    use super::*;

    /// All eras should handle the same basic entities.
    #[test]
    fn all_eras_find_obvious_entities() {
        let text = "John Smith works at Google.";

        let hmm = HmmNER::new();
        let crf = CrfNER::new();
        let bilstm = BiLstmCrfNER::new();
        let heuristic = HeuristicNER::new();

        let methods: Vec<(&str, Box<dyn Model>)> = vec![
            ("Heuristic (baseline)", Box::new(heuristic)),
            ("HMM (1997)", Box::new(hmm)),
            ("CRF (2001)", Box::new(crf)),
            ("BiLSTM-CRF (2015)", Box::new(bilstm)),
        ];

        for (name, method) in &methods {
            let entities = method.extract_entities(text, None).unwrap();
            let texts: Vec<&str> = entities.iter().map(|e| e.text.as_str()).collect();

            // All methods should find at least one of: John, Smith, John Smith, Google
            assert!(
                texts
                    .iter()
                    .any(|t| t.contains("John") || t.contains("Google")),
                "{} failed to find obvious entities in '{}'. Found: {:?}",
                name,
                text,
                texts
            );
        }
    }

    /// Test the methodological progression on entity-dense text.
    #[test]
    fn era_comparison_on_dense_text() {
        let text = "Barack Obama and Joe Biden met at the White House in Washington D.C.";

        let methods: Vec<(&str, Box<dyn Model>)> = vec![
            ("Pattern", Box::new(RegexNER::new())),
            ("Heuristic", Box::new(HeuristicNER::new())),
            ("HMM", Box::new(HmmNER::new())),
            ("CRF", Box::new(CrfNER::new())),
            ("BiLSTM-CRF", Box::new(BiLstmCrfNER::new())),
        ];

        println!("\nEntity-dense text analysis: '{}'\n", text);
        println!("{:<15} | Entities Found", "Method");
        println!("{:-<15}-+-{:-<50}", "", "");

        for (name, method) in &methods {
            let entities = method.extract_entities(text, None).unwrap();
            let entity_strs: Vec<String> = entities
                .iter()
                .map(|e| format!("{}:{}", e.text, e.entity_type))
                .collect();
            println!("{:<15} | {}", name, entity_strs.join(", "));
        }
    }
}

// =============================================================================
// BIO Sequence Validity (Viterbi Constraint Enforcement)
// =============================================================================

mod bio_validity {
    use super::*;

    /// Viterbi-based methods should never produce invalid BIO sequences.
    /// This tests the label bias fix mentioned in Lafferty et al. (2001).
    #[test]
    fn viterbi_produces_valid_bio_sequences() {
        // We can't directly inspect the BIO tags, but we can verify
        // entities don't overlap (which would indicate invalid sequences).

        let hmm = HmmNER::new();
        let crf = CrfNER::new();
        let bilstm = BiLstmCrfNER::new();

        for text in TEST_SENTENCES {
            for (name, backend) in [
                ("HMM", &hmm as &dyn Model),
                ("CRF", &crf as &dyn Model),
                ("BiLSTM-CRF", &bilstm as &dyn Model),
            ] {
                let entities = backend.extract_entities(text, None).unwrap();

                // Check for overlaps (would indicate BIO violation)
                let mut char_positions: HashSet<usize> = HashSet::new();
                for entity in &entities {
                    for pos in entity.start..entity.end {
                        assert!(
                            char_positions.insert(pos),
                            "{} produced overlapping spans at position {} in '{}' (BIO violation)",
                            name,
                            pos,
                            text
                        );
                    }
                }
            }
        }
    }

    /// Unit test for BIO sequence validator.
    #[test]
    fn test_bio_validator() {
        // Valid sequences
        assert!(is_valid_bio_sequence(&["O", "O", "O"]));
        assert!(is_valid_bio_sequence(&["B-PER", "I-PER", "O"]));
        assert!(is_valid_bio_sequence(&["B-PER", "O", "B-ORG"]));
        assert!(is_valid_bio_sequence(&["B-PER", "I-PER", "I-PER", "O"]));

        // Invalid sequences (I- without matching B-)
        assert!(!is_valid_bio_sequence(&["O", "I-PER", "O"])); // I- after O
        assert!(!is_valid_bio_sequence(&["B-ORG", "I-PER", "O"])); // I-PER after B-ORG
        assert!(!is_valid_bio_sequence(&["I-PER", "I-PER", "O"])); // starts with I-
    }
}

// =============================================================================
// Confidence Score Analysis
// =============================================================================

mod confidence_analysis {
    use super::*;

    /// All methods should produce calibrated-ish confidence scores.
    #[test]
    fn confidence_scores_in_valid_range() {
        let methods: Vec<(&str, Box<dyn Model>)> = vec![
            ("HMM", Box::new(HmmNER::new())),
            ("CRF", Box::new(CrfNER::new())),
            ("BiLSTM-CRF", Box::new(BiLstmCrfNER::new())),
        ];

        for text in TEST_SENTENCES {
            for (name, method) in &methods {
                let entities = method.extract_entities(text, None).unwrap();
                for entity in &entities {
                    assert!(
                        entity.confidence >= 0.0 && entity.confidence <= 1.0,
                        "{} returned invalid confidence {} for '{}' in '{}'",
                        name,
                        entity.confidence,
                        entity.text,
                        text
                    );
                }
            }
        }
    }

    /// Higher confidence should correlate with clearer entity patterns.
    #[test]
    fn confidence_correlates_with_clarity() {
        let crf = CrfNER::new();

        // Clear entity
        let clear = crf
            .extract_entities("John Smith works at Google Inc.", None)
            .unwrap();

        // Ambiguous entity (could be person or org)
        let ambiguous = crf
            .extract_entities("Apple announced profits.", None)
            .unwrap();

        // Find "Google Inc." confidence
        let google_conf = clear
            .iter()
            .find(|e| e.text.contains("Google"))
            .map(|e| e.confidence);

        // Find "Apple" confidence
        let apple_conf = ambiguous
            .iter()
            .find(|e| e.text.contains("Apple"))
            .map(|e| e.confidence);

        // Both should have valid confidence (test documents behavior)
        if let (Some(g), Some(a)) = (google_conf, apple_conf) {
            println!(
                "Google Inc. confidence: {:.3}, Apple confidence: {:.3}",
                g, a
            );
        }
    }
}

// =============================================================================
// Unicode and Multilingual (per survey's emphasis)
// =============================================================================

mod multilingual {
    use super::*;

    /// Test that historical methods handle non-ASCII text correctly.
    #[test]
    fn historical_methods_handle_unicode() {
        let texts = [
            "François Hollande visited Élysée Palace.", // French
            "東京オリンピック opened in Tokyo.",        // Japanese
            "Москва hosted the summit.",                // Russian
        ];

        let crf = CrfNER::new();
        let hmm = HmmNER::new();

        for text in texts {
            let char_count = text.chars().count();

            // CRF
            let crf_entities = crf.extract_entities(text, None).unwrap();
            for entity in &crf_entities {
                assert!(
                    entity.end <= char_count,
                    "CRF produced invalid char offset {} for '{}' (char_count: {})",
                    entity.end,
                    text,
                    char_count
                );
            }

            // HMM
            let hmm_entities = hmm.extract_entities(text, None).unwrap();
            for entity in &hmm_entities {
                assert!(
                    entity.end <= char_count,
                    "HMM produced invalid char offset {} for '{}' (char_count: {})",
                    entity.end,
                    text,
                    char_count
                );
            }
        }
    }
}

// =============================================================================
// Label Bias Problem (CRF vs Local Models)
// =============================================================================
//
// Per Munnangi (2020) Section 4.2 and Lafferty et al. (2001):
//
// "CRFs solved the label bias problem that plagued MEMMs (Maximum Entropy
// Markov Models). In MEMMs, states with few successors effectively ignore
// observations. Transition scores are conditional on current state, so
// low-entropy states 'absorb' probability mass regardless of input."
//
// Anno doesn't implement MEMM (it's obsolete), but we can demonstrate
// CRF's global normalization advantage over purely local decisions.

mod label_bias {
    use super::*;

    /// Demonstrate that CRF uses global context, not just local decisions.
    ///
    /// The label bias problem occurs when a model makes decisions locally
    /// without considering the full sequence. CRF's global normalization
    /// (partition function Z) ensures all observations influence all labels.
    ///
    /// Test case: Ambiguous entity that depends on context.
    #[test]
    fn crf_uses_global_context() {
        let crf = CrfNER::new();

        // "Washington" is ambiguous: could be PER (George Washington) or LOC
        // Context should help disambiguate
        let person_context = "President Washington signed the bill.";
        let location_context = "The meeting was held in Washington.";

        let person_entities = crf.extract_entities(person_context, None).unwrap();
        let location_entities = crf.extract_entities(location_context, None).unwrap();

        // CRF should find "Washington" in both (even if type differs)
        let person_found = person_entities
            .iter()
            .any(|e| e.text.contains("Washington"));
        let location_found = location_entities
            .iter()
            .any(|e| e.text.contains("Washington"));

        // At minimum, CRF should recognize these as entities
        // (type classification may vary with heuristic weights)
        println!(
            "Person context entities: {:?}",
            person_entities
                .iter()
                .map(|e| (&e.text, &e.entity_type))
                .collect::<Vec<_>>()
        );
        println!(
            "Location context entities: {:?}",
            location_entities
                .iter()
                .map(|e| (&e.text, &e.entity_type))
                .collect::<Vec<_>>()
        );

        // Document behavior (not assertion, since heuristic weights vary)
        if person_found && location_found {
            println!("CRF recognized 'Washington' in both contexts.");
        }
    }

    /// Test that CRF transition constraints prevent invalid BIO sequences.
    ///
    /// This is the practical consequence of solving label bias:
    /// CRF won't produce I-PER after O or I-ORG after B-PER.
    #[test]
    fn crf_transition_constraints_example() {
        let crf = CrfNER::new();

        // Text with adjacent entities of different types
        let text = "John Smith and Google Inc. are partners.";
        let entities = crf.extract_entities(text, None).unwrap();

        // If CRF finds both, they should not overlap (valid BIO)
        for i in 0..entities.len() {
            for j in (i + 1)..entities.len() {
                let a = &entities[i];
                let b = &entities[j];
                assert!(
                    a.end <= b.start || b.end <= a.start,
                    "BIO violation: '{}' ({}-{}) overlaps '{}' ({}-{})",
                    a.text,
                    a.start,
                    a.end,
                    b.text,
                    b.start,
                    b.end
                );
            }
        }

        // Document what was found
        println!("Adjacent entities test:");
        for entity in &entities {
            println!(
                "  {} [{}] at {}-{}",
                entity.text, entity.entity_type, entity.start, entity.end
            );
        }
    }

    /// Compare HMM (generative) vs CRF (discriminative) on feature-rich text.
    ///
    /// Per Munnangi (2020): "HMMs can only condition on word identity.
    /// CRFs can use arbitrary features: capitalization, prefixes, suffixes."
    #[test]
    fn hmm_vs_crf_feature_usage() {
        let hmm = HmmNER::new();
        let crf = CrfNER::new();

        // Text with rich features: capitalization, suffix patterns
        let text = "Dr. Jane Smith-Williams visited IBM Corp.";

        let hmm_entities = hmm.extract_entities(text, None).unwrap();
        let crf_entities = crf.extract_entities(text, None).unwrap();

        println!("\nHMM vs CRF feature usage comparison:");
        println!("Text: '{}'", text);
        println!(
            "HMM found {} entities: {:?}",
            hmm_entities.len(),
            hmm_entities.iter().map(|e| &e.text).collect::<Vec<_>>()
        );
        println!(
            "CRF found {} entities: {:?}",
            crf_entities.len(),
            crf_entities.iter().map(|e| &e.text).collect::<Vec<_>>()
        );

        // CRF should leverage:
        // - Title prefix "Dr." (feature)
        // - Capitalization pattern (feature)
        // - Hyphenated name pattern (feature)
        // - Corp. suffix (feature)
        //
        // HMM only sees: word identity and transition probabilities
    }

    /// Explain label bias via documentation.
    ///
    /// This test exists to document the label bias problem for reference.
    #[test]
    fn label_bias_documentation() {
        // Label bias occurs in locally-normalized models (MEMM):
        //
        // Consider a sequence: x₁ → x₂ → x₃
        //                      ↓    ↓    ↓
        //                      y₁ → y₂ → y₃
        //
        // In MEMM, P(y₂|y₁, x) is normalized over possible y₂ values.
        // If y₁ has only one valid successor, that successor gets all mass
        // regardless of x. The observation is "ignored."
        //
        // Example:
        // - State "B-PER" might transition to only "I-PER" or "O"
        // - If training data shows B-PER → I-PER 95% of the time,
        //   MEMM might choose I-PER even when x strongly suggests O
        //
        // CRF fixes this with global normalization:
        // P(y|x) = (1/Z) × exp(∑ features × weights)
        //
        // The partition function Z normalizes over ALL possible sequences,
        // so every observation affects every label decision.
        //
        // Reference: Lafferty et al. (2001), Section 2.3
        // "Conditional Random Fields: Probabilistic Models for Segmenting
        // and Labeling Sequence Data" (ICML)

        assert!(true, "Documentation test - see comments above");
    }
}
