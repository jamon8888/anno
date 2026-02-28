//! Simple coreference resolution backends.
//!
//! Provides rule-based and box-embedding resolvers to produce coreference chains from entities,
//! completing the loop between extraction and coreference metric computation.
//!
//! # Sieve pipeline
//!
//! [`SimpleCorefResolver`] applies six sieves in order, returning the first match:
//!
//! 1. **Pronoun resolution** -- links pronouns to a compatible antecedent within a lookback window,
//!    using gender inference from a name gazetteer and pronoun-entity-type compatibility rules.
//! 2. **Exact canonical match** -- matches entities with identical `type:lowercased_text` keys.
//! 3. **Acronym match** -- links single-token acronyms to multi-word names whose initials match
//!    (e.g., "IBM" ~ "International Business Machines"), same entity type required.
//! 4. **Relaxed head match** -- links multi-word mentions sharing a head word (last token) and
//!    entity type (e.g., "President Obama" ~ "Barack Obama").
//! 5. **Proper containment** -- links mentions where one is a contiguous word subsequence of the
//!    other in original text, same entity type required (e.g., "Obama" in "Barack Obama").
//! 6. **Substring/fuzzy match** -- links mentions sharing a substring (>= 3 chars) or a matching
//!    last word, same entity type required.

use crate::backends::box_embeddings::{BoxCorefConfig, BoxEmbedding};
use crate::{Entity, EntityType};
use anno_core::{CanonicalId, CoreferenceResolver, Gender};
use std::collections::HashMap;

