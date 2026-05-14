//! Anthropic-compatible SSE stream transforms.

use crate::{Error, Result};
use serde_json::Value;

/// Parsed SSE event with JSON `data`.
#[derive(Debug, Clone, PartialEq)]
pub struct SseFrame {
    /// Optional SSE event name.
    pub event: Option<String>,
    /// JSON payload from `data:`.
    pub data: Value,
}

impl SseFrame {
    /// Parse one complete SSE frame.
    pub fn parse(raw: &str) -> Result<Self> {
        let mut event = None;
        let mut data_lines = Vec::new();

        for line in raw.lines() {
            if let Some(rest) = line.strip_prefix("event:") {
                event = Some(rest.trim_start().to_string());
            } else if let Some(rest) = line.strip_prefix("data:") {
                data_lines.push(rest.trim_start().to_string());
            }
        }

        if data_lines.is_empty() {
            return Err(Error::Privacy("SSE frame is missing data".to_string()));
        }

        let data = data_lines.join("\n");
        let data = serde_json::from_str(&data)
            .map_err(|e| Error::Privacy(format!("invalid SSE JSON data: {e}")))?;

        Ok(Self { event, data })
    }

    /// Serialize to one SSE frame.
    #[must_use]
    pub fn to_sse(&self) -> String {
        let mut out = String::new();
        if let Some(event) = &self.event {
            out.push_str("event: ");
            out.push_str(event);
            out.push('\n');
        }
        out.push_str("data: ");
        out.push_str(&json_to_anthropic_string(&self.data));
        out.push_str("\n\n");
        out
    }

    /// Return assistant text delta when this is a text delta frame.
    #[must_use]
    pub fn text_delta(&self) -> Option<&str> {
        let delta = self.data.get("delta")?;
        if delta.get("type").and_then(Value::as_str) != Some("text_delta") {
            return None;
        }
        delta.get("text").and_then(Value::as_str)
    }

    /// Replace assistant text delta.
    pub fn set_text_delta(&mut self, text: &str) {
        let Some(delta) = self.data.get_mut("delta").and_then(Value::as_object_mut) else {
            return;
        };
        if delta.get("type").and_then(Value::as_str) == Some("text_delta") {
            delta.insert("text".to_string(), Value::String(text.to_string()));
        }
    }
}

/// Hybrid buffer for streamed assistant text.
#[derive(Debug, Clone)]
pub struct StreamBuffer {
    pending: String,
    max_chars: usize,
}

impl StreamBuffer {
    /// Build a buffer with a forced flush size.
    #[must_use]
    pub fn new(max_chars: usize) -> Self {
        Self {
            pending: String::new(),
            max_chars,
        }
    }

    /// Push a fragment and return a safe emission unit when available.
    pub fn push(&mut self, fragment: &str) -> Option<String> {
        self.pending.push_str(fragment);
        if ends_with_open_pseudonym_prefix(&self.pending) {
            return None;
        }
        if has_sentence_boundary(&self.pending) || self.pending.len() >= self.max_chars {
            return Some(std::mem::take(&mut self.pending));
        }
        None
    }

    /// Return true when the buffer holds text.
    #[must_use]
    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    /// Flush buffered text only when it cannot be an open pseudonym prefix.
    pub fn flush_if_safe(&mut self) -> Option<String> {
        if self.pending.is_empty() || ends_with_open_pseudonym_prefix(&self.pending) {
            None
        } else {
            Some(std::mem::take(&mut self.pending))
        }
    }

    /// Flush remaining buffered text.
    pub fn finish(&mut self) -> Option<String> {
        if self.pending.is_empty() {
            None
        } else {
            Some(std::mem::take(&mut self.pending))
        }
    }
}

fn has_sentence_boundary(text: &str) -> bool {
    text.ends_with('.') || text.ends_with('!') || text.ends_with('?') || text.ends_with('\n')
}

fn ends_with_open_pseudonym_prefix(text: &str) -> bool {
    const PREFIXES: &[&str] = &["PERSON_", "EMAIL_", "PHONE_", "IBAN_", "SIRET_"];
    text.char_indices().any(|(index, _)| {
        let suffix = &text[index..];
        PREFIXES.iter().any(|prefix| {
            if prefix.starts_with(suffix) {
                return true;
            }
            suffix
                .strip_prefix(prefix)
                .is_some_and(|id| !id.is_empty() && id.chars().all(|ch| ch.is_ascii_digit()))
        })
    })
}

