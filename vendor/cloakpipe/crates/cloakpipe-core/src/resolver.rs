//! Fuzzy entity resolution — merge variant spellings of the same entity.
//!
//! Resolves "Rishikesh", "Rishi", "Rishiksh" (typo) to the same vault token
//! using Jaro-Winkler similarity, prefix matching, and user-defined alias groups.

use crate::EntityCategory;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Configuration for the entity resolver.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolverConfig {
    /// Enable fuzzy entity resolution (default: false).
    #[serde(default)]
    pub enabled: bool,
    /// Minimum similarity score to merge entities (0.0–1.0, default: 0.90).
    #[serde(default = "default_threshold")]
    pub threshold: f64,
    /// Minimum string length for prefix matching (default: 4).
    #[serde(default = "default_min_prefix_len")]
    pub min_prefix_len: usize,
    /// User-defined alias groups — each group shares a single token.
    #[serde(default)]
    pub aliases: Vec<AliasGroup>,
}

/// A group of strings that should all resolve to the same entity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AliasGroup {
    pub group: Vec<String>,
}

fn default_threshold() -> f64 {
    0.90
}

fn default_min_prefix_len() -> usize {
    4
}

impl Default for ResolverConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            threshold: default_threshold(),
            min_prefix_len: default_min_prefix_len(),
            aliases: Vec::new(),
        }
    }
}

/// Fuzzy entity resolver that maps variant spellings to canonical forms.
pub struct EntityResolver {
    config: ResolverConfig,
    /// Alias lookup: normalized variant → canonical form (first entry in group).
    alias_map: HashMap<String, String>,
}

impl EntityResolver {
    pub fn new(config: ResolverConfig) -> Self {
        let mut alias_map = HashMap::new();
        for group in &config.aliases {
            if group.group.len() < 2 {
                continue;
            }
            let canonical = group.group[0].clone();
            for variant in &group.group {
                alias_map.insert(variant.to_lowercase().trim().to_string(), canonical.clone());
            }
        }
        Self { config, alias_map }
    }

    /// Resolve an entity's original text to a canonical form.
    ///
    /// Checks (in order):
    /// 1. User-defined alias groups (exact match, case-insensitive)
    /// 2. Fuzzy match against existing vault entries (same category only)
    ///
    /// Returns the canonical form if a match is found, or None (use original).
    pub fn resolve(
        &self,
        original: &str,
        category: &EntityCategory,
        existing_entries: &HashMap<String, EntityCategory>,
    ) -> Option<String> {
        if !self.config.enabled {
            return None;
        }

        // 1. Check alias groups
        let normalized = original.to_lowercase().trim().to_string();
        if let Some(canonical) = self.alias_map.get(&normalized) {
            return Some(canonical.clone());
        }

        // 2. Fuzzy match against existing entries in the same category
        let mut best_match: Option<String> = None;
        let mut best_score: f64 = 0.0;

        for (existing_original, existing_category) in existing_entries {
            // Only match within the same category
            if existing_category != category {
                continue;
            }

            // Skip exact match (vault already handles that)
            if existing_original == original {
                continue;
            }

            let score = self.similarity(original, existing_original);
            if score > best_score && score >= self.config.threshold {
                best_score = score;
                best_match = Some(existing_original.clone());
            }
        }

        best_match
    }

    /// Compute a combined similarity score between two strings.
    ///
    /// Uses Jaro-Winkler as the base metric, with bonuses for:
    /// - One string being a prefix of the other (if long enough)
    /// - Case-insensitive match
    fn similarity(&self, a: &str, b: &str) -> f64 {
        let a_lower = a.to_lowercase();
        let b_lower = b.to_lowercase();

        // Base: Jaro-Winkler distance (0.0–1.0, 1.0 = identical)
        let mut score = strsim::jaro_winkler(&a_lower, &b_lower);

        // Bonus: prefix matching (one is prefix of the other)
        let shorter = a_lower.len().min(b_lower.len());
        if shorter >= self.config.min_prefix_len
            && (a_lower.starts_with(&b_lower) || b_lower.starts_with(&a_lower))
        {
            score += 0.08;
        }

        // Cap at 1.0
        score.min(1.0)
    }

    /// Check if the resolver is enabled.
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enabled_config() -> ResolverConfig {
        ResolverConfig {
            enabled: true,
            threshold: 0.90,
            min_prefix_len: 4,
            aliases: Vec::new(),
        }
    }

