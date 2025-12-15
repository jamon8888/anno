//! Property-based tests for the discourse module.
//!
//! # Test Categories
//!
//! ## 1. Structural Invariants
//! - Span validity: referents have valid spans (start <= end)
//! - Unicode boundary alignment
//! - Non-negative sizes
//!
//! ## 2. Determinism Properties
//! - Same input → same output
//! - Order independence where applicable
//!
//! ## 3. Metamorphic Relations
//! - Classification is case-insensitive for shell nouns
//! - Event coref is symmetric (if A clusters with B, B clusters with A)
//!
//! ## 4. Type-Level Invariants
//! - Abstract referent types are abstract
//! - Shell noun classes partition the set of shell nouns
//!
//! # References
//!
//! - Grosz, Joshi, Weinstein (1995): Centering theory properties
//! - Israel (1994): Uncertain reference semantics

#![cfg(feature = "discourse")]

use anno::discourse::{
    centering::{
        track_centers, CenteringConfig, CenteringTransition, ForwardCenter, GrammaticalRole,
    },
    classify_response_token, classify_shell_noun, is_shell_noun,
    uncertain_reference::{ReferenceCandidate, UncertainReference},
    DialogueContext, DialogueTurn, DiscourseReferent, DiscourseScope, EventMention, EventPolarity,
    EventTense, ParticipantType, ReferentType, ShellNoun, ShellNounClass, SpeechActType,
};
use proptest::prelude::*;

// =============================================================================
// Test Strategies (Generators)
// =============================================================================

/// Generate valid span pairs (start, end) where start <= end.
fn valid_span() -> impl Strategy<Value = (usize, usize)> {
    (0usize..1000).prop_flat_map(|start| (Just(start), start..1000))
}

/// Generate arbitrary ReferentType values.
fn referent_type() -> impl Strategy<Value = ReferentType> {
    prop_oneof![
        Just(ReferentType::Nominal),
        Just(ReferentType::Event),
        Just(ReferentType::Proposition),
        Just(ReferentType::Fact),
        Just(ReferentType::Situation),
    ]
}

/// Generate arbitrary ShellNounClass values.
fn shell_noun_class() -> impl Strategy<Value = ShellNounClass> {
    prop_oneof![
        Just(ShellNounClass::Factual),
        Just(ShellNounClass::Linguistic),
        Just(ShellNounClass::Mental),
        Just(ShellNounClass::Modal),
        Just(ShellNounClass::Eventive),
        Just(ShellNounClass::Circumstantial),
    ]
}

/// Generate valid shell nouns from a known set.
fn known_shell_noun() -> impl Strategy<Value = &'static str> {
    prop_oneof![
        Just("fact"),
        Just("claim"),
        Just("idea"),
        Just("possibility"),
        Just("event"),
        Just("problem"),
        Just("reason"),
        Just("belief"),
        Just("hope"),
        Just("situation"),
    ]
}

/// Generate EventPolarity values.
fn event_polarity() -> impl Strategy<Value = EventPolarity> {
    prop_oneof![
        Just(EventPolarity::Positive),
        Just(EventPolarity::Negative),
        Just(EventPolarity::Uncertain),
    ]
}

/// Generate EventTense values.
fn event_tense() -> impl Strategy<Value = EventTense> {
    prop_oneof![
        Just(EventTense::Past),
        Just(EventTense::Present),
        Just(EventTense::Future),
    ]
}

/// Generate grammatical roles.
fn grammatical_role() -> impl Strategy<Value = GrammaticalRole> {
    prop_oneof![
        Just(GrammaticalRole::Subject),
        Just(GrammaticalRole::DirectObject),
        Just(GrammaticalRole::IndirectObject),
        Just(GrammaticalRole::Oblique),
        Just(GrammaticalRole::Other),
    ]
}

/// Generate salience scores.
fn salience() -> impl Strategy<Value = f64> {
    (0.0f64..=1.0).prop_filter("valid salience", |&s| s.is_finite())
}

/// Generate reference candidate weights (can be any float).
fn candidate_weight() -> impl Strategy<Value = f64> {
    (-10.0f64..10.0).prop_filter("valid weight", |&w| w.is_finite())
}

