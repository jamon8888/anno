//! HMM algorithm implementation: Viterbi decoding, forward-backward,
//! feature extraction, and emission scoring.
//!
//! All methods are on `impl HmmNER` and require the struct definition from `super`.

use super::*;

impl HmmNER {
    /// Create a new HMM NER model with default parameters.
    #[must_use]
    pub fn new() -> Self {
        Self::with_config(HmmConfig::default())
    }

    /// Create a new HMM NER model using only heuristic parameters (no bundled params).
    ///
    /// This is useful for E2E evaluation comparisons (heuristic vs bundled-trained).
    #[must_use]
    pub fn new_heuristic() -> Self {
        Self::with_config_no_bundled(HmmConfig::default())
    }

    /// Create with custom configuration.
    #[must_use]
    pub fn with_config(config: HmmConfig) -> Self {
        Self::with_config_internal(config, true)
    }

    /// Create with custom configuration, skipping bundled params even if the feature is enabled.
    #[must_use]
    pub fn with_config_no_bundled(config: HmmConfig) -> Self {
        Self::with_config_internal(config, false)
    }

    fn with_config_internal(config: HmmConfig, allow_bundled: bool) -> Self {
        let states = vec![
            "O".to_string(),
            "B-PER".to_string(),
            "I-PER".to_string(),
            "B-ORG".to_string(),
            "I-ORG".to_string(),
            "B-LOC".to_string(),
            "I-LOC".to_string(),
            "B-MISC".to_string(),
            "I-MISC".to_string(),
        ];

        let state_to_idx: HashMap<String, usize> = states
            .iter()
            .enumerate()
            .map(|(i, s)| (s.clone(), i))
            .collect();

        let n = states.len();

        // Initialize transition probabilities with BIO constraints
        let mut transitions = vec![vec![0.0; n]; n];
        Self::init_transitions(&mut transitions, &states, &config);

        // Initialize with uniform priors, biased toward O
        // Initial state distribution - more balanced to allow entities at start
        let mut initial = vec![0.0; n];
        for (i, state) in states.iter().enumerate() {
            if state == "O" {
                initial[i] = 0.4; // O is common but not dominant
            } else if state.starts_with("B-") {
                initial[i] = 0.15; // Entities can start sentences
            } else if state.starts_with("I-") {
                initial[i] = config.smoothing; // I- can't start
            }
        }
        Self::normalize(&mut initial);

        // Initialize emission probabilities with heuristics
        let emissions = Self::init_emissions(&states, &state_to_idx);

        let mut m = Self {
            config,
            states,
            state_to_idx,
            transitions,
            initial,
            emissions,
            vocab: HashMap::new(),
            backoff: None,
        };

        // Optional bundled params (priors + transitions only). These are small enough to ship,
        // and they don't embed word identity emissions.
        if allow_bundled {
            if let Some(p) = Self::bundled_params() {
                if p.states == m.states
                    && p.initial.len() == m.states.len()
                    && p.transitions.len() == m.states.len()
                    && p.transitions.iter().all(|r| r.len() == m.states.len())
                {
                    let backoff = HmmBackoff::from_params(&p);
                    m.backoff = Some(backoff);
                    // Prefer bundled dynamics when configured (the default config does),
                    // since the bundled params are intended to be a real end-to-end baseline.
                    //
                    // You can force-enable via env var, or force-disable via config.
                    let use_dynamics_env = std::env::var("ANNO_HMM_USE_BUNDLED_DYNAMICS")
                        .ok()
                        .is_some_and(|v| {
                            let s = v.trim();
                            s == "1"
                                || s.eq_ignore_ascii_case("true")
                                || s.eq_ignore_ascii_case("yes")
                        });
                    let use_dynamics = m.config.use_bundled_dynamics || use_dynamics_env;
                    if use_dynamics {
                        m.initial = p.initial.clone();
                        m.transitions = p.transitions.clone();
                    }
                }
            }
        }

        m
    }

