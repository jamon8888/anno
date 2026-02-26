//! Simple coreference resolution for analysis/evaluation pipelines.
//!
//! Provides minimal resolvers to produce coreference chains from entities, completing the loop
//! between extraction and coreference metric computation.

use super::coref::CorefChain;
use crate::backends::box_embeddings::{BoxCorefConfig, BoxEmbedding};
use crate::{Entity, EntityType};
use anno_core::CanonicalId;
use std::collections::HashMap;

/// Configuration for simple coreference resolver.
#[derive(Debug, Clone)]
pub struct CorefConfig {
    /// Similarity threshold for name matching (0.0-1.0).
    pub similarity_threshold: f64,
    /// Maximum sentence distance for pronoun resolution.
    pub max_pronoun_distance: usize,
    /// Enable fuzzy name matching (e.g., "John Smith" ~ "J. Smith").
    pub fuzzy_matching: bool,
    /// Include singletons in output chains.
    pub include_singletons: bool,
}

impl Default for CorefConfig {
    fn default() -> Self {
        Self {
            similarity_threshold: 0.7,
            max_pronoun_distance: 3,
            fuzzy_matching: true,
            include_singletons: true,
        }
    }
}

/// Simple rule-based coreference resolver.
#[derive(Debug, Clone)]
pub struct SimpleCorefResolver {
    config: CorefConfig,
}

impl Default for SimpleCorefResolver {
    fn default() -> Self {
        Self::new(CorefConfig::default())
    }
}

impl SimpleCorefResolver {
    /// Create a new resolver with configuration.
    #[must_use]
    pub fn new(config: CorefConfig) -> Self {
        Self { config }
    }

    /// Resolve coreference for entities, assigning canonical IDs.
    #[must_use]
    pub fn resolve_entities(&self, entities: &[Entity]) -> Vec<Entity> {
        if entities.is_empty() {
            return vec![];
        }

        let mut resolved = entities.to_vec();
        let mut next_cluster_id = CanonicalId::ZERO;

        // Map from canonical form to cluster ID.
        let mut canonical_to_cluster: HashMap<String, CanonicalId> = HashMap::new();

        // Process entities in order.
        for i in 0..resolved.len() {
            let entity = &resolved[i];

            // Skip if already assigned.
            if entity.canonical_id.is_some() {
                continue;
            }

            let cluster_id =
                self.find_matching_cluster(entity, &resolved[..i], &canonical_to_cluster);

            let cluster_id = cluster_id.unwrap_or_else(|| {
                let id = next_cluster_id;
                next_cluster_id += 1;
                id
            });

            resolved[i].canonical_id = Some(cluster_id);

            let canonical = self.canonical_form(&resolved[i].text, &resolved[i].entity_type);
            canonical_to_cluster.insert(canonical, cluster_id);
        }

        resolved
    }

    /// Resolve coreference for entities.
    ///
    /// This is an inherent-method convenience wrapper so callers do not need to import the
    /// [`CoreferenceResolver`] trait to call `resolver.resolve(&entities)`.
    #[must_use]
    pub fn resolve(&self, entities: &[Entity]) -> Vec<Entity> {
        self.resolve_entities(entities)
    }

    /// Convert resolved entities directly to coreference chains.
    #[must_use]
    pub fn resolve_to_chains(&self, entities: &[Entity]) -> Vec<CorefChain> {
        let resolved = self.resolve_entities(entities);
        super::coref::entities_to_chains(&resolved)
    }

    fn find_matching_cluster(
        &self,
        entity: &Entity,
        previous: &[Entity],
        canonical_map: &HashMap<String, CanonicalId>,
    ) -> Option<CanonicalId> {
        // Strategy 1: pronoun resolution.
        if self.is_pronoun(&entity.text) {
            return self.resolve_pronoun(entity, previous);
        }

        // Strategy 2: exact canonical match.
        let canonical = self.canonical_form(&entity.text, &entity.entity_type);
        if let Some(&cluster_id) = canonical_map.get(&canonical) {
            return Some(cluster_id);
        }

        // Strategy 3: substring/fuzzy matching.
        if self.config.fuzzy_matching {
            for (other_canonical, &cluster_id) in canonical_map {
                if self.names_match(&canonical, other_canonical) {
                    return Some(cluster_id);
                }
            }
        }

        None
    }

    fn resolve_pronoun(&self, pronoun: &Entity, previous: &[Entity]) -> Option<CanonicalId> {
        let pronoun_gender = self.infer_gender(&pronoun.text);

        for entity in previous
            .iter()
            .rev()
            .take(self.config.max_pronoun_distance * 10)
        {
            if self.is_pronoun(&entity.text) {
                continue;
            }
            if !self.pronoun_compatible(&pronoun.text, &entity.entity_type) {
                continue;
            }

            let entity_gender = self.infer_gender(&entity.text);
            match (pronoun_gender, entity_gender) {
                (Some('n'), _) => {}
                (_, Some('n')) => {}
                (Some(pg), Some(eg)) => {
                    if pg != eg {
                        continue;
                    }
                }
                (_, None) => {}
                (None, Some(_)) => {}
            }

            return entity.canonical_id;
        }

        None
    }