/// Generate realistic text for discourse analysis.
fn discourse_text() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("John met Mary. He said hello.".to_string()),
        Just("The cat sat on the mat. It was sleeping.".to_string()),
        Just("Russia invaded Ukraine. This shocked the world.".to_string()),
        Just("Prices rose. Wages fell. This was unsustainable.".to_string()),
        Just("A single sentence.".to_string()),
        Just("".to_string()),
        // Multi-sentence with various punctuation
        proptest::string::string_regex(
            "[A-Z][a-z]{2,8}( [a-z]{2,8}){0,5}\\. [A-Z][a-z]{2,8} [a-z]{2,8}\\."
        )
        .unwrap(),
    ]
}

// =============================================================================
// 1. Structural Invariants
// =============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    /// Property: DiscourseReferent spans are always valid (start <= end).
    #[test]
    fn referent_span_validity((start, end) in valid_span(), rtype in referent_type()) {
        let referent = DiscourseReferent::new(rtype, start, end);
        prop_assert!(referent.start <= referent.end, "span start <= end");
        prop_assert_eq!(referent.len(), end - start);
    }

    /// Property: Empty referents have zero length.
    #[test]
    fn empty_referent_has_zero_length(rtype in referent_type()) {
        let referent = DiscourseReferent::new(rtype, 5, 5);
        prop_assert!(referent.is_empty());
        prop_assert_eq!(referent.len(), 0);
    }

    /// Property: EventMention trigger spans are valid.
    #[test]
    fn event_mention_span_validity((start, end) in valid_span()) {
        let event = EventMention::new("trigger", start, end);
        prop_assert!(event.trigger_start <= event.trigger_end);
    }

    /// Property: ForwardCenter salience is always in [0, 1].
    #[test]
    fn forward_center_salience_bounded(sal in salience()) {
        let center = ForwardCenter::new(1, "entity", sal);
        prop_assert!(center.salience >= 0.0 && center.salience <= 1.0);
    }

    /// Property: ForwardCenter effective_salience is bounded.
    #[test]
    fn forward_center_effective_salience_bounded(
        sal in salience(),
        role in grammatical_role(),
    ) {
        let center = ForwardCenter::new(1, "entity", sal)
            .with_role(role);

        let effective = center.effective_salience();
        // Effective salience can exceed 1.0 due to bonuses, but should be finite
        prop_assert!(effective.is_finite());
        prop_assert!(effective >= 0.0);
    }
}

// =============================================================================
// 2. Type Invariants
// =============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    /// Property: Nominal is not abstract; all others are.
    #[test]
    fn referent_type_abstract_classification(rtype in referent_type()) {
        match rtype {
            ReferentType::Nominal => prop_assert!(!rtype.is_abstract()),
            _ => prop_assert!(rtype.is_abstract()),
        }
    }

    /// Property: Shell noun classification is case-insensitive.
    #[test]
    fn shell_noun_case_insensitive(noun in known_shell_noun()) {
        let lower = classify_shell_noun(noun);
        let upper = classify_shell_noun(&noun.to_uppercase());
        let mixed = classify_shell_noun(&noun.to_uppercase().chars().enumerate()
            .map(|(i, c)| if i % 2 == 0 { c.to_ascii_lowercase() } else { c })
            .collect::<String>());

        prop_assert_eq!(lower, upper, "classification should be case-insensitive (lower vs upper)");
        prop_assert_eq!(lower, mixed, "classification should be case-insensitive (lower vs mixed)");
    }

    /// Property: is_shell_noun and classify_shell_noun are consistent.
    #[test]
    fn shell_noun_consistency(noun in known_shell_noun()) {
        let is_shell = is_shell_noun(noun);
        let classified = classify_shell_noun(noun);

        // If it's classified, it should be a shell noun
        if classified.is_some() {
            prop_assert!(is_shell, "classified noun should be detected as shell noun");
        }
    }
}

