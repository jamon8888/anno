//! Relation extraction: configuration, trigger detection, and the main heuristic pipeline.
//!
//! Used by TPLinker and other relation-capable backends.

use crate::{Entity, EntityType};
use anno_core::Relation;
use super::registry::{LabelDefinition, SemanticRegistry};
use super::RelationTriple;


/// Configuration for relation extraction.
#[derive(Debug, Clone)]
pub struct RelationExtractionConfig {
    /// Maximum token distance between head and tail
    pub max_span_distance: usize,
    /// Minimum confidence for relation
    pub threshold: f32,
    /// Whether to extract relation triggers
    pub extract_triggers: bool,
}

impl Default for RelationExtractionConfig {
    fn default() -> Self {
        Self {
            max_span_distance: 50,
            threshold: 0.5,
            extract_triggers: true,
        }
    }
}

/// Extract relations between entities.
///
/// # Algorithm (Two-Pass)
///
/// 1. Run entity NER to find all entity mentions
/// 2. For each entity pair within distance threshold:
///    - Encode the span between them
///    - Match against relation type embeddings
///    - Optionally identify trigger span
///
/// # Returns
///
/// Relations with head/tail entities and optional trigger spans.
pub fn extract_relations(
    entities: &[Entity],
    text: &str,
    registry: &SemanticRegistry,
    config: &RelationExtractionConfig,
) -> Vec<Relation> {
    let mut relations = Vec::new();
    // `Entity` spans in anno are character offsets, but slicing a Rust `&str` requires byte
    // offsets. Build a converter once so we can safely slice and map trigger spans back.
    let span_converter = crate::offset::SpanConverter::new(text);

    // Get relation labels
    let relation_labels: Vec<_> = registry.relation_labels().collect();
    if relation_labels.is_empty() {
        return relations;
    }

    // Check all entity pairs
    for (i, head) in entities.iter().enumerate() {
        for (j, tail) in entities.iter().enumerate() {
            if i == j {
                continue;
            }

            // Check distance
            let distance = if head.end <= tail.start {
                tail.start - head.end
            } else {
                head.start.saturating_sub(tail.end)
            };

            if distance > config.max_span_distance {
                continue;
            }

            // Look for relation triggers in the text between entities
            let (span_start, span_end) = if head.end <= tail.start {
                (head.end, tail.start)
            } else {
                (tail.end, head.start)
            };

            let between_span = span_converter.from_chars(span_start, span_end);
            let between_text = text
                .get(between_span.byte_start..between_span.byte_end)
                .unwrap_or("");

            // Simple heuristic: check for common relation indicators
            let relation_type = detect_relation_type(head, tail, between_text, &relation_labels);

            if let Some((rel_type, mut confidence, trigger)) = relation_type {
                // Apply distance penalty: closer entities are more likely to be related
                // Confidence decays linearly from 1.0 at distance 0 to 0.5 at max_span_distance
                let distance_penalty = if distance < config.max_span_distance {
                    let penalty_factor =
                        1.0 - (distance as f64 / config.max_span_distance as f64) * 0.5;
                    penalty_factor.max(0.5) // Minimum 0.5 confidence even at max distance
                } else {
                    0.5 // At or beyond max distance, apply minimum confidence
                };
                confidence *= distance_penalty;

                if confidence < config.threshold as f64 {
                    continue;
                }

                // `detect_relation_type` returns byte offsets into `between_text`.
                let trigger_span = if config.extract_triggers {
                    trigger.map(|(s, e)| {
                        let trigger_start_byte = between_span.byte_start.saturating_add(s);
                        let trigger_end_byte = between_span.byte_start.saturating_add(e);
                        (
                            span_converter.byte_to_char(trigger_start_byte),
                            span_converter.byte_to_char(trigger_end_byte),
                        )
                    })
                } else {
                    None
                };

                relations.push(Relation {
                    head: head.clone(),
                    tail: tail.clone(),
                    relation_type: rel_type.to_string(),
                    trigger_span,
                    confidence: confidence.clamp(0.0, 1.0), // Clamp to [0, 1]
                });
            }
        }
    }

    relations
}

