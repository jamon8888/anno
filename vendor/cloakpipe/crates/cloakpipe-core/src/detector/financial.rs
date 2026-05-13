//! Layer 2: Financial intelligence — currency amounts, percentages, fiscal dates.

use crate::{DetectedEntity, EntityCategory, DetectionSource, config::DetectionConfig};
use anyhow::Result;
use regex::Regex;

pub struct FinancialDetector {
    amount_regex: Regex,
    amount_inr_regex: Regex,
    percent_regex: Regex,
    fiscal_regex: Regex,
    date_regex: Regex,
}

impl FinancialDetector {
    pub fn new(_config: &DetectionConfig) -> Result<Self> {
        Ok(Self {
            // Matches: $1.2M, ₹3.4L Cr, €500K, £2.1B, $12,345.67, etc.
            amount_regex: Regex::new(
                r"(?:[$€£₹¥])\s*[\d,]+(?:\.\d+)?\s*(?:[KMBT]|[Ll](?:akh)?|[Cc]r(?:ore)?)?(?:\s+(?:million|billion|trillion|crore|lakh))?"
            )?,
            // Matches: INR 18,00,000 / USD 1,234.56 / EUR 500
            amount_inr_regex: Regex::new(
                r"\b(?:INR|USD|EUR|GBP|JPY)\s+[\d,]+(?:\.\d+)?\s*(?:[KMBT]|[Ll](?:akh)?|[Cc]r(?:ore)?)?"
            )?,
            // Matches: 12%, 3.5%, -2.1%
            percent_regex: Regex::new(r"-?\d+(?:\.\d+)?%")?,
            // Matches: Q1 2025, FY2024, H1 2025, Q3 FY25
            fiscal_regex: Regex::new(r"(?:Q[1-4]|H[12]|FY)\s*(?:20)?\d{2}(?:-\d{2})?")?,
            // Matches: March 31, 2026 / January 1, 2025 / 15/01/2025 / 2025-03-31
            date_regex: Regex::new(
                r"(?:January|February|March|April|May|June|July|August|September|October|November|December)\s+\d{1,2},?\s+\d{4}|\b\d{1,2}/\d{1,2}/\d{4}\b|\b\d{4}-\d{2}-\d{2}\b"
            )?,
        })
    }

    pub fn detect(&self, text: &str) -> Result<Vec<DetectedEntity>> {
        let mut entities = Vec::new();

        for mat in self.amount_regex.find_iter(text) {
            entities.push(DetectedEntity {
                original: mat.as_str().to_string(),
                start: mat.start(),
                end: mat.end(),
                category: EntityCategory::Amount,
                confidence: 1.0,
                source: DetectionSource::Financial,
            });
        }

        for mat in self.amount_inr_regex.find_iter(text) {
            entities.push(DetectedEntity {
                original: mat.as_str().to_string(),
                start: mat.start(),
                end: mat.end(),
                category: EntityCategory::Amount,
                confidence: 1.0,
                source: DetectionSource::Financial,
            });
        }

        for mat in self.percent_regex.find_iter(text) {
            entities.push(DetectedEntity {
                original: mat.as_str().to_string(),
                start: mat.start(),
                end: mat.end(),
                category: EntityCategory::Percentage,
                confidence: 1.0,
                source: DetectionSource::Financial,
            });
        }

        for mat in self.fiscal_regex.find_iter(text) {
            entities.push(DetectedEntity {
                original: mat.as_str().to_string(),
                start: mat.start(),
                end: mat.end(),
                category: EntityCategory::Date,
                confidence: 1.0,
                source: DetectionSource::Financial,
            });
        }

        for mat in self.date_regex.find_iter(text) {
            entities.push(DetectedEntity {
                original: mat.as_str().to_string(),
                start: mat.start(),
                end: mat.end(),
                category: EntityCategory::Date,
                confidence: 1.0,
                source: DetectionSource::Financial,
            });
        }

        Ok(entities)
    }
}