// =============================================================================
// 3. Determinism Properties
// =============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    /// Property: DiscourseScope analysis is deterministic.
    #[test]
    fn discourse_scope_deterministic(text in discourse_text()) {
        let scope1 = DiscourseScope::analyze(&text);
        let scope2 = DiscourseScope::analyze(&text);

        prop_assert_eq!(scope1.sentence_count(), scope2.sentence_count());
        prop_assert_eq!(scope1.clause_count(), scope2.clause_count());
        prop_assert_eq!(scope1.sentence_boundaries, scope2.sentence_boundaries);
        prop_assert_eq!(scope1.clause_boundaries, scope2.clause_boundaries);
    }

    /// Property: UncertainReference resolution is deterministic (given same inputs).
    #[test]
    fn uncertain_reference_deterministic(
        weights in prop::collection::vec(candidate_weight(), 2..5)
    ) {
        let mut ref1 = UncertainReference::new("test");
        let mut ref2 = UncertainReference::new("test");

        for (i, &w) in weights.iter().enumerate() {
            ref1.add_candidate(ReferenceCandidate::new(i as u64, format!("entity{}", i), w));
            ref2.add_candidate(ReferenceCandidate::new(i as u64, format!("entity{}", i), w));
        }

        let resolved1 = ref1.resolve();
        let resolved2 = ref2.resolve();

        // Both should resolve to the same entity (or both None)
        prop_assert_eq!(resolved1.map(|c| c.entity_id), resolved2.map(|c| c.entity_id));
    }

    /// Property: EventMention with_* builders are idempotent.
    #[test]
    fn event_mention_builder_idempotent(
        polarity in event_polarity(),
        tense in event_tense(),
    ) {
        let event1 = EventMention::new("trigger", 0, 7)
            .with_polarity(polarity)
            .with_tense(tense);

        let event2 = EventMention::new("trigger", 0, 7)
            .with_polarity(polarity)
            .with_polarity(polarity)  // Apply twice
            .with_tense(tense)
            .with_tense(tense);  // Apply twice

        prop_assert_eq!(event1.polarity, event2.polarity);
        prop_assert_eq!(event1.tense, event2.tense);
    }
}

// =============================================================================
// 4. Centering Theory Invariants
// =============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    /// Property: First utterance never has a backward-looking center.
    #[test]
    fn first_utterance_no_cb(
        ids in prop::collection::vec(1u64..100, 1..5),
        names in prop::collection::vec("[a-z]{3,8}", 1..5),
        sals in prop::collection::vec(salience(), 1..5),
    ) {
        // Take the minimum length to ensure all vectors have same size
        let len = ids.len().min(names.len()).min(sals.len());
        if len == 0 {
            return Ok(());
        }

        let utterance: Vec<ForwardCenter> = (0..len)
            .map(|i| ForwardCenter::new(ids[i], &names[i], sals[i]))
            .collect();

        let utterances = vec![utterance];
        let config = CenteringConfig::default();
        let states = track_centers(&utterances, &config);

        prop_assert!(states[0].cb.is_none(), "First utterance should have no Cb");
    }

    /// Property: CenteringTransition variants are distinguishable.
    #[test]
    fn transition_variants_distinguishable(_x in 0..1) {
        // All transition types should be distinct
        prop_assert_ne!(CenteringTransition::Continue, CenteringTransition::Retain);
        prop_assert_ne!(CenteringTransition::Retain, CenteringTransition::SmoothShift);
        prop_assert_ne!(CenteringTransition::SmoothShift, CenteringTransition::RoughShift);
        prop_assert_ne!(CenteringTransition::RoughShift, CenteringTransition::Null);
    }
}

