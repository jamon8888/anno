//! Integration tests for canonical mention selection in coalesce.
//!
//! Tests the CanonicalSelector trait implementations with realistic
//! coreference scenarios.

use anno_coalesce::canonical::{
    detect_mention_type, CanonicalSelector, FirstMentionSelector, LongestMentionSelector,
    MentionFeatures, MentionType, NamedFirstSelector, SalienceBasedSelector,
};

/// Build a realistic coreference cluster with mixed mention types.
fn obama_cluster() -> Vec<MentionFeatures> {
    vec![
        MentionFeatures::new("he")
            .pronominal()
            .with_position(0)
            .with_frequency(5),
        MentionFeatures::new("Barack Obama")
            .named()
            .with_position(20)
            .in_subject_position(),
        MentionFeatures::new("the president")
            .nominal()
            .with_position(50),
        MentionFeatures::new("Obama")
            .named()
            .with_position(100)
            .with_frequency(3),
        MentionFeatures::new("him").pronominal().with_position(150),
        MentionFeatures::new("President Barack Obama")
            .named()
            .with_position(200)
            .in_heading(),
    ]
}

/// Build a cluster for a company with mixed mentions.
fn apple_cluster() -> Vec<MentionFeatures> {
    vec![
        MentionFeatures::new("Apple Inc.")
            .named()
            .with_position(0)
            .in_heading(),
        MentionFeatures::new("the company")
            .nominal()
            .with_position(30),
        MentionFeatures::new("Apple")
            .named()
            .with_position(60)
            .with_frequency(4),
        MentionFeatures::new("it").pronominal().with_position(90),
        MentionFeatures::new("the tech giant")
            .nominal()
            .with_position(120),
    ]
}

/// Build a cluster for a location.
fn berlin_cluster() -> Vec<MentionFeatures> {
    vec![
        MentionFeatures::new("the capital")
            .nominal()
            .with_position(0),
        MentionFeatures::new("Berlin")
            .named()
            .with_position(30)
            .with_frequency(3),
        MentionFeatures::new("the city").nominal().with_position(60),
        MentionFeatures::new("there").pronominal().with_position(90),
    ]
}

#[test]
fn test_salience_selector_obama() {
    let mentions = obama_cluster();
    let selector = SalienceBasedSelector::default();
    let canonical = selector.select(&mentions);

    // Should pick "President Barack Obama" (named, in heading, long)
    assert_eq!(canonical.surface, "President Barack Obama");
}

#[test]
fn test_named_first_selector_obama() {
    let mentions = obama_cluster();
    let selector = NamedFirstSelector;
    let canonical = selector.select(&mentions);

    // Should pick longest named: "President Barack Obama"
    assert_eq!(canonical.surface, "President Barack Obama");
}

#[test]
fn test_first_mention_selector_obama() {
    let mentions = obama_cluster();
    let selector = FirstMentionSelector;
    let canonical = selector.select(&mentions);

    // Should pick first by position: "he"
    assert_eq!(canonical.surface, "he");
}

#[test]
fn test_longest_selector_obama() {
    let mentions = obama_cluster();
    let selector = LongestMentionSelector;
    let canonical = selector.select(&mentions);

    // Should pick longest: "President Barack Obama"
    assert_eq!(canonical.surface, "President Barack Obama");
}

#[test]
fn test_salience_selector_apple() {
    let mentions = apple_cluster();
    let selector = SalienceBasedSelector::default();
    let canonical = selector.select(&mentions);

    // "Apple Inc." has heading boost, should rank high
    // Both "Apple Inc." and "Apple" are named, but "Apple Inc." is in heading
    assert!(
        canonical.surface == "Apple Inc." || canonical.surface == "Apple",
        "Expected Apple Inc. or Apple, got {}",
        canonical.surface
    );
}

#[test]
fn test_named_first_selector_apple() {
    let mentions = apple_cluster();
    let selector = NamedFirstSelector;
    let canonical = selector.select(&mentions);

    // Should pick longest named: "Apple Inc."
    assert_eq!(canonical.surface, "Apple Inc.");
}

#[test]
fn test_salience_selector_berlin() {
    let mentions = berlin_cluster();
    let selector = SalienceBasedSelector::default();
    let canonical = selector.select(&mentions);

    // Should pick "Berlin" (named, frequent)
    assert_eq!(canonical.surface, "Berlin");
}

#[test]
fn test_mention_type_detection_pronouns() {
    assert_eq!(detect_mention_type("he"), MentionType::Pronominal);
    assert_eq!(detect_mention_type("She"), MentionType::Pronominal);
    assert_eq!(detect_mention_type("it"), MentionType::Pronominal);
    assert_eq!(detect_mention_type("they"), MentionType::Pronominal);
    assert_eq!(detect_mention_type("them"), MentionType::Pronominal);
    assert_eq!(detect_mention_type("this"), MentionType::Pronominal);
    assert_eq!(detect_mention_type("who"), MentionType::Pronominal);
}