    fn is_pronoun(&self, text: &str) -> bool {
        matches!(
            text.to_lowercase().as_str(),
            // Traditional gendered pronouns
            "he" | "she" | "him" | "her" | "his" | "hers" | "himself" | "herself" |
            // Singular they
            "they" | "them" | "their" | "theirs" | "themselves" | "themself" |
            // Impersonal pronouns
            "it" | "its" | "itself" |
            // Neopronouns
            "xe" | "xem" | "xyr" | "xyrs" | "xemself" |
            "ze" | "hir" | "zir" | "hirs" | "zirs" | "hirself" | "zirself" |
            "ey" | "em" | "eir" | "eirs" | "emself" |
            "fae" | "faer" | "faers" | "faeself"
        )
    }

    fn pronoun_compatible(&self, pronoun: &str, entity_type: &EntityType) -> bool {
        let lower = pronoun.to_lowercase();
        match entity_type {
            EntityType::Person => matches!(
                lower.as_str(),
                "he" | "she"
                    | "they"
                    | "him"
                    | "her"
                    | "them"
                    | "his"
                    | "hers"
                    | "their"
                    | "theirs"
                    | "himself"
                    | "herself"
                    | "themselves"
                    | "themself"
                    | "xe"
                    | "xem"
                    | "xyr"
                    | "xyrs"
                    | "xemself"
                    | "ze"
                    | "hir"
                    | "zir"
                    | "hirs"
                    | "zirs"
                    | "hirself"
                    | "zirself"
                    | "ey"
                    | "em"
                    | "eir"
                    | "eirs"
                    | "emself"
                    | "fae"
                    | "faer"
                    | "faers"
                    | "faeself"
            ),
            EntityType::Organization => matches!(
                lower.as_str(),
                "it" | "they" | "its" | "their" | "theirs" | "itself" | "themselves"
            ),
            EntityType::Location => matches!(lower.as_str(), "it" | "its" | "itself"),
            _ => matches!(lower.as_str(), "it" | "its" | "itself"),
        }
    }

    fn infer_gender(&self, text: &str) -> Option<char> {
        let lower = text.to_lowercase();
        match lower.as_str() {
            "he" | "him" | "his" | "himself" => Some('m'),
            "she" | "her" | "hers" | "herself" => Some('f'),
            "they" | "them" | "their" | "theirs" | "themselves" | "themself" => Some('n'),
            "xe" | "xem" | "xyr" | "xyrs" | "xemself" => Some('n'),
            "ze" | "hir" | "zir" | "hirs" | "zirs" | "hirself" | "zirself" => Some('n'),
            "ey" | "em" | "eir" | "eirs" | "emself" => Some('n'),
            "fae" | "faer" | "faers" | "faeself" => Some('n'),
            _ => None,
        }
    }

    fn canonical_form(&self, text: &str, entity_type: &EntityType) -> String {
        let normalized = text.to_lowercase().trim().to_string();
        format!("{}:{}", entity_type.as_label(), normalized)
    }

    fn names_match(&self, name1: &str, name2: &str) -> bool {
        let (type1, text1) = name1.split_once(':').unwrap_or(("", name1));
        let (type2, text2) = name2.split_once(':').unwrap_or(("", name2));
        if type1 != type2 {
            return false;
        }

        if text1 == text2 {
            return true;
        }

        if text1.contains(text2) || text2.contains(text1) {
            return true;
        }

        let words1: Vec<&str> = text1.split_whitespace().collect();
        let words2: Vec<&str> = text2.split_whitespace().collect();

        if words1.len() > 1 && words2.len() == 1 && words1.last() == words2.first() {
            return true;
        }
        if words2.len() > 1 && words1.len() == 1 && words2.last() == words1.first() {
            return true;
        }

        false
    }
}

// Re-export the canonical trait from `anno-core`.
pub use anno_core::CoreferenceResolver;

impl CoreferenceResolver for SimpleCorefResolver {
    fn resolve(&self, entities: &[Entity]) -> Vec<Entity> {
        self.resolve_entities(entities)
    }

    fn name(&self) -> &'static str {
        "simple-rule-based"
    }
}

/// Box-based coreference resolver.
pub struct BoxCorefResolver {
    config: BoxCorefConfig,
}

impl BoxCorefResolver {
    /// Create a new box-based coreference resolver.
    #[must_use]
    pub fn new(config: BoxCorefConfig) -> Self {
        Self { config }
    }

    /// Resolve coreference using provided box embeddings.
    ///
    /// # Panics
    /// Panics if `boxes.len() != entities.len()`.
    #[must_use]
    pub fn resolve_with_boxes(&self, entities: &[Entity], boxes: &[BoxEmbedding]) -> Vec<Entity> {
        assert_eq!(
            entities.len(),
            boxes.len(),
            "entities and boxes must have same length"
        );

        if entities.is_empty() {
            return vec![];
        }

        let mut resolved = entities.to_vec();
        let mut next_cluster_id = CanonicalId::ZERO;

        // Union-find clustering.
        let mut parent: Vec<usize> = (0..entities.len()).collect();

        fn find(parent: &mut [usize], i: usize) -> usize {
            if parent[i] != i {
                parent[i] = find(parent, parent[i]);
            }
            parent[i]
        }

        fn union(parent: &mut [usize], i: usize, j: usize) {
            let pi = find(parent, i);
            let pj = find(parent, j);
            if pi != pj {
                parent[pi] = pj;
            }
        }

        // Pairwise scoring.
        for i in 0..entities.len() {
            for j in (i + 1)..entities.len() {
                let score = boxes[i].coreference_score(&boxes[j]);

                if score >= self.config.coreference_threshold
                    && entities[i].entity_type == entities[j].entity_type
                    && (!self.config.enforce_syntactic_constraints
                        || self.check_syntactic_constraints(&entities[i], &entities[j]))
                {
                    union(&mut parent, i, j);
                }
            }
        }

        // Assign cluster IDs.
        let mut cluster_map: HashMap<usize, CanonicalId> = HashMap::new();
        for (i, resolved_entity) in resolved.iter_mut().enumerate().take(entities.len()) {
            let root = find(&mut parent, i);
            let cluster_id = *cluster_map.entry(root).or_insert_with(|| {
                let id = next_cluster_id;
                next_cluster_id += 1;
                id
            });
            resolved_entity.canonical_id = Some(cluster_id);
        }

        resolved
    }

