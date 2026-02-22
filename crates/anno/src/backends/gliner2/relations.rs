//! GLiNER2 relation extraction heuristics.
//!
//! Used by both the ONNX and Candle backends. Implements proximity + entity-type
//! based relation inference (GLiREL-style) without a dedicated relation model.

use crate::Entity;

/// Maps (head_type, tail_type) -> likely relation types.
#[cfg(any(feature = "onnx", feature = "candle"))]
pub(crate) fn get_likely_relations(head_type: &str, tail_type: &str) -> Vec<(&'static str, f32)> {
    let head = head_type.to_uppercase();
    let tail = tail_type.to_uppercase();

    match (head.as_str(), tail.as_str()) {
        // CHisIEC-style entity type codes
        ("PER", "OFI") | ("PERSON", "OFI") => vec![("任职", 0.7), ("任職", 0.7)],
        ("OFI", "PER") => vec![("上下级", 0.6), ("上下級", 0.6)],
        ("PER", "LOC") => vec![
            ("到达", 0.55),
            ("到達", 0.55),
            ("出生于某地", 0.4),
            ("出生於某地", 0.4),
        ],
        ("LOC", "PER") => vec![("到达", 0.5), ("到達", 0.5)],
        ("PER", "PER") => vec![
            ("上下级", 0.45),
            ("上下級", 0.45),
            ("同僚", 0.4),
            ("父母", 0.3),
            ("兄弟", 0.3),
        ],
        ("OFI", "LOC") | ("LOC", "OFI") => vec![("管理", 0.5)],
        ("BOOK", "BOOK") | ("BOOK", "PER") | ("PER", "BOOK") => {
            vec![("别名", 0.35), ("別名", 0.35)]
        }
        // Person-Organization relations
        ("PERSON", "ORGANIZATION") | ("PER", "ORG") => vec![
            ("WORKS_FOR", 0.7),
            ("FOUNDED", 0.5),
            ("CEO_OF", 0.4),
            ("MEMBER_OF", 0.6),
        ],
        ("ORGANIZATION", "PERSON") | ("ORG", "PER") => {
            vec![("EMPLOYS", 0.7), ("FOUNDED_BY", 0.5), ("LED_BY", 0.4)]
        }
        // Person-Location relations
        ("PERSON", "LOCATION") | ("PERSON", "GPE") | ("PER", "GPE") => {
            vec![("LIVES_IN", 0.6), ("BORN_IN", 0.5), ("VISITED", 0.4)]
        }
        // Organization-Location relations
        ("ORGANIZATION", "LOCATION")
        | ("ORG", "LOC")
        | ("ORGANIZATION", "GPE")
        | ("ORG", "GPE") => vec![
            ("HEADQUARTERED_IN", 0.7),
            ("LOCATED_IN", 0.8),
            ("OPERATES_IN", 0.5),
        ],
        // Product-Organization relations
        ("PRODUCT", "ORGANIZATION") | ("PRODUCT", "ORG") => {
            vec![("MADE_BY", 0.8), ("PRODUCED_BY", 0.7)]
        }
        ("ORGANIZATION", "PRODUCT") | ("ORG", "PRODUCT") => {
            vec![("MAKES", 0.8), ("PRODUCES", 0.7), ("ANNOUNCED", 0.5)]
        }
        // Date relations
        (_, "DATE") | (_, "TIME") => vec![("OCCURRED_ON", 0.5), ("FOUNDED_ON", 0.4)],
        // Default: no strong relation signal
        _ => vec![],
    }
}

