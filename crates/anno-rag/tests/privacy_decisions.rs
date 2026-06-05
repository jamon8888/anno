use anno_rag::docx_instructions::InstructionAction;
use anno_rag::privacy_decisions::{apply_user_decisions, UserDecision};
use cloakpipe_core::{DetectedEntity, DetectionSource, EntityCategory};

#[test]
fn keep_visible_removes_matching_detection() {
    let text = "La société Orange signe.";
    let detected = vec![entity("Orange", 11, 17, EntityCategory::Organization)];
    let decisions = vec![UserDecision {
        action: InstructionAction::Keep,
        selected_text: "Orange".to_string(),
    }];

    let merged = apply_user_decisions(text, detected, &decisions);

    assert!(merged.is_empty());
}

#[test]
fn mask_adds_custom_private_detection_for_missed_text() {
    let text = "Le client Jean Dupont signe.";
    let detected = Vec::new();
    let decisions = vec![UserDecision {
        action: InstructionAction::Mask,
        selected_text: "Jean Dupont".to_string(),
    }];

    let merged = apply_user_decisions(text, detected, &decisions);

    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].original, "Jean Dupont");
    assert_eq!(merged[0].start, 10);
    assert_eq!(merged[0].end, 21);
    assert!(matches!(merged[0].category, EntityCategory::Custom(ref label) if label == "private"));
}

#[test]
fn mask_all_exact_occurrences_and_deduplicates_overlaps() {
    let text = "Jean Dupont et Jean Dupont signent.";
    let detected = vec![entity("Jean Dupont", 0, 11, EntityCategory::Person)];
    let decisions = vec![UserDecision {
        action: InstructionAction::Mask,
        selected_text: "Jean Dupont".to_string(),
    }];

    let merged = apply_user_decisions(text, detected, &decisions);

    assert_eq!(merged.len(), 2);
    assert_eq!(merged[0].start, 0);
    assert_eq!(merged[1].start, 15);
}

fn entity(original: &str, start: usize, end: usize, category: EntityCategory) -> DetectedEntity {
    DetectedEntity {
        original: original.to_string(),
        start,
        end,
        category,
        confidence: 1.0,
        source: DetectionSource::Ner,
    }
}