    fn bundled_params() -> Option<HmmParams> {
        #[cfg(feature = "bundled-hmm-params")]
        {
            static ONCE: OnceLock<Option<HmmParams>> = OnceLock::new();
            return ONCE
                .get_or_init(|| {
                    let s = include_str!("hmm_params.json");
                    let v: serde_json::Value = serde_json::from_str(s).ok()?;
                    let states = v
                        .get("states")?
                        .as_array()?
                        .iter()
                        .map(|x| x.as_str().map(|s| s.to_string()))
                        .collect::<Option<Vec<_>>>()?;
                    let initial = v
                        .get("initial")?
                        .as_array()?
                        .iter()
                        .map(|x| x.as_f64())
                        .collect::<Option<Vec<_>>>()?;
                    let transitions = v
                        .get("transitions")?
                        .as_array()?
                        .iter()
                        .map(|row| {
                            row.as_array()?
                                .iter()
                                .map(|x| x.as_f64())
                                .collect::<Option<Vec<_>>>()
                        })
                        .collect::<Option<Vec<_>>>()?;
                    let backoff = v.get("backoff")?.clone();
                    Some(HmmParams {
                        states,
                        initial,
                        transitions,
                        backoff,
                    })
                })
                .clone();
        }
        #[cfg(not(feature = "bundled-hmm-params"))]
        {
            None
        }
    }

    /// Initialize transition matrix with BIO constraints.
    fn init_transitions(trans: &mut [Vec<f64>], states: &[String], config: &HmmConfig) {
        let n = states.len();

        for i in 0..n {
            for j in 0..n {
                let from = &states[i];
                let to = &states[j];

                // BIO constraints
                if let Some(entity_type) = to.strip_prefix("I-") {
                    let valid_b = format!("B-{}", entity_type);
                    let valid_i = format!("I-{}", entity_type);

                    if from == &valid_b || from == &valid_i {
                        trans[i][j] = 0.3; // Valid continuation
                    } else {
                        trans[i][j] = config.smoothing; // Invalid (very low)
                    }
                } else if to.starts_with("B-") {
                    trans[i][j] = 0.1; // Entities are relatively rare
                } else {
                    // O tag
                    trans[i][j] = 0.5; // Most transitions go to O
                }
            }

            // Normalize row
            Self::normalize(&mut trans[i]);
        }
    }

