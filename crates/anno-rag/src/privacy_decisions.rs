//! Apply reviewed Word-comment decisions to detector output.

use crate::docx_instructions::InstructionAction;
use cloakpipe_core::{DetectedEntity, DetectionSource, EntityCategory};

/// One user decision derived from a Word comment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserDecision {
    /// Action requested by the user.
    pub action: InstructionAction,
    /// Text selected in Word.
    pub selected_text: String,
}

/// Apply keep/mask user decisions to detected PII.
///
/// Keep decisions remove exact matching detections. Mask decisions add custom
/// `private` detections for exact occurrences not already covered.
#[must_use]
pub fn apply_user_decisions(
    text: &str,
    detected: Vec<DetectedEntity>,
    decisions: &[UserDecision],
) -> Vec<DetectedEntity> {
    let mut out = detected;

    for decision in decisions {
        match decision.action {
            InstructionAction::Keep => {
                out.retain(|entity| entity.original.trim() != decision.selected_text.trim());
            }
            InstructionAction::Mask => {
                for (start, end) in exact_occurrences(text, &decision.selected_text) {
                    if out
                        .iter()
                        .any(|entity| ranges_overlap(start, end, entity.start, entity.end))
                    {
                        continue;
                    }
                    out.push(DetectedEntity {
                        original: text[start..end].to_string(),
                        start,
                        end,
                        category: EntityCategory::Custom("private".to_string()),
                        confidence: 1.0,
                        source: DetectionSource::Custom,
                    });
                }
            }
        }
    }

    out.sort_by(|a, b| {
        a.start
            .cmp(&b.start)
            .then_with(|| (b.end - b.start).cmp(&(a.end - a.start)))
    });
    dedup_overlaps(out)
}

fn exact_occurrences(text: &str, needle: &str) -> Vec<(usize, usize)> {
    let needle = needle.trim();
    if needle.is_empty() {
        return Vec::new();
    }
    text.match_indices(needle)
        .map(|(start, value)| (start, start + value.len()))
        .collect()
}

fn ranges_overlap(a_start: usize, a_end: usize, b_start: usize, b_end: usize) -> bool {
    a_start < b_end && b_start < a_end
}

fn dedup_overlaps(entities: Vec<DetectedEntity>) -> Vec<DetectedEntity> {
    let mut out: Vec<DetectedEntity> = Vec::with_capacity(entities.len());
    for entity in entities {
        if out
            .last()
            .is_some_and(|last| ranges_overlap(entity.start, entity.end, last.start, last.end))
        {
            continue;
        }
        out.push(entity);
    }
    out
}