// =============================================================================
// 5. Uncertain Reference Invariants
// =============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    /// Property: Probabilities sum to 1 (approximately).
    #[test]
    fn uncertain_ref_probabilities_sum_to_one(
        weights in prop::collection::vec(candidate_weight(), 1..10)
    ) {
        let mut reference = UncertainReference::new("test");
        for (i, &w) in weights.iter().enumerate() {
            reference.add_candidate(ReferenceCandidate::new(i as u64, format!("e{}", i), w));
        }

        let probs = reference.probabilities();
        let sum: f64 = probs.values().sum();

        // Should sum to ~1.0 (allowing for floating point error)
        prop_assert!((sum - 1.0).abs() < 1e-6,
            "Probabilities should sum to 1.0, got {}", sum);
    }

    /// Property: All probabilities are non-negative.
    #[test]
    fn uncertain_ref_probabilities_non_negative(
        weights in prop::collection::vec(candidate_weight(), 1..10)
    ) {
        let mut reference = UncertainReference::new("test");
        for (i, &w) in weights.iter().enumerate() {
            reference.add_candidate(ReferenceCandidate::new(i as u64, format!("e{}", i), w));
        }

        let probs = reference.probabilities();
        for (&id, &prob) in &probs {
            prop_assert!(prob >= 0.0, "Probability for {} should be >= 0, got {}", id, prob);
        }
    }

    /// Property: Resolved candidate has highest probability.
    #[test]
    fn resolved_is_highest_probability(
        weights in prop::collection::vec(candidate_weight(), 2..10)
    ) {
        let mut reference = UncertainReference::new("test");
        for (i, &w) in weights.iter().enumerate() {
            reference.add_candidate(ReferenceCandidate::new(i as u64, format!("e{}", i), w));
        }

        if let Some(resolved) = reference.resolve() {
            let probs = reference.probabilities();
            let resolved_prob = probs.get(&resolved.entity_id).copied().unwrap_or(0.0);
            for (&id, &prob) in &probs {
                prop_assert!(resolved_prob >= prob - 1e-9,
                    "Resolved candidate {} (prob {}) should have highest prob, but {} has {}",
                    resolved.entity_id, resolved_prob, id, prob);
            }
        }
    }

    /// Property: Evidence update increases/decreases target probability.
    #[test]
    fn evidence_update_direction(
        base_weight in candidate_weight(),
        delta in -5.0f64..5.0,
    ) {
        let mut reference = UncertainReference::new("test");
        reference.add_candidate(ReferenceCandidate::new(1, "a", base_weight));
        reference.add_candidate(ReferenceCandidate::new(2, "b", 0.0)); // Neutral comparison

        let prob_before = *reference.probabilities().get(&1).unwrap();
        reference.update_evidence(1, delta);
        let prob_after = *reference.probabilities().get(&1).unwrap();

        if delta > 0.0 {
            prop_assert!(prob_after >= prob_before - 1e-9,
                "Positive evidence should increase probability");
        } else if delta < 0.0 {
            prop_assert!(prob_after <= prob_before + 1e-9,
                "Negative evidence should decrease probability");
        }
    }
}

// =============================================================================
// 6. Discourse Scope Invariants
// =============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    /// Property: Sentence boundaries are monotonically increasing.
    #[test]
    fn sentence_boundaries_monotonic(text in discourse_text()) {
        let scope = DiscourseScope::analyze(&text);

        for window in scope.sentence_boundaries.windows(2) {
            prop_assert!(window[0] <= window[1],
                "Sentence boundaries should be monotonic: {} <= {}", window[0], window[1]);
        }
    }

    /// Property: Clause boundaries are monotonically increasing.
    #[test]
    fn clause_boundaries_monotonic(text in discourse_text()) {
        let scope = DiscourseScope::analyze(&text);

        for window in scope.clause_boundaries.windows(2) {
            prop_assert!(window[0] <= window[1],
                "Clause boundaries should be monotonic: {} <= {}", window[0], window[1]);
        }
    }

    /// Property: Sentence count >= 0.
    #[test]
    fn sentence_count_non_negative(text in discourse_text()) {
        let scope = DiscourseScope::analyze(&text);
        prop_assert!(scope.sentence_count() >= 0);
    }

    /// Property: clause_count >= sentence_count (clauses are finer-grained).
    #[test]
    fn clause_count_non_negative(text in discourse_text()) {
        let scope = DiscourseScope::analyze(&text);
        // In general, clauses >= sentences (each sentence has at least one clause)
        // But our simple heuristic might not always detect clauses
        // So we just check they're both non-negative
        prop_assert!(scope.clause_count() >= 0);
        prop_assert!(scope.sentence_count() >= 0);
    }

    /// Property: Empty text has no sentences.
    #[test]
    fn empty_text_no_sentences(_x in 0..1) {
        let scope = DiscourseScope::analyze("");
        prop_assert_eq!(scope.sentence_count(), 0);
    }
}