    fn config_with_aliases() -> ResolverConfig {
        ResolverConfig {
            enabled: true,
            threshold: 0.90,
            min_prefix_len: 4,
            aliases: vec![AliasGroup {
                group: vec![
                    "Rishikesh Kumar".into(),
                    "Rishi".into(),
                    "Rishi kesh".into(),
                ],
            }],
        }
    }

    #[test]
    fn test_alias_resolution() {
        let resolver = EntityResolver::new(config_with_aliases());
        let existing = HashMap::new();

        assert_eq!(
            resolver.resolve("Rishi", &EntityCategory::Person, &existing),
            Some("Rishikesh Kumar".into())
        );
        assert_eq!(
            resolver.resolve("Rishi kesh", &EntityCategory::Person, &existing),
            Some("Rishikesh Kumar".into())
        );
        // Case insensitive
        assert_eq!(
            resolver.resolve("rishi", &EntityCategory::Person, &existing),
            Some("Rishikesh Kumar".into())
        );
    }

    #[test]
    fn test_alias_canonical_self_resolves() {
        let resolver = EntityResolver::new(config_with_aliases());
        let existing = HashMap::new();

        assert_eq!(
            resolver.resolve("Rishikesh Kumar", &EntityCategory::Person, &existing),
            Some("Rishikesh Kumar".into())
        );
    }

    #[test]
    fn test_fuzzy_match_misspelling() {
        let resolver = EntityResolver::new(enabled_config());
        let mut existing = HashMap::new();
        existing.insert("Rishikesh".to_string(), EntityCategory::Person);

        // Typo: "Rishiksh" is close to "Rishikesh"
        let result = resolver.resolve("Rishiksh", &EntityCategory::Person, &existing);
        assert_eq!(result, Some("Rishikesh".into()));
    }

    #[test]
    fn test_fuzzy_match_prefix() {
        let resolver = EntityResolver::new(enabled_config());
        let mut existing = HashMap::new();
        existing.insert("Rishikesh".to_string(), EntityCategory::Person);

        // "Rishi" is a prefix of "Rishikesh" — gets prefix bonus
        let result = resolver.resolve("Rishi", &EntityCategory::Person, &existing);
        // Jaro-Winkler("rishi", "rishikesh") ≈ 0.87 + 0.08 prefix = 0.95 > 0.90
        assert_eq!(result, Some("Rishikesh".into()));
    }

    #[test]
    fn test_no_cross_category_match() {
        let resolver = EntityResolver::new(enabled_config());
        let mut existing = HashMap::new();
        existing.insert("Rishikesh".to_string(), EntityCategory::Location);

        // Same string but different category — should NOT match
        let result = resolver.resolve("Rishiksh", &EntityCategory::Person, &existing);
        assert_eq!(result, None);
    }

    #[test]
    fn test_no_match_different_entities() {
        let resolver = EntityResolver::new(enabled_config());
        let mut existing = HashMap::new();
        existing.insert("John".to_string(), EntityCategory::Person);

        // "Alice" is nothing like "John"
        let result = resolver.resolve("Alice", &EntityCategory::Person, &existing);
        assert_eq!(result, None);
    }

    #[test]
    fn test_disabled_resolver() {
        let resolver = EntityResolver::new(ResolverConfig::default());
        let mut existing = HashMap::new();
        existing.insert("Rishikesh".to_string(), EntityCategory::Person);

        let result = resolver.resolve("Rishiksh", &EntityCategory::Person, &existing);
        assert_eq!(result, None);
    }

    #[test]
    fn test_short_prefix_rejected() {
        let resolver = EntityResolver::new(enabled_config());
        let mut existing = HashMap::new();
        existing.insert("Alice".to_string(), EntityCategory::Person);

        // "Al" is too short for prefix matching (< min_prefix_len of 4)
        let result = resolver.resolve("Al", &EntityCategory::Person, &existing);
        // Jaro-Winkler("al", "alice") is too low without prefix bonus
        assert_eq!(result, None);
    }

    #[test]
    fn test_similarity_scores() {
        let resolver = EntityResolver::new(enabled_config());

        // Misspelling: very high
        assert!(resolver.similarity("Rishikesh", "Rishiksh") > 0.90);
        // Prefix: high with bonus
        assert!(resolver.similarity("Rishi", "Rishikesh") > 0.90);
        // Completely different: low
        assert!(resolver.similarity("John", "Mumbai") < 0.60);
        // Similar but dangerous: "John" vs "Joan"
        let john_joan = resolver.similarity("John", "Joan");
        assert!(john_joan < 0.92); // Should be below threshold or borderline
    }
}
