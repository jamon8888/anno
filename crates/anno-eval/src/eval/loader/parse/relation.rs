//! Relation extraction and relation-adjacent NER dataset parsers as free functions.

use crate::eval::loader::types::{AnnotatedSentence, AnnotatedToken, DataSource, LoadedDataset, RelationDocument};
use crate::eval::loader::DatasetLoader;
use crate::eval::loader::DatasetId;
use anno::{Error, Result};

pub(crate) fn parse_docred(content: &str, id: DatasetId) -> Result<LoadedDataset> {
    let mut sentences = Vec::new();

    // Parse as JSONL (one JSON object per line)
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let doc: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        // CrossRE format: sentence array + ner array
        let tokens_arr = match doc.get("sentence").and_then(|v| v.as_array()) {
            Some(t) => t,
            None => continue,
        };

        let ner_spans = doc.get("ner").and_then(|v| v.as_array());

        // Build token list with entity annotations
        let mut tokens: Vec<AnnotatedToken> = tokens_arr
            .iter()
            .filter_map(|t| t.as_str())
            .map(|word| AnnotatedToken {
                text: word.to_string(),
                ner_tag: "O".to_string(),
            })
            .collect();

        // Apply NER annotations: [start, end, type]
        if let Some(ner) = ner_spans {
            for span in ner {
                if let Some(arr) = span.as_array() {
                    if arr.len() >= 3 {
                        let start = arr[0].as_u64().unwrap_or(0) as usize;
                        let end = arr[1].as_u64().unwrap_or(0) as usize;
                        let ent_type = arr[2].as_str().unwrap_or("ENTITY");

                        // Apply BIO tags
                        for idx in start..=end {
                            if idx < tokens.len() {
                                tokens[idx].ner_tag = if idx == start {
                                    format!("B-{}", ent_type.to_uppercase())
                                } else {
                                    format!("I-{}", ent_type.to_uppercase())
                                };
                            }
                        }
                    }
                }
            }
        }

        if !tokens.is_empty() {
            sentences.push(AnnotatedSentence {
                tokens,
                source_dataset: id,
            });
        }
    }

    if sentences.is_empty() {
        return Err(Error::InvalidInput(format!(
            "DocRED/CrossRE JSON for {:?} contains no valid sentences",
            id
        )));
    }

    let now = chrono::Utc::now().to_rfc3339();
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

/// Parse Google's relation-extraction-corpus JSONL format.
///
/// Example record (one per line):
/// ```json
/// {"pred":"/people/person/place_of_birth","sub":"/m/...","obj":"/m/...","evidences":[{"url":"...","snippet":"..."}],"judgments":[{"rater":"...","judgment":"yes"}]}
/// ```
///
/// This dataset does **not** include token-level entity spans. For now we treat each
/// evidence snippet as a plain sentence with all tokens tagged as `O`, which is
/// sufficient for sanity evaluation plumbing (and avoids failing dataset loads).
pub(crate) fn parse_google_re_corpus(content: &str, id: DatasetId) -> Result<LoadedDataset> {
    let mut sentences: Vec<AnnotatedSentence> = Vec::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let rec: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let snippet = rec
            .get("evidences")
            .and_then(|v| v.as_array())
            .and_then(|arr| arr.first())
            .and_then(|ev| ev.get("snippet"))
            .and_then(|s| s.as_str());
        let Some(snippet) = snippet else {
            continue;
        };

        let tokens: Vec<AnnotatedToken> = snippet
            .split_whitespace()
            .filter(|t| !t.is_empty())
            .map(|t| AnnotatedToken {
                text: t.to_string(),
                ner_tag: "O".to_string(),
            })
            .collect();

        if tokens.is_empty() {
            continue;
        }

        sentences.push(AnnotatedSentence {
            tokens,
            source_dataset: id,
        });
    }

    if sentences.is_empty() {
        return Err(Error::InvalidInput(format!(
            "Google relation-extraction-corpus file for {:?} contains no usable evidence snippets",
            id
        )));
    }

    let now = chrono::Utc::now().to_rfc3339();
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