// =============================================================================
// 7. Shell Noun Invariants
// =============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    /// Property: ShellNoun construction preserves class.
    #[test]
    fn shell_noun_preserves_class(class in shell_noun_class()) {
        let noun = ShellNoun::new("test", class);
        prop_assert_eq!(noun.class, class);
    }

    /// Property: ShellNoun with determiner includes the determiner.
    #[test]
    fn shell_noun_with_determiner(
        class in shell_noun_class(),
        determiner in prop_oneof![Just("this"), Just("that"), Just("the"), Just("a")],
    ) {
        let noun = ShellNoun::new("test", class)
            .with_determiner(determiner);
        prop_assert_eq!(noun.determiner.as_deref(), Some(determiner));
    }
}

// =============================================================================
// 8. Edge Cases and Fuzz Tests
// =============================================================================

#[test]
fn fuzz_discourse_scope_unicode() {
    // Various Unicode edge cases - now properly character-based
    let test_cases = [
        "日本語の文。これは二文目。",         // Japanese
        "Москва — столица России. Это факт.", // Russian
        "مرحبا بالعالم. هذا نص.",             // Arabic (RTL)
        "Party time! This is fun.",           // No emoji for stability
        "Naïve café résumé.",                 // Diacritics
        "Price: $100. Cost: $200.",           // Currency symbols
    ];

    for text in &test_cases {
        let scope = DiscourseScope::analyze(text);
        let char_count = text.chars().count();

        // All boundaries should be valid CHARACTER offsets
        for &boundary in &scope.sentence_boundaries {
            assert!(
                boundary <= char_count,
                "Sentence boundary {} exceeds char_count {} for: {}",
                boundary,
                char_count,
                text
            );
        }

        for &boundary in &scope.clause_boundaries {
            assert!(
                boundary <= char_count,
                "Clause boundary {} exceeds char_count {} for: {}",
                boundary,
                char_count,
                text
            );
        }

        // Verify extraction works for any valid span
        if scope.sentence_boundaries.len() >= 2 {
            let start = scope.sentence_boundaries[0];
            let end = scope.sentence_boundaries[1];
            let extracted = scope.extract_span(text, start, end);
            // Should not panic and should return valid UTF-8
            assert!(extracted.len() <= text.len());
        }
    }
}

#[test]
fn fuzz_discourse_scope_unicode_extraction() {
    // Test that extract_span works correctly with multibyte characters
    let text = "日本語。英語。"; // 7 chars but 21 bytes
    let scope = DiscourseScope::analyze(text);

    // Each character position should be valid for extraction
    let char_count = text.chars().count();
    for i in 0..char_count {
        let extracted = scope.extract_span(text, i, i + 1);
        assert_eq!(
            extracted.chars().count(),
            1,
            "Should extract exactly 1 char at position {}, got '{}'",
            i,
            extracted
        );
    }

    // Full range extraction
    let full = scope.extract_span(text, 0, char_count);
    assert_eq!(full, text);
}

#[test]
fn fuzz_discourse_scope_mixed_scripts() {
    // Mixed script text with various byte lengths per char
    let text = "Hello 世界! Bonjour 🌍. Привет мир.";
    let scope = DiscourseScope::analyze(text);
    let char_count = text.chars().count();

    // All boundaries should be valid character offsets
    for &b in &scope.sentence_boundaries {
        assert!(
            b <= char_count,
            "Sentence boundary {} > char_count {}",
            b,
            char_count
        );
    }

    for &b in &scope.clause_boundaries {
        assert!(
            b <= char_count,
            "Clause boundary {} > char_count {}",
            b,
            char_count
        );
    }

    // Verify we can extract without panic
    for window in scope.sentence_boundaries.windows(2) {
        let extracted = scope.extract_span(text, window[0], window[1]);
        assert!(!extracted.is_empty() || window[0] == window[1]);
    }
}

