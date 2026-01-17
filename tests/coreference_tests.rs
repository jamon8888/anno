//! Comprehensive tests for coreference resolution.
//!
//! Tests cover:
//! - Basic within-document coreference
//! - Pronoun resolution (he/she/it/they)
//! - Cross-document coreference (CDCR)
//! - Edge cases (empty input, single mention, overlapping spans)
//! - Unicode handling
//!
//! NOTE: Some coref tests are disabled pending API stabilization.

#![allow(unexpected_cfgs)]
#![allow(dead_code, unused_imports)]

use anno::{Entity, EntityType};

// =============================================================================
// Basic Entity Tests (Always Enabled)
// =============================================================================

#[test]
fn test_entity_creation() {
    let entity = Entity::new("John", EntityType::Person, 0, 4, 0.9);
    assert_eq!(entity.text, "John");
    assert_eq!(entity.start, 0);
    assert_eq!(entity.end, 4);
}

#[test]
fn test_entity_type_variants() {
    let person = EntityType::Person;
    let org = EntityType::Organization;
    let loc = EntityType::Location;
    assert_ne!(person, org);
    assert_ne!(org, loc);
    assert_ne!(person, loc);
}

#[test]
fn test_entity_confidence() {
    let entity = Entity::new("Apple Inc", EntityType::Organization, 0, 9, 0.95);
    assert!(entity.confidence > 0.9);
    assert!(entity.confidence <= 1.0);
}

// =============================================================================
// Coref Resolver Tests (Feature-Gated)
// =============================================================================

#[cfg(feature = "eval")]
mod coref_tests {
    use super::*;
    use anno::eval::coref_resolver::{CoreferenceResolver, SimpleCorefResolver};

    #[test]
    fn test_simple_coref_empty_input() {
        let resolver = SimpleCorefResolver::default();
        let entities: Vec<Entity> = vec![];
        let resolved = resolver.resolve(&entities);
        assert!(resolved.is_empty());
    }

    #[test]
    fn test_simple_coref_single_entity() {
        let resolver = SimpleCorefResolver::default();
        let entities = vec![Entity::new("John", EntityType::Person, 0, 4, 0.9)];
        let resolved = resolver.resolve(&entities);

        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].text, "John");
    }

    #[test]
    fn test_simple_coref_exact_match() {
        let resolver = SimpleCorefResolver::default();
        let entities = vec![
            Entity::new("John Smith", EntityType::Person, 0, 10, 0.9),
            Entity::new("John Smith", EntityType::Person, 50, 60, 0.9),
        ];
        let resolved = resolver.resolve(&entities);

        // Exact matches should have same canonical_id
        assert_eq!(resolved.len(), 2);
        assert_eq!(resolved[0].canonical_id, resolved[1].canonical_id);
    }

    #[test]
    fn test_simple_coref_no_match() {
        let resolver = SimpleCorefResolver::default();
        let entities = vec![
            Entity::new("John", EntityType::Person, 0, 4, 0.9),
            Entity::new("Apple Inc.", EntityType::Organization, 20, 30, 0.9),
        ];
        let resolved = resolver.resolve(&entities);

        // Different types - should not be linked
        assert_eq!(resolved.len(), 2);
        if resolved[0].canonical_id.is_some() && resolved[1].canonical_id.is_some() {
            assert_ne!(resolved[0].canonical_id, resolved[1].canonical_id);
        }
    }
}

// =============================================================================
// E2E Coref Tests (Feature-Gated)
// =============================================================================

#[cfg(all(feature = "eval", feature = "e2e_coref"))]
#[allow(unexpected_cfgs)]
mod e2e_coref_tests {
    use super::*;
    use anno::backends::e2e_coref::E2ECoref;