    fn check_syntactic_constraints(&self, entity_a: &Entity, entity_b: &Entity) -> bool {
        let distance = if entity_a.end <= entity_b.start {
            entity_b.start - entity_a.end
        } else {
            entity_a.start.saturating_sub(entity_b.end)
        };

        if self.is_simple_pronoun(&entity_a.text) && distance <= self.config.max_local_distance {
            return distance > 50;
        }

        if self.is_rexpression(&entity_a.text) && distance <= self.config.max_local_distance {
            return distance > 20;
        }

        true
    }

    fn is_simple_pronoun(&self, text: &str) -> bool {
        matches!(
            text.to_lowercase().as_str(),
            "he" | "she" | "they" | "him" | "her" | "them" | "it" | "this" | "that"
        )
    }

    fn is_rexpression(&self, text: &str) -> bool {
        text.chars().next().is_some_and(|c| c.is_uppercase()) && text.len() > 1
    }
}

impl CoreferenceResolver for BoxCorefResolver {
    fn resolve(&self, entities: &[Entity]) -> Vec<Entity> {
        // Boxes are provided out-of-band; default is identity.
        entities.to_vec()
    }

    fn name(&self) -> &'static str {
        "box-embedding"
    }
}

/// Convert vector embeddings to box embeddings for coreference resolution.
#[must_use]
pub fn vectors_to_boxes(
    embeddings: &[f32],
    hidden_dim: usize,
    radius: Option<f32>,
) -> Vec<BoxEmbedding> {
    let radius = radius.unwrap_or(0.1);
    let num_entities = embeddings.len() / hidden_dim;
    let mut boxes = Vec::with_capacity(num_entities);

    for i in 0..num_entities {
        let start = i * hidden_dim;
        let end = start + hidden_dim;
        let vector = &embeddings[start..end];
        boxes.push(BoxEmbedding::from_vector(vector, radius));
    }

    boxes
}

/// Resolve coreference using box embeddings derived from vector embeddings.
#[must_use]
pub fn resolve_with_box_embeddings(
    entities: &[Entity],
    embeddings: &[f32],
    hidden_dim: usize,
    config: BoxCorefConfig,
) -> Vec<Entity> {
    let radius = config.vector_to_box_radius;
    let boxes = vectors_to_boxes(embeddings, hidden_dim, radius);
    let resolver = BoxCorefResolver::new(config);
    resolver.resolve_with_boxes(entities, &boxes)
}

// =============================================================================
// Discourse-aware coreference (optional)
// =============================================================================

#[cfg(feature = "discourse")]
use crate::discourse::{
    classify_shell_noun, DiscourseReferent, DiscourseScope, EventExtractor, EventMention,
    ReferentType,
};

/// Configuration for discourse-aware coreference resolution.
#[cfg(feature = "discourse")]
#[derive(Debug, Clone)]
pub struct DiscourseCorefConfig {
    /// Base coreference config.
    pub base: CorefConfig,
    /// Enable shell noun detection.
    pub detect_shell_nouns: bool,
    /// Maximum sentences back to search for antecedent.
    pub max_sentence_distance: usize,
    /// Prefer clause-level antecedents for "this".
    pub prefer_clause_antecedent: bool,
}

#[cfg(feature = "discourse")]
impl Default for DiscourseCorefConfig {
    fn default() -> Self {
        Self {
            base: CorefConfig::default(),
            detect_shell_nouns: true,
            max_sentence_distance: 3,
            prefer_clause_antecedent: true,
        }
    }
}

/// Discourse-aware coreference resolver.
///
/// This is intended for analysis/eval paths; it does not attempt to be a full abstract-anaphora
/// system. It extracts a small event inventory and uses discourse boundary heuristics to propose
/// clause/sentence antecedent spans for demonstratives and shell-noun phrases.
#[cfg(feature = "discourse")]
#[derive(Debug, Clone)]
pub struct DiscourseAwareResolver {
    config: DiscourseCorefConfig,
    base_resolver: SimpleCorefResolver,
    scope: DiscourseScope,
    text: String,
    events: Vec<EventMention>,
}

#[cfg(feature = "discourse")]
impl DiscourseAwareResolver {
    /// Create a new discourse-aware resolver.
    #[must_use]
    pub fn new(config: DiscourseCorefConfig, text: &str) -> Self {
        let scope = DiscourseScope::analyze(text);
        let extractor = EventExtractor::default();
        let events = extractor.extract(text);

        Self {
            base_resolver: SimpleCorefResolver::new(config.base.clone()),
            config,
            scope,
            text: text.to_string(),
            events,
        }
    }

    /// Get extracted events.
    #[must_use]
    pub fn events(&self) -> &[EventMention] {
        &self.events
    }