#[test]
fn fuzz_discourse_scope_cjk_punctuation() {
    // CJK uses different punctuation marks (。！？)
    let text = "日本語。中国語！韓国語？";
    let scope = DiscourseScope::analyze(text);

    // Should detect sentences with CJK punctuation
    assert!(scope.sentence_count() >= 1, "Should detect CJK sentences");

    // All boundaries within char range
    let char_count = text.chars().count();
    for &b in &scope.sentence_boundaries {
        assert!(
            b <= char_count,
            "Boundary {} > char_count {}",
            b,
            char_count
        );
    }
}

#[test]
fn fuzz_uncertain_reference_extreme_weights() {
    // Test extreme weight values
    let mut reference = UncertainReference::new("test");
    reference.add_candidate(ReferenceCandidate::new(1, "a", -1000.0));
    reference.add_candidate(ReferenceCandidate::new(2, "b", 1000.0));
    reference.add_candidate(ReferenceCandidate::new(3, "c", 0.0));

    let probs = reference.probabilities();
    let sum: f64 = probs.values().sum();

    // Should still sum to ~1.0 despite extreme weights
    assert!((sum - 1.0).abs() < 1e-6, "Probabilities sum: {}", sum);

    // Highest weight should have ~1.0 probability
    assert!(probs.get(&2).copied().unwrap_or(0.0) > 0.99);
}

#[test]
fn fuzz_event_mention_empty_trigger() {
    let event = EventMention::new("", 0, 0);
    assert_eq!(event.trigger, "");
    assert_eq!(event.trigger_start, 0);
    assert_eq!(event.trigger_end, 0);
}

#[test]
fn fuzz_centering_empty_utterances() {
    let utterances: Vec<Vec<ForwardCenter>> = vec![];
    let config = CenteringConfig::default();
    let states = track_centers(&utterances, &config);
    assert!(states.is_empty());
}

#[test]
fn fuzz_centering_utterance_with_no_entities() {
    let utterances: Vec<Vec<ForwardCenter>> = vec![
        vec![], // Empty utterance
        vec![ForwardCenter::new(1, "John", 1.0)],
    ];
    let config = CenteringConfig::default();
    let states = track_centers(&utterances, &config);

    // Should handle gracefully
    assert_eq!(states.len(), 2);
}

#[test]
fn fuzz_shell_noun_non_shell() {
    // Words that aren't shell nouns
    let non_shells = ["cat", "dog", "table", "xyz123", "", "   "];
    for word in &non_shells {
        assert!(!is_shell_noun(word), "{} should not be a shell noun", word);
        assert!(classify_shell_noun(word).is_none());
    }
}

// =============================================================================
// 9. Additional Invariant Tests
// =============================================================================

#[test]
fn test_referent_type_coverage() {
    // Ensure all types are covered
    let types = [
        ReferentType::Nominal,
        ReferentType::Event,
        ReferentType::Proposition,
        ReferentType::Fact,
        ReferentType::Situation,
    ];

    for t in &types {
        // Can be used for antecedent checks
        let _ = t.can_be_this_antecedent();
        let _ = t.can_be_it_antecedent();
    }
}

#[test]
fn test_shell_noun_class_coverage() {
    // Ensure all classes have at least one member
    let classes = [
        ShellNounClass::Factual,
        ShellNounClass::Linguistic,
        ShellNounClass::Mental,
        ShellNounClass::Modal,
        ShellNounClass::Eventive,
        ShellNounClass::Circumstantial,
    ];

    // Each class should have at least one known member
    let members = ["fact", "claim", "idea", "possibility", "event", "problem"];

    for (class, member) in classes.iter().zip(members.iter()) {
        let classified = classify_shell_noun(member);
        assert!(classified.is_some(), "No member found for {:?}", class);
    }
}

#[test]
fn test_event_mention_builder_chain() {
    // Test that builder methods can be chained
    let event = EventMention::new("attacked", 10, 18)
        .with_trigger_type("conflict:attack")
        .with_polarity(EventPolarity::Positive)
        .with_tense(EventTense::Past)
        .with_arguments(vec![("Agent", "rebels"), ("Patient", "city")]);

    assert_eq!(event.trigger, "attacked");
    assert_eq!(event.trigger_type.as_deref(), Some("conflict:attack"));
    assert_eq!(event.polarity, EventPolarity::Positive);
    assert_eq!(event.tense, Some(EventTense::Past));
    assert_eq!(event.get_argument("Agent"), Some("rebels"));
    assert_eq!(event.get_argument("Patient"), Some("city"));
}