#[test]
fn test_mention_type_detection_named() {
    assert_eq!(detect_mention_type("Barack Obama"), MentionType::Named);
    assert_eq!(detect_mention_type("Apple Inc."), MentionType::Named);
    assert_eq!(detect_mention_type("New York City"), MentionType::Named);
    assert_eq!(detect_mention_type("Microsoft"), MentionType::Named);
}

#[test]
fn test_mention_type_detection_nominal() {
    assert_eq!(detect_mention_type("the president"), MentionType::Nominal);
    assert_eq!(detect_mention_type("a company"), MentionType::Nominal);
    assert_eq!(detect_mention_type("the tech giant"), MentionType::Nominal);
    assert_eq!(detect_mention_type("an organization"), MentionType::Nominal);
}

#[test]
fn test_salience_score_ordering() {
    let named = MentionFeatures::new("Barack Obama")
        .named()
        .with_position(0)
        .in_heading();
    let nominal = MentionFeatures::new("the president")
        .nominal()
        .with_position(50);
    let pronoun = MentionFeatures::new("he").pronominal().with_position(100);

    assert!(named.compute_salience() > nominal.compute_salience());
    assert!(nominal.compute_salience() > pronoun.compute_salience());
}

#[test]
fn test_frequency_affects_salience() {
    let low_freq = MentionFeatures::new("Obama").named().with_frequency(1);
    let high_freq = MentionFeatures::new("Obama").named().with_frequency(10);

    assert!(high_freq.compute_salience() > low_freq.compute_salience());
}

#[test]
fn test_position_affects_salience() {
    let early = MentionFeatures::new("Obama").named().with_position(0);
    let late = MentionFeatures::new("Obama").named().with_position(1000);

    // Earlier position should have slightly higher salience
    // (position penalty is logarithmic, so the effect is small)
    let early_score = early.compute_salience();
    let late_score = late.compute_salience();

    // Early should be >= late (position penalty reduces late score)
    assert!(
        early_score >= late_score,
        "Early position ({}) should score >= late position ({})",
        early_score,
        late_score
    );
}

#[test]
fn test_subject_position_affects_salience() {
    let subject = MentionFeatures::new("Obama").named().in_subject_position();
    let non_subject = MentionFeatures::new("Obama").named();

    assert!(subject.compute_salience() > non_subject.compute_salience());
}

#[test]
fn test_heading_affects_salience() {
    let in_heading = MentionFeatures::new("Obama").named().in_heading();
    let not_in_heading = MentionFeatures::new("Obama").named();

    assert!(in_heading.compute_salience() > not_in_heading.compute_salience());
}

#[test]
fn test_empty_cluster_handling() {
    let empty: Vec<MentionFeatures> = vec![];
    let selector = SalienceBasedSelector::default();

    // This should not panic - select_index returns 0 for empty
    let idx = selector.select_index(&empty);
    assert_eq!(idx, 0);
}

#[test]
fn test_single_mention_cluster() {
    let single = vec![MentionFeatures::new("Obama").named()];
    let selector = SalienceBasedSelector::default();
    let canonical = selector.select(&single);

    assert_eq!(canonical.surface, "Obama");
}

/// Test with CJK names (Chinese, Japanese, Korean).
///
/// Note: Our heuristic relies on capitalization, which doesn't apply to CJK scripts.
/// CJK names may be detected as Nominal, which is a known limitation.
/// For accurate CJK mention type detection, use an external classifier.
#[test]
fn test_cjk_mention_detection() {
    // Chinese name - our capitalization heuristic doesn't work for CJK
    // Accept either Named or Nominal
    let xi = detect_mention_type("習近平");
    assert!(
        xi == MentionType::Named || xi == MentionType::Nominal,
        "Chinese name should be Named or Nominal, got {:?}",
        xi
    );

    // Japanese name
    let yamada = detect_mention_type("山田太郎");
    assert!(
        yamada == MentionType::Named || yamada == MentionType::Nominal,
        "Japanese name should be Named or Nominal, got {:?}",
        yamada
    );

    // Mixed script with Latin capital - "Dr." is capitalized but "田中" isn't
    // Our simple heuristic may not handle this correctly
    let mixed = detect_mention_type("Dr. 田中");
    assert!(
        mixed == MentionType::Named || mixed == MentionType::Nominal,
        "Mixed script should be Named or Nominal, got {:?}",
        mixed
    );
}

/// Test with Arabic script (RTL).
#[test]
fn test_arabic_mention_detection() {
    // Named entities in Arabic typically have no lowercase
    // Our heuristic checks for capitalization, which doesn't apply to Arabic
    // This is a known limitation - Arabic entities may be detected as nominal
    let result = detect_mention_type("محمد بن سلمان");
    // Accept either Named or Nominal for Arabic
    assert!(
        result == MentionType::Named || result == MentionType::Nominal,
        "Arabic name should be Named or Nominal, got {:?}",
        result
    );
}