    /// Initialize emission probabilities with comprehensive gazetteers.
    ///
    /// These are empirically-tuned emission probabilities based on word lists
    /// commonly found in NER training data (CoNLL-2003, OntoNotes, etc.).
    fn init_emissions(
        _states: &[String],
        state_to_idx: &HashMap<String, usize>,
    ) -> HashMap<(usize, String), f64> {
        let mut emissions = HashMap::new();

        // Comprehensive person indicators (names, titles, honorifics)
        let person_indicators = [
            // Common first names
            "john",
            "mary",
            "james",
            "david",
            "michael",
            "robert",
            "william",
            "richard",
            "sarah",
            "jennifer",
            "elizabeth",
            "lisa",
            "marie",
            "jane",
            "emily",
            "anna",
            "barack",
            "donald",
            "joe",
            "george",
            "bill",
            "hillary",
            "elon",
            "jeff",
            "angela",
            "vladimir",
            "emmanuel",
            "xi",
            "narendra",
            "justin",
            "rishi",
            "steve",
            "tim",
            "mark",
            "satya",
            "sundar",
            "sheryl",
            "sam",
            "dario",
            // Common surnames (political, tech, historical)
            "obama",
            "biden",
            "trump",
            "bush",
            "clinton",
            "reagan",
            "kennedy",
            "lincoln",
            "merkel",
            "macron",
            "putin",
            "jinping",
            "modi",
            "trudeau",
            "sunak",
            "musk",
            "bezos",
            "zuckerberg",
            "gates",
            "jobs",
            "wozniak",
            "cook",
            "pichai",
            "nadella",
            "altman",
            "amodei",
            "hassabis",
            "hinton",
            "lecun",
            "bengio",
            "smith",
            "johnson",
            "williams",
            "brown",
            "jones",
            "garcia",
            "miller",
            "davis",
            // Honorifics and titles
            "mr",
            "mrs",
            "ms",
            "dr",
            "prof",
            "sir",
            "lord",
            "lady",
            "president",
            "ceo",
            "chairman",
            "director",
            "minister",
            "senator",
            "mayor",
            "governor",
            "chancellor",
            "prime",
            "secretary",
            "ambassador",
            "general",
            "admiral",
        ];

        // Comprehensive organization indicators
        let org_indicators = [
            // Company names
            "google",
            "apple",
            "microsoft",
            "amazon",
            "facebook",
            "meta",
            "tesla",
            "ibm",
            "intel",
            "nvidia",
            "oracle",
            "cisco",
            "adobe",
            "netflix",
            "uber",
            "toyota",
            "honda",
            "ford",
            "chevrolet",
            "bmw",
            "mercedes",
            "audi",
            // Suffixes
            "inc",
            "corp",
            "ltd",
            "llc",
            "co",
            "plc",
            "gmbh",
            "ag",
            "sa",
            "company",
            "corporation",
            "incorporated",
            "limited",
            "group",
            "holdings",
            // Institutional
            "university",
            "institute",
            "college",
            "academy",
            "school",
            "hospital",
            "foundation",
            "association",
            "organization",
            "committee",
            "council",
            "department",
            "ministry",
            "agency",
            "bureau",
            "commission",
            // Government/International
            "fbi",
            "cia",
            "nsa",
            "nasa",
            "un",
            "nato",
            "who",
            "imf",
            "eu",
            "usa",
            "parliament",
            "congress",
            "senate",
            "house",
            "court",
            "bank",
        ];

        // Comprehensive location indicators
        let loc_indicators = [
            // US cities/states
            "new",
            "york",
            "california",
            "texas",
            "florida",
            "washington",
            "chicago",
            "boston",
            "seattle",
            "san",
            "francisco",
            "los",
            "angeles",
            "las",
            "vegas",
            "miami",
            "denver",
            "atlanta",
            "phoenix",
            "dallas",
            "houston",
            "portland",
            // World cities
            "london",
            "paris",
            "berlin",
            "tokyo",
            "beijing",
            "moscow",
            "sydney",
            "toronto",
            "vancouver",
            "rome",
            "madrid",
            "amsterdam",
            "brussels",
            "vienna",
            "seoul",
            "singapore",
            "hong",
            "kong",
            "dubai",
            "mumbai",
            "delhi",
            // Countries/regions
            "united",
            "states",
            "america",
            "china",
            "russia",
            "germany",
            "france",
            "japan",
            "india",
            "brazil",
            "canada",
            "australia",
            "uk",
            "britain",
            "italy",
            "spain",
            "mexico",
            "korea",
            "taiwan",
            "vietnam",
            "thailand",
            // Geographic terms
            "city",
            "county",
            "state",
            "country",
            "province",
            "region",
            "district",
            "river",
            "mountain",
            "lake",
            "ocean",
            "sea",
            "island",
            "peninsula",
            "north",
            "south",
            "east",
            "west",
            "central",
            "northern",
            "southern",
        ];

        // Set higher emission probabilities for known indicators
        for word in person_indicators {
            let b_idx = state_to_idx["B-PER"];
            let i_idx = state_to_idx["I-PER"];
            emissions.insert((b_idx, word.to_string()), 0.4);
            emissions.insert((i_idx, word.to_string()), 0.25);
        }

        for word in org_indicators {
            let b_idx = state_to_idx["B-ORG"];
            let i_idx = state_to_idx["I-ORG"];
            emissions.insert((b_idx, word.to_string()), 0.4);
            emissions.insert((i_idx, word.to_string()), 0.25);
        }

        for word in loc_indicators {
            let b_idx = state_to_idx["B-LOC"];
            let i_idx = state_to_idx["I-LOC"];
            emissions.insert((b_idx, word.to_string()), 0.4);
            emissions.insert((i_idx, word.to_string()), 0.25);
        }

        emissions
    }

