use anno::EntityType;
use serde_json;

/// Parse BIO tag into prefix and type.
#[cfg(test)]
pub(crate) fn parse_bio_tag(tag: &str) -> (&str, &str) {
    if tag == "O" {
        return ("O", "");
    }

    // Handle B-PER, I-LOC, etc.
    if let Some(pos) = tag.find('-') {
        (&tag[..pos], &tag[pos + 1..])
    } else {
        // No prefix, treat as entity type with implicit B
        ("B", tag)
    }
}

/// Map dataset-specific entity types to our EntityType enum.
///
/// **Prefer `crate::schema::map_to_canonical()` for new code** - it handles
/// NORP correctly (as GROUP, not ORG) and preserves GPE/FAC distinctions.
///
/// # Known Issues (preserved for backwards compatibility)
///
/// - NORP → Organization (WRONG: should be Group)
/// - GPE/FAC/LOC all → Location (loses semantic distinctions)
///
/// See `src/schema.rs` for the corrected mappings.
#[cfg(test)]
pub(crate) fn map_entity_type(original: &str) -> EntityType {
    // Use the new canonical mapper for consistent semantics
    anno::schema::map_to_canonical(original, None)
}

/// Extract spans (start_char, end_char) pairs from a JSON array.
pub(crate) fn spans_from_array(arr: Option<&Vec<serde_json::Value>>) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let Some(arr) = arr else { return out };
    for item in arr {
        let Some(s) = item.get("start_char").and_then(|v| v.as_u64()) else {
            continue;
        };
        let Some(e) = item.get("end_char").and_then(|v| v.as_u64()) else {
            continue;
        };
        let s = s as usize;
        let e = e as usize;
        if e > s {
            out.push((s, e));
        }
    }
    out
}

/// Check if a token (char span) overlaps with any span in the list.
pub(crate) fn overlaps(token_s: usize, token_e: usize, spans: &[(usize, usize)]) -> bool {
    spans.iter().any(|(s, e)| token_s < *e && token_e > *s)
}

/// Extract tag names from HF API features metadata.
pub(crate) fn extract_tag_names_from_features(parsed: &serde_json::Value) -> Vec<String> {
    let mut tag_names = Vec::new();

    if let Some(features) = parsed.get("features").and_then(|v| v.as_array()) {
        for feature in features {
            let name = feature.get("name").and_then(|v| v.as_str());
            if name == Some("ner_tags") {
                // Look for ClassLabel names
                if let Some(names) = feature
                    .get("type")
                    .and_then(|t| t.get("feature"))
                    .and_then(|f| f.get("names"))
                    .and_then(|n| n.as_array())
                {
                    for name in names {
                        if let Some(s) = name.as_str() {
                            tag_names.push(s.to_string());
                        }
                    }
                }
                break;
            }
        }
    }

    tag_names
}

/// Extract class label names for `label` fields in HF API features metadata.
pub(crate) fn extract_class_names_from_features(parsed: &serde_json::Value) -> Vec<String> {
    let mut names = Vec::new();
    if let Some(features) = parsed.get("features").and_then(|v| v.as_array()) {
        for feature in features {
            let name = feature.get("name").and_then(|v| v.as_str());
            if name == Some("label") {
                if let Some(label_names) = feature
                    .get("type")
                    .and_then(|t| t.get("names"))
                    .and_then(|n| n.as_array())
                {
                    for n in label_names {
                        if let Some(s) = n.as_str() {
                            names.push(s.to_string());
                        }
                    }
                }
                break;
            }
        }
    }
    names
}