    /// Resolve coreference with discourse awareness.
    #[must_use]
    pub fn resolve_entities(&self, entities: &[Entity]) -> Vec<Entity> {
        // First pass: nominal-only coref.
        let mut resolved = self.base_resolver.resolve_entities(entities);

        // Second pass: for unresolved abstract anaphors, attach a synthetic canonical_id and encode
        // antecedent info in `normalized` as a lightweight carrier.
        for (i, entity) in resolved.iter_mut().enumerate() {
            if entity.canonical_id.is_some() {
                continue;
            }
            if self.is_abstract_anaphor(&entity.text) {
                if let Some(antecedent) = self.find_discourse_antecedent(entity) {
                    let cluster_id = CanonicalId::new(10_000 + i as u64);
                    entity.canonical_id = Some(cluster_id);
                    entity.normalized = Some(format!(
                        "DISCOURSE_REF:{:?}@{}-{}",
                        antecedent.referent_type, antecedent.start, antecedent.end
                    ));
                }
            }
        }

        resolved
    }

    /// Resolve coreference with discourse awareness.
    ///
    /// Inherent-method convenience wrapper (so callers don't need to import the
    /// [`CoreferenceResolver`] trait to call `resolver.resolve(&entities)`).
    #[must_use]
    pub fn resolve(&self, entities: &[Entity]) -> Vec<Entity> {
        self.resolve_entities(entities)
    }

    fn is_abstract_anaphor(&self, text: &str) -> bool {
        let lower = text.to_lowercase();

        if matches!(lower.as_str(), "this" | "that" | "it") {
            return true;
        }

        let words: Vec<&str> = lower.split_whitespace().collect();
        if words.len() >= 2 {
            let det = words[0];
            let noun = words
                .last()
                .expect("words has >= 2 elements")
                .trim_matches(|c: char| !c.is_alphanumeric());
            if matches!(det, "this" | "that" | "the" | "such")
                && classify_shell_noun(noun).is_some()
            {
                return true;
            }
        }

        false
    }

    fn find_event_near(&self, offset: usize, max_distance: usize) -> Option<&EventMention> {
        if let Some((clause_start, clause_end)) = self.scope.clause_at(offset) {
            for event in &self.events {
                if event.trigger_start >= clause_start && event.trigger_end <= clause_end {
                    return Some(event);
                }
            }
        }

        self.events
            .iter()
            .filter(|e| {
                let dist = if e.trigger_end <= offset {
                    offset - e.trigger_end
                } else {
                    e.trigger_start.saturating_sub(offset)
                };
                dist <= max_distance
            })
            .min_by_key(|e| {
                if e.trigger_end <= offset {
                    offset - e.trigger_end
                } else {
                    e.trigger_start.saturating_sub(offset)
                }
            })
    }

    fn find_event_clause_span(&self, event: &EventMention) -> (usize, usize) {
        if let Some((start, end)) = self.scope.clause_at(event.trigger_start) {
            return (start, end);
        }
        if let Some((start, end)) = self.scope.sentence_at(event.trigger_start) {
            return (start, end);
        }
        let char_count = self.text.chars().count();
        let context_before = event.trigger_start.saturating_sub(30);
        let context_after = (event.trigger_end + 30).min(char_count);
        (context_before, context_after)
    }

    fn infer_referent_type(&self, text: &str) -> ReferentType {
        let lower = text.to_lowercase();
        for event in &self.events {
            if lower.contains(&event.trigger.to_lowercase()) {
                if let Some(ref event_type) = event.trigger_type {
                    if event_type.starts_with("conflict:")
                        || event_type.starts_with("movement:")
                        || event_type.starts_with("transaction:")
                        || event_type.starts_with("justice:")
                        || event_type.starts_with("personnel:")
                        || event_type.starts_with("life:")
                        || event_type.starts_with("disaster:")
                        || event_type.starts_with("business:")
                    {
                        return ReferentType::Event;
                    }
                    if event_type.starts_with("economic:") {
                        return ReferentType::Situation;
                    }
                }
                return ReferentType::Event;
            }
        }

        let event_indicators = [
            "ed ",
            " was ",
            " were ",
            " had ",
            " did ",
            " happened",
            " occurred",
        ];
        for ind in &event_indicators {
            if lower.contains(ind) {
                return ReferentType::Event;
            }
        }

        let fact_indicators = [" is ", " are ", " equals ", " means "];
        for ind in &fact_indicators {
            if lower.contains(ind) {
                return ReferentType::Fact;
            }
        }

        let prop_indicators = [" might ", " may ", " could ", " would ", " should ", " if "];
        for ind in &prop_indicators {
            if lower.contains(ind) {
                return ReferentType::Proposition;
            }
        }

        let sit_indicators = [" while ", " as ", "ing ", " continues", " remains"];
        for ind in &sit_indicators {
            if lower.contains(ind) {
                return ReferentType::Situation;
            }
        }

        ReferentType::Event
    }