    /// Normalize a probability vector.
    fn normalize(vec: &mut [f64]) {
        let sum: f64 = vec.iter().sum();
        if sum > 0.0 {
            for v in vec.iter_mut() {
                *v /= sum;
            }
        }
    }

    /// Get emission probability for (state, word).
    pub(super) fn emission_prob(&self, state_idx: usize, word: &str) -> f64 {
        let lower = word.to_lowercase();

        // Check explicit emissions (known entity names)
        if let Some(&prob) = self.emissions.get(&(state_idx, lower.clone())) {
            return prob;
        }

        // If we have bundled backoff emissions, prefer them over heuristics.
        // These are compact, trained probabilities over generic word features (no word identity).
        if let Some(b) = self.backoff.as_ref() {
            // Emission score uses a naive Bayes factorization:
            //   P(features | state) = P(len_bucket | state) * Π_f P(f|state)^(present) * (1-P(f|state))^(absent)
            // We only use the small set of features in the bundled table.
            let lb = Self::len_bucket(word);
            let mut sum_log = 0.0f64;
            if let Some(p) = b.len.get(lb).and_then(|v| v.get(state_idx).copied()) {
                sum_log += p.max(1e-12).ln();
            } else {
                sum_log += (1e-12f64).ln();
            }
            let feats = Self::bool_features(word);
            for k in &b.bool_keys {
                let present = feats.get(k.as_str()).copied().unwrap_or(false);
                let p_present = b
                    .bool_present
                    .get(k)
                    .and_then(|v| v.get(state_idx).copied())
                    .unwrap_or(1e-12)
                    .clamp(1e-12, 1.0 - 1e-12);
                let p = if present { p_present } else { 1.0 - p_present };
                sum_log += p.max(1e-12).ln();
            }
            let mut score = sum_log.exp().max(self.config.smoothing);
            // State 0 is "O" in our state list.
            if state_idx != 0 {
                score *= self.config.non_o_emission_scale.max(1e-6);
            }
            return score.max(self.config.smoothing);
        }

        // Heuristic emissions based on word features
        let state = &self.states[state_idx];
        let is_capitalized = word.chars().next().is_some_and(|c| c.is_uppercase());
        let is_all_caps =
            word.chars().all(|c| c.is_uppercase() || !c.is_alphabetic()) && word.len() > 1;
        let has_digit = word.chars().any(|c| c.is_ascii_digit());
        let is_title_case = is_capitalized && word.len() > 1;

        // Check for organization suffixes
        let org_suffixes = [
            "Inc", "Corp", "Ltd", "LLC", "Co", "Company", "Inc.", "Corp.", "Ltd.",
        ];
        let is_org_suffix = org_suffixes.contains(&word);

        if state == "O" {
            // Non-capitalized words and digits are likely O
            if !is_capitalized {
                return 0.7;
            }
            // Capitalized at sentence start - unclear
            if has_digit {
                return 0.5;
            }
            // Title case words are less likely to be O
            if is_title_case {
                return 0.15;
            }
            return 0.4;
        }

        if state.starts_with("B-") || state.starts_with("I-") {
            let entity_type = &state[2..];

            // Organization suffixes strongly indicate ORG
            if entity_type == "ORG" && is_org_suffix {
                return 0.8;
            }

            // All caps = likely ORG (acronyms like IBM, NASA)
            if is_all_caps && entity_type == "ORG" {
                return 0.6;
            }

            // Title case words are likely entities, but prefer PER for typical names
            // Most proper nouns starting with capital letters are person names
            // unless they have organization-specific markers
            if is_title_case && !has_digit {
                if entity_type == "PER" {
                    return 0.55; // Slightly prefer PER over others for title case
                } else if entity_type == "LOC" {
                    return 0.45; // Locations are second most common title case
                } else if entity_type == "ORG" {
                    return 0.35; // ORGs need more evidence (suffix, acronym)
                }
                return 0.4;
            }

            // Capitalized words at least somewhat likely
            if is_capitalized && !has_digit {
                return 0.3;
            }

            return self.config.smoothing;
        }

        self.config.smoothing
    }