/// Extract relations using proximity and type-based heuristics.
/// This is a lightweight approach that doesn't require a separate relation model.
#[cfg(any(feature = "onnx", feature = "candle"))]
pub(crate) fn extract_relations_heuristic(
    entities: &[Entity],
    text: &str,
    relation_types: &[&str],
    threshold: f32,
) -> Vec<crate::backends::inference::RelationTriple> {
    use crate::backends::inference::RelationTriple;

    // Normalize relation slugs so dataset styles like "part-of" match canonical "PART_OF".
    fn norm_rel_slug(s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        let mut prev_underscore = false;
        for ch in s.chars() {
            if ch.is_alphanumeric() {
                // Keep Unicode letters/digits; uppercase ASCII for stable matching.
                if ch.is_ascii_alphabetic() {
                    out.push(ch.to_ascii_uppercase());
                } else {
                    out.push(ch);
                }
                prev_underscore = false;
            } else if !prev_underscore {
                out.push('_');
                prev_underscore = true;
            }
        }
        while out.starts_with('_') {
            out.remove(0);
        }
        while out.ends_with('_') {
            out.pop();
        }
        out
    }

    fn pick_relation_label(canonical: &str, relation_types: &[&str]) -> Option<String> {
        if relation_types.is_empty() {
            return None;
        }
        let want = norm_rel_slug(canonical);
        relation_types
            .iter()
            .find(|r| norm_rel_slug(r) == want)
            .map(|s| (*s).to_string())
    }

    let mut relations = Vec::new();
    // Entity offsets in anno are character offsets. Keep all length math consistent with chars.
    let text_char_count = text.chars().count();
    let text_char_len = text_char_count.max(1) as f32;

    // Relation trigger patterns (canonicalized).
    //
    // IMPORTANT: Many evaluation datasets use hyphenated / lowercase labels (e.g. "part-of",
    // "general-affiliation"). We emit the dataset’s exact label when it’s present in
    // `relation_types`; otherwise we fall back to the canonical name.
    let trigger_patterns: Vec<(&str, &str)> = vec![
        // CrossRE/DocRED-style coarse labels
        ("part of", "PART_OF"),
        ("subset of", "PART_OF"),
        ("member of", "PART_OF"),
        ("type of", "TYPE_OF"),
        ("kind of", "TYPE_OF"),
        ("is a", "TYPE_OF"),
        ("are a", "TYPE_OF"),
        ("related to", "RELATED_TO"),
        ("also known as", "NAMED"),
        ("known as", "NAMED"),
        ("called", "NAMED"),
        ("named", "NAMED"),
        ("born", "TEMPORAL"),
        ("in 19", "TEMPORAL"),
        ("in 20", "TEMPORAL"),
        ("during", "TEMPORAL"),
        ("from", "ORIGIN"),
        ("based in", "PHYSICAL"),
        ("located in", "PHYSICAL"),
        ("headquartered", "PHYSICAL"),
        ("at ", "PHYSICAL"),
        ("vs", "COMPARE"),
        ("versus", "COMPARE"),
        ("compared", "COMPARE"),
        ("use", "USAGE"),
        ("used", "USAGE"),
        ("uses", "USAGE"),
        ("invented", "ARTIFACT"),
        ("created", "ARTIFACT"),
        ("built", "ARTIFACT"),
        ("developed", "ARTIFACT"),
        ("won", "WIN_DEFEAT"),
        ("defeated", "WIN_DEFEAT"),
        ("beat", "WIN_DEFEAT"),
        ("caused", "CAUSE_EFFECT"),
        ("causes", "CAUSE_EFFECT"),
        ("leads to", "CAUSE_EFFECT"),
        ("because", "CAUSE_EFFECT"),
        // CHisIEC (classical Chinese) relation labels + minimal triggers
        // Note: CHisIEC text is character-tokenized (no spaces), so we match short substrings.
        ("父", "父母"),
        ("母", "父母"),
        ("兄", "兄弟"),
        ("弟", "兄弟"),
        ("别名", "别名"),
        ("別名", "別名"),
        ("生于", "出生于某地"),
        ("生於", "出生於某地"),
        ("到", "到达"),
        ("到", "到達"),
        ("至", "到达"),
        ("至", "到達"),
        ("驻", "驻守"),
        ("駐", "駐守"),
        ("守", "驻守"),
        ("守", "駐守"),
        ("攻", "敌对攻伐"),
        ("伐", "敌对攻伐"),
        ("攻", "敵對攻伐"),
        ("伐", "敵對攻伐"),
        ("任", "任职"),
        ("任", "任職"),
        ("拜", "任职"),
        ("拜", "任職"),
        ("管", "管理"),
        ("治", "管理"),
        // Legacy canonical names (kept for non-CrossRE label sets)
        ("ceo", "CEO_OF"),
        ("founder", "FOUNDED"),
        ("founded", "FOUNDED"),
        ("works at", "WORKS_FOR"),
        ("works for", "WORKS_FOR"),
        ("employee", "WORKS_FOR"),
        ("born in", "BORN_IN"),
        ("lives in", "LIVES_IN"),
        ("announced", "ANNOUNCED"),
        ("released", "RELEASED"),
        ("acquired", "ACQUIRED"),
        ("bought", "ACQUIRED"),
        ("merged", "MERGED_WITH"),
    ];

    for (i, head) in entities.iter().enumerate() {
        for (j, tail) in entities.iter().enumerate() {
            if i == j {
                continue;
            }

            // Distance-based scoring: closer entities are more likely related
            let head_center = (head.start + head.end) as f32 / 2.0;
            let tail_center = (tail.start + tail.end) as f32 / 2.0;
            let distance = (head_center - tail_center).abs() / text_char_len;
            let proximity_score = 1.0 - distance.min(1.0);

            // Type-based relation candidates
            let head_type = head.entity_type.as_label();
            let tail_type = tail.entity_type.as_label();
            let type_relations = get_likely_relations(head_type, tail_type);

            // Check for trigger patterns in text between entities
            let (span_start, span_end) = if head.end < tail.start {
                (head.end, tail.start)
            } else if tail.end < head.start {
                (tail.end, head.start)
            } else {
                // Overlapping entities - use surrounding context
                let min_start = head.start.min(tail.start);
                let max_end = head.end.max(tail.end);
                (
                    min_start.saturating_sub(20),
                    (max_end + 20).min(text_char_count),
                )
            };

            let between_text = if span_end > span_start && span_end <= text_char_count {
                crate::offset::TextSpan::from_chars(text, span_start, span_end).extract(text)
            } else {
                ""
            };
            let between_lower = between_text.to_ascii_lowercase();

            // Check trigger patterns
            for (trigger, rel_type) in &trigger_patterns {
                let hit = if trigger.is_ascii() {
                    between_lower.contains(trigger)
                } else {
                    between_text.contains(trigger)
                };
                if hit {
                    // Filter by requested relation types if specified, using normalization.
                    // This allows "part-of" to match canonical "PART_OF", etc.
                    if !relation_types.is_empty()
                        && pick_relation_label(rel_type, relation_types).is_none()
                    {
                        continue;
                    }

                    let out_label = pick_relation_label(rel_type, relation_types)
                        .unwrap_or_else(|| rel_type.to_string());

                    let confidence = (proximity_score * 0.6 + 0.4)
                        * (head.confidence + tail.confidence) as f32
                        / 2.0;
                    if confidence >= threshold {
                        relations.push(RelationTriple {
                            head_idx: i,
                            tail_idx: j,
                            relation_type: out_label,
                            confidence,
                        });
                    }
                }
            }

            // Type-based relations (if no explicit trigger found)
            let has_trigger_relation = relations.iter().any(|r| r.head_idx == i && r.tail_idx == j);
            if !has_trigger_relation && proximity_score > 0.3 {
                for (rel_type, base_score) in type_relations {
                    if !relation_types.is_empty()
                        && pick_relation_label(rel_type, relation_types).is_none()
                    {
                        continue;
                    }
                    let out_label = pick_relation_label(rel_type, relation_types)
                        .unwrap_or_else(|| rel_type.to_string());

                    let confidence =
                        proximity_score * base_score * (head.confidence + tail.confidence) as f32
                            / 2.0;
                    if confidence >= threshold {
                        relations.push(RelationTriple {
                            head_idx: i,
                            tail_idx: j,
                            relation_type: out_label,
                            confidence,
                        });
                        break; // Only add one type-based relation per pair
                    }
                }
            }
        }
    }

    // Sort by confidence and deduplicate
    relations.sort_by(|a, b| {
        b.confidence
            .partial_cmp(&a.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Keep only top relation per entity pair
    let mut seen_pairs = std::collections::HashSet::new();
    relations.retain(|r| seen_pairs.insert((r.head_idx, r.tail_idx)));

    relations
}