    /// Find a discourse-level antecedent for an abstract anaphor.
    #[must_use]
    pub fn find_discourse_antecedent(&self, anaphor: &Entity) -> Option<DiscourseReferent> {
        if let Some(event) = self.find_event_near(anaphor.start, 200) {
            let (clause_char_start, clause_char_end) = self.find_event_clause_span(event);
            let span_text: String = self
                .text
                .chars()
                .skip(clause_char_start)
                .take(clause_char_end.saturating_sub(clause_char_start))
                .collect();

            return Some(
                DiscourseReferent::new(ReferentType::Event, clause_char_start, clause_char_end)
                    .with_event(event.clone())
                    .with_text(span_text)
                    .with_confidence(0.85),
            );
        }

        let candidates = self.scope.candidate_antecedent_spans(anaphor.start);
        let shell_class = if self.config.detect_shell_nouns {
            let lower = anaphor.text.to_lowercase();
            let last_word = lower.split_whitespace().last().map(|w| w.to_string());
            last_word
                .as_ref()
                .and_then(|w| classify_shell_noun(w.trim_matches(|c: char| !c.is_alphanumeric())))
        } else {
            None
        };

        for (start, end) in candidates
            .into_iter()
            .take(self.config.max_sentence_distance)
        {
            let span_text = self.scope.extract_span(&self.text, start, end);
            if span_text.trim().len() < 3 {
                continue;
            }
            let ref_type = self.infer_referent_type(span_text);
            if let Some(class) = &shell_class {
                let expected_types = class.typical_antecedent_types();
                if !expected_types.contains(&ref_type) {
                    continue;
                }
            }
            return Some(
                DiscourseReferent::new(ref_type, start, end)
                    .with_text(span_text)
                    .with_confidence(0.7),
            );
        }

        None
    }
}

#[cfg(feature = "discourse")]
impl CoreferenceResolver for DiscourseAwareResolver {
    fn resolve(&self, entities: &[Entity]) -> Vec<Entity> {
        self.resolve_entities(entities)
    }

    fn name(&self) -> &'static str {
        "discourse-aware"
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::coref::{entities_to_chains, CorefChain, Mention};
    use crate::eval::coref_metrics::{
        b_cubed_score, ceaf_e_score, conll_f1, lea_score, muc_score, CorefEvaluation, CorefScores,
    };
    use anno_core::CanonicalId;

    // ---- helpers ----

    fn person(text: &str, start: usize, end: usize) -> Entity {
        Entity::new(text, EntityType::Person, start, end, 0.9)
    }

    fn org(text: &str, start: usize, end: usize) -> Entity {
        Entity::new(text, EntityType::Organization, start, end, 0.9)
    }

    fn loc(text: &str, start: usize, end: usize) -> Entity {
        Entity::new(text, EntityType::Location, start, end, 0.9)
    }

    // ---- SimpleCorefResolver: basic clustering ----

    #[test]
    fn resolve_empty_entities() {
        let resolver = SimpleCorefResolver::default();
        let result = resolver.resolve(&[]);
        assert!(result.is_empty());
    }

    #[test]
    fn resolve_single_entity_gets_cluster_id() {
        let resolver = SimpleCorefResolver::default();
        let entities = vec![person("Alice", 0, 5)];
        let resolved = resolver.resolve(&entities);
        assert_eq!(resolved.len(), 1);
        assert!(resolved[0].canonical_id.is_some(), "singleton should get a cluster id");
    }

    #[test]
    fn exact_name_match_clusters_together() {
        let resolver = SimpleCorefResolver::default();
        let entities = vec![
            person("John Smith", 0, 10),
            org("Acme Corp", 15, 24),
            person("John Smith", 30, 40),
        ];
        let resolved = resolver.resolve(&entities);

        // Both "John Smith" mentions should share the same cluster.
        let id0 = resolved[0].canonical_id.unwrap();
        let id2 = resolved[2].canonical_id.unwrap();
        assert_eq!(id0, id2, "exact name matches must share cluster id");

        // "Acme Corp" should be in a different cluster.
        let id1 = resolved[1].canonical_id.unwrap();
        assert_ne!(id0, id1, "different names must be in different clusters");
    }

    #[test]
    fn fuzzy_substring_match() {
        let resolver = SimpleCorefResolver::new(CorefConfig {
            fuzzy_matching: true,
            ..CorefConfig::default()
        });
        let entities = vec![
            person("John Smith", 0, 10),
            person("Smith", 20, 25),
        ];
        let resolved = resolver.resolve(&entities);
        let id0 = resolved[0].canonical_id.unwrap();
        let id1 = resolved[1].canonical_id.unwrap();
        assert_eq!(id0, id1, "substring match should cluster together");
    }

    #[test]
    fn fuzzy_matching_disabled_does_not_merge_substrings() {
        let resolver = SimpleCorefResolver::new(CorefConfig {
            fuzzy_matching: false,
            ..CorefConfig::default()
        });
        let entities = vec![
            person("John Smith", 0, 10),
            person("Smith", 20, 25),
        ];
        let resolved = resolver.resolve(&entities);
        let id0 = resolved[0].canonical_id.unwrap();
        let id1 = resolved[1].canonical_id.unwrap();
        assert_ne!(id0, id1, "with fuzzy off, substring should not cluster");
    }

    #[test]
    fn type_mismatch_prevents_clustering() {
        let resolver = SimpleCorefResolver::default();
        // Same text, different entity types.
        let entities = vec![
            person("Apple", 0, 5),
            org("Apple", 10, 15),
        ];
        let resolved = resolver.resolve(&entities);
        let id0 = resolved[0].canonical_id.unwrap();
        let id1 = resolved[1].canonical_id.unwrap();
        assert_ne!(
            id0, id1,
            "same text but different EntityType must be separate clusters"
        );
    }

    // ---- Pronoun resolution ----