/// Configuration for [`SimpleCorefResolver`].
///
/// All boolean sieve toggles default to `true`. Disable individual sieves to measure their
/// contribution or to reduce false merges on a particular dataset.
#[derive(Debug, Clone)]
pub struct CorefConfig {
    /// Similarity threshold for name matching (0.0--1.0).
    ///
    /// Default: `0.7`.
    #[deprecated(
        note = "Not currently used by any matching logic. Will be wired into names_match() in a future version."
    )]
    pub similarity_threshold: f64,

    /// Maximum number of preceding entities to search when resolving a pronoun (sieve 1).
    ///
    /// The actual lookback window is `max_pronoun_lookback * 10` entities,
    /// a rough approximation of sentence distance by entity count.
    ///
    /// Default: `3` (i.e., 30 entities).
    pub max_pronoun_lookback: usize,

    /// Enable substring/fuzzy name matching (sieve 6).
    ///
    /// When enabled, two mentions match if one is a substring of the other (>= 3 chars)
    /// or if they share a last word and entity type.
    ///
    /// Default: `true`.
    pub fuzzy_matching: bool,

    /// Include singletons (entities with no coreferent) in output chains.
    ///
    /// Default: `true`.
    pub include_singletons: bool,

    /// Use a built-in name-to-gender gazetteer for pronoun resolution (sieve 1).
    ///
    /// When enabled, common English names (e.g., "Alice" -> Feminine,
    /// "Bob" -> Masculine) are used to infer gender for proper nouns,
    /// improving pronoun resolution accuracy. Disable for non-English
    /// text or when names don't follow English gender conventions.
    ///
    /// Default: `true`.
    pub use_name_gazetteer: bool,

    /// Enable acronym matching (sieve 3).
    ///
    /// Checks if one mention is a single-token acronym whose letters match the
    /// initial letters of the other mention's words. Requires same entity type.
    /// Example: "IBM" matches "International Business Machines".
    ///
    /// Default: `true`.
    pub acronym_matching: bool,

    /// Enable relaxed head match (sieve 4).
    ///
    /// Two mentions corefer if their head words (last word) match and they share the same
    /// entity type. Only applies when both mentions have 2+ words.
    /// Example: "President Obama" ~ "Barack Obama" via shared head "Obama".
    ///
    /// Default: `true`.
    pub relaxed_head_match: bool,

    /// Enable proper noun containment (sieve 5).
    ///
    /// One mention's original text must be a proper subsequence of the other, matching
    /// at word boundaries, and both must share the same entity type. Unlike sieve 6
    /// (fuzzy/substring), this operates on original text (not canonical lowercased form)
    /// and requires complete word-boundary alignment.
    /// Example: "Obama" is contained in "Barack Obama".
    ///
    /// Default: `true`.
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
///
/// Applies the six-sieve pipeline described in the [module documentation](self) to assign
/// [`CanonicalId`]s to a sequence of [`Entity`] values. Entities that corefer receive the
/// same id; singletons receive a unique id.
///
/// # Example
///
/// ```rust,no_run
/// use anno::Entity;
/// use anno::EntityType;
/// use anno::backends::coref::simple::{SimpleCorefResolver, CorefConfig};
///
/// let resolver = SimpleCorefResolver::new(CorefConfig::default());
///
/// let entities = vec![
///     Entity::new("John Smith", EntityType::Person, 0, 10, 0.9),
///     Entity::new("he", EntityType::Person, 15, 17, 0.8),
///     Entity::new("John Smith", EntityType::Person, 30, 40, 0.9),
/// ];
///
/// let resolved = resolver.resolve(&entities);
/// // All three mentions share the same canonical_id.
/// assert_eq!(resolved[0].canonical_id, resolved[1].canonical_id);
/// assert_eq!(resolved[0].canonical_id, resolved[2].canonical_id);
/// ```
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
    pub fn resolve_to_chains(&self, entities: &[Entity]) -> Vec<crate::eval::coref::CorefChain> {
        let resolved = self.resolve_entities(entities);
        crate::eval::coref::entities_to_chains(&resolved)
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
    pub(crate) fn is_pronoun(&self, text: &str) -> bool {
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

        // Substring containment -- guarded against short-name false positives.
        // "Li" must not match "political" just because "li" is a substring.
        // Rule: the shorter string must be >= 3 chars, OR it must match a
        // complete word in the longer string (word-boundary check).
        let (shorter, longer) = if text1.len() <= text2.len() {
            (text1, text2)
        } else {
            (text2, text1)
        };
        if longer.contains(shorter) {
            if shorter.chars().count() >= 3 {
                return true;
            }
            // For very short names (< 3 chars), only match at word boundaries.
            if longer.split_whitespace().any(|word| word == shorter) {
                return true;
            }
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
pub(crate) fn gender_from_name(text: &str) -> Option<Gender> {
    let first_word = text.split_whitespace().next()?;
    let lower = first_word.to_lowercase();
    // ~100 common English given names, covering the most frequent names
    // in US/UK census data. This is intentionally small and conservative.
    match lower.as_str() {
        // Masculine
        "james" | "john" | "robert" | "michael" | "david" | "william" | "richard" | "joseph"
        | "thomas" | "charles" | "christopher" | "daniel" | "matthew" | "anthony" | "mark"
        | "donald" | "steven" | "paul" | "andrew" | "joshua" | "kenneth" | "kevin" | "brian"
        | "george" | "timothy" | "ronald" | "edward" | "jason" | "jeffrey" | "ryan" | "jacob"
        | "gary" | "nicholas" | "eric" | "jonathan" | "stephen" | "larry" | "justin" | "scott"
        | "brandon" | "benjamin" | "samuel" | "raymond" | "gregory" | "frank" | "alexander"
        | "patrick" | "jack" | "dennis" | "peter" | "bob" | "jim" | "tom" | "mike" | "bill"
        | "joe" | "dan" | "matt" | "steve" | "chris" | "nick" | "ben" | "sam" | "jake" | "adam"
        | "henry" | "nathan" | "philip" | "carl" | "ahmed" | "ahmad" | "mohammed" | "muhammad"
        | "omar" | "ali" | "hassan" | "hussein" | "khalid" | "ibrahim" => Some(Gender::Masculine),
        // Feminine
        "mary" | "patricia" | "jennifer" | "linda" | "barbara" | "elizabeth" | "susan"
        | "jessica" | "sarah" | "karen" | "lisa" | "nancy" | "betty" | "margaret" | "sandra"
        | "ashley" | "dorothy" | "kimberly" | "emily" | "donna" | "michelle" | "carol"
        | "amanda" | "melissa" | "deborah" | "stephanie" | "rebecca" | "sharon" | "laura"
        | "cynthia" | "kathleen" | "amy" | "angela" | "shirley" | "anna" | "brenda" | "pamela"
        | "emma" | "nicole" | "helen" | "samantha" | "katherine" | "christine" | "debra"
        | "rachel" | "carolyn" | "janet" | "catherine" | "maria" | "heather" | "diane" | "ruth"
        | "julie" | "olivia" | "joyce" | "virginia" | "victoria" | "kelly" | "lauren"
        | "christina" | "joan" | "evelyn" | "judith" | "alice" | "ann" | "anne" | "jane"
        | "jean" | "marie" | "rose" | "grace" | "fatima" | "aisha" | "maryam" | "nour"
        | "layla" | "hana" => Some(Gender::Feminine),
        _ => None,
    }
}

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
