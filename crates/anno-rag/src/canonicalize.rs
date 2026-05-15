//! Deterministic entity canonicalizer for v0.2 entity-graph traversal.
//!
//! Stages, in order: lowercase → NFD-decompose-and-strip-combining-marks
//! (diacritic strip) → ASCII-punctuation strip → whitespace collapse →
//! tenant-scoped alias lookup. No LLM call. The output is the value the
//! `entity_refs` column carries on disk and the key the LabelList index
//! filters on.

use std::collections::HashMap;
use unicode_normalization::UnicodeNormalization;

/// Distinguishes the two entity sources merged into `Memory::entity_refs`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntityKind {
    /// Vault-tokenized PII entity. Canonical form embeds the vault token
    /// itself (`pii:PERSON:PERSON_a4f3`); resolves only within the
    /// owning tenant's vault.
    PiiToken,
    /// Non-PII named entity from `anno::StackedNER`. Canonical form is
    /// text-derived (`ent:ORG:cabinet dupont`).
    NamedEntity,
}

/// Canonicalize a named-entity surface form into its `entity_refs` value.
///
/// `entity_kind_tag` is the StackedNER tag (`"PER"`, `"ORG"`, `"LOC"`,
/// `"MISC"`). `aliases` is a per-tenant lookup applied to the cleaned
/// form before the final string is composed — empty means "no aliases".
#[must_use]
pub fn canonicalize_entity(
    text: &str,
    entity_kind_tag: &str,
    aliases: &HashMap<String, String>,
) -> String {
    let lower = text.to_lowercase();
    // NFD decompose, then drop combining marks (the canonical "strip
    // diacritics" trick on a unicode-correct path — café → cafe, à → a).
    let stripped: String = lower
        .nfd()
        .filter(|c| !unicode_normalization::char::is_combining_mark(*c))
        .collect();
    // Remove ASCII punctuation so "Me. Dupont" canonicalizes the same
    // as "Me Dupont" and the alias table can carry a single key.
    let no_punct: String = stripped
        .chars()
        .filter(|c| !c.is_ascii_punctuation())
        .collect();
    let collapsed = collapse_whitespace(&no_punct);
    let aliased = aliases.get(&collapsed).cloned().unwrap_or(collapsed);
    format!("ent:{entity_kind_tag}:{aliased}")
}

/// Canonicalize a vault-tokenized PII reference. Format mirrors the
/// `entity_refs` column on disk; the token itself is the canonical
/// resolution within the tenant vault.
#[must_use]
pub fn canonicalize_pii_token(label: &str, token: &str) -> String {
    format!("pii:{label}:{token}")
}

fn collapse_whitespace(s: &str) -> String {
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lowercase_and_strip_diacritics() {
        let aliases = HashMap::new();
        assert_eq!(
            canonicalize_entity("Maître Dupont", "PER", &aliases),
            "ent:PER:maitre dupont"
        );
        assert_eq!(
            canonicalize_entity("CAFÉ DE FLORE", "ORG", &aliases),
            "ent:ORG:cafe de flore"
        );
    }

    #[test]
    fn alias_table_applied() {
        let mut aliases = HashMap::new();
        aliases.insert("maitre dupont".into(), "dupont".into());
        aliases.insert("me dupont".into(), "dupont".into());
        assert_eq!(
            canonicalize_entity("Maître Dupont", "PER", &aliases),
            "ent:PER:dupont"
        );
        assert_eq!(
            canonicalize_entity("Me. DUPONT", "PER", &aliases),
            "ent:PER:dupont"
        );
    }

    #[test]
    fn pii_token_format() {
        assert_eq!(
            canonicalize_pii_token("PERSON", "PERSON_a4f3"),
            "pii:PERSON:PERSON_a4f3"
        );
    }

    #[test]
    fn whitespace_collapsed() {
        let aliases = HashMap::new();
        assert_eq!(
            canonicalize_entity("  Cabinet   Dupont  ", "ORG", &aliases),
            "ent:ORG:cabinet dupont"
        );
    }
}