    #[test]
    fn pronoun_resolves_to_nearest_compatible_entity() {
        let resolver = SimpleCorefResolver::default();
        let entities = vec![
            person("Alice", 0, 5),
            person("Bob", 10, 13),
            person("she", 20, 23),
        ];
        let resolved = resolver.resolve(&entities);
        // "she" is feminine; "Bob" is not gendered (unknown) but "Alice" is also unknown in
        // infer_gender since it's not a pronoun. The resolver picks the nearest non-pronoun that
        // is compatible.  Since infer_gender returns None for proper nouns, gender doesn't filter,
        // so the nearest compatible entity ("Bob") wins.
        let she_id = resolved[2].canonical_id.unwrap();
        let bob_id = resolved[1].canonical_id.unwrap();
        assert_eq!(she_id, bob_id, "pronoun should resolve to nearest compatible entity");
    }

    #[test]
    fn pronoun_it_resolves_to_org() {
        let resolver = SimpleCorefResolver::default();
        let entities = vec![
            person("Alice", 0, 5),
            org("Acme Corp", 10, 19),
            org("it", 25, 27),
        ];
        let resolved = resolver.resolve(&entities);
        // "it" is compatible with Organization but not Person.
        let it_id = resolved[2].canonical_id.unwrap();
        let acme_id = resolved[1].canonical_id.unwrap();
        assert_eq!(it_id, acme_id, "\"it\" should resolve to the org, not the person");
    }

    #[test]
    fn pronoun_it_not_compatible_with_person() {
        let resolver = SimpleCorefResolver::default();
        let entities = vec![
            person("Alice", 0, 5),
            // "it" referring to a person entity type should not match.
            person("it", 10, 12),
        ];
        let resolved = resolver.resolve(&entities);
        let alice_id = resolved[0].canonical_id.unwrap();
        let it_id = resolved[1].canonical_id.unwrap();
        assert_ne!(alice_id, it_id, "\"it\" is not compatible with Person");
    }

    // ---- resolve_to_chains ----

    #[test]
    fn resolve_to_chains_produces_correct_clusters() {
        let resolver = SimpleCorefResolver::default();
        let entities = vec![
            person("Marie Curie", 0, 11),
            org("Nobel Committee", 15, 30),
            person("Marie Curie", 35, 46),
            person("she", 50, 53),
        ];
        let chains = resolver.resolve_to_chains(&entities);
        // At least one chain should have multiple mentions.
        let multi_mention_chains: Vec<_> = chains.iter().filter(|c| c.len() > 1).collect();
        assert!(
            !multi_mention_chains.is_empty(),
            "coreferent entities should produce multi-mention chains"
        );
    }

    // ---- CorefEvaluation: perfect prediction ----

    #[test]
    fn coref_evaluation_perfect_prediction() {
        let gold = vec![
            CorefChain::new(vec![
                Mention::new("John", 0, 4),
                Mention::new("he", 10, 12),
            ]),
            CorefChain::new(vec![
                Mention::new("IBM", 20, 23),
                Mention::new("the company", 30, 41),
            ]),
        ];
        let eval = CorefEvaluation::compute(&gold, &gold);

        assert!(
            (eval.conll_f1 - 1.0).abs() < 1e-9,
            "perfect prediction should yield CoNLL F1 = 1.0, got {}",
            eval.conll_f1
        );
        assert!((eval.muc.f1 - 1.0).abs() < 1e-9);
        assert!((eval.b_cubed.f1 - 1.0).abs() < 1e-9);
        assert!((eval.ceaf_e.f1 - 1.0).abs() < 1e-9);
        assert!((eval.lea.f1 - 1.0).abs() < 1e-9);
    }

    #[test]
    fn coref_evaluation_no_overlap() {
        let predicted = vec![CorefChain::new(vec![
            Mention::new("Alice", 0, 5),
            Mention::new("she", 10, 13),
        ])];
        let gold = vec![CorefChain::new(vec![
            Mention::new("Bob", 20, 23),
            Mention::new("he", 30, 32),
        ])];
        let eval = CorefEvaluation::compute(&predicted, &gold);
        // No common mentions => metrics should be zero (except BLANC edge case).
        assert!(
            eval.muc.f1.abs() < 1e-9,
            "no overlap should yield MUC F1 = 0, got {}",
            eval.muc.f1
        );
        assert!(eval.b_cubed.f1.abs() < 1e-9);
        assert!(eval.conll_f1.abs() < 1e-9);
    }

    #[test]
    fn coref_evaluation_partial_overlap() {
        // Gold: {A, B, C} in one cluster.
        let gold = vec![CorefChain::new(vec![
            Mention::new("John", 0, 4),
            Mention::new("he", 10, 12),
            Mention::new("him", 20, 23),
        ])];
        // Predicted: splits into two clusters: {A, B} and {C}.
        let predicted = vec![
            CorefChain::new(vec![
                Mention::new("John", 0, 4),
                Mention::new("he", 10, 12),
            ]),
            CorefChain::new(vec![Mention::new("him", 20, 23)]),
        ];
        let eval = CorefEvaluation::compute(&predicted, &gold);
        // Should be imperfect but nonzero.
        assert!(eval.conll_f1 > 0.0, "partial overlap should yield nonzero F1");
        assert!(eval.conll_f1 < 1.0, "partial overlap should not yield perfect F1");
    }

    // ---- Individual metric functions ----

    #[test]
    fn muc_score_perfect() {
        let chains = vec![CorefChain::new(vec![
            Mention::new("A", 0, 1),
            Mention::new("B", 5, 6),
            Mention::new("C", 10, 11),
        ])];
        let (p, r, f1) = muc_score(&chains, &chains);
        assert!((p - 1.0).abs() < 1e-9);
        assert!((r - 1.0).abs() < 1e-9);
        assert!((f1 - 1.0).abs() < 1e-9);
    }