/// Parse DocRED/CrossRE format for relation extraction.
///
/// Format: JSONL with {"sentence": [...], "ner": [[start, end, type], ...], "relations": [[id1-start, id1-end, id2-start, id2-end, rel-type, ...], ...]}
pub(crate) fn parse_docred_relations(content: &str) -> Result<Vec<RelationDocument>> {
    use crate::eval::relation::RelationGold;

    let mut documents = Vec::new();

    // Parse as JSONL (one JSON object per line)
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let doc: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        // Get sentence tokens
        let tokens_arr = match doc.get("sentence").and_then(|v| v.as_array()) {
            Some(t) => t,
            None => continue,
        };

        // Build text from tokens (with proper spacing)
        let text: String = tokens_arr
            .iter()
            .filter_map(|t| t.as_str())
            .collect::<Vec<_>>()
            .join(" ");

        // Build token-to-character offset mapping
        // This maps each token index to its character start position in the text
        let mut token_to_char: Vec<usize> = Vec::new();
        let mut char_pos = 0;
        for (i, token) in tokens_arr.iter().enumerate() {
            if let Some(tok_str) = token.as_str() {
                token_to_char.push(char_pos);
                // Add token length + 1 for space (except last token)
                char_pos += tok_str.len();
                if i < tokens_arr.len() - 1 {
                    char_pos += 1; // Space between tokens
                }
            } else {
                token_to_char.push(char_pos);
            }
        }

        // Get NER spans: [start, end, type]
        let ner_spans = doc.get("ner").and_then(|v| v.as_array());

        // Build entity map: (token_start, token_end) -> (type, text, char_start, char_end)
        let mut entity_map: std::collections::HashMap<
            (usize, usize),
            (String, String, usize, usize),
        > = std::collections::HashMap::new();
        if let Some(ner) = ner_spans {
            for span in ner {
                if let Some(arr) = span.as_array() {
                    if arr.len() >= 3 {
                        let token_start = arr[0].as_u64().unwrap_or(0) as usize;
                        let token_end = arr[1].as_u64().unwrap_or(0) as usize;
                        let ent_type = arr[2].as_str().unwrap_or("ENTITY").to_string();

                        // Extract entity text from tokens
                        let entity_text: String = tokens_arr
                            .iter()
                            .skip(token_start)
                            .take(token_end - token_start + 1)
                            .filter_map(|t| t.as_str())
                            .collect::<Vec<_>>()
                            .join(" ");

                        // Calculate actual character offsets
                        let char_start = token_to_char.get(token_start).copied().unwrap_or(0);
                        let char_end = if token_end < token_to_char.len() {
                            // Get end position of last token
                            let last_token_char_start = token_to_char[token_end];
                            if let Some(last_token) =
                                tokens_arr.get(token_end).and_then(|t| t.as_str())
                            {
                                last_token_char_start + last_token.len()
                            } else {
                                char_start + entity_text.len()
                            }
                        } else {
                            char_start + entity_text.len()
                        };

                        entity_map.insert(
                            (token_start, token_end),
                            (ent_type, entity_text, char_start, char_end),
                        );
                    }
                }
            }
        }

        // Parse relations: [id1-start, id1-end, id2-start, id2-end, rel-type, ...]
        let relations_arr = doc.get("relations").and_then(|v| v.as_array());
        let mut relations = Vec::new();

        if let Some(rels) = relations_arr {
            for rel in rels {
                if let Some(arr) = rel.as_array() {
                    if arr.len() >= 5 {
                        let head_token_start = arr[0].as_u64().unwrap_or(0) as usize;
                        let head_token_end = arr[1].as_u64().unwrap_or(0) as usize;
                        let tail_token_start = arr[2].as_u64().unwrap_or(0) as usize;
                        let tail_token_end = arr[3].as_u64().unwrap_or(0) as usize;
                        let rel_type = arr[4].as_str().unwrap_or("RELATION").to_string();

                        // Get entity info from map (including character offsets)
                        let (head_type, head_text, head_char_start, head_char_end) = entity_map
                            .get(&(head_token_start, head_token_end))
                            .cloned()
                            .unwrap_or_else(|| {
                                // Fallback: compute from token positions
                                let char_start =
                                    token_to_char.get(head_token_start).copied().unwrap_or(0);
                                let char_end = if head_token_end < token_to_char.len() {
                                    let last_start = token_to_char[head_token_end];
                                    if let Some(last_tok) =
                                        tokens_arr.get(head_token_end).and_then(|t| t.as_str())
                                    {
                                        last_start + last_tok.len()
                                    } else {
                                        char_start
                                    }
                                } else {
                                    char_start
                                };
                                ("ENTITY".to_string(), String::new(), char_start, char_end)
                            });

                        let (tail_type, tail_text, tail_char_start, tail_char_end) = entity_map
                            .get(&(tail_token_start, tail_token_end))
                            .cloned()
                            .unwrap_or_else(|| {
                                // Fallback: compute from token positions
                                let char_start =
                                    token_to_char.get(tail_token_start).copied().unwrap_or(0);
                                let char_end = if tail_token_end < token_to_char.len() {
                                    let last_start = token_to_char[tail_token_end];
                                    if let Some(last_tok) =
                                        tokens_arr.get(tail_token_end).and_then(|t| t.as_str())
                                    {
                                        last_start + last_tok.len()
                                    } else {
                                        char_start
                                    }
                                } else {
                                    char_start
                                };
                                ("ENTITY".to_string(), String::new(), char_start, char_end)
                            });

                        relations.push(RelationGold::new(
                            (head_char_start, head_char_end),
                            head_type,
                            head_text,
                            (tail_char_start, tail_char_end),
                            tail_type,
                            tail_text,
                            rel_type,
                        ));
                    }
                }
            }
        }

        if !text.is_empty() {
            documents.push(RelationDocument { text, relations });
        }
    }

    Ok(documents)
}

