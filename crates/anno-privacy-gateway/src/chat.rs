//! Provider-neutral chat request and response adapters.

use crate::{Error, Result};
use serde_json::{json, Map, Value};

/// Chat request normalized for OpenAI-compatible providers.
#[derive(Debug, Clone, PartialEq)]
pub struct ChatRequest {
    model: String,
    messages: Vec<Value>,
    parameters: Map<String, Value>,
}

impl ChatRequest {
    /// Convert an Anthropic `/v1/messages` request into an OpenAI-compatible
    /// chat-completions request while replacing the public gateway model id.
    pub fn from_anthropic(input: &Value, upstream_model: &str) -> Result<Self> {
        let mut messages = Vec::new();
        if let Some(system) = input.get("system") {
            messages.push(json!({
                "role": "system",
                "content": system_content_to_openai(system)
            }));
        }

        let anthropic_messages = input
            .get("messages")
            .and_then(Value::as_array)
            .ok_or_else(|| Error::Privacy("messages must be an array".to_string()))?;
        for message in anthropic_messages {
            let role = message
                .get("role")
                .and_then(Value::as_str)
                .ok_or_else(|| Error::Privacy("message role must be a string".to_string()))?;
            let content = message
                .get("content")
                .ok_or_else(|| Error::Privacy("message content is required".to_string()))?;
            messages.push(json!({
                "role": role,
                "content": content_to_openai(content)
            }));
        }

        let mut parameters = Map::new();
        copy_optional(input, &mut parameters, "max_tokens", "max_tokens");
        copy_optional(input, &mut parameters, "temperature", "temperature");
        copy_optional(input, &mut parameters, "top_p", "top_p");
        copy_optional(input, &mut parameters, "stream", "stream");
        copy_optional(input, &mut parameters, "stop_sequences", "stop");
        if let Some(tools) = input.get("tools") {
            parameters.insert("tools".to_string(), tools_to_openai(tools));
        }
        copy_optional(input, &mut parameters, "tool_choice", "tool_choice");

        Ok(Self {
            model: upstream_model.to_string(),
            messages,
            parameters,
        })
    }

    /// Render the OpenAI-compatible JSON request body.
    #[must_use]
    pub fn to_openai_json(&self) -> Value {
        let mut body = Map::new();
        body.insert("model".to_string(), Value::String(self.model.clone()));
        body.insert("messages".to_string(), Value::Array(self.messages.clone()));
        for (key, value) in &self.parameters {
            body.insert(key.clone(), value.clone());
        }
        Value::Object(body)
    }
}

/// Convert an OpenAI-compatible chat response to Anthropic `/v1/messages` JSON.
pub fn anthropic_response_from_openai(input: &Value) -> Result<Value> {
    let choice = input
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .ok_or_else(|| Error::Upstream("OpenAI response missing choices[0]".to_string()))?;
    let message = choice
        .get("message")
        .ok_or_else(|| Error::Upstream("OpenAI response missing message".to_string()))?;

    let mut content = Vec::new();
    if let Some(text) = message.get("content").and_then(Value::as_str) {
        if !text.is_empty() {
            content.push(json!({"type": "text", "text": text}));
        }
    } else if let Some(parts) = message.get("content").and_then(Value::as_array) {
        for part in parts {
            if let Some(text) = part.get("text").and_then(Value::as_str) {
                content.push(json!({"type": "text", "text": text}));
            }
        }
    }

    if let Some(tool_calls) = message.get("tool_calls").and_then(Value::as_array) {
        for call in tool_calls {
            content.push(tool_call_to_anthropic(call)?);
        }
    }

    if content.is_empty() {
        content.push(json!({"type": "text", "text": ""}));
    }

    let usage = input.get("usage").unwrap_or(&Value::Null);
    Ok(json!({
        "id": input.get("id").and_then(Value::as_str).unwrap_or("msg_provider"),
        "type": "message",
        "role": "assistant",
        "model": input.get("model").and_then(Value::as_str).unwrap_or(""),
        "content": content,
        "stop_reason": stop_reason(choice),
        "stop_sequence": Value::Null,
        "usage": {
            "input_tokens": usage.get("prompt_tokens").and_then(Value::as_u64).unwrap_or(0),
            "output_tokens": usage.get("completion_tokens").and_then(Value::as_u64).unwrap_or(0)
        }
    }))
}

/// Convert one OpenAI streaming chunk to an Anthropic SSE frame payload.
pub fn anthropic_stream_frame_from_openai(
    value: &Value,
    index: usize,
) -> Result<crate::stream::SseFrame> {
    let delta = value
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("delta"))
        .ok_or_else(|| {
            Error::Upstream("OpenAI stream chunk missing choices[0].delta".to_string())
        })?;

    if let Some(content) = delta.get("content").and_then(Value::as_str) {
        return Ok(crate::stream::SseFrame {
            event: Some("content_block_delta".to_string()),
            data: json!({
                "type": "content_block_delta",
                "index": index,
                "delta": {"type": "text_delta", "text": content}
            }),
        });
    }

    if let Some(tool_call) = delta
        .get("tool_calls")
        .and_then(Value::as_array)
        .and_then(|calls| calls.first())
    {
        let function = tool_call.get("function").unwrap_or(&Value::Null);
        let args = function
            .get("arguments")
            .and_then(Value::as_str)
            .unwrap_or("{}");
        return Ok(crate::stream::SseFrame {
            event: Some("content_block_delta".to_string()),
            data: json!({
                "type": "content_block_delta",
                "index": index,
                "delta": {"type": "input_json_delta", "partial_json": args}
            }),
        });
    }

    Ok(crate::stream::SseFrame {
        event: Some("ping".to_string()),
        data: json!({"type": "ping"}),
    })
}