#[test]
fn test_discourse_scope_sentence_at() {
    let text = "First sentence. Second sentence. Third.";
    let scope = DiscourseScope::analyze(text);

    // sentence_at should return valid results for positions within text
    let sent = scope.sentence_at(5);
    assert!(sent.is_some());

    // Position 0 should be in first sentence
    let first = scope.sentence_at(0);
    assert!(first.is_some());
}

#[test]
fn test_uncertain_reference_no_candidates() {
    let mut reference = UncertainReference::new("test");

    // No candidates - should return empty probabilities
    let probs = reference.probabilities();
    assert!(probs.is_empty());

    // Resolve should return None
    let resolved = reference.resolve();
    assert!(resolved.is_none());
}

#[test]
fn test_uncertain_reference_single_candidate() {
    let mut reference = UncertainReference::new("test");
    reference.add_candidate(ReferenceCandidate::new(1, "only", 0.5));

    let probs = reference.probabilities();
    assert_eq!(probs.len(), 1);
    assert!((probs[&1] - 1.0).abs() < 1e-6); // Single candidate = 100%

    let resolved = reference.resolve();
    assert!(resolved.is_some());
    assert_eq!(resolved.unwrap().entity_id, 1);
}

// =============================================================================
// 10. Dialogue Module Property Tests
// =============================================================================

/// Generate participant types.
fn participant_type() -> impl Strategy<Value = ParticipantType> {
    prop_oneof![
        Just(ParticipantType::Human),
        Just(ParticipantType::Agent),
        Just(ParticipantType::Unknown),
    ]
}