    /// Viterbi decoding to find most likely state sequence.
    pub(super) fn viterbi(&self, words: &[&str]) -> Vec<usize> {
        if words.is_empty() {
            return vec![];
        }

        let n = words.len();
        let m = self.states.len();

        // Use log probabilities for numerical stability
        let log = |p: f64| if p > 0.0 { p.ln() } else { f64::NEG_INFINITY };

        // DP tables
        let mut dp = vec![vec![f64::NEG_INFINITY; m]; n];
        let mut backptr = vec![vec![0usize; m]; n];

        // Initialize first position
        for (j, cell) in dp[0].iter_mut().enumerate().take(m) {
            *cell = log(self.initial[j]) + log(self.emission_prob(j, words[0]));
        }

        // Forward pass
        for t in 1..n {
            for j in 0..m {
                let emit = log(self.emission_prob(j, words[t]));

                for i in 0..m {
                    let trans = log(self.transitions[i][j]);
                    let score = dp[t - 1][i] + trans + emit;

                    if score > dp[t][j] {
                        dp[t][j] = score;
                        backptr[t][j] = i;
                    }
                }
            }
        }

        // Find best final state
        let mut best_state = 0;
        let mut best_score = f64::NEG_INFINITY;
        for (j, &score) in dp[n - 1].iter().enumerate() {
            if score > best_score {
                best_score = score;
                best_state = j;
            }
        }

        // Backtrack
        let mut path = vec![0usize; n];
        path[n - 1] = best_state;
        for t in (0..n - 1).rev() {
            path[t] = backptr[t + 1][path[t + 1]];
        }

        path
    }

    /// Convert BIO labels to entities.
    ///
    /// Uses token position tracking to correctly handle duplicate entity texts.
    /// The previous implementation used `text.find()` which always returned the
    /// first occurrence, causing incorrect offsets for duplicate entities.
    pub(super) fn decode_entities(&self, text: &str, words: &[&str], labels: &[usize]) -> Vec<Entity> {
        use crate::offset::SpanConverter;

        let converter = SpanConverter::new(text);
        let mut entities = Vec::new();

        // Track token positions (byte offsets) as we iterate
        let token_positions: Vec<(usize, usize)> = Self::calculate_token_positions(text, words);

        let mut current: Option<(usize, usize, EntityType, Vec<&str>)> = None;

        for (i, (&label_idx, &word)) in labels.iter().zip(words.iter()).enumerate() {
            let label = &self.states[label_idx];

            if label.starts_with("B-") {
                // Save previous entity
                if let Some((start_idx, end_idx, entity_type, entity_words)) = current.take() {
                    Self::push_entity_from_positions(
                        &converter,
                        &token_positions,
                        start_idx,
                        end_idx,
                        &entity_words,
                        entity_type,
                        &mut entities,
                    );
                }

                // Start new entity
                let entity_type_str = label
                    .strip_prefix("B-")
                    .or_else(|| label.strip_prefix("I-"))
                    .expect("label should start with B- or I-");
                let entity_type = match entity_type_str {
                    "PER" => EntityType::Person,
                    "ORG" => EntityType::Organization,
                    "LOC" => EntityType::Location,
                    other => EntityType::Other(other.to_string()),
                };
                current = Some((i, i, entity_type, vec![word]));
            } else if label.starts_with("I-") && current.is_some() {
                if let Some((_, ref mut end_idx, _, ref mut entity_words)) = current {
                    entity_words.push(word);
                    *end_idx = i;
                }
            } else {
                // O tag
                if let Some((start_idx, end_idx, entity_type, entity_words)) = current.take() {
                    Self::push_entity_from_positions(
                        &converter,
                        &token_positions,
                        start_idx,
                        end_idx,
                        &entity_words,
                        entity_type,
                        &mut entities,
                    );
                }
            }
        }

        // Final entity
        if let Some((start_idx, end_idx, entity_type, entity_words)) = current {
            Self::push_entity_from_positions(
                &converter,
                &token_positions,
                start_idx,
                end_idx,
                &entity_words,
                entity_type,
                &mut entities,
            );
        }

        entities
    }