/// Parse CHisIEC (Chinese Historical Information Extraction Corpus) NER format.
pub(crate) fn parse_chisiec(content: &str, id: DatasetId) -> Result<LoadedDataset> {
    let mut sentences = Vec::new();

    // CHisIEC RE data is a JSON array
    let docs: Vec<serde_json::Value> = serde_json::from_str(content)
        .map_err(|e| Error::InvalidInput(format!("Failed to parse CHisIEC JSON: {}", e)))?;

    for doc in docs {
        // Get tokens string (characters concatenated, no spaces in Chinese)
        let text = match doc.get("tokens").and_then(|v| v.as_str()) {
            Some(t) => t.to_string(),
            None => continue,
        };

        if text.is_empty() {
            continue;
        }

        // Chinese text: each character is a token
        let chars: Vec<char> = text.chars().collect();
        let mut tokens: Vec<AnnotatedToken> = chars
            .iter()
            .map(|c| AnnotatedToken {
                text: c.to_string(),
                ner_tag: "O".to_string(),
            })
            .collect();

        // Get entities array and build BIO tags
        let entities_arr = doc.get("entities").and_then(|v| v.as_array());

        if let Some(entities) = entities_arr {
            for entity in entities {
                let ent_type = entity
                    .get("type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("ENTITY");
                let start = entity.get("start").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                let end = entity.get("end").and_then(|v| v.as_u64()).unwrap_or(0) as usize;

                // Apply BIO tags (CHisIEC uses character indices)
                for idx in start..end {
                    if idx < tokens.len() {
                        tokens[idx].ner_tag = if idx == start {
                            format!("B-{}", ent_type)
                        } else {
                            format!("I-{}", ent_type)
                        };
                    }
                }
            }
        }

        if !tokens.is_empty() {
            sentences.push(AnnotatedSentence {
                tokens,
                source_dataset: id,
            });
        }
    }

    if sentences.is_empty() {
        return Err(Error::InvalidInput(format!(
            "CHisIEC file for {:?} contains no valid sentences",
            id
        )));
    }

    let now = chrono::Utc::now().to_rfc3339();
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

/// Parse CHisIEC relation extraction format.
pub(crate) fn parse_chisiec_relations(content: &str) -> Result<Vec<RelationDocument>> {
    use crate::eval::relation::RelationGold;

    let mut documents = Vec::new();

    // Parse as JSON array
    let docs: Vec<serde_json::Value> = serde_json::from_str(content)
        .map_err(|e| Error::InvalidInput(format!("Failed to parse CHisIEC JSON: {}", e)))?;

    for doc in docs {
        // Get tokens string
        let text = match doc.get("tokens").and_then(|v| v.as_str()) {
            Some(t) => t.to_string(),
            None => continue,
        };

        if text.is_empty() {
            continue;
        }

        // Build entity list: [(type, start, end, text), ...]
        let entities_arr = doc.get("entities").and_then(|v| v.as_array());
        let mut entity_list: Vec<(String, usize, usize, String)> = Vec::new();

        if let Some(entities) = entities_arr {
            for entity in entities {
                let ent_type = entity
                    .get("type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("ENTITY")
                    .to_string();
                let start = entity.get("start").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                let end = entity.get("end").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                let span = entity
                    .get("span")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                // Fallback to extracting from text if span is empty
                let span_text = if !span.is_empty() {
                    span
                } else {
                    text.chars().skip(start).take(end - start).collect()
                };

                entity_list.push((ent_type, start, end, span_text));
            }
        }

        // Parse relations
        let relations_arr = doc.get("relations").and_then(|v| v.as_array());
        let mut relations = Vec::new();

        if let Some(rels) = relations_arr {
            for rel in rels {
                let rel_type = rel
                    .get("type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("RELATION")
                    .to_string();
                // CHisIEC uses entity indices for head/tail
                let head_idx = rel.get("head").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                let tail_idx = rel.get("tail").and_then(|v| v.as_u64()).unwrap_or(0) as usize;

                // Look up entities by index
                if head_idx < entity_list.len() && tail_idx < entity_list.len() {
                    let (head_type, head_start, head_end, head_text) = &entity_list[head_idx];
                    let (tail_type, tail_start, tail_end, tail_text) = &entity_list[tail_idx];

                    relations.push(RelationGold::new(
                        (*head_start, *head_end),
                        head_type.clone(),
                        head_text.clone(),
                        (*tail_start, *tail_end),
                        tail_type.clone(),
                        tail_text.clone(),
                        rel_type,
                    ));
                }
            }
        }

        if !text.is_empty() {
            documents.push(RelationDocument { text, relations });
        }
    }

    Ok(documents)
}