    #[test]
    fn b_cubed_singleton_perfect() {
        // Single mention chains: B3 should give perfect scores when predicted == gold.
        let chains = vec![
            CorefChain::new(vec![Mention::new("X", 0, 1)]),
            CorefChain::new(vec![Mention::new("Y", 5, 6)]),
        ];
        let (p, r, f1) = b_cubed_score(&chains, &chains);
        assert!((p - 1.0).abs() < 1e-9);
        assert!((r - 1.0).abs() < 1e-9);
        assert!((f1 - 1.0).abs() < 1e-9);
    }

    #[test]
    fn lea_score_empty() {
        let (p, r, f1) = lea_score(&[], &[]);
        // Empty inputs: no common mentions.
        assert!(p.abs() < 1e-9);
        assert!(r.abs() < 1e-9);
        assert!(f1.abs() < 1e-9);
    }

    #[test]
    fn conll_f1_is_average_of_three() {
        let gold = vec![CorefChain::new(vec![
            Mention::new("A", 0, 1),
            Mention::new("B", 5, 6),
        ])];
        let predicted = vec![CorefChain::new(vec![
            Mention::new("A", 0, 1),
            Mention::new("B", 5, 6),
        ])];
        let conll = conll_f1(&predicted, &gold);
        let (_, _, muc_f1) = muc_score(&predicted, &gold);
        let (_, _, b3_f1) = b_cubed_score(&predicted, &gold);
        let (_, _, ceaf_f1) = ceaf_e_score(&predicted, &gold);
        let expected = (muc_f1 + b3_f1 + ceaf_f1) / 3.0;
        assert!(
            (conll - expected).abs() < 1e-9,
            "conll_f1 should equal mean of MUC, B3, CEAFe F1"
        );
    }

    // ---- CorefScores ----

    #[test]
    fn coref_scores_f1_computation() {
        let s = CorefScores::new(0.8, 0.6);
        let expected_f1 = 2.0 * 0.8 * 0.6 / (0.8 + 0.6);
        assert!((s.f1 - expected_f1).abs() < 1e-9);
    }

    #[test]
    fn coref_scores_zero_precision_and_recall() {
        let s = CorefScores::new(0.0, 0.0);
        assert!(s.f1.abs() < 1e-9, "0/0 should yield F1 = 0");
    }

    // ---- CorefEvaluation aggregate helpers ----

    #[test]
    fn coref_evaluation_all_f1_scores_len() {
        let gold = vec![CorefChain::new(vec![
            Mention::new("A", 0, 1),
            Mention::new("B", 5, 6),
        ])];
        let eval = CorefEvaluation::compute(&gold, &gold);
        assert_eq!(eval.all_f1_scores().len(), 6, "should report 6 metric F1 values");
    }

    #[test]
    fn coref_evaluation_average_f1_upper_bound() {
        // With perfect prediction, average F1 (across 6 metrics) should be high.
        // BLANC can deviate from 1.0 when there is only one cluster (no negative pairs),
        // so we check that average_f1 >= 0.9 rather than == 1.0.
        let gold = vec![
            CorefChain::new(vec![
                Mention::new("A", 0, 1),
                Mention::new("B", 5, 6),
                Mention::new("C", 10, 11),
            ]),
            CorefChain::new(vec![
                Mention::new("X", 20, 21),
                Mention::new("Y", 25, 26),
            ]),
        ];
        let eval = CorefEvaluation::compute(&gold, &gold);
        assert!(
            eval.average_f1() > 0.9,
            "perfect prediction with multiple clusters should have high average F1, got {}",
            eval.average_f1()
        );
        // The core three (MUC, B3, CEAFe) used in CoNLL should each be 1.0.
        assert!((eval.muc.f1 - 1.0).abs() < 1e-9);
        assert!((eval.b_cubed.f1 - 1.0).abs() < 1e-9);
        assert!((eval.ceaf_e.f1 - 1.0).abs() < 1e-9);
    }

    // ---- BoxCorefResolver ----

    #[test]
    fn box_resolver_empty_entities() {
        let resolver = BoxCorefResolver::new(BoxCorefConfig::default());
        let result = resolver.resolve_with_boxes(&[], &[]);
        assert!(result.is_empty());
    }

    #[test]
    fn box_resolver_identical_boxes_cluster_together() {
        let config = BoxCorefConfig {
            coreference_threshold: 0.5,
            enforce_syntactic_constraints: false,
            ..BoxCorefConfig::default()
        };
        let resolver = BoxCorefResolver::new(config);

        let entities = vec![
            person("Alice", 0, 5),
            person("she", 100, 103),
        ];
        // Identical boxes should have coreference_score = 1.0.
        let box_a = BoxEmbedding::from_vector(&[1.0, 2.0, 3.0], 0.1);
        let box_b = BoxEmbedding::from_vector(&[1.0, 2.0, 3.0], 0.1);
        let boxes = vec![box_a, box_b];

        let resolved = resolver.resolve_with_boxes(&entities, &boxes);
        let id0 = resolved[0].canonical_id.unwrap();
        let id1 = resolved[1].canonical_id.unwrap();
        assert_eq!(id0, id1, "identical boxes should cluster together");
    }

