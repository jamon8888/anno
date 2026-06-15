//! Coreference-related dataset parsers as free functions.

use crate::eval::coref::CorefDocument;
use crate::eval::loader::types::{AnnotatedSentence, AnnotatedToken, DataSource, LoadedDataset};
use crate::eval::loader::DatasetLoader;
use crate::eval::loader::DatasetId;
use anno::{Error, Result};

pub(crate) fn parse_gap(content: &str, id: DatasetId) -> Result<LoadedDataset> {
    let mut sentences = Vec::new();
    let mut first_line = true;

    for line in content.lines() {
        // Skip header
        if first_line {
            first_line = false;
            continue;
        }

        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() < 10 {
            continue;
        }

        let text = parts[1];

        // Create tokens (whitespace tokenization for simplicity)
        let tokens: Vec<AnnotatedToken> = text
            .split_whitespace()
            .map(|w| AnnotatedToken {
                text: w.to_string(),
                ner_tag: "O".to_string(),
            })
            .collect();

        if !tokens.is_empty() {
            sentences.push(AnnotatedSentence {
                tokens,
                source_dataset: id,
            });
        }
    }

    if sentences.is_empty() {
        return Err(Error::InvalidInput(format!(
            "GAP TSV file for {:?} contains no valid sentences",
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

/// Parse PreCo JSONL format from HuggingFace.
///
/// PreCo JSONL format: One JSON object per line with "sentences" array.
/// Note: PreCo is a coreference dataset, not NER. This parser extracts sentences
/// for NER evaluation (which will have 0 entities). Use `load_coref()` for coreference.
pub(crate) fn parse_preco_jsonl(content: &str, id: DatasetId) -> Result<LoadedDataset> {
    let mut sentences = Vec::new();
    let mut line_count = 0usize;
    let mut parsed_count = 0usize;

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        line_count += 1;

        let parsed: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(e) => {
                // Log first few parse errors for debugging
                if parsed_count < 3 {
                    log::warn!("PreCo JSONL parse error on line {}: {}", line_count, e);
                }
                continue; // Skip malformed lines
            }
        };

        // PreCo format: {"sentences": [[token1, token2, ...], ...]}
        if let Some(sents) = parsed.get("sentences").and_then(|v| v.as_array()) {
            parsed_count += 1;
            for sent_tokens in sents {
                if let Some(token_array) = sent_tokens.as_array() {
                    let tokens: Vec<AnnotatedToken> = token_array
                        .iter()
                        .filter_map(|t| t.as_str())
                        .map(|t| AnnotatedToken {
                            text: t.to_string(),
                            ner_tag: "O".to_string(), // PreCo has no NER annotations
                        })
                        .collect();

                    if !tokens.is_empty() {
                        sentences.push(AnnotatedSentence {
                            tokens,
                            source_dataset: id,
                        });
                    }
                }
            }
        }
    }

    if sentences.is_empty() {
        return Err(Error::InvalidInput(format!(
            "PreCo JSONL file contains no valid sentences (parsed {} of {} lines)",
            parsed_count, line_count
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

/// Parse LitBank annotation format for NER.
///
/// LitBank .ann format: T<id>\t<Type> <start> <end>\t<text>
/// Note: LitBank is primarily a coreference dataset. This parser extracts
/// entity mentions as NER annotations, but LitBank should be used with
/// `load_coref()` for proper coreference evaluation.
pub(crate) fn parse_litbank(content: &str, id: DatasetId) -> Result<LoadedDataset> {
    // LitBank .ann format: each line is T<id>\t<Type> <start> <end>\t<text>
    // Extract entity mentions as NER annotations
    let now = chrono::Utc::now().to_rfc3339();
    let mut sentences = Vec::new();
    let mut entities: Vec<(usize, usize, String, String)> = Vec::new(); // (start, end, text, label)

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if line.starts_with('T') {
            // Entity annotation: T1\tPER 0 5\tAlice
            // LitBank uses ACE-style labels like PROP_PER, NOM_LOC, PRON_FAC etc.
            // We normalize to just the entity type (PER, LOC, ORG, GPE, FAC, VEH)
            // Note: LitBank also has coreference chain annotations like "character_name-ID"
            // which should be skipped for NER evaluation
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 3 {
                let type_span: Vec<&str> = parts[1].split_whitespace().collect();
                if type_span.len() >= 3 {
                    let raw_label = type_span[0];

                    // Valid entity types (with or without prefix)
                    const VALID_ENTITY_TYPES: &[&str] = &[
                        "PER",
                        "LOC",
                        "ORG",
                        "GPE",
                        "FAC",
                        "VEH",
                        "PERSON",
                        "LOCATION",
                        "ORGANIZATION",
                    ];

                    // Entity type annotations start with PROP_, NOM_, PRON_ OR are plain types
                    let is_prefixed_entity = raw_label.starts_with("PROP_")
                        || raw_label.starts_with("NOM_")
                        || raw_label.starts_with("PRON_");
                    let is_plain_entity = VALID_ENTITY_TYPES.contains(&raw_label);

                    if !is_prefixed_entity && !is_plain_entity {
                        // Skip coreference chain annotations (e.g., "jarndyce_2-73")
                        continue;
                    }

                    // Normalize: PROP_PER -> PER, NOM_LOC -> LOC, PRON_FAC -> FAC
                    let label = if is_prefixed_entity {
                        raw_label.split('_').next_back().unwrap_or(raw_label)
                    } else {
                        raw_label
                    };
                    let start: usize = type_span[1].parse().unwrap_or(0);
                    let end: usize = type_span[2].parse().unwrap_or(0);
                    let text = parts[2];

                    entities.push((start, end, text.to_string(), label.to_string()));
                }
            }
        }
    }

    if entities.is_empty() {
        return Err(Error::InvalidInput(
            "LitBank .ann file contains no entity annotations (T lines)".to_string(),
        ));
    }

    // Sort entities by start position
    entities.sort_by_key(|(start, _, _, _)| *start);

    // Reconstruct text and create tokens with NER tags
    let max_end = entities
        .iter()
        .map(|(_, end, _, _)| *end)
        .max()
        .unwrap_or(0);
    let mut text_chars: Vec<char> = vec![' '; max_end.max(1)];
    let mut token_starts: Vec<usize> = Vec::new();

    // Fill in entity text
    for (start, _end, text, _) in &entities {
        let text_chars_vec: Vec<char> = text.chars().collect();
        let _actual_end = (*start + text_chars_vec.len()).min(text_chars.len());
        if *start < text_chars.len() {
            for (i, ch) in text_chars_vec.iter().enumerate() {
                let pos = *start + i;
                if pos < text_chars.len() {
                    text_chars[pos] = *ch;
                }
            }
        }
        token_starts.push(*start);
    }

    // Note: text reconstruction is not used directly since we tokenize from entity text
    // but we keep it for potential future use (e.g., validation)
    let _text: String = text_chars
        .into_iter()
        .collect::<String>()
        .trim()
        .to_string();

    // Create tokens with NER tags
    // Improved approach: tokenize entity text into words and apply BIO tags
    let mut tokens: Vec<AnnotatedToken> = Vec::new();

    // Sort entities by start position for processing
    entities.sort_by_key(|(start, _, _, _)| *start);

    let mut last_end = 0usize;
    for (start, end, entity_text, label) in &entities {
        // Add "O" tokens for gaps between entities
        if *start > last_end {
            // For gaps, we can't reconstruct the actual text without the .txt file
            // But we can estimate based on character distance
            let gap_size = *start - last_end;
            if gap_size > 0 {
                // Estimate number of words in gap (roughly 5 chars per word + space)
                let estimated_words = (gap_size / 6).max(1);
                for _ in 0..estimated_words.min(10) {
                    // Limit gap tokens to avoid excessive placeholders
                    tokens.push(AnnotatedToken {
                        text: "[...]".to_string(),
                        ner_tag: "O".to_string(),
                    });
                }
            }
        }

        // Tokenize entity text into words and apply BIO tags
        let entity_words: Vec<&str> = entity_text.split_whitespace().collect();
        for (i, word) in entity_words.iter().enumerate() {
            let ner_tag = if i == 0 {
                format!("B-{}", label)
            } else {
                format!("I-{}", label)
            };
            tokens.push(AnnotatedToken {
                text: word.to_string(),
                ner_tag,
            });
        }

        last_end = *end;
    }

    if !tokens.is_empty() {
        sentences.push(AnnotatedSentence {
            tokens,
            source_dataset: id,
        });
    }

    if sentences.is_empty() {
        return Err(Error::InvalidInput(
            "LitBank file produced no sentences after parsing".to_string(),
        ));
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

/// Parse ECB+ CSV format for event coreference.
///
/// ECB+ uses CSV format with columns for event mentions and coreference links.
/// For now, extracts entities as NER annotations (event triggers).
pub(crate) fn parse_ecb_plus(content: &str, id: DatasetId) -> Result<LoadedDataset> {
    let mut sentences = Vec::new();
    let mut first_line = true;

    for line in content.lines() {
        // Skip header
        if first_line {
            first_line = false;
            continue;
        }

        let parts: Vec<&str> = line.split(',').collect();
        if parts.len() < 3 {
            continue;
        }

        // ECB+ CSV format: sentence_id, text, event_mention, ...
        // Extract text and create tokens
        let text = parts.get(1).unwrap_or(&"");
        let tokens: Vec<AnnotatedToken> = text
            .split_whitespace()
            .map(|w| AnnotatedToken {
                text: w.to_string(),
                ner_tag: "O".to_string(),
            })
            .collect();

        if !tokens.is_empty() {
            sentences.push(AnnotatedSentence {
                tokens,
                source_dataset: id,
            });
        }
    }

    if sentences.is_empty() {
        return Err(Error::InvalidInput(format!(
            "ECB+ CSV file for {:?} contains no valid sentences",
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

/// Parse LitBank for coreference chains.
///
/// LitBank format: .ann files with T lines (mentions) and R lines (coreference relations).
/// T lines: T<id>\t<Type> <start> <end>\t<text>
/// R lines: R<id>\tCoref Arg1:T<id1> Arg2:T<id2>
///
/// Note: LitBank .ann files reference character offsets in corresponding .txt files.
/// This parser reconstructs text from mentions, but ideally should read .txt file.
pub(crate) fn parse_litbank_coref(content: &str) -> Result<Vec<CorefDocument>> {
    use crate::eval::coref::{CorefChain, Mention};
    use std::collections::HashMap;

    // LitBank .ann format includes coreference with R lines
    // R1\tCoref Arg1:T1 Arg2:T2
    let mut mentions: HashMap<String, Mention> = HashMap::new();
    let mut coref_links: Vec<(String, String)> = Vec::new();
    let mut max_end = 0usize;

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if line.starts_with('T') {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 3 {
                let id = parts[0];
                let type_span: Vec<&str> = parts[1].split_whitespace().collect();
                if type_span.len() >= 3 {
                    let start: usize = type_span[1].parse().unwrap_or(0);
                    let end: usize = type_span[2].parse().unwrap_or(0);
                    let text = parts[2];
                    max_end = max_end.max(end);
                    mentions.insert(id.to_string(), Mention::new(text, start, end));
                }
            }
        } else if line.starts_with('R') && line.contains("Coref") {
            // R1\tCoref Arg1:T1 Arg2:T2
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 3 {
                let arg1 = parts[1].trim_start_matches("Arg1:");
                let arg2 = parts[2].trim_start_matches("Arg2:");
                coref_links.push((arg1.to_string(), arg2.to_string()));
            }
        }
    }

    if mentions.is_empty() {
        return Err(Error::InvalidInput(
            "LitBank file contains no mentions (T lines)".to_string(),
        ));
    }

    // Reconstruct text from mentions by sorting by start offset
    let mut sorted_mentions: Vec<(usize, &Mention)> =
        mentions.values().map(|m| (m.start, m)).collect();
    sorted_mentions.sort_by_key(|(start, _)| *start);

    // Build text by inserting mentions at their offsets
    let mut text_chars: Vec<char> = vec![' '; max_end.max(1)];
    for (start, mention) in &sorted_mentions {
        let mention_text: Vec<char> = mention.text.chars().collect();
        let _end = (*start + mention_text.len()).min(text_chars.len());
        if *start < text_chars.len() {
            for (i, ch) in mention_text.iter().enumerate() {
                let pos = *start + i;
                if pos < text_chars.len() {
                    text_chars[pos] = *ch;
                }
            }
        }
    }
    let text: String = text_chars
        .into_iter()
        .collect::<String>()
        .trim()
        .to_string();

    // Build chains from links using union-find
    let mut chains: Vec<Vec<Mention>> = Vec::new();
    let mut mention_to_chain: HashMap<String, usize> = HashMap::new();

    // First, add all mentions as singletons if they're not in any chain
    for (id, mention) in &mentions {
        if !mention_to_chain.contains_key(id) {
            let idx = chains.len();
            chains.push(vec![mention.clone()]);
            mention_to_chain.insert(id.clone(), idx);
        }
    }

    // Then merge chains based on coref links
    for (id1, id2) in coref_links {
        let chain_idx = match (mention_to_chain.get(&id1), mention_to_chain.get(&id2)) {
            (Some(&idx1), Some(&idx2)) if idx1 != idx2 => {
                // Merge chains
                let to_merge = std::mem::take(&mut chains[idx2]);
                chains[idx1].extend(to_merge);
                // Update all mentions in merged chain to point to idx1
                for m in &chains[idx1] {
                    // Find mention ID by matching text and position
                    for (mid, mref) in &mentions {
                        if mref.text == m.text && mref.start == m.start {
                            mention_to_chain.insert(mid.clone(), idx1);
                        }
                    }
                }
                idx1
            }
            (Some(&idx), None) => {
                if let Some(m) = mentions.get(&id2) {
                    chains[idx].push(m.clone());
                    mention_to_chain.insert(id2, idx);
                }
                idx
            }
            (None, Some(&idx)) => {
                if let Some(m) = mentions.get(&id1) {
                    chains[idx].push(m.clone());
                    mention_to_chain.insert(id1, idx);
                }
                idx
            }
            (None, None) => {
                let idx = chains.len();
                let mut chain = Vec::new();
                if let Some(m) = mentions.get(&id1) {
                    chain.push(m.clone());
                    mention_to_chain.insert(id1.clone(), idx);
                }
                if let Some(m) = mentions.get(&id2) {
                    chain.push(m.clone());
                    mention_to_chain.insert(id2, idx);
                }
                if !chain.is_empty() {
                    chains.push(chain);
                }
                idx
            }
            (Some(&idx), Some(_)) => idx,
        };
        let _ = chain_idx; // Used above
    }

    // Filter empty chains and convert
    let coref_chains: Vec<CorefChain> = chains
        .into_iter()
        .filter(|c| !c.is_empty())
        .enumerate()
        .map(|(i, mentions)| CorefChain::with_id(mentions, i as u64))
        .collect();

    if coref_chains.is_empty() {
        return Err(Error::InvalidInput(
            "LitBank file contains no coreference chains".to_string(),
        ));
    }

    // Create document with reconstructed text
    let doc = CorefDocument::new(&text, coref_chains);
    Ok(vec![doc])
}