/// Extract relations as index-based triples (for joint extraction backends).
///
/// This is the same heuristic logic as [`extract_relations`], but returns
/// [`RelationTriple`] with indices into the provided `entities` slice.
///
/// Notes:
/// - Entity spans are **character offsets**.
/// - Trigger spans are not currently exposed in `RelationTriple`.
#[must_use]
pub fn extract_relation_triples(
    entities: &[Entity],
    text: &str,
    registry: &SemanticRegistry,
    config: &RelationExtractionConfig,
) -> Vec<RelationTriple> {
    let mut triples = Vec::new();
    if entities.len() < 2 {
        return triples;
    }

    // `Entity` spans are character offsets; slicing needs byte offsets.
    let span_converter = crate::offset::SpanConverter::new(text);

    let relation_labels: Vec<_> = registry.relation_labels().collect();
    if relation_labels.is_empty() {
        return triples;
    }

    for (i, head) in entities.iter().enumerate() {
        for (j, tail) in entities.iter().enumerate() {
            if i == j {
                continue;
            }

            // Skip overlapping spans (avoids self-nesting artifacts like "New York" vs "York").
            if head.start < tail.end && tail.start < head.end {
                continue;
            }

            // Check distance (character offsets)
            let distance = if head.end <= tail.start {
                tail.start - head.end
            } else {
                head.start.saturating_sub(tail.end)
            };
            if distance > config.max_span_distance {
                continue;
            }

            let (span_start, span_end) = if head.end <= tail.start {
                (head.end, tail.start)
            } else {
                (tail.end, head.start)
            };

            let between_span = span_converter.from_chars(span_start, span_end);
            let between_text = text
                .get(between_span.byte_start..between_span.byte_end)
                .unwrap_or("");

            if let Some((rel_type, mut confidence, _trigger)) =
                detect_relation_type(head, tail, between_text, &relation_labels)
            {
                // Apply distance penalty (same logic as extract_relations)
                let distance_penalty = if distance < config.max_span_distance {
                    let penalty_factor =
                        1.0 - (distance as f64 / config.max_span_distance as f64) * 0.5;
                    penalty_factor.max(0.5)
                } else {
                    0.5
                };
                confidence *= distance_penalty;

                if confidence < config.threshold as f64 {
                    continue;
                }

                triples.push(RelationTriple {
                    head_idx: i,
                    tail_idx: j,
                    relation_type: rel_type.to_string(),
                    confidence: confidence as f32,
                });
            }
        }
    }

    triples
}

/// Result of relation detection: (label, confidence, optional span).
type RelationMatch<'a> = (&'a str, f64, Option<(usize, usize)>);