fn json_to_anthropic_string(value: &Value) -> String {
    match value {
        Value::Object(object) => {
            let mut emitted = Vec::new();
            let mut fields = Vec::new();

            for key in [
                "type",
                "index",
                "delta",
                "content_block",
                "message",
                "usage",
            ] {
                if let Some(value) = object.get(key) {
                    emitted.push(key);
                    fields.push(format!(
                        "{}:{}",
                        serde_json::to_string(key).expect("serializing a string key cannot fail"),
                        json_to_anthropic_string(value)
                    ));
                }
            }

            for (key, value) in object {
                if emitted.contains(&key.as_str()) {
                    continue;
                }
                fields.push(format!(
                    "{}:{}",
                    serde_json::to_string(key).expect("serializing a string key cannot fail"),
                    json_to_anthropic_string(value)
                ));
            }

            format!("{{{}}}", fields.join(","))
        }
        Value::Array(items) => {
            let items = items
                .iter()
                .map(json_to_anthropic_string)
                .collect::<Vec<_>>()
                .join(",");
            format!("[{items}]")
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {
            serde_json::to_string(value).expect("serializing serde_json::Value cannot fail")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_and_serializes_sse_event() {
        let raw = "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"Bon\"}}\n\n";

        let event = SseFrame::parse(raw).unwrap();

        assert_eq!(event.event.as_deref(), Some("content_block_delta"));
        assert_eq!(event.data["delta"]["text"], "Bon");
        assert_eq!(event.to_sse(), raw);
    }

    #[test]
    fn rewrites_text_delta_only() {
        let mut event = SseFrame {
            event: Some("content_block_delta".to_string()),
            data: json!({
                "type": "content_block_delta",
                "delta": {"type": "text_delta", "text": "PERSON_1"}
            }),
        };

        assert_eq!(event.text_delta(), Some("PERSON_1"));
        event.set_text_delta("Marie Dupont");
        assert_eq!(event.data["delta"]["text"], "Marie Dupont");
    }

    #[test]
    fn ignores_non_text_delta_with_text_field() {
        let mut event = SseFrame {
            event: Some("content_block_delta".to_string()),
            data: json!({
                "type": "content_block_delta",
                "delta": {"type": "input_json_delta", "text": "PERSON_1"}
            }),
        };

        assert_eq!(event.text_delta(), None);
        event.set_text_delta("Marie Dupont");
        assert_eq!(event.data["delta"]["text"], "PERSON_1");
    }

    #[test]
    fn buffer_holds_incomplete_pseudonym_token() {
        let mut buffer = StreamBuffer::new(4096);

        assert_eq!(buffer.push("Bonjour PER"), None);
        assert_eq!(buffer.push("SON_1."), Some("Bonjour PERSON_1.".to_string()));
    }

    #[test]
    fn buffer_flushes_complete_sentence() {
        let mut buffer = StreamBuffer::new(4096);

        assert_eq!(buffer.push("Bonjour Marie"), None);
        assert_eq!(
            buffer.push(" Dupont."),
            Some("Bonjour Marie Dupont.".to_string())
        );
    }

    #[test]
    fn buffer_flushes_on_size_limit() {
        let mut buffer = StreamBuffer::new(10);

        assert_eq!(buffer.push("abcdefghijk"), Some("abcdefghijk".to_string()));
    }

    #[test]
    fn timed_flush_does_not_emit_open_pseudonym_prefix() {
        let mut buffer = StreamBuffer::new(4096);

        assert_eq!(buffer.push("Bonjour PER"), None);
        assert_eq!(buffer.flush_if_safe(), None);
        assert_eq!(buffer.push("SON_1"), None);
        assert_eq!(buffer.flush_if_safe(), None);
        assert_eq!(buffer.push("."), Some("Bonjour PERSON_1.".to_string()));
    }

    #[test]
    fn timed_flush_holds_ambiguous_pseudonym_suffix() {
        let mut buffer = StreamBuffer::new(4096);

        assert_eq!(buffer.push("Bonjour PERSON_1"), None);
        assert_eq!(buffer.flush_if_safe(), None);
        assert_eq!(buffer.push("2."), Some("Bonjour PERSON_12.".to_string()));
    }
}
