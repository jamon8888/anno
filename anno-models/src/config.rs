//! Model-specific configurations.

/// Configuration for GLiNER models.
#[derive(Debug, Clone)]
pub struct GLiNERConfig {
    /// Model identifier (HuggingFace repo or local path).
    pub model_id: String,
    /// Maximum sequence length.
    pub max_length: usize,
    /// Confidence threshold for entity extraction.
    pub threshold: f32,
    /// Whether to use flat NER (no nested entities).
    pub flat_ner: bool,
    /// Whether to use multi-label classification.
    pub multi_label: bool,
    /// Maximum width for span candidates.
    pub max_width: usize,
}

impl Default for GLiNERConfig {
    fn default() -> Self {
        Self {
            model_id: "urchade/gliner_base".into(),
            max_length: 512,
            threshold: 0.5,
            flat_ner: true,
            multi_label: false,
            max_width: 12,
        }
    }
}

impl GLiNERConfig {
    /// Create config for GLiNER base model.
    pub fn base() -> Self {
        Self::default()
    }

    /// Create config for GLiNER large model.
    pub fn large() -> Self {
        Self {
            model_id: "urchade/gliner_large-v2.1".into(),
            ..Self::default()
        }
    }

    /// Create config for GLiNER multi model (multilingual).
    pub fn multi() -> Self {
        Self {
            model_id: "urchade/gliner_multi-v2.1".into(),
            ..Self::default()
        }
    }

    /// Set the confidence threshold.
    pub fn with_threshold(mut self, threshold: f32) -> Self {
        self.threshold = threshold;
        self
    }

    /// Set the maximum sequence length.
    pub fn with_max_length(mut self, max_length: usize) -> Self {
        self.max_length = max_length;
        self
    }
}

/// Configuration for NuNER models.
#[derive(Debug, Clone)]
pub struct NuNERConfig {
    /// Model identifier.
    pub model_id: String,
    /// Maximum sequence length.
    pub max_length: usize,
    /// Confidence threshold.
    pub threshold: f32,
    /// Whether to use the v2 architecture.
    pub v2: bool,
    /// Negation handling (for "not a person" patterns).
    pub handle_negation: bool,
}

impl Default for NuNERConfig {
    fn default() -> Self {
        Self {
            model_id: "numind/NuNER_Zero".into(),
            max_length: 512,
            threshold: 0.5,
            v2: false,
            handle_negation: true,
        }
    }
}

impl NuNERConfig {
    /// Create config for NuNER Zero (zero-shot).
    pub fn zero() -> Self {
        Self::default()
    }

    /// Create config for NuNER v2.
    pub fn v2() -> Self {
        Self {
            model_id: "numind/NuNER_Zero_v2.0".into(),
            v2: true,
            ..Self::default()
        }
    }

    /// Set the confidence threshold.
    pub fn with_threshold(mut self, threshold: f32) -> Self {
        self.threshold = threshold;
        self
    }
}

/// Configuration for CRF-based models.
#[derive(Debug, Clone)]
pub struct CRFConfig {
    /// Whether to use BIO or BIOES tagging.
    pub bioes: bool,
    /// L2 regularization strength.
    pub l2_reg: f32,
    /// Maximum iterations for training.
    pub max_iter: usize,
}

impl Default for CRFConfig {
    fn default() -> Self {
        Self {
            bioes: false,
            l2_reg: 0.1,
            max_iter: 100,
        }
    }
}

/// Configuration for BiLSTM-CRF models.
#[derive(Debug, Clone)]
pub struct BiLSTMCRFConfig {
    /// Hidden size of LSTM layers.
    pub hidden_size: usize,
    /// Number of LSTM layers.
    pub num_layers: usize,
    /// Dropout rate.
    pub dropout: f32,
    /// CRF configuration.
    pub crf: CRFConfig,
}

impl Default for BiLSTMCRFConfig {
    fn default() -> Self {
        Self {
            hidden_size: 256,
            num_layers: 2,
            dropout: 0.3,
            crf: CRFConfig::default(),
        }
    }
}
