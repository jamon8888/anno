//! Anthropic request/response privacy transforms.

use crate::{Error, Result};
use cloakpipe_core::{
    rehydrator::Rehydrator, replacer::Replacer, vault::Vault, DetectedEntity, DetectionSource,
    EntityCategory, PseudonymizedText,
};
use regex::Regex;
use serde_json::Value;

/// Counts emitted by a privacy transform.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PrivacyReport {
    /// Number of values pseudonymized in the request.
    pub pseudonymized_values: usize,
    /// Number of entities replaced.
    pub entities: usize,
    /// Number of known tokens rehydrated in the response.
    pub rehydrated_tokens: usize,
}

/// Stateful privacy engine. The vault owns cleartext mappings and never leaves
/// this boundary.
pub struct PrivacyEngine {
    vault: Vault,
    email: Regex,
    phone_fr: Regex,
    iban_fr: Regex,
    siret: Regex,
    person_fr: Regex,
}

impl Default for PrivacyEngine {
    fn default() -> Self {
        Self::new(Vault::ephemeral())
    }
}

impl PrivacyEngine {
    /// Build a privacy engine from a vault.
    #[must_use]
    pub fn new(vault: Vault) -> Self {
        Self {
            vault,
            email: Regex::new(r"(?i)\b[a-z0-9._%+-]+@[a-z0-9.-]+\.[a-z]{2,}\b")
                .expect("email regex compiles"),
            phone_fr: Regex::new(r"\b(?:\+33\s?|0)[1-9](?:[\s.-]?\d{2}){4}\b")
                .expect("phone regex compiles"),
            iban_fr: Regex::new(r"\bFR\d{2}(?:\s?[0-9A-Z]{4}){5}\s?[0-9A-Z]{3}\b")
                .expect("iban regex compiles"),
            siret: Regex::new(r"\b\d{14}\b").expect("siret regex compiles"),
            person_fr: Regex::new(r"\b[A-ZÀ-ÖØ-Þ][a-zà-öø-ÿ]+[- ][A-ZÀ-ÖØ-Þ][a-zà-öø-ÿ]+\b")
                .expect("person regex compiles"),
        }
    }

    /// Pseudonymize all v0.3-supported request text fields in place.
    pub fn pseudonymize_request(&mut self, request: &mut Value) -> Result<PrivacyReport> {
        if request
            .get("stream")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            return Err(Error::UnsupportedFeature(
                "stream=true is deferred to anno-privacy-gateway v0.4".to_string(),
            ));
        }

        reject_blocks(request)?;

        let mut report = PrivacyReport::default();
        if let Some(system) = request.get_mut("system") {
            self.transform_content_value(system, &mut report)?;
        }

        if let Some(messages) = request.get_mut("messages").and_then(Value::as_array_mut) {
            for message in messages {
                if let Some(content) = message.get_mut("content") {
                    self.transform_content_value(content, &mut report)?;
                }
            }
        }