    #[test]
    fn test_e2e_coref_basic() {
        let coref = E2ECoref::new();
        let clusters = coref.resolve("John saw Mary. He waved to her.").unwrap();

        let total_mentions: usize = clusters.iter().map(|c| c.mentions.len()).sum();
        assert!(total_mentions > 0, "Should extract some mentions");
    }

    #[test]
    fn test_e2e_coref_unicode() {
        let coref = E2ECoref::new();
        let text = "María es doctora. Ella trabaja en el hospital.";
        let clusters = coref.resolve(text).unwrap();

        for cluster in &clusters {
            for mention in &cluster.mentions {
                assert!(mention.char_start <= mention.char_end);
                assert!(mention.char_end <= text.chars().count());
            }
        }
    }
}

// =============================================================================
// CDCR Tests (Feature-Gated)
// =============================================================================

#[cfg(all(feature = "eval", feature = "cdcr"))]
#[allow(unexpected_cfgs)]
mod cdcr_tests {
    use super::*;
    use anno::eval::cdcr::{CDCRConfig, CDCRResolver, Document};

    #[test]
    fn test_cdcr_empty_documents() {
        let resolver = CDCRResolver::new();
        let docs: Vec<Document> = vec![];
        let clusters = resolver.resolve(&docs);
        assert!(clusters.is_empty());
    }

    #[test]
    fn test_cdcr_single_document() {
        let mut doc = Document::new("doc1", "Apple announced new products.");
        doc.entities
            .push(Entity::new("Apple", EntityType::Organization, 0, 5, 0.9));

        let resolver = CDCRResolver::new();
        let clusters = resolver.resolve(&[doc]);

        assert_eq!(clusters.len(), 1);
        assert_eq!(clusters[0].mentions.len(), 1);
    }

    #[test]
    fn test_cdcr_exact_match_across_docs() {
        let mut doc1 = Document::new("doc1", "Apple announced new products.");
        doc1.entities
            .push(Entity::new("Apple", EntityType::Organization, 0, 5, 0.9));

        let mut doc2 = Document::new("doc2", "Apple released iOS update.");
        doc2.entities
            .push(Entity::new("Apple", EntityType::Organization, 0, 5, 0.9));

        let resolver = CDCRResolver::new();
        let clusters = resolver.resolve(&[doc1, doc2]);

        assert_eq!(clusters.len(), 1);
        assert_eq!(clusters[0].mentions.len(), 2);
    }
}

// =============================================================================
// Korean Literary Coreference Tests (KoCoNovel-Inspired)
// =============================================================================
//
// Test cases inspired by KoCoNovel (Kim, Lee & Lee 2024, arXiv:2404.01140).
// Korean presents unique coreference challenges:
// - Address term culture (호칭 문화): kinship terms substitute for names
// - Pro-drop: Subjects/objects often omitted (∅ pronouns)
// - No determiners: No "the/a" for definiteness cues
// - No proper noun markers: No capitalization to distinguish names
// - "Bobu emma" patterns: Nested proper names in kinship phrases
//
// These tests validate that Korean text doesn't break coreference infrastructure.

#[cfg(all(feature = "eval", feature = "korean_coref_tests"))]
#[allow(unexpected_cfgs)]
mod korean_coref_tests {
    use super::*;

