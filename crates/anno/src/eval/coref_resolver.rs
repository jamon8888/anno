//! Simple coreference resolution for analysis/evaluation pipelines.
//!
//! Provides minimal resolvers to produce coreference chains from entities, completing the loop
//! between extraction and coreference metric computation.

use super::coref::CorefChain;
use crate::backends::box_embeddings::{BoxCorefConfig, BoxEmbedding};
use crate::{Entity, EntityType};
use anno_core::{CanonicalId, Gender};
use std::collections::HashMap;

/// Configuration for simple coreference resolver.
#[derive(Debug, Clone)]
pub struct CorefConfig {
    /// Similarity threshold for name matching (0.0-1.0).
    #[deprecated(note = "Not currently used by any matching logic. Will be wired into names_match() in a future version.")]
    pub similarity_threshold: f64,
    /// Maximum number of preceding entities to search when resolving a pronoun.
    ///
    /// The actual lookback window is `max_pronoun_lookback * 10` entities
    /// (a rough approximation of sentence distance by entity count).
    pub max_pronoun_lookback: usize,
    /// Enable fuzzy name matching (e.g., "John Smith" ~ "J. Smith").
    pub fuzzy_matching: bool,
    /// Include singletons in output chains.
    pub include_singletons: bool,
    /// Use a built-in name-to-gender gazetteer for pronoun resolution.
    ///
    /// When enabled, common English names (e.g., "Alice" -> Feminine,
    /// "Bob" -> Masculine) are used to infer gender for proper nouns,
    /// improving pronoun resolution accuracy. Disable for non-English
    /// text or when names don't follow English gender conventions.
    pub use_name_gazetteer: bool,
    /// Enable acronym matching (e.g., "IBM" matches "International Business Machines").
    ///
    /// Checks if one mention is an uppercase acronym whose letters match the
    /// initial letters of the other mention's words. Requires same entity type.
    pub acronym_matching: bool,
    /// Enable relaxed head match (e.g., "President Obama" ~ "Barack Obama" via shared head "Obama").
    ///
    /// Two mentions corefer if their head words (last word) match and they share the same
    /// entity type. Only applies when both mentions have 2+ words.
    pub relaxed_head_match: bool,
    /// Enable proper noun containment (e.g., "Obama" is contained in "Barack Obama").
    ///
    /// One mention's original text must be a proper subsequence of the other, matching
    /// at word boundaries, and both must share the same entity type. Unlike fuzzy/substring
    /// matching, this operates on original text (not canonical lowercased form) and requires
    /// complete word boundary alignment.
    pub proper_containment: bool,
}