fn copy_optional(input: &Value, output: &mut Map<String, Value>, from: &str, to: &str) {
    if let Some(value) = input.get(from) {
        output.insert(to.to_string(), value.clone());
    }
}

fn system_content_to_openai(value: &Value) -> Value {
    match value {
        Value::String(_) => value.clone(),
        Value::Array(_) => Value::String(text_from_content_blocks(value)),
        _ => Value::String(value.to_string()),
    }
}

fn content_to_openai(value: &Value) -> Value {
    match value {
        Value::String(_) => value.clone(),
        Value::Array(_) => Value::String(text_from_content_blocks(value)),
        _ => value.clone(),
    }
}

fn text_from_content_blocks(value: &Value) -> String {
    let Some(blocks) = value.as_array() else {
        return value.to_string();
    };
    blocks
        .iter()
        .filter_map(|block| block.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("\n")
}

fn tools_to_openai(value: &Value) -> Value {
    let Some(tools) = value.as_array() else {
        return value.clone();
    };
    Value::Array(
        tools
            .iter()
            .filter_map(|tool| {
                let name = tool.get("name")?.clone();
                let description = tool.get("description").cloned().unwrap_or(Value::Null);
                let parameters = tool
                    .get("input_schema")
                    .cloned()
                    .unwrap_or_else(|| json!({"type": "object"}));
                Some(json!({
                    "type": "function",
                    "function": {
                        "name": name,
                        "description": description,
                        "parameters": parameters
                    }
                }))
            })
            .collect(),
    )
}

fn tool_call_to_anthropic(call: &Value) -> Result<Value> {
    let function = call.get("function").unwrap_or(&Value::Null);
    let arguments = function
        .get("arguments")
        .and_then(Value::as_str)
        .unwrap_or("{}");
    let input = serde_json::from_str::<Value>(arguments)
        .map_err(|e| Error::Upstream(format!("OpenAI tool call arguments are not JSON: {e}")))?;

    Ok(json!({
        "type": "tool_use",
        "id": call.get("id").and_then(Value::as_str).unwrap_or("call_provider"),
        "name": function.get("name").and_then(Value::as_str).unwrap_or("tool"),
        "input": input
    }))
}

fn stop_reason(choice: &Value) -> &'static str {
    match choice.get("finish_reason").and_then(Value::as_str) {
        Some("length") => "max_tokens",
        Some("tool_calls") => "tool_use",
        _ => "end_turn",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn chat_anthropic_request_converts_to_openai_messages() {
        let input = json!({
            "model": "anno/mistral/mistral-large-latest:pseudonymized",
            "system": "Tu es juriste.",
            "messages": [{"role": "user", "content": "Bonjour PERSON_1"}],
            "max_tokens": 128,
            "temperature": 0.2
        });

        let request = ChatRequest::from_anthropic(&input, "mistral-large-latest").expect("request");
        let openai = request.to_openai_json();

        assert_eq!(openai["model"], "mistral-large-latest");
        assert_eq!(openai["messages"][0]["role"], "system");
        assert_eq!(openai["messages"][1]["role"], "user");
        assert_eq!(openai["max_tokens"], 128);
    }

    #[test]
    fn chat_openai_text_response_renders_anthropic_content() {
        let upstream = json!({
            "choices": [{
                "message": {"role": "assistant", "content": "Bonjour PERSON_1"}
            }],
            "usage": {"prompt_tokens": 3, "completion_tokens": 4}
        });

        let response = anthropic_response_from_openai(&upstream).expect("response");

        assert_eq!(response["content"][0]["type"], "text");
        assert_eq!(response["content"][0]["text"], "Bonjour PERSON_1");
    }

    #[test]
    fn openai_stream_text_chunk_to_anthropic_delta() {
        let chunk = json!({
            "choices": [{"delta": {"content": "Bonjour PERSON_1"}}]
        });

        let frame = anthropic_stream_frame_from_openai(&chunk, 0).expect("frame");

        assert_eq!(frame.data["type"], "content_block_delta");
        assert_eq!(frame.data["delta"]["type"], "text_delta");
        assert_eq!(frame.data["delta"]["text"], "Bonjour PERSON_1");
    }

    #[test]
    fn openai_tool_call_arguments_render_as_single_safe_input_json_delta() {
        let chunk = json!({
            "choices": [{
                "delta": {
                    "tool_calls": [{
                        "id": "call_1",
                        "type": "function",
                        "function": {"name": "search", "arguments": "{\"query\":\"PERSON_1\"}"}
                    }]
                }
            }]
        });

        let frame = anthropic_stream_frame_from_openai(&chunk, 0).expect("frame");

        assert_eq!(frame.data["delta"]["type"], "input_json_delta");
        assert_eq!(
            frame.data["delta"]["partial_json"],
            "{\"query\":\"PERSON_1\"}"
        );
    }

    #[test]
    fn openai_stream_role_chunk_maps_to_ping_not_stop() {
        let chunk = json!({
            "choices": [{"delta": {"role": "assistant"}}]
        });

        let frame = anthropic_stream_frame_from_openai(&chunk, 0).expect("frame");

        assert_eq!(frame.event.as_deref(), Some("ping"));
        assert_eq!(frame.data["type"], "ping");
    }
}