    /// Calculate byte positions for each token in the text.
    pub(super) fn calculate_token_positions(text: &str, tokens: &[&str]) -> Vec<(usize, usize)> {
        let mut positions = Vec::with_capacity(tokens.len());
        let mut byte_pos = 0;

        for token in tokens {
            // Find token starting from current position
            if let Some(rel_pos) = text[byte_pos..].find(token) {
                let start = byte_pos + rel_pos;
                let end = start + token.len();
                positions.push((start, end));
                byte_pos = end; // Move past this token
            } else {
                // Fallback: use current position (shouldn't happen with whitespace tokenization)
                positions.push((byte_pos, byte_pos));
            }
        }

        positions
    }

    /// Helper to create entity with correct character offsets using token positions.
    fn push_entity_from_positions(
        converter: &crate::offset::SpanConverter,
        positions: &[(usize, usize)],
        start_token_idx: usize,
        end_token_idx: usize,
        words: &[&str],
        entity_type: EntityType,
        entities: &mut Vec<Entity>,
    ) {
        if start_token_idx >= positions.len() || end_token_idx >= positions.len() {
            return;
        }

        let byte_start = positions[start_token_idx].0;
        let byte_end = positions[end_token_idx].1;
        let char_start = converter.byte_to_char(byte_start);
        let char_end = converter.byte_to_char(byte_end);
        let entity_text = words.join(" ");

        entities.push(Entity::new(
            entity_text,
            entity_type,
            char_start,
            char_end,
            0.65, // HMM confidence
        ));
    }

    /// Train the HMM from labeled data.
    ///
    /// # Arguments
    /// * `sentences` - List of (words, tags) pairs
    pub fn train(&mut self, sentences: &[(&[&str], &[&str])]) {
        // Count transitions
        let n = self.states.len();
        let mut trans_counts = vec![vec![0usize; n]; n];
        let mut initial_counts = vec![0usize; n];
        let mut emission_counts: HashMap<(usize, String), usize> = HashMap::new();
        let mut state_counts = vec![0usize; n];

        for (words, tags) in sentences {
            if tags.is_empty() {
                continue;
            }

            // Initial state
            if let Some(&idx) = self.state_to_idx.get(tags[0]) {
                initial_counts[idx] += 1;
            }

            // Transitions and emissions
            for (i, (word, tag)) in words.iter().zip(tags.iter()).enumerate() {
                if let Some(&tag_idx) = self.state_to_idx.get(*tag) {
                    // Emission count
                    *emission_counts
                        .entry((tag_idx, word.to_lowercase()))
                        .or_insert(0) += 1;
                    state_counts[tag_idx] += 1;

                    // Transition count
                    if i > 0 {
                        if let Some(&prev_idx) = self.state_to_idx.get(tags[i - 1]) {
                            trans_counts[prev_idx][tag_idx] += 1;
                        }
                    }
                }
            }
        }

        // Convert counts to probabilities (with smoothing)
        let total_initial: f64 =
            initial_counts.iter().sum::<usize>() as f64 + self.config.smoothing * n as f64;
        for (i, &count) in initial_counts.iter().enumerate() {
            self.initial[i] = (count as f64 + self.config.smoothing) / total_initial;
        }

        for (i, row) in trans_counts.iter().enumerate().take(n) {
            let total: f64 = row.iter().sum::<usize>() as f64 + self.config.smoothing * n as f64;
            for (j, &count) in row.iter().enumerate().take(n) {
                self.transitions[i][j] = (count as f64 + self.config.smoothing) / total;
            }
        }

        for ((state_idx, word), count) in emission_counts {
            let total = state_counts[state_idx] as f64;
            if total > 0.0 {
                self.emissions
                    .insert((state_idx, word), count as f64 / total);
            }
        }
    }