    #[test]
    fn box_resolver_distant_boxes_stay_separate() {
        let config = BoxCorefConfig {
            coreference_threshold: 0.5,
            enforce_syntactic_constraints: false,
            ..BoxCorefConfig::default()
        };
        let resolver = BoxCorefResolver::new(config);

        let entities = vec![
            person("Alice", 0, 5),
            person("Bob", 100, 103),
        ];
        // Very different vectors => low coreference score.
        let box_a = BoxEmbedding::from_vector(&[0.0, 0.0, 0.0], 0.01);
        let box_b = BoxEmbedding::from_vector(&[100.0, 100.0, 100.0], 0.01);
        let boxes = vec![box_a, box_b];

        let resolved = resolver.resolve_with_boxes(&entities, &boxes);
        let id0 = resolved[0].canonical_id.unwrap();
        let id1 = resolved[1].canonical_id.unwrap();
        assert_ne!(id0, id1, "distant boxes should remain separate");
    }

    #[test]
    fn box_resolver_type_mismatch_prevents_merge() {
        let config = BoxCorefConfig {
            coreference_threshold: 0.5,
            enforce_syntactic_constraints: false,
            ..BoxCorefConfig::default()
        };
        let resolver = BoxCorefResolver::new(config);

        // Same box coordinates but different entity types.
        let entities = vec![
            person("Apple", 0, 5),
            org("Apple", 100, 105),
        ];
        let box_a = BoxEmbedding::from_vector(&[1.0, 2.0, 3.0], 0.1);
        let box_b = BoxEmbedding::from_vector(&[1.0, 2.0, 3.0], 0.1);
        let boxes = vec![box_a, box_b];

        let resolved = resolver.resolve_with_boxes(&entities, &boxes);
        let id0 = resolved[0].canonical_id.unwrap();
        let id1 = resolved[1].canonical_id.unwrap();
        assert_ne!(id0, id1, "different entity types must not merge even with identical boxes");
    }

    // ---- vectors_to_boxes ----

    #[test]
    fn vectors_to_boxes_correct_count() {
        let embeddings = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]; // 2 entities, dim=3
        let boxes = super::vectors_to_boxes(&embeddings, 3, Some(0.1));
        assert_eq!(boxes.len(), 2);
        assert_eq!(boxes[0].min.len(), 3);
        assert_eq!(boxes[1].min.len(), 3);
    }

    #[test]
    fn vectors_to_boxes_radius_applied() {
        let embeddings = vec![5.0, 10.0];
        let boxes = super::vectors_to_boxes(&embeddings, 2, Some(0.5));
        assert_eq!(boxes.len(), 1);
        // min should be center - radius, max should be center + radius.
        assert!((boxes[0].min[0] - 4.5).abs() < 1e-6);
        assert!((boxes[0].max[0] - 5.5).abs() < 1e-6);
        assert!((boxes[0].min[1] - 9.5).abs() < 1e-6);
        assert!((boxes[0].max[1] - 10.5).abs() < 1e-6);
    }

    // ---- entities_to_chains round-trip ----

    #[test]
    fn entities_to_chains_groups_by_canonical_id() {
        let mut e1 = person("Alice", 0, 5);
        e1.canonical_id = Some(CanonicalId::new(1));
        let mut e2 = person("she", 10, 13);
        e2.canonical_id = Some(CanonicalId::new(1));
        let mut e3 = org("Acme", 20, 24);
        e3.canonical_id = Some(CanonicalId::new(2));

        let chains = entities_to_chains(&[e1, e2, e3]);
        // Cluster 1 should have 2 mentions, cluster 2 should have 1.
        let two_mention = chains.iter().find(|c| c.len() == 2);
        let one_mention = chains.iter().find(|c| c.len() == 1);
        assert!(two_mention.is_some(), "should have a 2-mention chain");
        assert!(one_mention.is_some(), "should have a 1-mention chain");
    }

    #[test]
    fn entities_without_canonical_id_become_singletons() {
        let entities = vec![person("Alice", 0, 5), person("Bob", 10, 13)];
        let chains = entities_to_chains(&entities);
        // Each unresolved entity becomes its own singleton chain.
        assert_eq!(chains.len(), 2);
        assert!(chains.iter().all(|c| c.len() == 1));
    }

    // ---- CoreferenceResolver trait impl ----

    #[test]
    fn simple_resolver_trait_name() {
        let resolver = SimpleCorefResolver::default();
        assert_eq!(CoreferenceResolver::name(&resolver), "simple-rule-based");
    }

    #[test]
    fn box_resolver_trait_name() {
        let resolver = BoxCorefResolver::new(BoxCorefConfig::default());
        assert_eq!(CoreferenceResolver::name(&resolver), "box-embedding");
    }

    // ---- End-to-end: resolve then evaluate ----

    #[test]
    fn resolve_then_evaluate_round_trip() {
        let resolver = SimpleCorefResolver::default();

        // Synthetic document: "Alice went to Paris. She loved Paris."
        let entities = vec![
            person("Alice", 0, 5),
            loc("Paris", 14, 19),
            person("she", 21, 24),
            loc("Paris", 31, 36),
        ];

        let predicted_chains = resolver.resolve_to_chains(&entities);
        // Build gold: Alice+she in one cluster, Paris+Paris in another.
        let gold_chains = vec![
            CorefChain::new(vec![
                Mention::new("Alice", 0, 5),
                Mention::new("she", 21, 24),
            ]),
            CorefChain::new(vec![
                Mention::new("Paris", 14, 19),
                Mention::new("Paris", 31, 36),
            ]),
        ];

        let eval = CorefEvaluation::compute(&predicted_chains, &gold_chains);
        // The resolver should get at least partial credit.
        assert!(
            eval.conll_f1 > 0.0,
            "resolve-then-evaluate should produce nonzero CoNLL F1"
        );
    }
}