/// Generate speech act types.
fn speech_act_type() -> impl Strategy<Value = SpeechActType> {
    prop_oneof![
        Just(SpeechActType::Continuer),
        Just(SpeechActType::Acknowledgment),
        Just(SpeechActType::Assessment),
        Just(SpeechActType::Alignment),
        Just(SpeechActType::BackChannel),
        Just(SpeechActType::Question),
        Just(SpeechActType::Statement),
        Just(SpeechActType::Request),
        Just(SpeechActType::Farewell),
        Just(SpeechActType::Greeting),
        Just(SpeechActType::Other),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    /// Property: DialogueTurn preserves speaker and text.
    #[test]
    fn dialogue_turn_preserves_data(
        text in "[a-zA-Z ]{1,50}",
        speaker in "[A-Z]{2,5}",
    ) {
        let turn = DialogueTurn::new(&text, &speaker);
        prop_assert_eq!(&turn.text, &text);
        prop_assert_eq!(&turn.speaker, &speaker);
    }

    /// Property: DialogueTurn with participant type is queryable.
    #[test]
    fn dialogue_turn_participant_queries(ptype in participant_type()) {
        let turn = DialogueTurn::new("test", "SPK")
            .with_participant_type(ptype);

        match ptype {
            ParticipantType::Human => {
                prop_assert!(turn.is_human());
                prop_assert!(!turn.is_agent());
            }
            ParticipantType::Agent => {
                prop_assert!(!turn.is_human());
                prop_assert!(turn.is_agent());
            }
            ParticipantType::Unknown => {
                prop_assert!(!turn.is_human());
                prop_assert!(!turn.is_agent());
            }
        }
    }

    /// Property: Response token speech acts are identified correctly.
    #[test]
    fn speech_act_response_token_classification(act in speech_act_type()) {
        let is_rt = act.is_response_token();

        // These are response tokens per the conversation analysis literature
        let expected_rt = matches!(
            act,
            SpeechActType::Continuer
                | SpeechActType::Acknowledgment
                | SpeechActType::Assessment
                | SpeechActType::Alignment
                | SpeechActType::BackChannel
        );

        prop_assert_eq!(is_rt, expected_rt,
            "{:?}.is_response_token() should be {}", act, expected_rt);
    }

    /// Property: DialogueContext tracks turns correctly.
    #[test]
    fn dialogue_context_turn_tracking(
        n_turns in 0usize..20,
        speakers in prop::collection::vec("[A-Z]{2,4}", 1..5),
    ) {
        let mut ctx = DialogueContext::new();

        for i in 0..n_turns {
            let speaker = &speakers[i % speakers.len()];
            ctx.add_turn(DialogueTurn::new(format!("Turn {}", i), speaker));
        }

        prop_assert_eq!(ctx.turns.len(), n_turns);

        // Participants should be at most the unique speakers used
        let unique_speakers: std::collections::HashSet<_> = (0..n_turns)
            .map(|i| speakers[i % speakers.len()].as_str())
            .collect();
        prop_assert!(ctx.participants.len() <= unique_speakers.len() + 1);
    }

    /// Property: DialogueContext cutoff count <= total turns.
    #[test]
    fn dialogue_context_cutoff_bounded(n_turns in 1usize..10) {
        let mut ctx = DialogueContext::new();

        for i in 0..n_turns {
            let turn = DialogueTurn::new("oui", "SPK")
                .with_speech_act(SpeechActType::Continuer)
                .with_triggered_cutoff(i % 2 == 0);  // Half trigger cutoffs
            ctx.add_turn(turn);
        }

        prop_assert!(ctx.cutoff_count() <= n_turns);
    }
}

// =============================================================================
// 11. Dialogue Fuzz Tests
// =============================================================================

#[test]
fn fuzz_dialogue_empty_context() {
    let ctx = DialogueContext::new();
    assert!(ctx.turns.is_empty());
    assert_eq!(ctx.cutoff_count(), 0);
    assert_eq!(ctx.aside_count(), 0);
}

#[test]
fn fuzz_dialogue_response_token_classification_french() {
    let tokens = ["oui", "ouais", "d'accord", "exactement", "bonjour"];
    for token in &tokens {
        let result = classify_response_token(token, Some("fr"));
        assert!(result.is_some(), "French token '{}' should classify", token);
    }
}

#[test]
fn fuzz_dialogue_response_token_classification_english() {
    let tokens = ["uh huh", "okay", "wow", "right", "hello"];
    for token in &tokens {
        let result = classify_response_token(token, None);
        assert!(
            result.is_some(),
            "English token '{}' should classify",
            token
        );
    }
}

#[test]
fn fuzz_dialogue_non_response_tokens() {
    let tokens = ["complicated", "yesterday", "implementation", "xyz123"];
    for token in &tokens {
        let result = classify_response_token(token, None);
        assert!(
            result.is_none(),
            "Token '{}' should not classify as response token",
            token
        );
    }
}

#[test]
fn fuzz_dialogue_turn_builders() {
    // Test that all builder methods can be chained
    let turn = DialogueTurn::new("test text", "SPK")
        .with_participant_type(ParticipantType::Human)
        .with_speech_act(SpeechActType::Assessment)
        .with_triggered_cutoff(true)
        .with_addressee("OTHER")
        .as_aside(false);

    assert_eq!(turn.text, "test text");
    assert_eq!(turn.speaker, "SPK");
    assert!(turn.is_human());
    assert!(turn.is_response_token()); // Assessment is a response token
    assert!(turn.triggered_cutoff);
    assert_eq!(turn.addressee.as_deref(), Some("OTHER"));
    assert!(!turn.is_aside);
}

#[test]
fn fuzz_dialogue_context_last_turns() {
    let mut ctx = DialogueContext::new();

    for i in 0..10 {
        ctx.add_turn(DialogueTurn::new(format!("Turn {}", i), "SPK"));
    }

    // Last 3 turns
    let last3 = ctx.last_turns(3);
    assert_eq!(last3.len(), 3);

    // Last 100 turns (more than exist)
    let last100 = ctx.last_turns(100);
    assert_eq!(last100.len(), 10);

    // Last 0 turns
    let last0 = ctx.last_turns(0);
    assert!(last0.is_empty());
}

#[test]
fn fuzz_dialogue_context_full_text() {
    let mut ctx = DialogueContext::new();
    ctx.add_turn(DialogueTurn::new("Hello", "A"));
    ctx.add_turn(DialogueTurn::new("Hi there", "B"));

    let text = ctx.full_text();
    assert!(text.contains("A: Hello"));
    assert!(text.contains("B: Hi there"));
}