#[allow(deprecated)]
impl Default for CorefConfig {
    fn default() -> Self {
        Self {
            similarity_threshold: 0.7,
            max_pronoun_lookback: 3,
            fuzzy_matching: true,
            include_singletons: true,
            use_name_gazetteer: true,
            acronym_matching: true,
            relaxed_head_match: true,
            proper_containment: true,
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

        // Strategy 3: acronym matching (e.g., "IBM" ~ "International Business Machines").
        if self.config.acronym_matching {
            for (other_canonical, &cluster_id) in canonical_map {
                if self.is_acronym_match(&canonical, other_canonical) {
                    return Some(cluster_id);
                }
            }
        }

        // Strategy 4: relaxed head match (e.g., "President Obama" ~ "Barack Obama").
        if self.config.relaxed_head_match {
            for prev in previous.iter().rev() {
                if let Some(cluster_id) = prev.canonical_id {
                    if self.is_relaxed_head_match(entity, prev) {
                        return Some(cluster_id);
                    }
                }
            }
        }

        // Strategy 5: proper noun containment (e.g., "Obama" in "Barack Obama").
        if self.config.proper_containment {
            for prev in previous.iter().rev() {
                if let Some(cluster_id) = prev.canonical_id {
                    if self.is_proper_containment(entity, prev) {
                        return Some(cluster_id);
                    }
                }
            }
        }

        // Strategy 6: substring/fuzzy matching.
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
            .take(self.config.max_pronoun_lookback * 10)
        {
            if self.is_pronoun(&entity.text) {
                continue;
            }
            if !self.pronoun_compatible(&pronoun.text, &entity.entity_type) {
                continue;
            }

            let entity_gender = self.infer_gender(&entity.text);
            if let (Some(pg), Some(eg)) = (pronoun_gender, entity_gender) {
                if !pg.is_compatible(&eg) {
                    continue;
                }
            }

            return entity.canonical_id;
        }

        None
    }

    /// # Multilingual Note
    ///
    /// English-only. This resolver is used in the eval pipeline where English
    /// datasets (OntoNotes, PreCo, LitBank) dominate. For multilingual eval,
    /// use model-based mention detection or extend with language-specific lists.
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
            "fae" | "faer" | "faers" | "faeself" | "faerself"
        )
    }

    /// # Multilingual Note
    ///
    /// English-only. Pronoun-entity type compatibility rules are language-specific
    /// (e.g., French gendered articles, German grammatical gender on nouns).
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
                    // it/its as personal pronouns (used by some genderqueer individuals)
                    | "it"
                    | "its"
                    | "itself"
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

    /// # Multilingual Note
    ///
    /// English-only. Delegates to `Gender::from_pronoun()` which covers English
    /// pronouns and neopronouns. Falls back to a name gazetteer (if enabled)
    /// for common English proper nouns. Other languages need language-specific
    /// gender inference (grammatical gender, morphological agreement, etc.).
    fn infer_gender(&self, text: &str) -> Option<Gender> {
        if let Some(g) = Gender::from_pronoun(text) {
            return Some(g);
        }
        if self.config.use_name_gazetteer {
            return gender_from_name(text);
        }
        None
    }

    fn canonical_form(&self, text: &str, entity_type: &EntityType) -> String {
        let normalized = text.to_lowercase().trim().to_string();
        format!("{}:{}", entity_type.as_label(), normalized)
    }

    /// Check if one mention is an acronym of the other.
    ///
    /// "IBM" matches "International Business Machines" because the first letter of
    /// each word in the full name matches the acronym characters. Both mentions must
    /// share the same entity type (enforced by the canonical form prefix).
    fn is_acronym_match(&self, name1: &str, name2: &str) -> bool {
        let (type1, text1) = name1.split_once(':').unwrap_or(("", name1));
        let (type2, text2) = name2.split_once(':').unwrap_or(("", name2));
        if type1 != type2 {
            return false;
        }

        // Canonical form lowercases text, so we can't check for uppercase.
        // Instead: one side must be a single "word" (no spaces) with length > 1,
        // the other must be multi-word with word count == single-word char count.
        let words1: Vec<&str> = text1.split_whitespace().collect();
        let words2: Vec<&str> = text2.split_whitespace().collect();

        let (acronym, words) = if words1.len() == 1 && words2.len() > 1 {
            (text1, &words2)
        } else if words2.len() == 1 && words1.len() > 1 {
            (text2, &words1)
        } else {
            return false;
        };

        let acronym_chars: Vec<char> = acronym.chars().collect();
        // Acronym must be at least 2 chars and match word count.
        if acronym_chars.len() < 2 || acronym_chars.len() != words.len() {
            return false;
        }

        // Each acronym letter must match the first letter of the corresponding word.
        acronym_chars
            .iter()
            .zip(words.iter())
            .all(|(&ac, word)| word.starts_with(ac))
    }

    /// Check if two mentions share a head word (last word) and entity type.
    ///
    /// Only applies when both mentions have 2+ words, so single-word mentions
    /// are not spuriously matched (those are handled by exact/fuzzy sieves).
    fn is_relaxed_head_match(&self, a: &Entity, b: &Entity) -> bool {
        if a.entity_type != b.entity_type {
            return false;
        }
        let words_a: Vec<&str> = a.text.split_whitespace().collect();
        let words_b: Vec<&str> = b.text.split_whitespace().collect();
        if words_a.len() < 2 || words_b.len() < 2 {
            return false;
        }
        // Head word = last word, compared case-insensitively.
        words_a
            .last()
            .unwrap()
            .eq_ignore_ascii_case(words_b.last().unwrap())
    }

    /// Check if one mention's original text is properly contained in the other
    /// at word boundaries, with matching entity type.
    ///
    /// "Obama" is properly contained in "Barack Obama" because "Obama" appears
    /// as a complete word. Unlike `names_match`, this uses the original text
    /// (not canonical lowercased form) and requires word-boundary alignment.
    fn is_proper_containment(&self, a: &Entity, b: &Entity) -> bool {
        if a.entity_type != b.entity_type {
            return false;
        }
        let text_a = a.text.trim();
        let text_b = b.text.trim();
        if text_a.is_empty() || text_b.is_empty() || text_a.eq_ignore_ascii_case(text_b) {
            return false; // must be a *proper* subsequence, not equal
        }

        let (shorter, longer) = if text_a.len() < text_b.len() {
            (text_a, text_b)
        } else if text_b.len() < text_a.len() {
            (text_b, text_a)
        } else {
            return false; // same length but different text => not contained
        };

        // Check if shorter appears as a complete word sequence in longer.
        let longer_lower = longer.to_lowercase();
        let shorter_lower = shorter.to_lowercase();
        let longer_words: Vec<&str> = longer_lower.split_whitespace().collect();
        let shorter_words: Vec<&str> = shorter_lower.split_whitespace().collect();

        if shorter_words.is_empty() || shorter_words.len() >= longer_words.len() {
            return false;
        }

        // Check if shorter_words appears as a contiguous subsequence of longer_words.
        longer_words
            .windows(shorter_words.len())
            .any(|window| window == shorter_words.as_slice())
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

/// Infer gender from a proper noun using a small English name gazetteer.
///
/// Returns `None` for unrecognized names. Case-insensitive on the first word
/// of the input (handles "Alice Smith" by checking "alice").
fn gender_from_name(text: &str) -> Option<Gender> {
    let first_word = text.split_whitespace().next()?;
    let lower = first_word.to_lowercase();
    // ~100 common English given names, covering the most frequent names
    // in US/UK census data. This is intentionally small and conservative.
    match lower.as_str() {
        // Masculine
        "james" | "john" | "robert" | "michael" | "david" | "william" | "richard"
        | "joseph" | "thomas" | "charles" | "christopher" | "daniel" | "matthew"
        | "anthony" | "mark" | "donald" | "steven" | "paul" | "andrew" | "joshua"
        | "kenneth" | "kevin" | "brian" | "george" | "timothy" | "ronald" | "edward"
        | "jason" | "jeffrey" | "ryan" | "jacob" | "gary" | "nicholas" | "eric"
        | "jonathan" | "stephen" | "larry" | "justin" | "scott" | "brandon"
        | "benjamin" | "samuel" | "raymond" | "gregory" | "frank" | "alexander"
        | "patrick" | "jack" | "dennis" | "peter" | "bob" | "jim" | "tom" | "mike"
        | "bill" | "joe" | "dan" | "matt" | "steve" | "chris" | "nick" | "ben"
        | "sam" | "jake" | "adam" | "henry" | "nathan" | "philip" | "carl"
        | "ahmed" | "ahmad" | "mohammed" | "muhammad" | "omar" | "ali" | "hassan"
        | "hussein" | "khalid" | "ibrahim" => Some(Gender::Masculine),
        // Feminine
        "mary" | "patricia" | "jennifer" | "linda" | "barbara" | "elizabeth"
        | "susan" | "jessica" | "sarah" | "karen" | "lisa" | "nancy" | "betty"
        | "margaret" | "sandra" | "ashley" | "dorothy" | "kimberly" | "emily"
        | "donna" | "michelle" | "carol" | "amanda" | "melissa" | "deborah"
        | "stephanie" | "rebecca" | "sharon" | "laura" | "cynthia" | "kathleen"
        | "amy" | "angela" | "shirley" | "anna" | "brenda" | "pamela" | "emma"
        | "nicole" | "helen" | "samantha" | "katherine" | "christine" | "debra"
        | "rachel" | "carolyn" | "janet" | "catherine" | "maria" | "heather"
        | "diane" | "ruth" | "julie" | "olivia" | "joyce" | "virginia" | "victoria"
        | "kelly" | "lauren" | "christina" | "joan" | "evelyn" | "judith"
        | "alice" | "ann" | "anne" | "jane" | "jean" | "marie" | "rose" | "grace"
        | "fatima" | "aisha" | "maryam" | "nour" | "layla" | "hana" => Some(Gender::Feminine),
        _ => None,
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
            proper_containment: false,
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
    fn pronoun_resolves_to_gender_compatible_entity() {
        let resolver = SimpleCorefResolver::default();
        let entities = vec![
            person("Alice", 0, 5),
            person("Bob", 10, 13),
            person("she", 20, 23),
        ];
        let resolved = resolver.resolve(&entities);
        // With the name gazetteer, "Alice" -> Feminine, "Bob" -> Masculine.
        // "she" is feminine, so it should match "Alice" (gender compatible),
        // skipping "Bob" (masculine, incompatible).
        let she_id = resolved[2].canonical_id.unwrap();
        let alice_id = resolved[0].canonical_id.unwrap();
        assert_eq!(she_id, alice_id, "pronoun should resolve to gender-compatible entity");
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
    fn pronoun_it_compatible_with_person() {
        // "it/its" can be used as personal pronouns (e.g., some genderqueer individuals).
        let resolver = SimpleCorefResolver::default();
        let entities = vec![
            person("Alice", 0, 5),
            person("it", 10, 12),
        ];
        let resolved = resolver.resolve(&entities);
        let alice_id = resolved[0].canonical_id.unwrap();
        let it_id = resolved[1].canonical_id.unwrap();
        assert_eq!(alice_id, it_id, "\"it\" should resolve to Person antecedent");
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

    #[test]
    fn it_pronoun_resolves_to_person() {
        // "it/its" should be compatible with Person entities (used as personal pronouns
        // by some genderqueer individuals, e.g., "Alex uses it/its pronouns").
        let resolver = SimpleCorefResolver::default();
        let entities = vec![
            person("Alex", 0, 4),
            person("it", 6, 8),
        ];
        let chains = resolver.resolve_to_chains(&entities);
        // Alex and "it" should be in the same chain.
        let alex_chain = chains
            .iter()
            .find(|c| c.mentions.iter().any(|m| m.text == "Alex"));
        assert!(
            alex_chain.is_some(),
            "Alex should appear in a chain"
        );
        let alex_chain = alex_chain.unwrap();
        assert!(
            alex_chain.mentions.iter().any(|m| m.text == "it"),
            "\"it\" should corefer with Person entity \"Alex\""
        );
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

    // ====================================================================
    // Audit-driven regression tests
    // ====================================================================

    // ---- Gender-blind pronoun resolution (documenting known limitation) ----

    #[test]
    fn pronoun_resolves_with_gazetteer() {
        // With name gazetteer enabled (default), "She" should resolve to "Alice"
        // (Feminine) instead of "Bob" (Masculine), even though Bob is nearer.
        let resolver = SimpleCorefResolver::default();
        let entities = vec![
            person("Alice", 0, 5),
            person("Bob", 20, 23),
            person("She", 30, 33),
        ];
        let resolved = resolver.resolve(&entities);

        let she_id = resolved[2].canonical_id.unwrap();
        let alice_id = resolved[0].canonical_id.unwrap();
        assert_eq!(
            she_id, alice_id,
            "With gazetteer, 'She' should resolve to 'Alice' (gender match)"
        );
    }

    #[test]
    fn pronoun_gender_blind_without_gazetteer() {
        // Without gazetteer, falls back to nearest compatible entity.
        let resolver = SimpleCorefResolver::new(CorefConfig {
            use_name_gazetteer: false,
            ..CorefConfig::default()
        });
        let entities = vec![
            person("Alice", 0, 5),
            person("Bob", 20, 23),
            person("She", 30, 33),
        ];
        let resolved = resolver.resolve(&entities);

        let she_id = resolved[2].canonical_id.unwrap();
        let bob_id = resolved[1].canonical_id.unwrap();
        // "She" resolves to "Bob" (nearest) because infer_gender("Bob") returns None,
        // which is compatible with Feminine via the (None, Some) match arm.
        assert_eq!(
            she_id, bob_id,
            "Without gazetteer, 'She' resolves to nearest entity 'Bob' (gender-blind)"
        );
    }

    // ---- Dead config documentation ----

    #[test]
    #[allow(deprecated)]
    fn similarity_threshold_has_no_effect() {
        // similarity_threshold is currently dead code -- it is stored in CorefConfig
        // but never read by any matching logic in SimpleCorefResolver.
        let resolver_low = SimpleCorefResolver::new(CorefConfig {
            similarity_threshold: 0.1,
            ..CorefConfig::default()
        });
        let resolver_high = SimpleCorefResolver::new(CorefConfig {
            similarity_threshold: 0.99,
            ..CorefConfig::default()
        });

        let entities = vec![
            person("John Smith", 0, 10),
            person("Smith", 20, 25),
            person("he", 30, 32),
        ];

        let resolved_low = resolver_low.resolve(&entities);
        let resolved_high = resolver_high.resolve(&entities);

        for (a, b) in resolved_low.iter().zip(resolved_high.iter()) {
            assert_eq!(
                a.canonical_id, b.canonical_id,
                "similarity_threshold is dead code: changing it must not alter output"
            );
        }
    }

    // ---- String matching edge cases ----

    #[test]
    fn names_match_substring_short_name() {
        // "Li" should NOT match "political" via substring containment, but currently
        // names_match uses `contains()` which is too broad for short names.
        // This test documents the current (incorrect) behavior.
        let resolver = SimpleCorefResolver::new(CorefConfig {
            fuzzy_matching: true,
            ..CorefConfig::default()
        });
        let entities = vec![
            person("political", 0, 9),
            person("Li", 20, 22),
        ];
        let resolved = resolver.resolve(&entities);
        let id0 = resolved[0].canonical_id.unwrap();
        let id1 = resolved[1].canonical_id.unwrap();
        // Known bug: "li" is a substring of "political", so they erroneously cluster.
        assert_eq!(
            id0, id1,
            "Known bug: 'Li' matches 'political' via substring containment (too broad for short names)"
        );
    }

    #[test]
    fn names_match_last_word() {
        // "John Smith" should match "Smith" via last-word heuristic.
        let resolver = SimpleCorefResolver::new(CorefConfig {
            fuzzy_matching: true,
            ..CorefConfig::default()
        });
        let entities = vec![
            person("John Smith", 0, 10),
            person("Smith", 20, 25),
        ];
        let resolved = resolver.resolve(&entities);
        assert_eq!(
            resolved[0].canonical_id.unwrap(),
            resolved[1].canonical_id.unwrap(),
            "'John Smith' and 'Smith' should cluster via last-word match"
        );
    }

    #[test]
    fn names_match_acronym() {
        // Acronym matching: "IBM" matches "International Business Machines".
        let resolver = SimpleCorefResolver::new(CorefConfig {
            acronym_matching: true,
            ..CorefConfig::default()
        });
        let entities = vec![
            org("International Business Machines", 0, 31),
            org("IBM", 40, 43),
        ];
        let resolved = resolver.resolve(&entities);
        assert_eq!(
            resolved[0].canonical_id.unwrap(),
            resolved[1].canonical_id.unwrap(),
            "'IBM' should cluster with 'International Business Machines' via acronym matching"
        );
    }

    #[test]
    fn acronym_matching_disabled() {
        // When acronym_matching is disabled, "IBM" does NOT match.
        let resolver = SimpleCorefResolver::new(CorefConfig {
            acronym_matching: false,
            fuzzy_matching: false,
            ..CorefConfig::default()
        });
        let entities = vec![
            org("International Business Machines", 0, 31),
            org("IBM", 40, 43),
        ];
        let resolved = resolver.resolve(&entities);
        assert_ne!(
            resolved[0].canonical_id.unwrap(),
            resolved[1].canonical_id.unwrap(),
            "Acronym matching disabled: 'IBM' should not cluster"
        );
    }

    #[test]
    fn acronym_type_mismatch_rejected() {
        // Acronym must share entity type.
        let resolver = SimpleCorefResolver::default();
        let entities = vec![
            org("International Business Machines", 0, 31),
            person("IBM", 40, 43),
        ];
        let resolved = resolver.resolve(&entities);
        assert_ne!(
            resolved[0].canonical_id.unwrap(),
            resolved[1].canonical_id.unwrap(),
            "Acronym across different entity types should not cluster"
        );
    }

    #[test]
    fn acronym_length_mismatch_rejected() {
        // "UN" (2 letters) does not match "United Nations Organization" (3 words)
        // via acronym matching. Fuzzy matching disabled to isolate the acronym check.
        let config = CorefConfig {
            fuzzy_matching: false,
            ..Default::default()
        };
        let resolver = SimpleCorefResolver::new(config);
        let entities = vec![
            org("United Nations Organization", 0, 26),
            org("UN", 30, 32),
        ];
        let resolved = resolver.resolve(&entities);
        assert_ne!(
            resolved[0].canonical_id.unwrap(),
            resolved[1].canonical_id.unwrap(),
            "Acronym letter count must match word count"
        );
    }

    #[test]
    fn acronym_un_matches() {
        // "UN" (2 letters) matches "United Nations" (2 words).
        let resolver = SimpleCorefResolver::default();
        let entities = vec![
            org("United Nations", 0, 14),
            org("UN", 20, 22),
        ];
        let resolved = resolver.resolve(&entities);
        assert_eq!(
            resolved[0].canonical_id.unwrap(),
            resolved[1].canonical_id.unwrap(),
            "'UN' should cluster with 'United Nations'"
        );
    }

    // ---- Relaxed head match ----

    #[test]
    fn relaxed_head_match_shared_last_word() {
        // "President Obama" and "Barack Obama" share head word "Obama".
        let resolver = SimpleCorefResolver::new(CorefConfig {
            fuzzy_matching: false,
            proper_containment: false,
            ..CorefConfig::default()
        });
        let entities = vec![
            person("President Obama", 0, 15),
            person("Barack Obama", 20, 32),
        ];
        let resolved = resolver.resolve(&entities);
        assert_eq!(
            resolved[0].canonical_id.unwrap(),
            resolved[1].canonical_id.unwrap(),
            "'President Obama' and 'Barack Obama' should cluster via shared head 'Obama'"
        );
    }

    #[test]
    fn relaxed_head_match_requires_two_words() {
        // Single-word mentions should NOT match via relaxed head match.
        let resolver = SimpleCorefResolver::new(CorefConfig {
            fuzzy_matching: false,
            proper_containment: false,
            ..CorefConfig::default()
        });
        let entities = vec![
            person("Obama", 0, 5),
            person("Barack Obama", 20, 32),
        ];
        let resolved = resolver.resolve(&entities);
        assert_ne!(
            resolved[0].canonical_id.unwrap(),
            resolved[1].canonical_id.unwrap(),
            "Relaxed head match requires both mentions to have 2+ words"
        );
    }

    #[test]
    fn relaxed_head_match_type_mismatch() {
        // Different entity types should not match even with same head word.
        let resolver = SimpleCorefResolver::new(CorefConfig {
            fuzzy_matching: false,
            proper_containment: false,
            ..CorefConfig::default()
        });
        let entities = vec![
            person("President Obama", 0, 15),
            org("Foundation Obama", 20, 36),
        ];
        let resolved = resolver.resolve(&entities);
        assert_ne!(
            resolved[0].canonical_id.unwrap(),
            resolved[1].canonical_id.unwrap(),
            "Relaxed head match requires same entity type"
        );
    }

    #[test]
    fn relaxed_head_match_case_insensitive() {
        // Head word comparison should be case-insensitive.
        let resolver = SimpleCorefResolver::new(CorefConfig {
            fuzzy_matching: false,
            proper_containment: false,
            ..CorefConfig::default()
        });
        let entities = vec![
            person("President OBAMA", 0, 15),
            person("Barack obama", 20, 32),
        ];
        let resolved = resolver.resolve(&entities);
        assert_eq!(
            resolved[0].canonical_id.unwrap(),
            resolved[1].canonical_id.unwrap(),
            "Relaxed head match should be case-insensitive"
        );
    }

    #[test]
    fn relaxed_head_match_disabled() {
        let resolver = SimpleCorefResolver::new(CorefConfig {
            relaxed_head_match: false,
            fuzzy_matching: false,
            proper_containment: false,
            ..CorefConfig::default()
        });
        let entities = vec![
            person("President Obama", 0, 15),
            person("Barack Obama", 20, 32),
        ];
        let resolved = resolver.resolve(&entities);
        assert_ne!(
            resolved[0].canonical_id.unwrap(),
            resolved[1].canonical_id.unwrap(),
            "With relaxed_head_match disabled, should not cluster"
        );
    }

    // ---- Proper noun containment ----

    #[test]
    fn proper_containment_word_boundary() {
        // "Obama" is a complete word in "Barack Obama".
        let resolver = SimpleCorefResolver::new(CorefConfig {
            fuzzy_matching: false,
            relaxed_head_match: false,
            ..CorefConfig::default()
        });
        let entities = vec![
            person("Barack Obama", 0, 12),
            person("Obama", 20, 25),
        ];
        let resolved = resolver.resolve(&entities);
        assert_eq!(
            resolved[0].canonical_id.unwrap(),
            resolved[1].canonical_id.unwrap(),
            "'Obama' should cluster with 'Barack Obama' via proper containment"
        );
    }

    #[test]
    fn proper_containment_not_partial_word() {
        // "Nation" is NOT a complete word in "United Nations" (it would be "Nations").
        let resolver = SimpleCorefResolver::new(CorefConfig {
            fuzzy_matching: false,
            relaxed_head_match: false,
            ..CorefConfig::default()
        });
        let entities = vec![
            org("United Nations", 0, 14),
            org("Nation", 20, 26),
        ];
        let resolved = resolver.resolve(&entities);
        assert_ne!(
            resolved[0].canonical_id.unwrap(),
            resolved[1].canonical_id.unwrap(),
            "'Nation' is not a word-boundary match in 'United Nations'"
        );
    }

    #[test]
    fn proper_containment_type_mismatch() {
        let resolver = SimpleCorefResolver::new(CorefConfig {
            fuzzy_matching: false,
            relaxed_head_match: false,
            ..CorefConfig::default()
        });
        let entities = vec![
            person("Barack Obama", 0, 12),
            org("Obama", 20, 25),
        ];
        let resolved = resolver.resolve(&entities);
        assert_ne!(
            resolved[0].canonical_id.unwrap(),
            resolved[1].canonical_id.unwrap(),
            "Proper containment requires same entity type"
        );
    }

    #[test]
    fn proper_containment_disabled() {
        let resolver = SimpleCorefResolver::new(CorefConfig {
            proper_containment: false,
            fuzzy_matching: false,
            relaxed_head_match: false,
            ..CorefConfig::default()
        });
        let entities = vec![
            person("Barack Obama", 0, 12),
            person("Obama", 20, 25),
        ];
        let resolved = resolver.resolve(&entities);
        assert_ne!(
            resolved[0].canonical_id.unwrap(),
            resolved[1].canonical_id.unwrap(),
            "With proper_containment disabled, should not cluster"
        );
    }

    #[test]
    fn proper_containment_multi_word_subsequence() {
        // "New York" is contained as a word sequence in "New York City".
        let resolver = SimpleCorefResolver::new(CorefConfig {
            fuzzy_matching: false,
            relaxed_head_match: false,
            ..CorefConfig::default()
        });
        let entities = vec![
            loc("New York City", 0, 13),
            loc("New York", 20, 28),
        ];
        let resolved = resolver.resolve(&entities);
        assert_eq!(
            resolved[0].canonical_id.unwrap(),
            resolved[1].canonical_id.unwrap(),
            "'New York' should cluster with 'New York City' via proper containment"
        );
    }

    #[test]
    fn names_match_case_insensitive() {
        let resolver = SimpleCorefResolver::new(CorefConfig {
            fuzzy_matching: true,
            ..CorefConfig::default()
        });
        let entities = vec![
            person("alice", 0, 5),
            person("Alice", 20, 25),
        ];
        let resolved = resolver.resolve(&entities);
        assert_eq!(
            resolved[0].canonical_id.unwrap(),
            resolved[1].canonical_id.unwrap(),
            "'alice' and 'Alice' should cluster (canonical_form lowercases)"
        );
    }

    // ---- Missing coref features (negative tests documenting gaps) ----

    #[test]
    fn no_apposition_handling() {
        // Known gap: appositions require syntactic parse. "Obama" and "the president"
        // are not clustered because SimpleCorefResolver has no apposition detection.
        let resolver = SimpleCorefResolver::default();
        let entities = vec![
            person("Obama", 0, 5),
            person("the president", 7, 20),
            person("He", 28, 30),
        ];
        let resolved = resolver.resolve(&entities);
        let obama_id = resolved[0].canonical_id.unwrap();
        let president_id = resolved[1].canonical_id.unwrap();
        assert_ne!(
            obama_id, president_id,
            "Known gap: apposition 'Obama' / 'the president' not clustered (needs syntactic parse)"
        );
    }

    #[test]
    fn no_predicate_nominal() {
        // Known gap: "John is a doctor. The doctor left." -- predicate nominal
        // coreference requires understanding copular constructions.
        let resolver = SimpleCorefResolver::default();
        let entities = vec![
            person("John", 0, 4),
            person("The doctor", 22, 32),
        ];
        let resolved = resolver.resolve(&entities);
        assert_ne!(
            resolved[0].canonical_id.unwrap(),
            resolved[1].canonical_id.unwrap(),
            "Known gap: predicate nominal 'John' / 'The doctor' not clustered"
        );
    }

    #[test]
    fn no_demonstrative_resolution() {
        // Known gap: "This" is not in the pronoun list for SimpleCorefResolver,
        // so abstract demonstrative anaphora is not resolved.
        let resolver = SimpleCorefResolver::default();
        let entities = vec![
            org("The company", 0, 11),
            // "This" as demonstrative anaphor
            person("This", 35, 39),
        ];
        let resolved = resolver.resolve(&entities);
        assert_ne!(
            resolved[0].canonical_id.unwrap(),
            resolved[1].canonical_id.unwrap(),
            "Known gap: demonstrative 'This' not resolved by SimpleCorefResolver"
        );
    }

    #[test]
    fn no_split_antecedent() {
        // Known gap: "They" cannot resolve to the set {John, Mary}. The data model
        // represents canonical_id as a single ID, not a set of antecedents.
        let resolver = SimpleCorefResolver::default();
        let entities = vec![
            person("John", 0, 4),
            person("Mary", 9, 13),
            person("They", 20, 24),
        ];
        let resolved = resolver.resolve(&entities);
        let they_id = resolved[2].canonical_id.unwrap();
        // "They" will resolve to the nearest compatible entity (Mary), not {John, Mary}.
        let john_id = resolved[0].canonical_id.unwrap();
        let mary_id = resolved[1].canonical_id.unwrap();
        let resolves_to_both = they_id == john_id && they_id == mary_id;
        assert!(
            !resolves_to_both,
            "Known gap: split antecedent -- 'They' cannot resolve to {{John, Mary}}"
        );
    }

    #[test]
    fn no_relative_pronoun() {
        // "who" is correctly NOT in the pronoun list. Relative pronouns should not
        // be rewritten in coreference output.
        let resolver = SimpleCorefResolver::default();
        assert!(
            !resolver.is_pronoun("who"),
            "'who' should not be treated as a resolvable pronoun"
        );
        assert!(!resolver.is_pronoun("which"));
        assert!(!resolver.is_pronoun("that"));
    }

    // ---- Boundary conditions ----

    #[test]
    fn empty_entities_no_crash() {
        let resolver = SimpleCorefResolver::default();
        let result = resolver.resolve(&[]);
        assert!(result.is_empty());
    }

    #[test]
    fn single_entity_gets_canonical_id() {
        let resolver = SimpleCorefResolver::default();
        let entities = vec![person("Alice", 0, 5)];
        let resolved = resolver.resolve(&entities);
        assert!(resolved[0].canonical_id.is_some());
    }

    #[test]
    fn all_same_type_cluster_by_name() {
        // Multiple Person entities with the same name should all cluster together.
        let resolver = SimpleCorefResolver::default();
        let entities = vec![
            person("Alice", 0, 5),
            person("Alice", 20, 25),
            person("Alice", 40, 45),
        ];
        let resolved = resolver.resolve(&entities);
        let id = resolved[0].canonical_id.unwrap();
        assert!(
            resolved.iter().all(|e| e.canonical_id.unwrap() == id),
            "All 'Alice' entities should be in the same cluster"
        );
    }

    #[test]
    fn max_pronoun_lookback_respected() {
        // With max_pronoun_lookback=1, the pronoun search window is small (1*10 = 10 entities).
        // Place a pronoun far after many intervening entities to test boundary.
        let mut entities = Vec::new();
        entities.push(person("Alice", 0, 5));
        // Insert enough non-pronoun entities to exceed the window.
        for i in 1..=20 {
            entities.push(org(&format!("Corp{}", i), i * 10, i * 10 + 5));
        }
        entities.push(person("she", 300, 303));

        let resolver = SimpleCorefResolver::new(CorefConfig {
            max_pronoun_lookback: 1,
            ..CorefConfig::default()
        });
        let resolved = resolver.resolve(&entities);

        let alice_id = resolved[0].canonical_id.unwrap();
        let she_id = resolved.last().unwrap().canonical_id.unwrap();
        // "she" is beyond the pronoun distance window (1 * 10 = 10 entities back),
        // but it may still find a compatible entity within those 10. The key invariant
        // is that Alice (entity 0) is NOT within the window when there are 20 intervening.
        assert_ne!(
            alice_id, she_id,
            "Pronoun too far from antecedent should stay unresolved or resolve to nearer entity"
        );
    }

    #[test]
    fn fuzzy_matching_disabled_prevents_substring() {
        let resolver = SimpleCorefResolver::new(CorefConfig {
            fuzzy_matching: false,
            proper_containment: false,
            ..CorefConfig::default()
        });
        let entities = vec![
            person("John Smith", 0, 10),
            person("Smith", 20, 25),
        ];
        let resolved = resolver.resolve(&entities);
        assert_ne!(
            resolved[0].canonical_id.unwrap(),
            resolved[1].canonical_id.unwrap(),
            "fuzzy_matching=false should prevent substring matching"
        );
    }

    // ---- Task #5 regression tests ----

    #[test]
    fn test_alice_bob_she_resolves_to_alice() {
        // With gazetteer: Alice -> Feminine, Bob -> Masculine.
        // "she" (Feminine) should skip Bob and resolve to Alice.
        let resolver = SimpleCorefResolver::default();
        let entities = vec![
            person("Alice", 0, 5),
            person("Bob", 10, 13),
            person("she", 20, 23),
        ];
        let resolved = resolver.resolve(&entities);
        let she_id = resolved[2].canonical_id.unwrap();
        let alice_id = resolved[0].canonical_id.unwrap();
        assert_eq!(
            she_id, alice_id,
            "With gazetteer, 'she' should resolve to 'Alice' (Feminine), not 'Bob' (Masculine)"
        );
    }

    #[test]
    fn test_gazetteer_disabled_falls_back_to_nearest() {
        let resolver = SimpleCorefResolver::new(CorefConfig {
            use_name_gazetteer: false,
            ..CorefConfig::default()
        });
        let entities = vec![
            person("Alice", 0, 5),
            person("Bob", 10, 13),
            person("she", 20, 23),
        ];
        let resolved = resolver.resolve(&entities);
        let she_id = resolved[2].canonical_id.unwrap();
        let bob_id = resolved[1].canonical_id.unwrap();
        assert_eq!(
            she_id, bob_id,
            "Without gazetteer, 'she' falls back to nearest compatible entity 'Bob'"
        );
    }

    #[test]
    #[allow(deprecated)]
    fn test_similarity_threshold_deprecated() {
        // similarity_threshold is deprecated and has no effect.
        let config = CorefConfig {
            similarity_threshold: 0.99,
            ..CorefConfig::default()
        };
        // Should compile with deprecation warning, and not affect output.
        let resolver = SimpleCorefResolver::new(config);
        let entities = vec![person("Alice", 0, 5), person("Alice", 10, 15)];
        let resolved = resolver.resolve(&entities);
        assert_eq!(
            resolved[0].canonical_id, resolved[1].canonical_id,
            "similarity_threshold should not affect exact name matching"
        );
    }

    // ---- Property tests ----

    mod proptests {
        use super::*;
        use proptest::prelude::*;

        fn arb_entity_type() -> impl Strategy<Value = EntityType> {
            prop_oneof![
                Just(EntityType::Person),
                Just(EntityType::Organization),
                Just(EntityType::Location),
            ]
        }

        fn arb_entity() -> impl Strategy<Value = Entity> {
            (
                "[a-zA-Z ]{1,20}",   // text
                arb_entity_type(),
                0..1000usize,         // start
            )
                .prop_map(|(text, entity_type, start)| {
                    let end = start + text.len();
                    Entity::new(text, entity_type, start, end, 0.9)
                })
        }

        fn arb_entity_vec() -> impl Strategy<Value = Vec<Entity>> {
            proptest::collection::vec(arb_entity(), 0..30)
        }

        proptest! {
            #[test]
            fn resolve_never_panics(entities in arb_entity_vec()) {
                let resolver = SimpleCorefResolver::default();
                let _ = resolver.resolve(&entities);
            }

            #[test]
            fn every_entity_has_canonical_id(entities in arb_entity_vec()) {
                let resolver = SimpleCorefResolver::default();
                let resolved = resolver.resolve(&entities);
                for (i, entity) in resolved.iter().enumerate() {
                    prop_assert!(
                        entity.canonical_id.is_some(),
                        "Entity {} ({:?}) missing canonical_id after resolve",
                        i, entity.text
                    );
                }
            }

            #[test]
            fn canonical_id_reflexive(entities in arb_entity_vec()) {
                // Every entity's canonical_id should point to a cluster that contains
                // at least the entity itself (i.e., some entity in the output shares
                // the same canonical_id).
                let resolver = SimpleCorefResolver::default();
                let resolved = resolver.resolve(&entities);
                for entity in &resolved {
                    if let Some(cid) = entity.canonical_id {
                        let count = resolved.iter()
                            .filter(|e| e.canonical_id == Some(cid))
                            .count();
                        prop_assert!(
                            count >= 1,
                            "canonical_id {:?} has no members in output",
                            cid
                        );
                    }
                }
            }
        }
    }
}