    fn len_bucket(word: &str) -> &'static str {
        let n = word.chars().count();
        if n <= 1 {
            "len:1"
        } else if n == 2 {
            "len:2"
        } else if n == 3 {
            "len:3"
        } else if (4..=5).contains(&n) {
            "len:4_5"
        } else if (6..=8).contains(&n) {
            "len:6_8"
        } else {
            "len:9p"
        }
    }

    fn bool_features(word: &str) -> HashMap<&'static str, bool> {
        let is_capitalized = word.chars().next().is_some_and(|c| c.is_uppercase());
        let is_all_caps = word.chars().all(|c| c.is_uppercase() || !c.is_alphabetic())
            && word.chars().count() > 1;
        let is_digit = !word.is_empty() && word.chars().all(|c| c.is_ascii_digit());
        let has_digit = word.chars().any(|c| c.is_ascii_digit());
        let has_hyphen = word.contains('-');
        let has_dot = word.contains('.');
        let mut m = HashMap::new();
        m.insert("is_capitalized", is_capitalized);
        m.insert("is_all_caps", is_all_caps);
        m.insert("is_digit", is_digit);
        m.insert("has_digit", has_digit);
        m.insert("has_hyphen", has_hyphen);
        m.insert("has_dot", has_dot);
        m
    }
}

impl HmmBackoff {
    fn from_params(p: &HmmParams) -> Self {
        // backoff schema:
        // {
        //   "len": { bucket: { state: prob } },
        //   "bool": { feat: { state: p_present } }
        // }
        let mut len: HashMap<String, Vec<f64>> = HashMap::new();
        let mut bool_present: HashMap<String, Vec<f64>> = HashMap::new();

        if let Some(obj) = p.backoff.as_object() {
            if let Some(len_obj) = obj.get("len").and_then(|v| v.as_object()) {
                for (bucket, distv) in len_obj {
                    let mut v = vec![1e-12; p.states.len()];
                    if let Some(dist) = distv.as_object() {
                        for (i, state) in p.states.iter().enumerate() {
                            if let Some(x) = dist.get(state).and_then(|x| x.as_f64()) {
                                v[i] = x;
                            }
                        }
                    }
                    len.insert(bucket.clone(), v);
                }
            }
            if let Some(bool_obj) = obj.get("bool").and_then(|v| v.as_object()) {
                for (feat, distv) in bool_obj {
                    let mut v = vec![1e-12; p.states.len()];
                    if let Some(dist) = distv.as_object() {
                        for (i, state) in p.states.iter().enumerate() {
                            if let Some(x) = dist.get(state).and_then(|x| x.as_f64()) {
                                v[i] = x;
                            }
                        }
                    }
                    bool_present.insert(feat.clone(), v);
                }
            }
        }

        let mut bool_keys: Vec<String> = bool_present.keys().cloned().collect();
        bool_keys.sort();
        Self {
            len,
            bool_present,
            bool_keys,
        }
    }
}

impl Default for HmmNER {
    fn default() -> Self {
        Self::new()
    }
}

