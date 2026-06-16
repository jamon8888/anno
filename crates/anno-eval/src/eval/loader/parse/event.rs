use crate::eval::loader::types::{AnnotatedSentence, AnnotatedToken, DataSource, LoadedDataset};
use crate::eval::loader::DatasetId;
use crate::eval::loader::DatasetLoader;
use anno::{Error, Result};

/// Parse MAVEN event detection format.
///
/// MAVEN provides event triggers with 168 event types.
/// Supports both:
/// - Full MAVEN JSONL format (train.jsonl/valid.jsonl with events array)
/// - Fallback: docid2topic.json mapping file
///
/// Full format structure:
/// ```json
/// {
///   "id": "doc_id",
///   "content": [{"sentence": "...", "tokens": [...]}],
///   "events": [{
///     "type": "EventType",
///     "mention": [{"trigger_word": "...", "sent_id": 0, "offset": [3, 4]}]
///   }]
/// }
/// ```
///
/// Source: <https://github.com/THU-KEG/MAVEN-dataset>
pub(crate) fn parse_maven(content: &str, id: DatasetId) -> Result<LoadedDataset> {
    let mut sentences = Vec::new();
    let now = chrono::Utc::now().to_rfc3339();

    // Try parsing as JSONL (full MAVEN format)
    let mut is_jsonl = false;
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if let Ok(doc) = serde_json::from_str::<serde_json::Value>(line) {
            // Check if this is full MAVEN format with events
            if let Some(events) = doc.get("events").and_then(|e| e.as_array()) {
                is_jsonl = true;

                // Get document content for context
                let doc_content: Vec<String> = doc
                    .get("content")
                    .and_then(|c| c.as_array())
                    .map(|sents| {
                        sents
                            .iter()
                            .filter_map(|s| s.get("sentence").and_then(|v| v.as_str()))
                            .map(|s| s.to_string())
                            .collect()
                    })
                    .unwrap_or_default();

                // Process each event
                for event in events {
                    let event_type = event
                        .get("type")
                        .and_then(|t| t.as_str())
                        .unwrap_or("EVENT");

                    // Process each mention of this event
                    if let Some(mentions) = event.get("mention").and_then(|m| m.as_array()) {
                        for mention in mentions {
                            let trigger_word = mention
                                .get("trigger_word")
                                .and_then(|t| t.as_str())
                                .unwrap_or("");

                            let sent_id =
                                mention.get("sent_id").and_then(|s| s.as_u64()).unwrap_or(0)
                                    as usize;

                            // Get sentence context if available
                            let context = doc_content
                                .get(sent_id)
                                .cloned()
                                .unwrap_or_else(|| trigger_word.to_string());

                            let tokens = vec![AnnotatedToken {
                                text: context,
                                ner_tag: format!("B-{}", event_type),
                            }];

                            sentences.push(AnnotatedSentence {
                                tokens,
                                source_dataset: id,
                            });
                        }
                    }
                }
            }
        }
    }

    // Fallback: parse as docid2topic.json mapping
    if !is_jsonl {
        if let Ok(obj) = serde_json::from_str::<serde_json::Value>(content) {
            if let Some(map) = obj.as_object() {
                for (doc_id, event_type) in map {
                    let event_type_str = event_type.as_str().unwrap_or("event");

                    let tokens = vec![AnnotatedToken {
                        text: doc_id.clone(),
                        ner_tag: format!(
                            "B-EVENT_{}",
                            event_type_str.to_uppercase().replace(' ', "_")
                        ),
                    }];

                    sentences.push(AnnotatedSentence {
                        tokens,
                        source_dataset: id,
                    });
                }
            }
        }
    }

    Ok(LoadedDataset {
        id,
        sentences,
        loaded_at: now,
        source_url: id.download_url().to_string(),
        data_source: DataSource::LocalCache,
        temporal_metadata: DatasetLoader::get_temporal_metadata(id),
        metadata: id.default_metadata(),
    })
}

