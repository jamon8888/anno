//! Joint model training experiment.
//!
//! This example demonstrates training the joint entity analysis model
//! with synthetic data to verify the training loop works correctly.
//!
//! Run with: cargo run --example joint_training_experiment --features joint

use std::collections::HashMap;

/// Minimal reproduction of types needed for training experiment.
/// In production, use the actual anno types.

// ===========================================================================
// Minimal Types (to avoid full feature dependencies)
// ===========================================================================

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum EntityType {
    Person,
    Organization,
    Location,
    Other(String),
}

impl EntityType {
    pub fn as_label(&self) -> &str {
        match self {
            EntityType::Person => "PER",
            EntityType::Organization => "ORG",
            EntityType::Location => "LOC",
            EntityType::Other(s) => s,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Entity {
    pub text: String,
    pub entity_type: EntityType,
    pub start: usize,
    pub end: usize,
    pub confidence: f64,
}

impl Entity {
    pub fn new(
        text: &str,
        entity_type: EntityType,
        start: usize,
        end: usize,
        confidence: f64,
    ) -> Self {
        Self {
            text: text.to_string(),
            entity_type,
            start,
            end,
            confidence,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MentionKind {
    Proper,
    Nominal,
    Pronominal,
}

#[derive(Debug, Clone)]
pub struct JointMention {
    pub idx: usize,
    pub text: String,
    pub head: String,
    pub start: usize,
    pub end: usize,
    pub mention_kind: MentionKind,
    pub entity: Option<Entity>,
}

impl JointMention {
    pub fn new(idx: usize, text: &str, start: usize, end: usize, entity_type: EntityType) -> Self {
        let head = text.split_whitespace().last().unwrap_or(text).to_string();
        let mention_kind = if text.chars().next().is_some_and(|c| c.is_uppercase()) {
            MentionKind::Proper
        } else if [
            "he", "she", "it", "they", "him", "her", "them", "his", "hers", "its", "their",
        ]
        .contains(&text.to_lowercase().as_str())
        {
            MentionKind::Pronominal
        } else {
            MentionKind::Nominal
        };

        Self {
            idx,
            text: text.to_string(),
            head,
            start,
            end,
            mention_kind,
            entity: Some(Entity::new(text, entity_type, start, end, 0.9)),
        }
    }

    pub fn entity_type(&self) -> Option<EntityType> {
        self.entity.as_ref().map(|e| e.entity_type.clone())
    }
}

// ===========================================================================
// Training Types (simplified version)
// ===========================================================================

#[derive(Debug, Clone)]
pub struct TrainingConfig {
    pub learning_rate: f64,
    pub epsilon: f64,
    pub epochs: usize,
    pub batch_size: usize,
    pub l2_lambda: f64,
    pub cost_weight: f64,
    pub grad_clip: f64,
}

impl Default for TrainingConfig {
    fn default() -> Self {
        Self {
            learning_rate: 0.1,
            epsilon: 1e-8,
            epochs: 20,
            batch_size: 1,
            l2_lambda: 1e-4,
            cost_weight: 1.0,
            grad_clip: 5.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TrainingExample {
    pub text: String,
    pub mentions: Vec<JointMention>,
    pub gold_ner: HashMap<usize, EntityType>,
    pub gold_coref: HashMap<usize, Option<usize>>,
    pub gold_links: HashMap<usize, Option<String>>,
}

impl TrainingExample {
    pub fn hamming_loss(
        &self,
        pred_ner: &HashMap<usize, EntityType>,
        pred_coref: &HashMap<usize, Option<usize>>,
        _pred_links: &HashMap<usize, Option<String>>,
    ) -> f64 {
        let mut loss = 0.0;
        let n = self.mentions.len() as f64;

        for (idx, gold_type) in &self.gold_ner {
            if pred_ner.get(idx) != Some(gold_type) {
                loss += 1.0;
            }
        }

        for (idx, gold_ante) in &self.gold_coref {
            if pred_coref.get(idx) != Some(gold_ante) {
                loss += 1.0;
            }
        }

        if n > 0.0 {
            loss / n
        } else {
            0.0
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct JointWeights {
    // Unary NER
    pub type_bias: HashMap<String, f64>,
    pub context_weight: f64,
    // Unary coref
    pub new_cluster_bias: f64,
    pub distance_decay: f64,
    pub string_match: f64,
    // Pairwise coref-NER
    pub type_match: f64,
    pub type_mismatch: f64,
}

#[derive(Debug, Clone, Default)]
struct AdaGradState {
    sum_sq_grad: f64,
}

impl AdaGradState {
    fn update(&mut self, grad: f64, lr: f64, epsilon: f64) -> f64 {
        self.sum_sq_grad += grad * grad;
        let adjusted_lr = lr / (self.sum_sq_grad.sqrt() + epsilon);
        -adjusted_lr * grad
    }
}

#[derive(Debug, Clone, Default)]
struct Gradients {
    type_bias: HashMap<String, f64>,
    context_weight: f64,
    new_cluster_bias: f64,
    distance_decay: f64,
    string_match: f64,
    type_match: f64,
    type_mismatch: f64,
}

impl Gradients {
    fn clip(&mut self, threshold: f64) {
        let clip = |x: &mut f64| *x = x.clamp(-threshold, threshold);
        for v in self.type_bias.values_mut() {
            clip(v);
        }
        clip(&mut self.context_weight);
        clip(&mut self.new_cluster_bias);
        clip(&mut self.distance_decay);
        clip(&mut self.string_match);
        clip(&mut self.type_match);
        clip(&mut self.type_mismatch);
    }
}

pub struct Trainer {
    config: TrainingConfig,
    weights: JointWeights,
    examples: Vec<TrainingExample>,
    // AdaGrad states
    type_bias_states: HashMap<String, AdaGradState>,
    #[allow(dead_code)]
    context_weight_state: AdaGradState,
    new_cluster_bias_state: AdaGradState,
    distance_decay_state: AdaGradState,
    string_match_state: AdaGradState,
    type_match_state: AdaGradState,
    type_mismatch_state: AdaGradState,
}

impl Trainer {
    pub fn new(config: TrainingConfig) -> Self {
        Self {
            config,
            weights: JointWeights::default(),
            examples: Vec::new(),
            type_bias_states: HashMap::new(),
            context_weight_state: AdaGradState::default(),
            new_cluster_bias_state: AdaGradState::default(),
            distance_decay_state: AdaGradState::default(),
            string_match_state: AdaGradState::default(),
            type_match_state: AdaGradState::default(),
            type_mismatch_state: AdaGradState::default(),
        }
    }

    pub fn add_example(&mut self, example: TrainingExample) {
        self.examples.push(example);
    }

    pub fn train(&mut self) -> Vec<f64> {
        let mut losses = Vec::new();

        for epoch in 0..self.config.epochs {
            let mut epoch_loss = 0.0;

            for idx in 0..self.examples.len() {
                let (loss, grads) = self.compute_loss_and_gradients(idx);
                epoch_loss += loss;
                self.apply_updates(&grads);
            }

            let avg_loss = epoch_loss / self.examples.len().max(1) as f64;
            losses.push(avg_loss);

            if epoch % 5 == 0 {
                println!("Epoch {}: loss = {:.4}", epoch, avg_loss);
                self.print_weights();
            }
        }

        losses
    }

    fn compute_loss_and_gradients(&self, example_idx: usize) -> (f64, Gradients) {
        let example = &self.examples[example_idx];
        let mut grads = Gradients::default();

        // Compute gold score
        let gold_score = self.compute_score(example, &example.gold_ner, &example.gold_coref);

        // Compute predicted assignment (greedy decode with cost augmentation)
        let (pred_ner, pred_coref) = self.decode_with_cost(example);
        let pred_score = self.compute_score(example, &pred_ner, &pred_coref);

        let cost = example.hamming_loss(&pred_ner, &pred_coref, &HashMap::new());
        let margin = pred_score + self.config.cost_weight * cost - gold_score;
        let loss = margin.max(0.0);

        if loss > 0.0 {
            // Gradient: features(pred) - features(gold)
            self.accumulate_features(&mut grads, example, &pred_ner, &pred_coref, 1.0);
            self.accumulate_features(
                &mut grads,
                example,
                &example.gold_ner,
                &example.gold_coref,
                -1.0,
            );
        }

        (loss, grads)
    }

    fn compute_score(
        &self,
        example: &TrainingExample,
        ner: &HashMap<usize, EntityType>,
        coref: &HashMap<usize, Option<usize>>,
    ) -> f64 {
        let mut score = 0.0;

        // Unary NER
        for entity_type in ner.values() {
            if let Some(&bias) = self.weights.type_bias.get(entity_type.as_label()) {
                score += bias;
            }
        }

        // Unary coref
        for (idx, ante) in coref {
            if ante.is_none() {
                score += self.weights.new_cluster_bias;
            } else if let Some(ante_idx) = ante {
                let dist = (*idx as f64 - *ante_idx as f64).abs();
                score -= self.weights.distance_decay * (dist + 1.0).ln();

                if *idx < example.mentions.len() && *ante_idx < example.mentions.len() {
                    let m_i = &example.mentions[*idx];
                    let m_j = &example.mentions[*ante_idx];
                    if m_i.text.to_lowercase() == m_j.text.to_lowercase() {
                        score += self.weights.string_match;
                    }
                }
            }
        }

        // Pairwise coref-NER
        for (idx, ante) in coref {
            if let Some(ante_idx) = ante {
                if let (Some(type_i), Some(type_j)) = (ner.get(idx), ner.get(ante_idx)) {
                    if type_i == type_j {
                        score += self.weights.type_match;
                    } else {
                        score += self.weights.type_mismatch;
                    }
                }
            }
        }

        score
    }

    fn decode_with_cost(
        &self,
        example: &TrainingExample,
    ) -> (HashMap<usize, EntityType>, HashMap<usize, Option<usize>>) {
        let mut pred_ner = HashMap::new();
        let mut pred_coref = HashMap::new();

        for (idx, mention) in example.mentions.iter().enumerate() {
            // NER: use mention's entity type
            if let Some(t) = mention.entity_type() {
                pred_ner.insert(idx, t);
            }

            // Coref: greedy antecedent selection
            let mut best_ante: Option<usize> = None;
            let mut best_score = self.weights.new_cluster_bias;

            for ante_idx in 0..idx {
                let mut ante_score = 0.0;

                let dist = (idx - ante_idx) as f64;
                ante_score -= self.weights.distance_decay * (dist + 1.0).ln();

                if mention.text.to_lowercase() == example.mentions[ante_idx].text.to_lowercase() {
                    ante_score += self.weights.string_match;
                }

                if let (Some(type_i), Some(type_j)) = (pred_ner.get(&idx), pred_ner.get(&ante_idx))
                {
                    if type_i == type_j {
                        ante_score += self.weights.type_match;
                    } else {
                        ante_score += self.weights.type_mismatch;
                    }
                }

                // Cost augmentation
                if let Some(gold_ante) = example.gold_coref.get(&idx) {
                    if gold_ante != &Some(ante_idx) {
                        ante_score += self.config.cost_weight;
                    }
                }

                if ante_score > best_score {
                    best_score = ante_score;
                    best_ante = Some(ante_idx);
                }
            }
            pred_coref.insert(idx, best_ante);
        }

        (pred_ner, pred_coref)
    }

    fn accumulate_features(
        &self,
        grads: &mut Gradients,
        example: &TrainingExample,
        ner: &HashMap<usize, EntityType>,
        coref: &HashMap<usize, Option<usize>>,
        scale: f64,
    ) {
        // Unary NER
        for entity_type in ner.values() {
            *grads
                .type_bias
                .entry(entity_type.as_label().to_string())
                .or_insert(0.0) += scale;
        }

        // Unary coref
        for (idx, ante) in coref {
            if ante.is_none() {
                grads.new_cluster_bias += scale;
            } else if let Some(ante_idx) = ante {
                let dist = (*idx as f64 - *ante_idx as f64).abs();
                grads.distance_decay -= scale * (dist + 1.0).ln();

                if *idx < example.mentions.len() && *ante_idx < example.mentions.len() {
                    let m_i = &example.mentions[*idx];
                    let m_j = &example.mentions[*ante_idx];
                    if m_i.text.to_lowercase() == m_j.text.to_lowercase() {
                        grads.string_match += scale;
                    }
                }
            }
        }

        // Pairwise coref-NER
        for (idx, ante) in coref {
            if let Some(ante_idx) = ante {
                if let (Some(type_i), Some(type_j)) = (ner.get(idx), ner.get(ante_idx)) {
                    if type_i == type_j {
                        grads.type_match += scale;
                    } else {
                        grads.type_mismatch += scale;
                    }
                }
            }
        }
    }

    fn apply_updates(&mut self, grads: &Gradients) {
        let lr = self.config.learning_rate;
        let eps = self.config.epsilon;

        let mut clipped_grads = grads.clone();
        clipped_grads.clip(self.config.grad_clip);

        for (type_name, &grad) in &clipped_grads.type_bias {
            let state = self.type_bias_states.entry(type_name.clone()).or_default();
            let delta = state.update(grad, lr, eps);
            *self
                .weights
                .type_bias
                .entry(type_name.clone())
                .or_insert(0.0) += delta;
        }

        let delta = self
            .new_cluster_bias_state
            .update(clipped_grads.new_cluster_bias, lr, eps);
        self.weights.new_cluster_bias += delta;

        let delta = self
            .distance_decay_state
            .update(clipped_grads.distance_decay, lr, eps);
        self.weights.distance_decay += delta;

        let delta = self
            .string_match_state
            .update(clipped_grads.string_match, lr, eps);
        self.weights.string_match += delta;

        let delta = self
            .type_match_state
            .update(clipped_grads.type_match, lr, eps);
        self.weights.type_match += delta;

        let delta = self
            .type_mismatch_state
            .update(clipped_grads.type_mismatch, lr, eps);
        self.weights.type_mismatch += delta;
    }

    fn print_weights(&self) {
        println!("  Weights:");
        println!("    new_cluster_bias: {:.4}", self.weights.new_cluster_bias);
        println!("    distance_decay: {:.4}", self.weights.distance_decay);
        println!("    string_match: {:.4}", self.weights.string_match);
        println!("    type_match: {:.4}", self.weights.type_match);
        println!("    type_mismatch: {:.4}", self.weights.type_mismatch);
        for (t, w) in &self.weights.type_bias {
            println!("    type_bias[{}]: {:.4}", t, w);
        }
    }

    pub fn get_weights(&self) -> &JointWeights {
        &self.weights
    }
}

// ===========================================================================
// Synthetic Data Generation
// ===========================================================================

fn create_synthetic_examples() -> Vec<TrainingExample> {
    let mut examples = Vec::new();

    // Example 1: Simple coreference with consistent types
    // "Barack Obama met Angela Merkel. He greeted her."
    {
        let mentions = vec![
            JointMention::new(0, "Barack Obama", 0, 12, EntityType::Person),
            JointMention::new(1, "Angela Merkel", 17, 30, EntityType::Person),
            JointMention::new(2, "He", 32, 34, EntityType::Person),
            JointMention::new(3, "her", 43, 46, EntityType::Person),
        ];

        let mut gold_ner = HashMap::new();
        gold_ner.insert(0, EntityType::Person);
        gold_ner.insert(1, EntityType::Person);
        gold_ner.insert(2, EntityType::Person);
        gold_ner.insert(3, EntityType::Person);

        let mut gold_coref = HashMap::new();
        gold_coref.insert(0, None); // New cluster
        gold_coref.insert(1, None); // New cluster
        gold_coref.insert(2, Some(0)); // He -> Barack Obama
        gold_coref.insert(3, Some(1)); // her -> Angela Merkel

        examples.push(TrainingExample {
            text: "Barack Obama met Angela Merkel. He greeted her.".to_string(),
            mentions,
            gold_ner,
            gold_coref,
            gold_links: HashMap::new(),
        });
    }

    // Example 2: Organization coreference
    // "Apple announced new products. The company reported strong sales."
    {
        let mentions = vec![
            JointMention::new(0, "Apple", 0, 5, EntityType::Organization),
            JointMention::new(1, "The company", 28, 39, EntityType::Organization),
        ];

        let mut gold_ner = HashMap::new();
        gold_ner.insert(0, EntityType::Organization);
        gold_ner.insert(1, EntityType::Organization);

        let mut gold_coref = HashMap::new();
        gold_coref.insert(0, None);
        gold_coref.insert(1, Some(0)); // The company -> Apple

        examples.push(TrainingExample {
            text: "Apple announced new products. The company reported strong sales.".to_string(),
            mentions,
            gold_ner,
            gold_coref,
            gold_links: HashMap::new(),
        });
    }

    // Example 3: Type consistency across coreference
    // "Microsoft acquired GitHub. The tech giant expanded."
    {
        let mentions = vec![
            JointMention::new(0, "Microsoft", 0, 9, EntityType::Organization),
            JointMention::new(1, "GitHub", 19, 25, EntityType::Organization),
            JointMention::new(2, "The tech giant", 27, 41, EntityType::Organization),
        ];

        let mut gold_ner = HashMap::new();
        gold_ner.insert(0, EntityType::Organization);
        gold_ner.insert(1, EntityType::Organization);
        gold_ner.insert(2, EntityType::Organization);

        let mut gold_coref = HashMap::new();
        gold_coref.insert(0, None);
        gold_coref.insert(1, None);
        gold_coref.insert(2, Some(0)); // The tech giant -> Microsoft

        examples.push(TrainingExample {
            text: "Microsoft acquired GitHub. The tech giant expanded.".to_string(),
            mentions,
            gold_ner,
            gold_coref,
            gold_links: HashMap::new(),
        });
    }

    // Example 4: Location reference
    // "Paris hosted the Olympics. The city was decorated."
    {
        let mentions = vec![
            JointMention::new(0, "Paris", 0, 5, EntityType::Location),
            JointMention::new(1, "The city", 26, 34, EntityType::Location),
        ];

        let mut gold_ner = HashMap::new();
        gold_ner.insert(0, EntityType::Location);
        gold_ner.insert(1, EntityType::Location);

        let mut gold_coref = HashMap::new();
        gold_coref.insert(0, None);
        gold_coref.insert(1, Some(0));

        examples.push(TrainingExample {
            text: "Paris hosted the Olympics. The city was decorated.".to_string(),
            mentions,
            gold_ner,
            gold_coref,
            gold_links: HashMap::new(),
        });
    }

    // Example 5: String match coreference
    // "Obama spoke. Obama then left."
    {
        let mentions = vec![
            JointMention::new(0, "Obama", 0, 5, EntityType::Person),
            JointMention::new(1, "Obama", 13, 18, EntityType::Person),
        ];

        let mut gold_ner = HashMap::new();
        gold_ner.insert(0, EntityType::Person);
        gold_ner.insert(1, EntityType::Person);

        let mut gold_coref = HashMap::new();
        gold_coref.insert(0, None);
        gold_coref.insert(1, Some(0)); // Obama -> Obama

        examples.push(TrainingExample {
            text: "Obama spoke. Obama then left.".to_string(),
            mentions,
            gold_ner,
            gold_coref,
            gold_links: HashMap::new(),
        });
    }

    examples
}

// ===========================================================================
// Main
// ===========================================================================

fn main() {
    println!("Joint Model Training Experiment");
    println!("================================\n");

    // Create training examples
    let examples = create_synthetic_examples();
    println!("Created {} synthetic training examples\n", examples.len());

    // Initialize trainer
    let config = TrainingConfig {
        epochs: 30,
        learning_rate: 0.1,
        batch_size: 1,
        l2_lambda: 0.001,
        cost_weight: 1.0,
        grad_clip: 5.0,
        ..Default::default()
    };

    let mut trainer = Trainer::new(config);
    for example in examples {
        trainer.add_example(example);
    }

    // Train
    println!("Starting training...\n");
    let losses = trainer.train();

    // Report
    println!("\nTraining complete!");
    println!("Final loss: {:.4}", losses.last().unwrap_or(&0.0));
    println!("\nLearned weights:");
    let weights = trainer.get_weights();
    println!("  new_cluster_bias: {:.4}", weights.new_cluster_bias);
    println!("  distance_decay: {:.4}", weights.distance_decay);
    println!("  string_match: {:.4}", weights.string_match);
    println!("  type_match: {:.4}", weights.type_match);
    println!("  type_mismatch: {:.4}", weights.type_mismatch);

    // Verify learned patterns
    println!("\nExpected patterns:");
    println!("  - string_match should be positive (exact string matches are coreferent)");
    println!("  - type_match should be positive (same types are more likely coreferent)");
    println!("  - type_mismatch should be negative (different types less likely)");
    println!("  - distance_decay should be small/positive (closer mentions preferred)");

    let string_match_ok = weights.string_match > 0.0;
    let type_match_ok = weights.type_match > weights.type_mismatch;

    println!("\nValidation:");
    println!(
        "  string_match > 0: {} (actual: {:.4})",
        if string_match_ok { "PASS" } else { "FAIL" },
        weights.string_match
    );
    println!(
        "  type_match > type_mismatch: {} (actual: {:.4} vs {:.4})",
        if type_match_ok { "PASS" } else { "FAIL" },
        weights.type_match,
        weights.type_mismatch
    );

    // Plot loss curve (text-based)
    println!("\nLoss curve:");
    let max_loss = losses.iter().cloned().fold(0.0f64, f64::max);
    let scale = if max_loss > 0.0 { 50.0 / max_loss } else { 1.0 };
    for (i, &loss) in losses.iter().enumerate() {
        let bar_len = (loss * scale) as usize;
        let bar: String = "#".repeat(bar_len);
        println!("  {:2}: {:>6.4} |{}", i, loss, bar);
    }
}
