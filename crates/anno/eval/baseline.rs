//! Baseline performance expectations for datasets.
//!
//! Provides structured baseline performance data that includes:
//! - Model architecture (BERT, BioBERT, mBERT, etc.)
//! - Task type (NER, coreference, relation extraction)
//! - Dataset split (train/dev/test)
//! - Evaluation metric (F1, precision, recall, CoNLL F1)
//! - Citation/reference
//!
//! # Motivation
//!
//! Expected F1 scores are meaningless without context. A score of 92.5% could be:
//! - BERT-base on CoNLL-2003 test set (NER)
//! - BioBERT on BC5CDR dev set (biomedical NER)
//! - mBERT on MultiCoNER test set (multilingual NER)
//!
//! This module provides structured baseline data that captures all necessary context.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Baseline performance expectation for a dataset.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BaselinePerformance {
    /// F1 score (as percentage, e.g., 92.5 for 92.5%)
    pub f1: f32,
    /// Precision (as percentage, if available)
    pub precision: Option<f32>,
    /// Recall (as percentage, if available)
    pub recall: Option<f32>,
    /// Model architecture used (e.g., "BERT-base", "BioBERT", "mBERT")
    pub model: String,
    /// Task type (e.g., "ner", "coref", "re")
    pub task: String,
    /// Dataset split (e.g., "test", "dev", "train")
    pub split: String,
    /// Evaluation metric name (e.g., "F1", "CoNLL F1", "Macro F1")
    pub metric: String,
    /// Citation/reference (e.g., "Devlin et al. 2018", "Lee et al. 2020")
    pub citation: Option<String>,
    /// Additional notes (e.g., "average across languages", "fine-tuned")
    pub notes: Option<String>,
}

impl BaselinePerformance {
    /// Create a new baseline performance record.
    pub fn new(
        f1: f32,
        model: impl Into<String>,
        task: impl Into<String>,
        split: impl Into<String>,
    ) -> Self {
        Self {
            f1,
            precision: None,
            recall: None,
            model: model.into(),
            task: task.into(),
            split: split.into(),
            metric: "F1".to_string(),
            citation: None,
            notes: None,
        }
    }

    /// Set precision and recall.
    pub fn with_prf(mut self, precision: f32, recall: f32) -> Self {
        self.precision = Some(precision);
        self.recall = Some(recall);
        self
    }

    /// Set metric name.
    pub fn with_metric(mut self, metric: impl Into<String>) -> Self {
        self.metric = metric.into();
        self
    }

    /// Set citation.
    pub fn with_citation(mut self, citation: impl Into<String>) -> Self {
        self.citation = Some(citation.into());
        self
    }

    /// Set notes.
    pub fn with_notes(mut self, notes: impl Into<String>) -> Self {
        self.notes = Some(notes.into());
        self
    }

    /// Get expected F1 for zero-shot models (typically 5-15% lower).
    pub fn zero_shot_adjustment(&self) -> Self {
        let adjusted_f1 = (self.f1 - 10.0).max(30.0);
        let mut adjusted = self.clone();
        adjusted.f1 = adjusted_f1;
        adjusted.notes = Some(format!("Zero-shot adjustment (original: {:.1}%)", self.f1));
        adjusted
    }
}

impl fmt::Display for BaselinePerformance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:.1}% F1 ({}, {}, {})",
            self.f1, self.model, self.task, self.split
        )?;
        if let Some(ref citation) = self.citation {
            write!(f, " [{}]", citation)?;
        }
        Ok(())
    }
}

/// Helper to create common baseline patterns.
pub mod common {
    use super::BaselinePerformance;

    /// BERT-base baseline for NER (Devlin et al. 2018).
    pub fn bert_base_ner(split: &str, f1: f32) -> BaselinePerformance {
        BaselinePerformance::new(f1, "BERT-base", "ner", split)
            .with_citation("Devlin et al. 2018")
            .with_metric("F1")
    }

    /// BioBERT baseline for biomedical NER (Lee et al. 2020).
    pub fn biobert_ner(split: &str, f1: f32) -> BaselinePerformance {
        BaselinePerformance::new(f1, "BioBERT", "ner", split)
            .with_citation("Lee et al. 2020")
            .with_metric("F1")
    }

    /// mBERT baseline for multilingual NER.
    pub fn mbert_ner(split: &str, f1: f32) -> BaselinePerformance {
        BaselinePerformance::new(f1, "mBERT", "ner", split).with_metric("F1")
    }

    /// BERT baseline for coreference resolution.
    pub fn bert_coref(split: &str, f1: f32) -> BaselinePerformance {
        BaselinePerformance::new(f1, "BERT-base", "coref", split).with_metric("CoNLL F1")
    }

    /// BERT baseline for relation extraction.
    pub fn bert_re(split: &str, f1: f32) -> BaselinePerformance {
        BaselinePerformance::new(f1, "BERT-base", "re", split).with_metric("F1")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_baseline_display() {
        let baseline = BaselinePerformance::new(92.5, "BERT-base", "ner", "test")
            .with_citation("Devlin et al. 2018");
        let display = format!("{}", baseline);
        assert!(display.contains("92.5"));
        assert!(display.contains("BERT-base"));
        assert!(display.contains("Devlin"));
    }

    #[test]
    fn test_zero_shot_adjustment() {
        let baseline = BaselinePerformance::new(92.5, "BERT-base", "ner", "test");
        let adjusted = baseline.zero_shot_adjustment();
        assert!((adjusted.f1 - 82.5).abs() < 0.1);
    }
}