/// Parse CASIE cybersecurity event extraction format.
///
/// CASIE has 5 event types: Attack-Pattern, Vulnerability, Data-Breach, Malware, Patch
///
/// Format structure:
/// ```json
/// {
///   "content": "document text...",
///   "cyberevent": {
///     "hopper": [{
///       "events": [{
///         "subtype": "Databreach",
///         "nugget": {"text": "trigger", "startOffset": 0, "endOffset": 10},
///         "argument": [{"text": "arg", "role": {"type": "RoleType"}}]
///       }]
///     }]
///   }
/// }
/// ```
///
/// Source: <https://github.com/Ebiquity/CASIE>
pub(crate) fn parse_casie(content: &str, id: DatasetId) -> Result<LoadedDataset> {
    let mut sentences = Vec::new();
    let now = chrono::Utc::now().to_rfc3339();

    // Handle JSONL format (multiple documents)
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if let Ok(doc) = serde_json::from_str::<serde_json::Value>(line) {
            let content_text = doc
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or_default();

            if content_text.is_empty() {
                continue;
            }

            // Extract events from cyberevent.hopper[].events[]
            let mut found_events = false;
            if let Some(hopper) = doc
                .get("cyberevent")
                .and_then(|ce| ce.get("hopper"))
                .and_then(|h| h.as_array())
            {
                for cluster in hopper {
                    if let Some(events) = cluster.get("events").and_then(|e| e.as_array()) {
                        for event in events {
                            found_events = true;

                            // Get event subtype
                            let subtype = event
                                .get("subtype")
                                .and_then(|s| s.as_str())
                                .unwrap_or("Event");

                            // Get trigger (nugget)
                            let trigger_text = event
                                .get("nugget")
                                .and_then(|n| n.get("text"))
                                .and_then(|t| t.as_str())
                                .unwrap_or("");

                            // Create entry with trigger and event type
                            let tokens = vec![AnnotatedToken {
                                text: trigger_text.to_string(),
                                ner_tag: format!("B-{}", subtype),
                            }];

                            sentences.push(AnnotatedSentence {
                                tokens,
                                source_dataset: id,
                            });

                            // Also extract arguments if present
                            if let Some(args) = event.get("argument").and_then(|a| a.as_array()) {
                                for arg in args {
                                    let arg_text =
                                        arg.get("text").and_then(|t| t.as_str()).unwrap_or("");
                                    let role = arg
                                        .get("role")
                                        .and_then(|r| r.get("type"))
                                        .and_then(|t| t.as_str())
                                        .unwrap_or("Argument");

                                    if !arg_text.is_empty() {
                                        let tokens = vec![AnnotatedToken {
                                            text: arg_text.to_string(),
                                            ner_tag: format!("B-ARG_{}", role),
                                        }];

                                        sentences.push(AnnotatedSentence {
                                            tokens,
                                            source_dataset: id,
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // If no events found, add document with O tag
            if !found_events {
                let tokens = vec![AnnotatedToken {
                    text: content_text.chars().take(200).collect(),
                    ner_tag: "O".to_string(),
                }];

                sentences.push(AnnotatedSentence {
                    tokens,
                    source_dataset: id,
                });
            }
        }
    }

    if sentences.is_empty() {
        return Err(Error::InvalidInput(format!(
            "MAVEN file for {:?} contains no valid sentences",
            id
        )));
    }

    Ok(LoadedDataset {
        id,
        sentences,
        loaded_at: now,
        source_url: id.download_url().to_string(),
        data_source: DataSource::LocalCache,
        temporal_metadata: DatasetLoader::get_temporal_metadata(id),
        metadata: id.default_metadata(),
    })
}

/// Parse MAVEN-ARG event argument extraction format.
///
/// MAVEN-ARG extends MAVEN with 612 argument roles across 162 event types.
///
/// Format structure:
/// ```json
/// {
///   "id": "doc_id",
///   "document": "full document text...",
///   "events": [{
///     "type": "EventType",
///     "mention": [{"trigger_word": "...", "offset": [start, end]}],
///     "argument": {"Role": [{"content": "arg text", "offset": [s, e]}]}
///   }]
/// }
/// ```
///
/// Source: <https://github.com/THU-KEG/MAVEN-Argument>
pub(crate) fn parse_maven_arg(content: &str, id: DatasetId) -> Result<LoadedDataset> {
    let mut sentences = Vec::new();
    let now = chrono::Utc::now().to_rfc3339();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if let Ok(doc) = serde_json::from_str::<serde_json::Value>(line) {
            // Get document text
            let _doc_text = doc.get("document").and_then(|d| d.as_str()).unwrap_or("");

            // Process events
            if let Some(events) = doc.get("events").and_then(|e| e.as_array()) {
                for event in events {
                    let event_type = event
                        .get("type")
                        .and_then(|t| t.as_str())
                        .unwrap_or("EVENT");

                    // Process event mentions (triggers)
                    if let Some(mentions) = event.get("mention").and_then(|m| m.as_array()) {
                        for mention in mentions {
                            let trigger = mention
                                .get("trigger_word")
                                .and_then(|t| t.as_str())
                                .unwrap_or("");

                            if !trigger.is_empty() {
                                let tokens = vec![AnnotatedToken {
                                    text: trigger.to_string(),
                                    ner_tag: format!("B-{}", event_type),
                                }];

                                sentences.push(AnnotatedSentence {
                                    tokens,
                                    source_dataset: id,
                                });
                            }
                        }
                    }

                    // Process arguments
                    if let Some(args) = event.get("argument").and_then(|a| a.as_object()) {
                        for (role, arg_list) in args {
                            if let Some(arg_arr) = arg_list.as_array() {
                                for arg in arg_arr {
                                    // Arguments can be text or entity references
                                    if let Some(content) =
                                        arg.get("content").and_then(|c| c.as_str())
                                    {
                                        if !content.is_empty() {
                                            let tokens = vec![AnnotatedToken {
                                                text: content.to_string(),
                                                ner_tag: format!("B-ARG_{}", role),
                                            }];

                                            sentences.push(AnnotatedSentence {
                                                tokens,
                                                source_dataset: id,
                                            });
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    if sentences.is_empty() {
        return Err(Error::InvalidInput(format!(
            "MAVEN-ARG file for {:?} contains no valid sentences",
            id
        )));
    }

    Ok(LoadedDataset {
        id,
        sentences,
        loaded_at: now,
        source_url: id.download_url().to_string(),
        data_source: DataSource::LocalCache,
        temporal_metadata: DatasetLoader::get_temporal_metadata(id),
        metadata: id.default_metadata(),
    })
}

/// Parse RAMS event argument extraction format.
///
/// RAMS (Roles Across Multiple Sentences) covers 139 event types
/// with arguments that can span multiple sentences.
///
/// Format structure:
/// ```json
/// {
///   "doc_key": "...",
///   "sentences": [["token1", "token2", ...]],
///   "evt_triggers": [[start, end, [["event.type", score]]]],
///   "gold_evt_links": [[[evt_idx], [arg_start, arg_end], "role"]]
/// }
/// ```
///
/// Source: <https://nlp.jhu.edu/rams/>
pub(crate) fn parse_rams(content: &str, id: DatasetId) -> Result<LoadedDataset> {
    let mut sentences = Vec::new();
    let now = chrono::Utc::now().to_rfc3339();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if let Ok(doc) = serde_json::from_str::<serde_json::Value>(line) {
            // Get all tokens (flattened sentences)
            let all_tokens: Vec<String> = doc
                .get("sentences")
                .and_then(|s| s.as_array())
                .map(|sents| {
                    sents
                        .iter()
                        .filter_map(|s| s.as_array())
                        .flat_map(|toks| toks.iter().filter_map(|t| t.as_str().map(String::from)))
                        .collect()
                })
                .unwrap_or_default();

            // Process event triggers
            if let Some(triggers) = doc.get("evt_triggers").and_then(|t| t.as_array()) {
                for trigger in triggers {
                    if let Some(trigger_arr) = trigger.as_array() {
                        if trigger_arr.len() >= 3 {
                            let start = trigger_arr[0].as_u64().unwrap_or(0) as usize;
                            let end = trigger_arr[1].as_u64().unwrap_or(0) as usize;

                            // Get event type from nested array
                            let event_type = trigger_arr[2]
                                .as_array()
                                .and_then(|types| types.first())
                                .and_then(|t| t.as_array())
                                .and_then(|t| t.first())
                                .and_then(|t| t.as_str())
                                .unwrap_or("event");

                            // Extract trigger text
                            if end <= all_tokens.len() {
                                let trigger_text = all_tokens[start..=end.min(start)].join(" ");
                                let tokens = vec![AnnotatedToken {
                                    text: trigger_text,
                                    ner_tag: format!("B-{}", event_type),
                                }];

                                sentences.push(AnnotatedSentence {
                                    tokens,
                                    source_dataset: id,
                                });
                            }
                        }
                    }
                }
            }

            // Process argument links
            if let Some(links) = doc.get("gold_evt_links").and_then(|l| l.as_array()) {
                for link in links {
                    if let Some(link_arr) = link.as_array() {
                        if link_arr.len() >= 3 {
                            // Argument span
                            if let Some(span) = link_arr[1].as_array() {
                                if span.len() >= 2 {
                                    let start = span[0].as_u64().unwrap_or(0) as usize;
                                    let end = span[1].as_u64().unwrap_or(0) as usize;
                                    let role = link_arr[2].as_str().unwrap_or("argument");

                                    if end < all_tokens.len() {
                                        let arg_text = all_tokens[start..=end.min(start)].join(" ");
                                        let tokens = vec![AnnotatedToken {
                                            text: arg_text,
                                            ner_tag: format!("B-ARG_{}", role),
                                        }];

                                        sentences.push(AnnotatedSentence {
                                            tokens,
                                            source_dataset: id,
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    if sentences.is_empty() {
        return Err(Error::InvalidInput(format!(
            "RAMS file for {:?} contains no valid sentences",
            id
        )));
    }

    Ok(LoadedDataset {
        id,
        sentences,
        loaded_at: now,
        source_url: id.download_url().to_string(),
        data_source: DataSource::LocalCache,
        temporal_metadata: DatasetLoader::get_temporal_metadata(id),
        metadata: id.default_metadata(),
    })
}