/// Detect relation type from context (heuristic fallback).
fn detect_relation_type<'a>(
    head: &Entity,
    tail: &Entity,
    between_text: &str,
    relation_labels: &[&'a LabelDefinition],
) -> Option<RelationMatch<'a>> {
    // Use Unicode-aware lowercasing for multilingual support
    // Note: For CJK languages, case doesn't apply, but this is safe
    let between_lower = between_text.to_lowercase();

    // Normalize relation slugs so datasets that use kebab-case / colon-separated schemas
    // (e.g. DocRED: "part-of", "general-affiliation") can match our canonical patterns
    // (e.g. "PART_OF", "GENERAL_AFFILIATION").
    fn norm_rel_slug(s: &str) -> String {
        // Uppercase + map non-alphanumerics to '_' so we can compare across naming schemes.
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

    // Common patterns: (relation_slug, triggers, confidence)
    struct RelPattern {
        slug: &'static str,
        triggers: &'static [&'static str],
        confidence: f64,
    }

    let patterns: &[RelPattern] = &[
        // Employment relations
        RelPattern {
            slug: "CEO_OF",
            triggers: &[
                "ceo of",
                "chief executive",
                "chief executive officer",
                "leads",
                "founded",
                "founder of",
            ],
            confidence: 0.8,
        },
        RelPattern {
            slug: "WORKS_FOR",
            triggers: &[
                "works for",
                "works at",
                "employed by",
                "employee of",
                "works with",
                "staff at",
                "member of",
            ],
            confidence: 0.7,
        },
        RelPattern {
            slug: "FOUNDED",
            triggers: &[
                "founded",
                "co-founded",
                "cofounder",
                "started",
                "established",
                "created",
                "launched",
            ],
            confidence: 0.8,
        },
        RelPattern {
            slug: "MANAGES",
            triggers: &[
                "manages",
                "managing",
                "oversees",
                "directs",
                "supervises",
                "runs",
            ],
            confidence: 0.75,
        },
        RelPattern {
            slug: "REPORTS_TO",
            triggers: &["reports to", "reported to", "under", "reports directly to"],
            confidence: 0.7,
        },
        // Location relations
        RelPattern {
            slug: "LOCATED_IN",
            triggers: &[
                "in",
                "at",
                "based in",
                "located in",
                "headquartered in",
                "situated in",
                "found in",
            ],
            confidence: 0.6,
        },
        RelPattern {
            slug: "BORN_IN",
            triggers: &[
                "born in",
                "native of",
                "from",
                "hails from",
                "originated in",
            ],
            confidence: 0.7,
        },
        RelPattern {
            slug: "LIVES_IN",
            triggers: &["lives in", "resides in", "living in", "based in"],
            confidence: 0.65,
        },
        RelPattern {
            slug: "DIED_IN",
            triggers: &["died in", "passed away in", "deceased in"],
            confidence: 0.8,
        },
        // Temporal relations
        RelPattern {
            slug: "OCCURRED_ON",
            triggers: &["on", "occurred on", "happened on", "took place on", "dated"],
            confidence: 0.6,
        },
        RelPattern {
            slug: "STARTED_ON",
            triggers: &["started on", "began on", "commenced on", "initiated on"],
            confidence: 0.7,
        },
        RelPattern {
            slug: "ENDED_ON",
            triggers: &["ended on", "concluded on", "finished on", "completed on"],
            confidence: 0.7,
        },
        // Organizational relations
        RelPattern {
            slug: "PART_OF",
            triggers: &[
                "part of",
                "member of",
                "belongs to",
                "subsidiary of",
                "division of",
                "branch of",
            ],
            confidence: 0.7,
        },
        RelPattern {
            slug: "ACQUIRED",
            triggers: &[
                "acquired",
                "bought",
                "purchased",
                "took over",
                "merged with",
            ],
            confidence: 0.75,
        },
        RelPattern {
            slug: "MERGED_WITH",
            triggers: &["merged with", "merged into", "combined with", "joined with"],
            confidence: 0.8,
        },
        RelPattern {
            slug: "PARENT_OF",
            triggers: &["parent of", "parent company of", "owns", "owner of"],
            confidence: 0.75,
        },
        // Social relations
        RelPattern {
            slug: "MARRIED_TO",
            triggers: &["married to", "wed to", "spouse of", "husband of", "wife of"],
            confidence: 0.85,
        },
        RelPattern {
            slug: "CHILD_OF",
            triggers: &["son of", "daughter of", "child of", "offspring of"],
            confidence: 0.8,
        },
        RelPattern {
            slug: "SIBLING_OF",
            triggers: &["brother of", "sister of", "sibling of"],
            confidence: 0.8,
        },
        // Academic/Professional
        RelPattern {
            slug: "STUDIED_AT",
            triggers: &[
                "studied at",
                "attended",
                "graduated from",
                "alumni of",
                "educated at",
            ],
            confidence: 0.75,
        },
        RelPattern {
            slug: "TEACHES_AT",
            triggers: &["teaches at", "professor at", "instructor at", "faculty at"],
            confidence: 0.8,
        },
        // Product/Service relations
        RelPattern {
            slug: "DEVELOPS",
            triggers: &[
                "develops",
                "created",
                "built",
                "designed",
                "produces",
                "manufactures",
            ],
            confidence: 0.7,
        },
        RelPattern {
            slug: "USES",
            triggers: &["uses", "utilizes", "employs", "adopts", "implements"],
            confidence: 0.6,
        },
        // Dataset-style relation labels (DocRED/CHisIEC-like)
        //
        // These are the *coarse* label names we actually see in the CrossRE/DocRED-style
        // exports used by this repo (e.g. `docred_dev.json`), which differ from the
        // “canonical” IE labels above.
        RelPattern {
            slug: "NAMED",
            triggers: &[
                "called",
                "known as",
                "also known as",
                "named",
                "referred to as",
                "nickname",
            ],
            confidence: 0.6,
        },
        RelPattern {
            slug: "TYPE_OF",
            triggers: &[
                "type of",
                "kind of",
                "form of",
                "a type of",
                "is a",
                "are a",
            ],
            confidence: 0.6,
        },
        RelPattern {
            slug: "RELATED_TO",
            triggers: &["related to", "associated with", "connected to", "linked to"],
            confidence: 0.55,
        },
        RelPattern {
            slug: "ORIGIN",
            triggers: &[
                "from",
                "born",
                "originated",
                "created by",
                "invented by",
                "derived from",
                "spinoff",
                "spin-off",
            ],
            confidence: 0.55,
        },
        RelPattern {
            slug: "ROLE",
            triggers: &[
                "president",
                "ceo",
                "chair",
                "director",
                "editor",
                "producer",
                "actor",
                "professor",
                "fellow",
                "member",
            ],
            confidence: 0.55,
        },
        RelPattern {
            slug: "TEMPORAL",
            triggers: &[
                "in 19", "in 20", "during", "before", "after", "between", "until", "since",
            ],
            confidence: 0.5,
        },
        RelPattern {
            slug: "PHYSICAL",
            triggers: &["located in", "based in", "headquartered in", "at "],
            confidence: 0.55,
        },
        RelPattern {
            slug: "TOPIC",
            triggers: &["topic", "about", "on", "regarding", "focused on"],
            confidence: 0.5,
        },
        RelPattern {
            slug: "OPPOSITE",
            triggers: &["opposite", "contrasts with", "as opposed to"],
            confidence: 0.6,
        },
        RelPattern {
            slug: "WIN_DEFEAT",
            triggers: &["defeated", "beat", "won", "win", "lose", "lost to"],
            confidence: 0.6,
        },
        RelPattern {
            slug: "CAUSE_EFFECT",
            triggers: &["caused", "causes", "leads to", "results in", "because"],
            confidence: 0.55,
        },
        RelPattern {
            slug: "USAGE",
            triggers: &["use", "uses", "used", "using", "utilize", "employ", "adopt"],
            confidence: 0.55,
        },
        RelPattern {
            slug: "ARTIFACT",
            triggers: &[
                "tool",
                "library",
                "framework",
                "system",
                "artifact",
                "implementation",
            ],
            confidence: 0.55,
        },
        RelPattern {
            slug: "COMPARE",
            triggers: &[
                "compare",
                "compared to",
                "versus",
                "vs",
                "better than",
                "worse than",
            ],
            confidence: 0.55,
        },
        RelPattern {
            slug: "GENERAL_AFFILIATION",
            triggers: &[
                "affiliation",
                "affiliated with",
                "member of",
                "part of",
                "associated with",
            ],
            confidence: 0.55,
        },
        // CHisIEC (classical Chinese) relations (match either simplified or traditional labels)
        RelPattern {
            slug: "父母",
            triggers: &["父", "母", "父母"],
            confidence: 0.7,
        },
        RelPattern {
            slug: "兄弟",
            triggers: &["兄", "弟", "兄弟"],
            confidence: 0.7,
        },
        RelPattern {
            slug: "別名",
            triggers: &["別名", "别名"],
            confidence: 0.75,
        },
        RelPattern {
            slug: "到達",
            triggers: &["到", "至", "達", "到達", "到达"],
            confidence: 0.6,
        },
        RelPattern {
            slug: "出生於某地",
            triggers: &["生於", "生于", "出生於", "出生于"],
            confidence: 0.65,
        },
        RelPattern {
            slug: "任職",
            triggers: &["任", "拜", "任職", "任职"],
            confidence: 0.6,
        },
        RelPattern {
            slug: "管理",
            triggers: &["管", "治", "守", "管理"],
            confidence: 0.55,
        },
        RelPattern {
            slug: "駐守",
            triggers: &["駐", "驻", "守", "駐守", "驻守"],
            confidence: 0.55,
        },
        RelPattern {
            slug: "敵對攻伐",
            triggers: &["敵", "敌", "攻", "伐", "戰", "战"],
            confidence: 0.55,
        },
        RelPattern {
            slug: "同僚",
            triggers: &["同僚"],
            confidence: 0.55,
        },
        RelPattern {
            slug: "政治奧援",
            triggers: &["奧援", "奥援"],
            confidence: 0.55,
        },
        // Communication/Interaction
        RelPattern {
            slug: "MET_WITH",
            triggers: &["met with", "met", "met up with", "encountered", "saw"],
            confidence: 0.65,
        },
        RelPattern {
            slug: "SPOKE_WITH",
            triggers: &[
                "spoke with",
                "talked with",
                "discussed with",
                "conversed with",
            ],
            confidence: 0.7,
        },
        // Ownership
        RelPattern {
            slug: "OWNS",
            triggers: &["owns", "owner of", "possesses", "holds"],
            confidence: 0.75,
        },
        // =========================================================================
        // Multilingual relation triggers
        // =========================================================================
        // Spanish (es)
        RelPattern {
            slug: "WORKS_FOR",
            triggers: &["trabaja en", "trabaja para", "empleado de", "trabaja con"],
            confidence: 0.7,
        },
        RelPattern {
            slug: "FOUNDED",
            triggers: &["fundó", "fundada", "creó", "creada", "estableció", "inició"],
            confidence: 0.8,
        },
        RelPattern {
            slug: "LOCATED_IN",
            triggers: &[
                "en",
                "ubicado en",
                "situado en",
                "basado en",
                "localizado en",
            ],
            confidence: 0.6,
        },
        RelPattern {
            slug: "BORN_IN",
            triggers: &["nació en", "nacido en", "originario de", "de"],
            confidence: 0.7,
        },
        RelPattern {
            slug: "LIVES_IN",
            triggers: &["cerno en", "reside en", "viviendo en"],
            confidence: 0.65,
        },
        RelPattern {
            slug: "MARRIED_TO",
            triggers: &["casado con", "casada con", "esposo de", "esposa de"],
            confidence: 0.85,
        },
        // French (fr)
        RelPattern {
            slug: "WORKS_FOR",
            triggers: &[
                "travaille pour",
                "travaille à",
                "employé de",
                "travaille avec",
            ],
            confidence: 0.7,
        },
        RelPattern {
            slug: "FOUNDED",
            triggers: &["fondé", "fondée", "créé", "créée", "établi", "établie"],
            confidence: 0.8,
        },
        RelPattern {
            slug: "LOCATED_IN",
            triggers: &["dans", "à", "situé en", "basé en", "localisé en"],
            confidence: 0.6,
        },
        RelPattern {
            slug: "BORN_IN",
            triggers: &["né en", "née en", "originaire de", "de"],
            confidence: 0.7,
        },
        RelPattern {
            slug: "LIVES_IN",
            triggers: &["vit en", "réside en", "vivant en"],
            confidence: 0.65,
        },
        RelPattern {
            slug: "MARRIED_TO",
            triggers: &["marié avec", "mariée avec", "époux de", "épouse de"],
            confidence: 0.85,
        },
        // German (de)
        RelPattern {
            slug: "WORKS_FOR",
            triggers: &[
                "arbeitet für",
                "arbeitet bei",
                "angestellt bei",
                "arbeitet mit",
            ],
            confidence: 0.7,
        },
        RelPattern {
            slug: "FOUNDED",
            triggers: &[
                "gegründet",
                "gründete",
                "erstellt",
                "errichtet",
                "etabliert",
            ],
            confidence: 0.8,
        },
        RelPattern {
            slug: "LOCATED_IN",
            triggers: &["in", "bei", "situiert in", "basiert in", "befindet sich in"],
            confidence: 0.6,
        },
        RelPattern {
            slug: "BORN_IN",
            triggers: &["geboren in", "geboren am", "stammt aus", "aus"],
            confidence: 0.7,
        },
        RelPattern {
            slug: "LIVES_IN",
            triggers: &["lebt in", "wohnt in", "lebend in"],
            confidence: 0.65,
        },
        RelPattern {
            slug: "MARRIED_TO",
            triggers: &["verheiratet mit", "ehemann von", "ehefrau von"],
            confidence: 0.85,
        },
        // Chinese (zh) - Simplified
        RelPattern {
            slug: "WORKS_FOR",
            triggers: &["为", "在", "工作于", "就职于", "任职于"],
            confidence: 0.7,
        },
        RelPattern {
            slug: "FOUNDED",
            triggers: &["创立", "创建", "建立", "成立", "创办"],
            confidence: 0.8,
        },
        RelPattern {
            slug: "LOCATED_IN",
            triggers: &["在", "位于", "坐落于", "地处"],
            confidence: 0.6,
        },
        RelPattern {
            slug: "BORN_IN",
            triggers: &["出生于", "生于", "来自", "出生于"],
            confidence: 0.7,
        },
        RelPattern {
            slug: "LIVES_IN",
            triggers: &["居住于", "住在", "生活在"],
            confidence: 0.65,
        },
        RelPattern {
            slug: "MARRIED_TO",
            triggers: &["与...结婚", "嫁给", "娶了"],
            confidence: 0.85,
        },
        // Japanese (ja)
        RelPattern {
            slug: "WORKS_FOR",
            triggers: &["で働く", "に勤務", "に所属", "で就職"],
            confidence: 0.7,
        },
        RelPattern {
            slug: "FOUNDED",
            triggers: &["設立", "創立", "設立した", "創設"],
            confidence: 0.8,
        },
        RelPattern {
            slug: "LOCATED_IN",
            triggers: &["に", "で", "に位置", "に所在"],
            confidence: 0.6,
        },
        RelPattern {
            slug: "BORN_IN",
            triggers: &["に生まれた", "の出身", "で生まれた"],
            confidence: 0.7,
        },
        RelPattern {
            slug: "LIVES_IN",
            triggers: &["に住む", "に居住", "に在住"],
            confidence: 0.65,
        },
        RelPattern {
            slug: "MARRIED_TO",
            triggers: &["と結婚", "と結婚した", "の配偶者"],
            confidence: 0.85,
        },
        // Arabic (ar) - RTL
        RelPattern {
            slug: "WORKS_FOR",
            triggers: &["يعمل في", "يعمل لصالح", "موظف في", "يعمل مع"],
            confidence: 0.7,
        },
        RelPattern {
            slug: "FOUNDED",
            triggers: &["أسس", "أنشأ", "تأسست", "أنشأت"],
            confidence: 0.8,
        },
        RelPattern {
            slug: "LOCATED_IN",
            triggers: &["في", "ب", "يقع في", "موجود في"],
            confidence: 0.6,
        },
        RelPattern {
            slug: "BORN_IN",
            triggers: &["ولد في", "من مواليد", "من"],
            confidence: 0.7,
        },
        RelPattern {
            slug: "LIVES_IN",
            triggers: &["يعيش في", "يسكن في", "مقيم في"],
            confidence: 0.65,
        },
        RelPattern {
            slug: "MARRIED_TO",
            triggers: &["متزوج من", "زوج", "زوجة"],
            confidence: 0.85,
        },
        // Russian (ru)
        RelPattern {
            slug: "WORKS_FOR",
            triggers: &["работает в", "работает на", "работает для", "сотрудник"],
            confidence: 0.7,
        },
        RelPattern {
            slug: "FOUNDED",
            triggers: &["основал", "основала", "создал", "создала", "учредил"],
            confidence: 0.8,
        },
        RelPattern {
            slug: "LOCATED_IN",
            triggers: &["в", "на", "расположен в", "находится в"],
            confidence: 0.6,
        },
        RelPattern {
            slug: "BORN_IN",
            triggers: &["родился в", "родилась в", "родом из", "из"],
            confidence: 0.7,
        },
        RelPattern {
            slug: "LIVES_IN",
            triggers: &["живет в", "проживает в", "живущий в"],
            confidence: 0.65,
        },
        RelPattern {
            slug: "MARRIED_TO",
            triggers: &["женат на", "замужем за", "супруг", "супруга"],
            confidence: 0.85,
        },
    ];

    for pattern in patterns {
        // Find the canonical label in the registry (case-insensitive).
        // We return the label's *original* slug so callers preserve user-provided casing.
        let label = match relation_labels.iter().find(|l| {
            // Match both:
            // - exact canonical names (e.g. "PART_OF")
            // - normalized dataset slugs (e.g. "part-of" -> "PART_OF")
            norm_rel_slug(&l.slug) == pattern.slug || l.slug.eq_ignore_ascii_case(pattern.slug)
        }) {
            Some(l) => *l,
            None => continue,
        };

        for trigger in pattern.triggers {
            if let Some(pos) = between_lower.find(trigger) {
                // Validate entity types make sense for the relation
                let valid = match pattern.slug {
                    // Person-Organization relations
                    "CEO_OF" | "WORKS_FOR" | "FOUNDED" | "MANAGES" | "REPORTS_TO" => {
                        // If either side is unknown/misc, don't reject on type alone (relation datasets
                        // often use a richer schema than `EntityType`).
                        matches!(
                            head.entity_type,
                            EntityType::Other(_) | EntityType::Custom { .. }
                        ) || matches!(
                            tail.entity_type,
                            EntityType::Other(_) | EntityType::Custom { .. }
                        ) || (matches!(head.entity_type, EntityType::Person)
                            && matches!(tail.entity_type, EntityType::Organization))
                    }
                    // Location relations (any entity can be located in/born in a location)
                    "LOCATED_IN" | "BORN_IN" | "LIVES_IN" | "DIED_IN" => {
                        matches!(
                            tail.entity_type,
                            EntityType::Other(_) | EntityType::Custom { .. }
                        ) || matches!(tail.entity_type, EntityType::Location)
                    }
                    // Temporal relations (any entity can have temporal attributes)
                    "OCCURRED_ON" | "STARTED_ON" | "ENDED_ON" => {
                        matches!(
                            tail.entity_type,
                            EntityType::Other(_) | EntityType::Custom { .. }
                        ) || matches!(tail.entity_type, EntityType::Date | EntityType::Time)
                    }
                    // Organizational relations
                    "PART_OF" | "ACQUIRED" | "MERGED_WITH" | "PARENT_OF" => {
                        matches!(
                            head.entity_type,
                            EntityType::Other(_) | EntityType::Custom { .. }
                        ) || matches!(
                            tail.entity_type,
                            EntityType::Other(_) | EntityType::Custom { .. }
                        ) || (matches!(head.entity_type, EntityType::Organization)
                            && matches!(tail.entity_type, EntityType::Organization))
                    }
                    // Social relations
                    "MARRIED_TO" | "CHILD_OF" | "SIBLING_OF" => {
                        matches!(
                            head.entity_type,
                            EntityType::Other(_) | EntityType::Custom { .. }
                        ) || matches!(
                            tail.entity_type,
                            EntityType::Other(_) | EntityType::Custom { .. }
                        ) || (matches!(head.entity_type, EntityType::Person)
                            && matches!(tail.entity_type, EntityType::Person))
                    }
                    // Academic relations
                    "STUDIED_AT" | "TEACHES_AT" => {
                        matches!(
                            head.entity_type,
                            EntityType::Other(_) | EntityType::Custom { .. }
                        ) || matches!(
                            tail.entity_type,
                            EntityType::Other(_) | EntityType::Custom { .. }
                        ) || (matches!(head.entity_type, EntityType::Person)
                            && matches!(
                                tail.entity_type,
                                EntityType::Organization | EntityType::Location
                            ))
                    }
                    // Product relations
                    "DEVELOPS" | "USES" => {
                        matches!(
                            head.entity_type,
                            EntityType::Other(_) | EntityType::Custom { .. }
                        ) || matches!(
                            head.entity_type,
                            EntityType::Organization | EntityType::Person
                        )
                    }
                    // Interaction relations
                    "MET_WITH" | "SPOKE_WITH" => {
                        matches!(
                            head.entity_type,
                            EntityType::Other(_) | EntityType::Custom { .. }
                        ) || matches!(
                            tail.entity_type,
                            EntityType::Other(_) | EntityType::Custom { .. }
                        ) || (matches!(head.entity_type, EntityType::Person)
                            && matches!(
                                tail.entity_type,
                                EntityType::Person | EntityType::Organization
                            ))
                    }
                    // Ownership
                    "OWNS" => {
                        matches!(
                            head.entity_type,
                            EntityType::Other(_) | EntityType::Custom { .. }
                        ) || matches!(
                            head.entity_type,
                            EntityType::Person | EntityType::Organization
                        )
                    }
                    _ => true, // Default: allow any combination
                };

                if valid {
                    return Some((
                        label.slug.as_str(),
                        pattern.confidence,
                        Some((pos, pos + trigger.len())),
                    ));
                }
            }
        }
    }

    None
}