        Ok(report)
    }

    /// Rehydrate known pseudo-tokens in assistant response text fields.
    pub fn rehydrate_response(&self, response: &mut Value) -> Result<PrivacyReport> {
        let mut report = PrivacyReport::default();
        if let Some(content) = response.get_mut("content") {
            self.rehydrate_content_value(content, &mut report)?;
        }
        Ok(report)
    }

    fn transform_content_value(
        &mut self,
        content: &mut Value,
        report: &mut PrivacyReport,
    ) -> Result<()> {
        match content {
            Value::String(text) => {
                let pseudo = self.pseudonymize_text(text)?;
                if pseudo.text != *text {
                    report.pseudonymized_values += 1;
                    report.entities += pseudo.entities.len();
                    *text = pseudo.text;
                }
            }
            Value::Array(blocks) => {
                for block in blocks {
                    self.transform_content_block(block, report)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn transform_content_block(
        &mut self,
        block: &mut Value,
        report: &mut PrivacyReport,
    ) -> Result<()> {
        match block.get("type").and_then(Value::as_str) {
            Some("text") | Some("thinking") => {
                if let Some(text) = block.get_mut("text") {
                    self.transform_content_value(text, report)?;
                }
            }
            Some("tool_result") => {
                if let Some(content) = block.get_mut("content") {
                    self.transform_content_value(content, report)?;
                }
            }
            Some("tool_use") => {
                if let Some(input) = block.get_mut("input") {
                    self.transform_json_string_leaves(input, report)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn transform_json_string_leaves(
        &mut self,
        value: &mut Value,
        report: &mut PrivacyReport,
    ) -> Result<()> {
        match value {
            Value::String(_) => self.transform_content_value(value, report),
            Value::Array(items) => {
                for item in items {
                    self.transform_json_string_leaves(item, report)?;
                }
                Ok(())
            }
            Value::Object(map) => {
                for value in map.values_mut() {
                    self.transform_json_string_leaves(value, report)?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn rehydrate_content_value(
        &self,
        content: &mut Value,
        report: &mut PrivacyReport,
    ) -> Result<()> {
        match content {
            Value::String(text) => {
                let rehydrated = Rehydrator::rehydrate(text, &self.vault)
                    .map_err(|e| Error::Privacy(e.to_string()))?;
                report.rehydrated_tokens += rehydrated.rehydrated_count;
                *text = rehydrated.text;
            }
            Value::Array(blocks) => {
                for block in blocks {
                    if let Some(text) = block.get_mut("text") {
                        self.rehydrate_content_value(text, report)?;
                    }
                    if let Some(input) = block.get_mut("input") {
                        self.rehydrate_json_string_leaves(input, report)?;
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn rehydrate_json_string_leaves(
        &self,
        value: &mut Value,
        report: &mut PrivacyReport,
    ) -> Result<()> {
        match value {
            Value::String(_) => self.rehydrate_content_value(value, report),
            Value::Array(items) => {
                for item in items {
                    self.rehydrate_json_string_leaves(item, report)?;
                }
                Ok(())
            }
            Value::Object(map) => {
                for value in map.values_mut() {
                    self.rehydrate_json_string_leaves(value, report)?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn pseudonymize_text(&mut self, text: &str) -> Result<PseudonymizedText> {
        let entities = self.detect(text);
        Replacer::pseudonymize(text, &entities, &mut self.vault)
            .map_err(|e| Error::Privacy(e.to_string()))
    }

    fn detect(&self, text: &str) -> Vec<DetectedEntity> {
        let mut entities = Vec::new();
        self.push_matches(text, &self.email, EntityCategory::Email, &mut entities);
        self.push_matches(
            text,
            &self.phone_fr,
            EntityCategory::PhoneNumber,
            &mut entities,
        );
        self.push_matches(
            text,
            &self.iban_fr,
            EntityCategory::Custom("IBAN".into()),
            &mut entities,
        );
        self.push_matches(
            text,
            &self.siret,
            EntityCategory::Custom("SIRET".into()),
            &mut entities,
        );
        self.push_matches(text, &self.person_fr, EntityCategory::Person, &mut entities);

        entities.sort_by_key(|e| e.start);
        let mut deduped: Vec<DetectedEntity> = Vec::new();
        for entity in entities {
            if deduped.last().is_some_and(|last| entity.start < last.end) {
                continue;
            }
            deduped.push(entity);
        }
        deduped
    }

    fn push_matches(
        &self,
        text: &str,
        regex: &Regex,
        category: EntityCategory,
        entities: &mut Vec<DetectedEntity>,
    ) {
        for mat in regex.find_iter(text) {
            entities.push(DetectedEntity {
                original: mat.as_str().to_string(),
                start: mat.start(),
                end: mat.end(),
                category: category.clone(),
                confidence: 1.0,
                source: DetectionSource::Pattern,
            });
        }
    }
}

fn reject_blocks(value: &Value) -> Result<()> {
    match value {
        Value::Object(map) => {
            if let Some(block_type) = map.get("type").and_then(Value::as_str) {
                match block_type {
                    "document" => {
                        return Err(Error::UnsupportedFeature(
                            "native document blocks are deferred to v0.5".to_string(),
                        ));
                    }
                    "image" => {
                        return Err(Error::UnsupportedFeature(
                            "image blocks are rejected in strict v0.3 mode".to_string(),
                        ));
                    }
                    _ => {}
                }
            }
            for value in map.values() {
                reject_blocks(value)?;
            }
        }
        Value::Array(items) => {
            for item in items {
                reject_blocks(item)?;
            }
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn rejects_streaming() {
        let mut engine = PrivacyEngine::default();
        let mut request = json!({
            "model": "claude",
            "stream": true,
            "messages": [{"role": "user", "content": "Bonjour"}]
        });

        let err = engine.pseudonymize_request(&mut request).unwrap_err();
        assert!(matches!(err, Error::UnsupportedFeature(_)));
    }

    #[test]
    fn rejects_document_blocks() {
        let mut engine = PrivacyEngine::default();
        let mut request = json!({
            "messages": [{
                "role": "user",
                "content": [{
                    "type": "document",
                    "source": {"type": "file", "file_id": "file_123"}
                }]
            }]
        });

        let err = engine.pseudonymize_request(&mut request).unwrap_err();
        assert!(err.to_string().contains("document"));
    }

    #[test]
    fn pseudonymizes_system_messages_and_tool_inputs() {
        let mut engine = PrivacyEngine::default();
        let mut request = json!({
            "system": "Tu aides Marie Dupont.",
            "messages": [{
                "role": "user",
                "content": [
                    {"type": "text", "text": "Contact: marie@example.com"},
                    {
                        "type": "tool_use",
                        "id": "toolu_1",
                        "name": "read_file",
                        "input": {"query": "IBAN FR76 3000 6000 0112 3456 7890 189"}
                    }
                ]
            }]
        });

        let report = engine.pseudonymize_request(&mut request).unwrap();
        let body = serde_json::to_string(&request).unwrap();

        assert!(report.entities >= 3);
        assert!(!body.contains("Marie Dupont"));
        assert!(!body.contains("marie@example.com"));
        assert!(!body.contains("FR76 3000 6000 0112 3456 7890 189"));
        assert!(body.contains("PERSON_"));
        assert!(body.contains("EMAIL_"));
    }

    #[test]
    fn rehydrates_known_response_tokens() {
        let mut engine = PrivacyEngine::default();
        let mut request = json!({
            "messages": [{"role": "user", "content": "Marie Dupont"}]
        });
        engine.pseudonymize_request(&mut request).unwrap();

        let pseudo = request["messages"][0]["content"]
            .as_str()
            .unwrap()
            .to_string();
        let token = pseudo
            .split_whitespace()
            .find(|part| part.starts_with("PERSON_"))
            .expect("person token");
        let mut response = json!({
            "content": [{"type": "text", "text": format!("Bonjour {token}")}]
        });

        let report = engine.rehydrate_response(&mut response).unwrap();

        assert_eq!(report.rehydrated_tokens, 1);
        assert_eq!(response["content"][0]["text"], "Bonjour Marie Dupont");
    }
}