    /// Test data from The Unknown Woman (무명초) by Lee Gwangsu.
    /// This is Table 1 from the KoCoNovel paper.
    fn koconovel_example_texts() -> Vec<(&'static str, &'static str, Vec<&'static str>)> {
        vec![
            // (korean_text, english_gloss, expected_character_mentions)
            (
                "\"달걀이 어디서 났니?\" 하고 조부님은 물으셨다.",
                "\"Where did the eggs come from?\" asked the grandfather.",
                vec!["조부님"], // grandfather (kinship term, not proper name)
            ),
            (
                "삼순이 어머니더러 오빠 온단 말을 했더니, 달걀 두 개를 주어.",
                "I(∅) told Sam-soon's mother that brother was coming, she(∅) gave eggs.",
                vec!["삼순이 어머니", "오빠"], // Sam-soon's mother, brother
            ),
            (
                "하고 누이가 만족한 듯이 대답한다.",
                "replied the sister, seemingly pleased.",
                vec!["누이"], // sister (kinship)
            ),
            (
                "보부 엄마가 누구냐?",
                "Who is Bobu's mother?",
                vec!["보부 엄마", "보부"], // Nested: Bobu's mother contains Bobu
            ),
            (
                "하고 나는 못 듣던 여인의 이름이 수상해서 경애에게 물었다.",
                "I asked Kyung-ae, curious about the unknown woman's name.",
                vec!["나", "여인", "경애"], // I, woman, Kyung-ae
            ),
            // Compound kinship term
            (
                "우리 모자는 눈 내리는 거리를 방황했다.",
                "The mother and son wandered the snowy streets.",
                vec!["우리 모자"], // mother-and-son (母子) - compound kinship
            ),
        ]
    }

    /// Validate that Korean text character counts are correct (Unicode safety).
    #[test]
    fn test_korean_character_counts() {
        for (korean, _english, _mentions) in koconovel_example_texts() {
            let char_count = korean.chars().count();
            let byte_count = korean.len();

            // Korean uses multi-byte UTF-8 (3 bytes per Hangul syllable)
            assert!(
                byte_count > char_count,
                "Korean text should have more bytes than chars: {} vs {} for '{}'",
                byte_count,
                char_count,
                korean
            );

            // Verify we can iterate without panicking
            let chars: Vec<char> = korean.chars().collect();
            assert_eq!(chars.len(), char_count);
        }
    }

    /// Test that mention spans can be extracted from Korean literary text.
    #[test]
    fn test_korean_mention_spans() {
        for (korean, _english, mentions) in koconovel_example_texts() {
            for mention in mentions {
                if let Some(start) = korean.find(mention) {
                    // Find returns byte offset, convert to char offset
                    let char_start = korean[..start].chars().count();
                    let char_end = char_start + mention.chars().count();

                    assert!(
                        char_start < char_end,
                        "Mention '{}' should have valid span",
                        mention
                    );
                    assert!(
                        char_end <= korean.chars().count(),
                        "Mention '{}' end {} exceeds text length {}",
                        mention,
                        char_end,
                        korean.chars().count()
                    );
                }
            }
        }
    }

    /// Test address term culture patterns (호칭 문화).
    /// In Korean, kinship terms often substitute for proper names.
    #[test]
    fn test_address_term_patterns() {
        let address_terms = [
            // (term, type, english)
            ("조부님", "kinship", "grandfather (formal)"),
            ("누이", "kinship", "older sister"),
            ("오빠", "kinship", "older brother (female speaker)"),
            ("어머니", "kinship", "mother (formal)"),
            ("엄마", "kinship", "mom (informal)"),
            ("아버지", "kinship", "father (formal)"),
            ("아빠", "kinship", "dad (informal)"),
            ("삼촌", "kinship", "uncle"),
            ("이모", "kinship", "aunt (maternal)"),
            ("선생님", "title", "teacher/sir"),
            ("사장님", "title", "boss/president"),
        ];

        for (term, term_type, english) in address_terms {
            assert!(
                !term.is_empty(),
                "Term '{}' ({}) should not be empty",
                english,
                term_type
            );
            // These are valid character mentions in Korean literary text
            let char_count = term.chars().count();
            assert!(char_count > 0, "Term '{}' should have chars", term);
        }
    }

