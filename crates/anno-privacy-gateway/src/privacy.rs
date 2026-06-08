//! Anthropic request/response privacy transforms.

use crate::{Error, GatewayConfig, Result};
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
    /// Number of fresh cleartext PII entities redacted from a response.
    pub fresh_pii_redacted: usize,
}

/// Text output and counts emitted by a stream text transform.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamTextReport {
    /// Transformed text safe to emit to Cowork.
    pub output: String,
    /// Privacy counters from this fragment.
    pub privacy: PrivacyReport,
}

/// Text output and counts emitted by a plain text transform.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlainTextPrivacyReport {
    /// Pseudonymized text.
    pub text: String,
    /// Number of entities replaced.
    pub entities: usize,
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
    /// Build from gateway config. Persistent vaults require both a path and a
    /// 32-byte hex key; otherwise v0.3 uses an in-memory single-user vault.
    pub fn from_config(config: &GatewayConfig) -> Result<Self> {
        match (&config.vault_path, &config.vault_key_hex) {
            (Some(path), Some(key_hex)) => {
                let key = decode_hex_32(key_hex)?;
                let vault = Vault::open(path, key).map_err(|e| Error::Config(e.to_string()))?;
                Ok(Self::new(vault))
            }
            (None, None) => Ok(Self::default()),
            _ => Err(Error::Config(
                "ANNO_GATEWAY_VAULT_PATH and ANNO_GATEWAY_VAULT_KEY_HEX must be set together"
                    .to_string(),
            )),
        }
    }

    /// Borrow the underlying vault for the GDPR rights handlers.
    #[must_use]
    pub fn vault(&self) -> &Vault {
        &self.vault
    }

    /// Mutable vault access — used by GDPR Art. 17 erasure.
    pub fn vault_mut(&mut self) -> &mut Vault {
        &mut self.vault
    }

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
            person_fr: Regex::new(r"\b[A-ZÀ-ÖØ-Þ][a-zà-öø-ÿ]+(?:[- ][A-ZÀ-ÖØ-Þ][a-zà-öø-ÿ]+)+\b")
                .expect("person regex compiles"),
        }
    }

    /// Pseudonymize all v0.3-supported request text fields in place.
    pub fn pseudonymize_request(&mut self, request: &mut Value) -> Result<PrivacyReport> {
        self.pseudonymize_request_with_streaming(request, false)
    }

    /// Pseudonymize one cleartext string and return the transformed derivative.
    pub fn pseudonymize_plain_text(&mut self, text: &str) -> Result<PlainTextPrivacyReport> {
        let mut value = Value::String(text.to_string());
        let mut report = PrivacyReport::default();
        self.transform_content_value(&mut value, &mut report)?;
        let text = value
            .as_str()
            .ok_or_else(|| Error::Privacy("plain text transform returned non-string".to_string()))?
            .to_string();
        Ok(PlainTextPrivacyReport {
            text,
            entities: report.entities,
        })
    }

    /// Transform a request according to the selected privacy mode.
    pub fn transform_request_for_mode(
        &mut self,
        request: &mut Value,
        mode: crate::privacy_mode::PrivacyMode,
        allow_streaming: bool,
    ) -> Result<PrivacyReport> {
        match mode {
            crate::privacy_mode::PrivacyMode::Pseudonymized => {
                self.pseudonymize_request_with_streaming(request, allow_streaming)
            }
            crate::privacy_mode::PrivacyMode::CleartextDpa
            | crate::privacy_mode::PrivacyMode::CleartextLocal => {
                if request
                    .get("stream")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                    && !allow_streaming
                {
                    return Err(Error::UnsupportedFeature(
                        "stream=true is disabled; set ANNO_GATEWAY_STREAMING=enabled".to_string(),
                    ));
                }
                reject_blocks(request)?;
                Ok(PrivacyReport::default())
            }
        }
    }

    /// Pseudonymize request text, optionally allowing `stream=true`.
    pub fn pseudonymize_request_with_streaming(
        &mut self,
        request: &mut Value,
        allow_streaming: bool,
    ) -> Result<PrivacyReport> {
        if request
            .get("stream")
            .and_then(Value::as_bool)
            .unwrap_or(false)
            && !allow_streaming
        {
            return Err(Error::UnsupportedFeature(
                "stream=true is disabled; set ANNO_GATEWAY_STREAMING=enabled".to_string(),
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

    /// Transform a stream text fragment before it is emitted to Cowork.
    pub fn transform_stream_text(
        &self,
        text: &mut String,
        scan_fresh_pii: bool,
    ) -> Result<StreamTextReport> {
        let mut report = PrivacyReport::default();
        if scan_fresh_pii {
            report.fresh_pii_redacted += self.redact_fresh_pii(text);
        }
        let rehydrated =
            Rehydrator::rehydrate(text, &self.vault).map_err(|e| Error::Privacy(e.to_string()))?;
        report.rehydrated_tokens += rehydrated.rehydrated_count;
        Ok(StreamTextReport {
            output: rehydrated.text,
            privacy: report,
        })
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
                report.fresh_pii_redacted += self.redact_fresh_pii(text);
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
        self.push_person_matches(text, &mut entities);

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

    fn push_person_matches(&self, text: &str, entities: &mut Vec<DetectedEntity>) {
        for mat in self.person_fr.find_iter(text) {
            let original = mat.as_str();
            let (start, original) = strip_leading_title_or_greeting(mat.start(), original);
            entities.push(DetectedEntity {
                original: original.to_string(),
                start,
                end: start + original.len(),
                category: EntityCategory::Person,
                confidence: 1.0,
                source: DetectionSource::Pattern,
            });
        }
    }

    fn redact_fresh_pii(&self, text: &mut String) -> usize {
        let mut entities: Vec<_> = self
            .detect(text)
            .into_iter()
            .filter(|entity| !self.vault.contains_original(&entity.original))
            .collect();
        entities.sort_by_key(|entity| std::cmp::Reverse(entity.start));

        let count = entities.len();
        for entity in entities {
            text.replace_range(entity.start..entity.end, "[REDACTED]");
        }
        count
    }
}

fn strip_leading_title_or_greeting(start: usize, original: &str) -> (usize, &str) {
    const PREFIXES: &[&str] = &[
        "Bonjour", "Madame", "Monsieur", "Maître", "Maitre", "Docteur",
    ];

    let Some(prefix) = PREFIXES
        .iter()
        .find(|prefix| original.starts_with(**prefix))
    else {
        return (start, original);
    };

    let rest = original[prefix.len()..].trim_start_matches([' ', '-']);
    if rest.split([' ', '-']).count() < 2 {
        return (start, original);
    }
    let offset = original.len() - rest.len();
    (start + offset, rest)
}

fn decode_hex_32(hex: &str) -> Result<Vec<u8>> {
    if hex.len() != 64 {
        return Err(Error::Config(
            "vault key must be 64 hex characters (32 bytes)".to_string(),
        ));
    }

    let mut out = Vec::with_capacity(32);
    for index in (0..hex.len()).step_by(2) {
        let byte = u8::from_str_radix(&hex[index..index + 2], 16)
            .map_err(|_| Error::Config("vault key must be valid hex".to_string()))?;
        out.push(byte);
    }
    Ok(out)
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
    fn cleartext_dpa_validates_blocks_without_pseudonymizing() {
        let mut engine = PrivacyEngine::default();
        let mut request = json!({
            "messages": [{"role": "user", "content": "Bonjour Marie Dupont"}]
        });

        let report = engine
            .transform_request_for_mode(
                &mut request,
                crate::privacy_mode::PrivacyMode::CleartextDpa,
                false,
            )
            .expect("cleartext allowed");

        assert_eq!(report.pseudonymized_values, 0);
        assert!(serde_json::to_string(&request)
            .unwrap()
            .contains("Marie Dupont"));
    }

    #[test]
    fn pseudonymized_mode_pseudonymizes() {
        let mut engine = PrivacyEngine::default();
        let mut request = json!({
            "messages": [{"role": "user", "content": "Bonjour Marie Dupont"}]
        });

        let report = engine
            .transform_request_for_mode(
                &mut request,
                crate::privacy_mode::PrivacyMode::Pseudonymized,
                false,
            )
            .expect("pseudonymized");

        assert!(report.pseudonymized_values > 0);
        assert!(!serde_json::to_string(&request)
            .unwrap()
            .contains("Marie Dupont"));
    }

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
    fn allows_streaming_when_policy_enabled() {
        let mut engine = PrivacyEngine::default();
        let mut request = json!({
            "model": "claude",
            "stream": true,
            "messages": [{"role": "user", "content": "Bonjour Marie Dupont"}]
        });

        let report = engine
            .pseudonymize_request_with_streaming(&mut request, true)
            .unwrap();
        let body = serde_json::to_string(&request).unwrap();

        assert_eq!(request["stream"], true);
        assert!(report.entities >= 1);
        assert!(!body.contains("Marie Dupont"));
        assert!(body.contains("PERSON_"));
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
    fn pseudonymizes_system_array_tool_result_and_message_strings() {
        let mut engine = PrivacyEngine::default();
        let mut request = json!({
            "system": [{"type": "text", "text": "Contexte pour Jean Martin."}],
            "messages": [
                {"role": "user", "content": "Contact: claire@example.com"},
                {
                    "role": "user",
                    "content": [{
                        "type": "tool_result",
                        "content": [{"type": "text", "text": "SIRET 12345678901234"}]
                    }]
                }
            ]
        });

        let report = engine.pseudonymize_request(&mut request).unwrap();
        let body = serde_json::to_string(&request).unwrap();

        assert!(report.entities >= 3);
        assert!(!body.contains("Jean Martin"));
        assert!(!body.contains("claire@example.com"));
        assert!(!body.contains("12345678901234"));
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

    #[test]
    fn stream_transform_rehydrates_known_tokens() {
        let mut engine = PrivacyEngine::default();
        let mut request = json!({
            "messages": [{"role": "user", "content": "Marie Dupont"}]
        });
        engine.pseudonymize_request(&mut request).unwrap();
        let token = request["messages"][0]["content"]
            .as_str()
            .unwrap()
            .split_whitespace()
            .find(|part| part.starts_with("PERSON_"))
            .unwrap()
            .to_string();

        let report = engine
            .transform_stream_text(&mut format!("Bonjour {token}"), true)
            .unwrap();

        assert_eq!(report.output, "Bonjour Marie Dupont");
        assert_eq!(report.privacy.rehydrated_tokens, 1);
        assert_eq!(report.privacy.fresh_pii_redacted, 0);
    }

    #[test]
    fn stream_transform_redacts_fresh_pii_when_scanning() {
        let engine = PrivacyEngine::default();
        let report = engine
            .transform_stream_text(
                &mut "Le fournisseur invente Jean Martin et jean.martin@example.com".to_string(),
                true,
            )
            .unwrap();

        assert!(!report.output.contains("Jean Martin"));
        assert!(!report.output.contains("jean.martin@example.com"));
        assert_eq!(report.privacy.fresh_pii_redacted, 2);
    }

    #[test]
    fn stream_transform_can_skip_fresh_pii_scan() {
        let engine = PrivacyEngine::default();
        let report = engine
            .transform_stream_text(&mut "Le fournisseur invente Jean Martin".to_string(), false)
            .unwrap();

        assert!(report.output.contains("Jean Martin"));
        assert_eq!(report.privacy.fresh_pii_redacted, 0);
    }

    #[test]
    fn pseudonymizes_plain_text_for_file_derivative() {
        let mut engine = PrivacyEngine::from_config(&GatewayConfig::default()).unwrap();
        let report = engine
            .pseudonymize_plain_text("Bonjour Marie Dupont")
            .expect("plain text");

        assert!(report.text.contains("PERSON_"));
        assert_eq!(report.entities, 1);
    }

    #[test]
    fn rehydrates_response_tool_use_json_string_leaves() {
        let mut engine = PrivacyEngine::default();
        let mut request = json!({
            "messages": [{"role": "user", "content": "Marie Dupont"}]
        });
        engine.pseudonymize_request(&mut request).unwrap();
        let pseudo = request["messages"][0]["content"].as_str().unwrap();
        let token = pseudo
            .split_whitespace()
            .find(|part| part.starts_with("PERSON_"))
            .expect("person token");
        let mut response = json!({
            "content": [{
                "type": "tool_use",
                "input": {"summary": format!("Synthèse pour {token}")}
            }]
        });

        let report = engine.rehydrate_response(&mut response).unwrap();

        assert_eq!(report.rehydrated_tokens, 1);
        assert_eq!(
            response["content"][0]["input"]["summary"],
            "Synthèse pour Marie Dupont"
        );
    }

    #[test]
    fn redacts_fresh_pii_before_returning_response() {
        let engine = PrivacyEngine::default();
        let mut response = json!({
            "content": [{
                "type": "text",
                "text": "Le modèle a inventé Jean Martin et jean.martin@example.com."
            }]
        });

        let report = engine.rehydrate_response(&mut response).unwrap();
        let text = response["content"][0]["text"].as_str().unwrap();

        assert_eq!(report.fresh_pii_redacted, 2);
        assert!(!text.contains("Jean Martin"));
        assert!(!text.contains("jean.martin@example.com"));
        assert!(text.contains("[REDACTED]"));
    }
}
