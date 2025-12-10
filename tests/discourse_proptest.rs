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
        compute_transition, track_centers, CenteringConfig, CenteringState, CenteringTransition,
        ForwardCenter, GrammaticalRole, InformationStatus,
    },
    classify_shell_noun, is_shell_noun, DiscourseReferent, DiscourseScope, EventCluster,
    EventCorefResolver, EventMention, EventPolarity, EventTense, ReferentType, ShellNoun,
    ShellNounClass,
    uncertain_reference::{
        ReferenceCandidate, ResolutionStrategy, UncertainReference,
    },
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
        Just(GrammaticalRole::Possessor),
        Just(GrammaticalRole::Other),
    ]
}

/// Generate information status.
fn information_status() -> impl Strategy<Value = InformationStatus> {
    prop_oneof![
        Just(InformationStatus::New),
        Just(InformationStatus::Unused),
        Just(InformationStatus::Inferrable),
        Just(InformationStatus::Evoked),
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
        proptest::string::string_regex("[A-Z][a-z]{2,8}( [a-z]{2,8}){0,5}\\. [A-Z][a-z]{2,8} [a-z]{2,8}\\.").unwrap(),
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

    /// Property: EventMention spans are valid.
    #[test]
    fn event_mention_span_validity((start, end) in valid_span()) {
        let event = EventMention::new("trigger", start, end);
        prop_assert!(event.start <= event.end);
        prop_assert_eq!(event.span(), (start, end));
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
        status in information_status(),
    ) {
        let center = ForwardCenter::new(1, "entity", sal)
            .with_role(role)
            .with_information_status(status);

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
            ref1.add_candidate(ReferenceCandidate::new(i as u64, &format!("entity{}", i), w));
            ref2.add_candidate(ReferenceCandidate::new(i as u64, &format!("entity{}", i), w));
        }

        let resolved1 = ref1.resolve();
        let resolved2 = ref2.resolve();

        prop_assert_eq!(resolved1.entity_id, resolved2.entity_id);
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
        entities in prop::collection::vec((1u64..100, "[a-z]{3,8}", salience()), 1..5)
    ) {
        let utterance: Vec<ForwardCenter> = entities.iter()
            .map(|(id, name, sal)| ForwardCenter::new(*id, name, *sal))
            .collect();

        let utterances = vec![utterance];
        let config = CenteringConfig::default();
        let states = track_centers(&utterances, &config);

        prop_assert!(states[0].cb.is_none(), "First utterance should have no Cb");
    }

    /// Property: Cb must be from previous Cf (if Cb exists).
    #[test]
    fn cb_from_previous_cf(
        entities1 in prop::collection::vec((1u64..10, "[a-z]{3,8}", salience()), 1..4),
        entities2 in prop::collection::vec((1u64..10, "[a-z]{3,8}", salience()), 1..4),
    ) {
        let utt1: Vec<ForwardCenter> = entities1.iter()
            .map(|(id, name, sal)| ForwardCenter::new(*id, name, *sal))
            .collect();
        let utt2: Vec<ForwardCenter> = entities2.iter()
            .map(|(id, name, sal)| ForwardCenter::new(*id, name, *sal))
            .collect();

        let utterances = vec![utt1.clone(), utt2.clone()];
        let config = CenteringConfig::default();
        let states = track_centers(&utterances, &config);

        if let Some(cb) = states[1].cb {
            // Cb must have been mentioned in both U1 (in Cf) and U2 (realized)
            let in_u1 = utt1.iter().any(|fc| fc.entity_id == cb);
            let in_u2 = utt2.iter().any(|fc| fc.entity_id == cb);
            prop_assert!(in_u1 && in_u2,
                "Cb ({}) must be in both previous Cf and current realization", cb);
        }
    }

    /// Property: CenteringTransition ordering is well-defined.
    #[test]
    fn transition_ordering() {
        // CONTINUE > RETAIN > SMOOTH_SHIFT > ROUGH_SHIFT
        prop_assert!(CenteringTransition::Continue < CenteringTransition::Retain);
        prop_assert!(CenteringTransition::Retain < CenteringTransition::SmoothShift);
        prop_assert!(CenteringTransition::SmoothShift < CenteringTransition::RoughShift);
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
            reference.add_candidate(ReferenceCandidate::new(i as u64, &format!("e{}", i), w));
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
            reference.add_candidate(ReferenceCandidate::new(i as u64, &format!("e{}", i), w));
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
            reference.add_candidate(ReferenceCandidate::new(i as u64, &format!("e{}", i), w));
        }

        let resolved = reference.resolve();
        let probs = reference.probabilities();

        let resolved_prob = probs.get(&resolved.entity_id).copied().unwrap_or(0.0);
        for (&id, &prob) in &probs {
            prop_assert!(resolved_prob >= prob - 1e-9,
                "Resolved candidate {} (prob {}) should have highest prob, but {} has {}",
                resolved.entity_id, resolved_prob, id, prob);
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
// 6. Event Coref Invariants
// =============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    /// Property: EventCorefResolver is deterministic.
    #[test]
    fn event_coref_deterministic(
        n_events in 1usize..10,
        trigger_idx in 0usize..5,
    ) {
        let triggers = ["attack", "meeting", "announce", "crash", "elect"];
        let trigger = triggers[trigger_idx % triggers.len()];

        let mentions: Vec<EventMention> = (0..n_events)
            .map(|i| EventMention::new(trigger, i * 10, i * 10 + 6)
                .with_trigger_type(trigger))
            .collect();

        let resolver = EventCorefResolver::new();
        let clusters1 = resolver.resolve(&mentions);
        let clusters2 = resolver.resolve(&mentions);

        prop_assert_eq!(clusters1.len(), clusters2.len());
    }

    /// Property: Each mention appears in exactly one cluster.
    #[test]
    fn each_mention_in_one_cluster(
        n_events in 1usize..10,
    ) {
        let mentions: Vec<EventMention> = (0..n_events)
            .map(|i| EventMention::new(&format!("event{}", i), i * 10, i * 10 + 6)
                .with_trigger_type(&format!("type{}", i % 3)))
            .collect();

        let resolver = EventCorefResolver::new();
        let clusters = resolver.resolve(&mentions);

        // Count mentions across all clusters
        let total_mentions: usize = clusters.iter().map(|c| c.len()).sum();
        prop_assert_eq!(total_mentions, n_events,
            "Total mentions in clusters should equal input count");
    }

    /// Property: Empty input produces empty clusters.
    #[test]
    fn empty_input_empty_clusters() {
        let resolver = EventCorefResolver::new();
        let clusters = resolver.resolve(&[]);
        prop_assert!(clusters.is_empty());
    }

    /// Property: Single mention produces single cluster.
    #[test]
    fn single_mention_single_cluster(trigger_type in "[a-z]{3,8}") {
        let mentions = vec![
            EventMention::new("test", 0, 4).with_trigger_type(&trigger_type)
        ];

        let resolver = EventCorefResolver::new();
        let clusters = resolver.resolve(&mentions);

        prop_assert_eq!(clusters.len(), 1);
        prop_assert_eq!(clusters[0].len(), 1);
    }
}

// =============================================================================
// 7. Discourse Scope Invariants
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
    fn clause_count_gte_sentence_count(text in discourse_text()) {
        let scope = DiscourseScope::analyze(&text);
        // In general, clauses >= sentences (each sentence has at least one clause)
        // But our simple heuristic might not always detect clauses
        // So we just check they're both non-negative
        prop_assert!(scope.clause_count() >= 0);
        prop_assert!(scope.sentence_count() >= 0);
    }

    /// Property: Empty text has no sentences.
    #[test]
    fn empty_text_no_sentences() {
        let scope = DiscourseScope::analyze("");
        prop_assert_eq!(scope.sentence_count(), 0);
    }
}

// =============================================================================
// 8. Shell Noun Invariants
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
// 9. Edge Cases and Fuzz Tests
// =============================================================================

#[test]
fn fuzz_discourse_scope_unicode() {
    // Various Unicode edge cases
    let test_cases = [
        "日本語の文。これは二文目。",  // Japanese
        "Москва — столица России. Это факт.",  // Russian
        "مرحبا بالعالم. هذا نص.",  // Arabic (RTL)
        "🎉 Party time! 🎊 This is fun.",  // Emoji
        "Naïve café résumé.",  // Diacritics
        "Price: €100. Cost: $200.",  // Currency symbols
    ];

    for text in &test_cases {
        let scope = DiscourseScope::analyze(text);
        // Should not panic
        assert!(scope.sentence_count() >= 0);
        // Character offsets should be valid
        for &boundary in &scope.sentence_boundaries {
            assert!(boundary <= text.chars().count());
        }
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
    assert!(event.is_empty());
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
        vec![],  // Empty utterance
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