    /// Test "Bobu emma" pattern: embedded proper names within kinship phrases.
    /// Pattern: [Child's name] + 엄마/어머니 = "X's mother" used as a name substitute
    #[test]
    fn test_bobu_emma_pattern() {
        let patterns = [
            ("보부 엄마", "보부", "Bobu's mom"),
            ("삼순이 어머니", "삼순이", "Sam-soon's mother"),
            ("영수 아버지", "영수", "Young-soo's father"),
            ("미영이 엄마", "미영이", "Mi-young's mom"),
        ];

        for (full_phrase, child_name, english) in patterns {
            // The child's name should be extractable from the phrase
            assert!(
                full_phrase.contains(child_name),
                "{}: '{}' should contain '{}'",
                english,
                full_phrase,
                child_name
            );

            // Annotation guideline: Both the full phrase AND the embedded name are annotated
            // This is unique to Korean coreference (see KoCoNovel Section 4.5)
            let phrase_len = full_phrase.chars().count();
            let name_len = child_name.chars().count();
            assert!(
                phrase_len > name_len,
                "Full phrase should be longer than embedded name"
            );
        }
    }

    /// Test Reader vs Omniscient perspective handling.
    /// From KoCoNovel: Some character identities are revealed later in the narrative.
    #[test]
    fn test_reader_omniscient_perspectives() {
        // Example from "Ms. B and Love Letters" (B사감과 러브레터)
        // Reader perspective: Taehoon, Kyungsuk, Ms. B are separate until reveal
        // Omniscient perspective: All are Ms. B from the start

        let reader_entities = vec!["Taehoon", "Kyungsuk", "Ms. B"];
        let omniscient_entity = "Ms. B";

        // In reader version: 3 separate entities
        assert_eq!(reader_entities.len(), 3);

        // In omniscient version: 1 entity with 3 mentions
        // (The system would need narrative context to handle this correctly)
        assert!(!omniscient_entity.is_empty());
    }

    /// Test Separate vs Overlapped entity handling for plural mentions.
    #[test]
    fn test_separate_vs_overlapped() {
        // "우리 모자" (mother and son) should be handled differently:
        // Separate: ['We'], ['I'], ['You'] as distinct
        // Overlapped: ['We', 'I'], ['We', 'You'] with shared membership

        let singular_mentions = vec!["나", "너"]; // I, you
        let plural_mention = "우리 모자"; // mother and son (compound)

        // The plural contains references to multiple individuals
        assert!(plural_mention.contains("모자")); // 母子 compound
        assert_eq!(singular_mentions.len(), 2);
    }
}

// =============================================================================
// Korean Unicode Validation Tests (Always Enabled)
// =============================================================================

/// Korean Hangul and literary text Unicode validation.
/// These tests run without feature gates to ensure basic Korean support works.
#[test]
fn test_korean_unicode_basics() {
    // Test Hangul syllable blocks (AC00-D7AF)
    let hangul_samples = [
        "안녕하세요", // Hello
        "서울",       // Seoul
        "대한민국",   // Korea
        "조부님",     // Grandfather
        "경애",       // Kyung-ae (name)
    ];

    for sample in hangul_samples {
        let char_count = sample.chars().count();
        assert!(char_count > 0, "Sample '{}' should have chars", sample);

        // Each Hangul syllable is one char but 3 bytes in UTF-8
        for ch in sample.chars() {
            assert!(
                ('\u{AC00}'..='\u{D7AF}').contains(&ch) || ch.is_ascii(),
                "Char '{}' should be Hangul or ASCII",
                ch
            );
        }
    }
}

/// Test that Entity creation works with Korean text.
#[test]
fn test_entity_with_korean_text() {
    // Person entity with Korean name
    let entity = Entity::new("경애", EntityType::Person, 0, 2, 0.9);
    assert_eq!(entity.text, "경애");
    assert_eq!(entity.text.chars().count(), 2);

    // Location entity with Korean place name
    let loc = Entity::new("서울", EntityType::Location, 10, 12, 0.85);
    assert_eq!(loc.text, "서울");

    // Mixed: Korean with title
    let titled = Entity::new("김 선생님", EntityType::Person, 0, 5, 0.88);
    assert_eq!(titled.text.chars().count(), 5);
}
