//! NER dataset parsers as free functions.
//!
//! Each function was extracted from `impl DatasetLoader` and takes the same
//! parameters as before (minus `&self` which was never used).

use crate::eval::loader::types::{AnnotatedSentence, AnnotatedToken, DataSource, LoadedDataset};
use crate::eval::loader::DatasetId;
use crate::eval::loader::DatasetLoader;
use anno::{Error, Result};

/// Parse CoNLL/BIO format content.
pub(crate) fn parse_conll(content: &str, id: DatasetId) -> Result<LoadedDataset> {
    let mut sentences = Vec::new();
    let mut current_tokens = Vec::new();

    // Detect format: MIT datasets use TAB separator with TAG first
    let is_mit_format = matches!(id, DatasetId::MitMovie | DatasetId::MitRestaurant);

    for line in content.lines() {
        let line = line.trim();

        // Empty line = sentence boundary
        if line.is_empty() {
            if !current_tokens.is_empty() {
                sentences.push(AnnotatedSentence {
                    tokens: std::mem::take(&mut current_tokens),
                    source_dataset: id,
                });
            }
            continue;
        }

        // Skip document markers
        if line.starts_with("-DOCSTART-") {
            continue;
        }

        // Parse based on format
        let (text, ner_tag) = if is_mit_format {
            // MIT format: TAG\tword (tab-separated)
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 2 {
                (parts[1].to_string(), parts[0].to_string())
            } else {
                continue;
            }
        } else {
            // Standard CoNLL/BIO format (space-separated)
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.is_empty() {
                continue;
            }

            if parts.len() >= 4 {
                // CoNLL-2003 format: word POS chunk NER
                (parts[0].to_string(), parts[3].to_string())
            } else if parts.len() >= 2 {
                // BIO format: word NER
                (parts[0].to_string(), parts[parts.len() - 1].to_string())
            } else {
                // Single column - assume O tag
                (parts[0].to_string(), "O".to_string())
            }
        };

        // Normalize BIO tags: some corpora (e.g., WikiGold) use `I-XXX` even at the
        // beginning of an entity. Treat such `I-` tags as `B-` when they don't
        // continue an entity of the same type.
        let ner_tag = if let Some(label) = ner_tag.strip_prefix("I-") {
            let continues_same = current_tokens
                .last()
                .map(|t: &AnnotatedToken| t.ner_tag.as_str())
                .is_some_and(|prev| {
                    (prev.starts_with("B-") || prev.starts_with("I-"))
                        && prev.get(2..).is_some_and(|prev_label| prev_label == label)
                });

            if continues_same {
                ner_tag
            } else {
                format!("B-{}", label)
            }
        } else {
            ner_tag
        };

        current_tokens.push(AnnotatedToken { text, ner_tag });
    }

    // Don't forget last sentence
    if !current_tokens.is_empty() {
        sentences.push(AnnotatedSentence {
            tokens: current_tokens,
            source_dataset: id,
        });
    }

    if sentences.is_empty() {
        return Err(Error::InvalidInput(format!(
            "CoNLL file for {:?} contains no valid sentences",
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

/// Parse JSONL NER format (HuggingFace style, e.g., MultiNERD).
///
/// Expected format: `{"tokens": ["word1", "word2"], "ner_tags": [0, 1, 0]}`
pub(crate) fn parse_jsonl_ner(content: &str, id: DatasetId) -> Result<LoadedDataset> {
    let mut sentences = Vec::new();

    // MultiNERD tag mapping (index -> label), used only when `ner_tags` are integers.
    //
    // CAVEAT: this function is also routed to for 50+ other JsonlNer datasets
    // (see DatasetParsePlan::JsonlNer in types.rs). Datasets whose `ner_tags` are
    // integer indices into a *different* schema than MultiNERD's will silently
    // get wrong labels here.
    let tag_labels = [
        "O", "B-PER", "I-PER", "B-ORG", "I-ORG", "B-LOC", "I-LOC", "B-ANIM", "I-ANIM", "B-BIO",
        "I-BIO", "B-CEL", "I-CEL", "B-DIS", "I-DIS", "B-EVE", "I-EVE", "B-FOOD", "I-FOOD",
        "B-INST", "I-INST", "B-MEDIA", "I-MEDIA", "B-MYTH", "I-MYTH", "B-PLANT", "I-PLANT",
        "B-TIME", "I-TIME", "B-VEHI", "I-VEHI",
    ];

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // Parse JSON line
        let parsed: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue, // Skip malformed lines
        };

        let tokens = match parsed.get("tokens").and_then(|v| v.as_array()) {
            Some(t) => t,
            None => continue,
        };

        let ner_tags = match parsed.get("ner_tags").and_then(|v| v.as_array()) {
            Some(t) => t,
            None => continue,
        };

        if tokens.len() != ner_tags.len() {
            continue; // Skip malformed entries
        }

        let mut annotated_tokens = Vec::new();
        for (token, tag) in tokens.iter().zip(ner_tags.iter()) {
            let text = token.as_str().unwrap_or("").to_string();
            // String tags are dataset-provided labels; use them directly rather than
            // forcing them through the (MultiNERD-specific) integer-index table.
            let ner_tag = if let Some(tag_str) = tag.as_str() {
                tag_str.to_string()
            } else {
                let tag_idx = tag.as_u64().unwrap_or(0) as usize;
                tag_labels.get(tag_idx).unwrap_or(&"O").to_string()
            };
            annotated_tokens.push(AnnotatedToken { text, ner_tag });
        }

        if !annotated_tokens.is_empty() {
            sentences.push(AnnotatedSentence {
                tokens: annotated_tokens,
                source_dataset: id,
            });
        }
    }

    if sentences.is_empty() {
        return Err(Error::InvalidInput(format!(
            "JSONL NER file for {:?} contains no valid sentences",
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

/// Parse HuggingFace datasets-server API response.
///
/// Expected format:
/// ```json
/// {
///   "features": [{"name": "tokens", ...}, {"name": "ner_tags", ...}],
///   "rows": [{"row_idx": 0, "row": {"tokens": [...], "ner_tags": [...]}}, ...]
/// }
/// ```
pub(crate) fn parse_hf_api_response(content: &str, id: DatasetId) -> Result<LoadedDataset> {
    let parsed: serde_json::Value = serde_json::from_str(content)
        .map_err(|e| Error::InvalidInput(format!("Failed to parse HF API response: {}", e)))?;

    let mut sentences = Vec::new();

    // Extract tag names from features if available (for integer tag mapping)
    let tag_names = super::util::extract_tag_names_from_features(&parsed);
    let class_names = super::util::extract_class_names_from_features(&parsed);

    let rows = parsed
        .get("rows")
        .and_then(|v| v.as_array())
        .ok_or_else(|| Error::InvalidInput("No 'rows' array in HF API response".to_string()))?;

    for row_obj in rows {
        let row = match row_obj.get("row") {
            Some(r) => r,
            None => continue,
        };

        // Primary path: token-level NER rows (`tokens` + `ner_tags`)
        if let (Some(tokens), Some(ner_tags)) = (
            row.get("tokens").and_then(|v| v.as_array()),
            row.get("ner_tags").and_then(|v| v.as_array()),
        ) {
            if tokens.len() != ner_tags.len() {
                continue;
            }

            let mut annotated_tokens = Vec::new();
            for (token, tag) in tokens.iter().zip(ner_tags.iter()) {
                let text = token.as_str().unwrap_or("").to_string();

                // Handle both integer and string tags
                let ner_tag = if let Some(tag_idx) = tag.as_u64() {
                    // Integer tag - map using feature names or default
                    tag_names
                        .get(tag_idx as usize)
                        .cloned()
                        .unwrap_or_else(|| format!("TAG_{}", tag_idx))
                } else if let Some(tag_str) = tag.as_str() {
                    // String tag - use directly
                    tag_str.to_string()
                } else {
                    "O".to_string()
                };

                annotated_tokens.push(AnnotatedToken { text, ner_tag });
            }

            if !annotated_tokens.is_empty() {
                sentences.push(AnnotatedSentence {
                    tokens: annotated_tokens,
                    source_dataset: id,
                });
            }
            continue;
        }

        // Fallback: temporal standoff rows (`text` + `*_expressions` with char offsets).
        //
        // Some HF datasets expose TIMEX/EVENT spans as character ranges in lists like:
        // - `time_expressions`: [{start_char, end_char, ...}, ...]
        // - `event_expressions`: ...
        // - `signal_expressions`: ...
        //
        // We tokenize `text` by whitespace and assign BIO tags based on overlap with any span.
        if let Some(text) = row.get("text").and_then(|v| v.as_str()).map(str::trim) {
            let has_temporal_spans = row
                .get("time_expressions")
                .and_then(|v| v.as_array())
                .is_some()
                || row
                    .get("event_expressions")
                    .and_then(|v| v.as_array())
                    .is_some()
                || row
                    .get("signal_expressions")
                    .and_then(|v| v.as_array())
                    .is_some();

            if has_temporal_spans && !text.is_empty() {
                // Tokenize by whitespace, tracking char offsets (Rust `char` count).
                let mut tokens: Vec<(String, usize, usize)> = Vec::new();
                let mut cur = String::new();
                let mut cur_start: Option<usize> = None;

                for (i, ch) in text.chars().enumerate() {
                    if ch.is_whitespace() {
                        if let Some(s) = cur_start.take() {
                            let e = i;
                            if !cur.is_empty() {
                                tokens.push((std::mem::take(&mut cur), s, e));
                            }
                        }
                    } else {
                        if cur_start.is_none() {
                            cur_start = Some(i);
                        }
                        cur.push(ch);
                    }
                }
                if let Some(s) = cur_start.take() {
                    let e = text.chars().count();
                    if !cur.is_empty() {
                        tokens.push((cur, s, e));
                    }
                }

                if tokens.is_empty() {
                    continue;
                }

                let timex_spans = super::util::spans_from_array(
                    row.get("time_expressions").and_then(|v| v.as_array()),
                );
                let event_spans = super::util::spans_from_array(
                    row.get("event_expressions").and_then(|v| v.as_array()),
                );
                let signal_spans = super::util::spans_from_array(
                    row.get("signal_expressions").and_then(|v| v.as_array()),
                );

                let mut annotated_tokens = Vec::with_capacity(tokens.len());
                let mut prev_label: Option<&'static str> = None;
                for (tok, s, e) in tokens {
                    let label = if super::util::overlaps(s, e, &timex_spans) {
                        Some("TIMEX")
                    } else if super::util::overlaps(s, e, &event_spans) {
                        Some("EVENT")
                    } else if super::util::overlaps(s, e, &signal_spans) {
                        Some("SIGNAL")
                    } else {
                        None
                    };

                    let ner_tag = match (label, prev_label) {
                        (None, _) => {
                            prev_label = None;
                            "O".to_string()
                        }
                        (Some(l), Some(p)) if l == p => format!("I-{}", l),
                        (Some(l), _) => {
                            prev_label = Some(l);
                            format!("B-{}", l)
                        }
                    };

                    annotated_tokens.push(AnnotatedToken { text: tok, ner_tag });
                }

                sentences.push(AnnotatedSentence {
                    tokens: annotated_tokens,
                    source_dataset: id,
                });
                continue;
            }
        }

        // Fallback: DISRPT-style CoNLL-U rows for discourse segmentation.
        //
        // DISRPT `*.conllu` configs are exported via HF datasets-server as arrays:
        // - `form`: token surface strings
        // - `misc`: per-token features, including `Seg=B-seg` (segment boundary) or `Seg=O`
        //
        // We convert boundaries into BIO tags of a single entity type (`SEG`), treating each
        // segment (EDU) as an entity span.
        if let (Some(forms), Some(misc)) = (
            row.get("form").and_then(|v| v.as_array()),
            row.get("misc").and_then(|v| v.as_array()),
        ) {
            if !forms.is_empty() && forms.len() == misc.len() {
                let mut annotated_tokens = Vec::with_capacity(forms.len());
                let mut in_seg = false;
                for (i, (f, m)) in forms.iter().zip(misc.iter()).enumerate() {
                    let tok = f.as_str().unwrap_or("").to_string();
                    let misc_s = m.as_str().unwrap_or("");
                    let start = misc_s.contains("Seg=B-seg");
                    let ner_tag = if i == 0 || start {
                        in_seg = true;
                        "B-SEG".to_string()
                    } else if in_seg {
                        "I-SEG".to_string()
                    } else {
                        "O".to_string()
                    };
                    annotated_tokens.push(AnnotatedToken { text: tok, ner_tag });
                }

                sentences.push(AnnotatedSentence {
                    tokens: annotated_tokens,
                    source_dataset: id,
                });
                continue;
            }
        }

        // Fallback: classification-ish rows (`<text>` + `label`).
        //
        // We encode the gold label as `B-<LABEL>` on a single token, matching the loader's
        // convention for other classification datasets.
        let text = if let Some(s) = row.get("text").and_then(|v| v.as_str()) {
            s.trim().to_string()
        } else if let (Some(a), Some(b)) = (
            row.get("unit1_txt").and_then(|v| v.as_str()),
            row.get("unit2_txt").and_then(|v| v.as_str()),
        ) {
            format!("{} [SEP] {}", a.trim(), b.trim())
        } else if let (Some(a), Some(b)) = (
            row.get("sentence1").and_then(|v| v.as_str()),
            row.get("sentence2").and_then(|v| v.as_str()),
        ) {
            format!("{} [SEP] {}", a.trim(), b.trim())
        } else if let (Some(a), Some(b)) = (
            row.get("premise").and_then(|v| v.as_str()),
            row.get("hypothesis").and_then(|v| v.as_str()),
        ) {
            format!("{} [SEP] {}", a.trim(), b.trim())
        } else {
            continue;
        };
        if text.trim().is_empty() {
            continue;
        }

        let label_value = row.get("label").or_else(|| row.get("labels"));
        let label = match label_value {
            Some(v) if v.is_string() => v.as_str().unwrap_or("").to_string(),
            Some(v) if v.is_number() => {
                let idx = v.as_u64().unwrap_or(0) as usize;
                class_names
                    .get(idx)
                    .cloned()
                    .unwrap_or_else(|| format!("LABEL_{}", idx))
            }
            _ => "label".to_string(),
        };
        if label.trim().is_empty() {
            continue;
        }

        sentences.push(AnnotatedSentence {
            tokens: vec![AnnotatedToken {
                text,
                ner_tag: format!("B-{}", label),
            }],
            source_dataset: id,
        });
    }

    if sentences.is_empty() {
        return Err(Error::InvalidInput(format!(
            "HF API response for {:?} contains no valid sentences",
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

/// Parse TweetNER7 JSON format.
///
/// TweetNER7 is JSONL with each line: {"tokens": [...], "tags": [...]}
/// Tag mapping from label.json (tag -> id format, we need id -> tag):
pub(crate) fn parse_tweetner7(content: &str, id: DatasetId) -> Result<LoadedDataset> {
    // TweetNER7 tag mapping from label.json (index order!)
    // {"B-corporation": 0, "B-creative_work": 1, "B-event": 2, "B-group": 3,
    //  "B-location": 4, "B-person": 5, "B-product": 6, "I-corporation": 7,
    //  "I-creative_work": 8, "I-event": 9, "I-group": 10, "I-location": 11,
    //  "I-person": 12, "I-product": 13, "O": 14}
    let tag_labels = [
        "B-corporation",   // 0
        "B-creative_work", // 1
        "B-event",         // 2
        "B-group",         // 3
        "B-location",      // 4
        "B-person",        // 5
        "B-product",       // 6
        "I-corporation",   // 7
        "I-creative_work", // 8
        "I-event",         // 9
        "I-group",         // 10
        "I-location",      // 11
        "I-person",        // 12
        "I-product",       // 13
        "O",               // 14
    ];

    let mut sentences = Vec::new();

    // Parse as JSONL (one JSON object per line)
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let parsed: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue, // Skip malformed lines
        };

        let tokens = match parsed.get("tokens").and_then(|v| v.as_array()) {
            Some(t) => t,
            None => continue,
        };

        let tags = match parsed.get("tags").and_then(|v| v.as_array()) {
            Some(t) => t,
            None => continue,
        };

        if tokens.len() != tags.len() {
            continue;
        }

        let mut annotated_tokens = Vec::new();
        for (token, tag) in tokens.iter().zip(tags.iter()) {
            let text = token.as_str().unwrap_or("").to_string();
            let tag_idx = tag.as_u64().unwrap_or(0) as usize;
            let ner_tag = tag_labels.get(tag_idx).unwrap_or(&"O").to_string();
            annotated_tokens.push(AnnotatedToken { text, ner_tag });
        }

        if !annotated_tokens.is_empty() {
            sentences.push(AnnotatedSentence {
                tokens: annotated_tokens,
                source_dataset: id,
            });
        }
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

/// Parse HIPE-2022 style TSV NER format.
///
/// Expected format:
/// ```text
/// TOKEN    NE-COARSE-LIT   NE-COARSE-METO   NE-FINE-LIT   ...
/// # hipe2022:document_id = doc123
/// word1    B-PER           _                B-pers.author ...
/// word2    I-PER           _                I-pers.author ...
/// word3    O               _                O             ...
/// ```
///
/// - First line is header (starts with TOKEN)
/// - Lines starting with `#` are metadata comments
/// - Data lines are tab-separated with token in first column
/// - NE-COARSE-LIT (column 2) contains BIO-tagged NER labels
/// - `_` means no annotation
pub(crate) fn parse_tsv_ner(content: &str, id: DatasetId) -> Result<LoadedDataset> {
    let mut sentences = Vec::new();
    let mut current_tokens = Vec::new();

    for line in content.lines() {
        let line = line.trim();

        // Skip empty lines - they indicate sentence boundaries
        if line.is_empty() {
            if !current_tokens.is_empty() {
                sentences.push(AnnotatedSentence {
                    tokens: std::mem::take(&mut current_tokens),
                    source_dataset: id,
                });
            }
            continue;
        }

        // Skip header line
        if line.starts_with("TOKEN\t") || line.starts_with("TOKEN ") {
            continue;
        }

        // Skip metadata comments
        if line.starts_with('#') {
            // Document boundary comment can also serve as sentence boundary
            if line.contains("document_id") && !current_tokens.is_empty() {
                sentences.push(AnnotatedSentence {
                    tokens: std::mem::take(&mut current_tokens),
                    source_dataset: id,
                });
            }
            continue;
        }

        // Parse data line
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() < 2 {
            continue; // Malformed line
        }

        let token_text = parts[0].to_string();
        let ner_label = parts.get(1).unwrap_or(&"O");

        // Convert underscore to O (no annotation)
        let ner_tag = if *ner_label == "_" || ner_label.is_empty() {
            "O".to_string()
        } else {
            ner_label.to_string()
        };

        current_tokens.push(AnnotatedToken {
            text: token_text,
            ner_tag,
        });
    }

    // Don't forget the last sentence
    if !current_tokens.is_empty() {
        sentences.push(AnnotatedSentence {
            tokens: current_tokens,
            source_dataset: id,
        });
    }

    if sentences.is_empty() {
        return Err(Error::InvalidInput(format!(
            "TSV NER file for {:?} contains no valid sentences",
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

/// Parse CSV NER format (E-NER/EDGAR-NER style).
///
/// Expected format: `Token,Tag` (comma-separated)
/// Uses `-DOCSTART-` for document boundaries and empty lines for sentence boundaries.
/// Tags use BIO scheme (e.g., O, B-PERSON, I-BUSINESS).
///
/// # Errors
///
/// Returns error if CSV is malformed or has no valid sentences.
pub(crate) fn parse_csv_ner(content: &str, id: DatasetId) -> Result<LoadedDataset> {
    let mut sentences = Vec::new();
    let mut current_tokens = Vec::new();

    for line in content.lines() {
        let line = line.trim();

        // Empty lines indicate sentence boundaries
        if line.is_empty() {
            if !current_tokens.is_empty() {
                sentences.push(AnnotatedSentence {
                    tokens: std::mem::take(&mut current_tokens),
                    source_dataset: id,
                });
            }
            continue;
        }

        // Handle -DOCSTART- markers (document boundary, also a sentence boundary)
        if line.starts_with("-DOCSTART-") {
            if !current_tokens.is_empty() {
                sentences.push(AnnotatedSentence {
                    tokens: std::mem::take(&mut current_tokens),
                    source_dataset: id,
                });
            }
            continue;
        }

        // Skip header line if present
        if line.eq_ignore_ascii_case("token,tag")
            || line.eq_ignore_ascii_case("text,label")
            || line.eq_ignore_ascii_case("word,ner")
        {
            continue;
        }

        // Parse comma-separated line
        // Handle cases where the token itself might be a comma: ",O" means token is comma
        let (token_text, ner_tag) = if let Some(rest) = line.strip_prefix(',') {
            // Line started with comma: either empty token or comma is the token.
            if let Some(idx) = rest.rfind(',') {
                // ",,tag" or ",something,tag" - everything before the last comma is the token.
                let token_part = &rest[..idx];
                let tag_part = &rest[idx + 1..];
                if token_part.is_empty() {
                    // ",,tag" -> token is the leading comma we stripped
                    (",".to_string(), tag_part.to_string())
                } else {
                    // ",text,tag" -> token includes the leading comma
                    (format!(",{}", token_part), tag_part.to_string())
                }
            } else {
                // ",tag" with no more commas -> empty token
                (String::new(), rest.to_string())
            }
        } else if let Some(idx) = line.rfind(',') {
            // Normal case: Token,Tag
            let token = line[..idx].to_string();
            let tag = line[idx + 1..].to_string();
            (token, tag)
        } else {
            // Malformed line, skip
            continue;
        };

        // Skip if we got empty results
        if token_text.is_empty() && ner_tag.is_empty() {
            continue;
        }

        // Convert empty tag to O
        let ner_tag = if ner_tag.is_empty() || ner_tag == "_" {
            "O".to_string()
        } else {
            ner_tag
        };

        current_tokens.push(AnnotatedToken {
            text: token_text,
            ner_tag,
        });
    }

    // Don't forget the last sentence
    if !current_tokens.is_empty() {
        sentences.push(AnnotatedSentence {
            tokens: current_tokens,
            source_dataset: id,
        });
    }

    if sentences.is_empty() {
        return Err(Error::InvalidInput(format!(
            "CSV NER file for {:?} contains no valid sentences",
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

/// Parse WikiANN-style JSON array format.
///
/// Expected format: `[{"text": "...", "tokens": ["word1", "word2"], "ner_tags": ["O", "B-LOC"]}, ...]`
/// Used by UNER and MSNER datasets converted via download_hf_datasets.py.
pub(crate) fn parse_wikiann_json(content: &str, id: DatasetId) -> Result<LoadedDataset> {
    let parsed: serde_json::Value = serde_json::from_str(content)
        .map_err(|e| Error::InvalidInput(format!("Failed to parse JSON: {}", e)))?;

    let mut sentences = Vec::new();

    // Handle JSON array format
    if let Some(items) = parsed.as_array() {
        for item in items {
            let tokens = match item.get("tokens").and_then(|v| v.as_array()) {
                Some(t) => t,
                None => continue,
            };

            let ner_tags = match item.get("ner_tags").and_then(|v| v.as_array()) {
                Some(t) => t,
                None => continue,
            };

            if tokens.len() != ner_tags.len() {
                continue;
            }

            let mut annotated_tokens = Vec::new();
            for (token, tag) in tokens.iter().zip(ner_tags.iter()) {
                let text = token.as_str().unwrap_or("").to_string();
                let ner_tag = tag.as_str().unwrap_or("O").to_string();
                annotated_tokens.push(AnnotatedToken { text, ner_tag });
            }

            if !annotated_tokens.is_empty() {
                sentences.push(AnnotatedSentence {
                    tokens: annotated_tokens,
                    source_dataset: id,
                });
            }
        }
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

/// Parse CADEC from HuggingFace datasets-server API.
///
/// CADEC HF API format: {"text": "...", "ade": "...", "term_PT": "..."}
/// Each row is a text-ADE pair (one sentence per ADE mention).
/// The `ade` field contains the adverse drug event mention within `text`.
pub(crate) fn parse_cadec_hf_api(content: &str, id: DatasetId) -> Result<LoadedDataset> {
    let parsed: serde_json::Value = serde_json::from_str(content).map_err(|e| {
        Error::InvalidInput(format!("Failed to parse CADEC HF API response: {}", e))
    })?;

    let mut sentences = Vec::new();

    let rows = parsed
        .get("rows")
        .and_then(|v| v.as_array())
        .ok_or_else(|| {
            Error::InvalidInput("No 'rows' array in CADEC HF API response".to_string())
        })?;

    for row_obj in rows {
        let row = match row_obj.get("row") {
            Some(r) => r,
            None => continue,
        };

        let text = match row.get("text").and_then(|v| v.as_str()) {
            Some(t) => t,
            None => continue,
        };

        let ade_text = match row.get("ade").and_then(|v| v.as_str()) {
            Some(a) => a,
            None => continue,
        };

        // Find ADE span in text.
        //
        // IMPORTANT: anno uses **character offsets** globally, but this parser assigns BIO tags
        // token-by-token and uses byte spans internally. We must preserve indices and avoid
        // Unicode casefolding (which can change string length).
        //
        // Strategy:
        // 1) Try exact match (fast, preserves byte indices).
        // 2) Fall back to ASCII case-insensitive match (ADE strings are ASCII-centric).
        let ade_start_byte = text.find(ade_text).or_else(|| {
            let needle_len = ade_text.len();
            if needle_len == 0 {
                return None;
            }
            for (b, _) in text.char_indices() {
                if let Some(hay) = text.get(b..b + needle_len) {
                    if hay.eq_ignore_ascii_case(ade_text) {
                        return Some(b);
                    }
                }
            }
            None
        });
        let Some(ade_start_byte) = ade_start_byte else {
            continue; // ADE not found in text
        };
        let ade_end_byte = ade_start_byte + ade_text.len();

        // Tokenize text preserving byte offsets.
        let mut tokens: Vec<AnnotatedToken> = Vec::new();
        let mut byte_idx = 0;
        let words: Vec<&str> = text.split_whitespace().collect();

        for word in words {
            let word_start =
                text.get(byte_idx..).and_then(|s| s.find(word)).unwrap_or(0) + byte_idx;
            let word_end = word_start + word.len();

            // Check if this word overlaps with ADE span (byte spans)
            let ner_tag = if word_start >= ade_start_byte && word_end <= ade_end_byte {
                // Check if this is the first word of the ADE span
                let is_continuation = !tokens.is_empty()
                    && tokens.last().is_some_and(|t| {
                        t.ner_tag.starts_with("B-adverse") || t.ner_tag.starts_with("I-adverse")
                    });
                if word_start == ade_start_byte || !is_continuation {
                    "B-adverse_drug_event".to_string()
                } else {
                    "I-adverse_drug_event".to_string()
                }
            } else {
                "O".to_string()
            };

            tokens.push(AnnotatedToken {
                text: word.to_string(),
                ner_tag,
            });

            // Update byte position to after this word (including trailing space)
            byte_idx = word_end;
            if byte_idx < text.len() && text.as_bytes().get(byte_idx) == Some(&b' ') {
                byte_idx += 1;
            }
        }

        if !tokens.is_empty() {
            sentences.push(AnnotatedSentence {
                tokens,
                source_dataset: id,
            });
        }
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

/// Parse CADEC JSONL format with support for discontinuous entities.
///
/// CADEC format can include:
/// - Standard BIO tags: `{"tokens": [...], "ner_tags": [...]}`
/// - Entity spans: `{"tokens": [...], "entities": [{"text": "...", "label": "...", "start": 0, "end": 10}]}`
/// - Discontinuous entities: `{"entities": [{"text": "...", "label": "...", "spans": [[0, 5], [10, 15]]}]}`
///
/// For discontinuous entities, we convert them to BIO tags by marking all tokens
/// within any span as part of the entity.
pub(crate) fn parse_cadec_jsonl(content: &str, id: DatasetId) -> Result<LoadedDataset> {
    let mut sentences = Vec::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // Parse JSON line
        let parsed: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue, // Skip malformed lines
        };

        // Try to get tokens
        let tokens = match parsed.get("tokens").and_then(|v| v.as_array()) {
            Some(t) => t,
            None => continue,
        };

        let mut annotated_tokens = Vec::new();
        let mut char_offset = 0;

        // Build token list with character offsets
        let mut token_offsets = Vec::new();
        for token in tokens {
            let text = token.as_str().unwrap_or("").to_string();
            let start = char_offset;
            char_offset += text.chars().count() + 1; // +1 for space
            let end = char_offset - 1;
            token_offsets.push((text, start, end));
        }

        // Initialize all tokens as "O"
        for (text, _, _) in &token_offsets {
            annotated_tokens.push(AnnotatedToken {
                text: text.clone(),
                ner_tag: "O".to_string(),
            });
        }

        // Try to parse entities (for discontinuous support)
        if let Some(entities) = parsed.get("entities").and_then(|v| v.as_array()) {
            for entity in entities {
                let label = entity
                    .get("label")
                    .or_else(|| entity.get("entity_type"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("UNKNOWN")
                    .to_string();

                // Check for discontinuous spans
                if let Some(spans) = entity.get("spans").and_then(|v| v.as_array()) {
                    // Discontinuous entity with multiple spans
                    for span in spans {
                        if let Some(span_array) = span.as_array() {
                            if span_array.len() >= 2 {
                                let start = span_array[0].as_u64().unwrap_or(0) as usize;
                                let end = span_array[1].as_u64().unwrap_or(0) as usize;

                                // Mark tokens within this span
                                for (idx, (_, token_start, token_end)) in
                                    token_offsets.iter().enumerate()
                                {
                                    if *token_start >= start && *token_end <= end {
                                        if idx > 0
                                            && (annotated_tokens[idx - 1]
                                                .ner_tag
                                                .starts_with(&format!("I-{}", label))
                                                || annotated_tokens[idx - 1]
                                                    .ner_tag
                                                    .starts_with(&format!("B-{}", label)))
                                        {
                                            annotated_tokens[idx].ner_tag = format!("I-{}", label);
                                        } else {
                                            annotated_tokens[idx].ner_tag = format!("B-{}", label);
                                        }
                                    }
                                }
                            }
                        }
                    }
                } else if let (Some(start_val), Some(end_val)) = (
                    entity.get("start").and_then(|v| v.as_u64()),
                    entity.get("end").and_then(|v| v.as_u64()),
                ) {
                    // Contiguous entity
                    let start = start_val as usize;
                    let end = end_val as usize;

                    // Mark tokens within this span
                    for (idx, (_, token_start, token_end)) in token_offsets.iter().enumerate() {
                        if *token_start >= start && *token_end <= end {
                            if idx > 0
                                && (annotated_tokens[idx - 1]
                                    .ner_tag
                                    .starts_with(&format!("I-{}", label))
                                    || annotated_tokens[idx - 1]
                                        .ner_tag
                                        .starts_with(&format!("B-{}", label)))
                            {
                                annotated_tokens[idx].ner_tag = format!("I-{}", label);
                            } else {
                                annotated_tokens[idx].ner_tag = format!("B-{}", label);
                            }
                        }
                    }
                }
            }
        } else if let Some(ner_tags) = parsed.get("ner_tags").and_then(|v| v.as_array()) {
            // Fallback to standard BIO tags
            let tag_labels = [
                "O",
                "B-PER",
                "I-PER",
                "B-ORG",
                "I-ORG",
                "B-LOC",
                "I-LOC",
                "B-MISC",
                "I-MISC",
                "B-DRUG",
                "I-DRUG",
                "B-ADR",
                "I-ADR",
                "B-DISEASE",
                "I-DISEASE",
            ];

            for (idx, (text, _, _)) in token_offsets.iter().enumerate() {
                if let Some(tag_val) = ner_tags.get(idx) {
                    let tag_idx = tag_val.as_u64().unwrap_or(0) as usize;
                    let ner_tag = tag_labels.get(tag_idx).unwrap_or(&"O").to_string();
                    annotated_tokens[idx] = AnnotatedToken {
                        text: text.clone(),
                        ner_tag,
                    };
                }
            }
        }

        if !annotated_tokens.is_empty() {
            sentences.push(AnnotatedSentence {
                tokens: annotated_tokens,
                source_dataset: id,
            });
        }
    }

    if sentences.is_empty() {
        return Err(Error::InvalidInput(format!(
            "CADEC JSONL file for {:?} contains no valid sentences",
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

/// Parse BC5CDR dataset in CoNLL format from BioFLAIR.
///
/// Format: WORD\tPOS\tCHUNK\tNER_TAG
pub(crate) fn parse_bc5cdr(content: &str, id: DatasetId) -> Result<LoadedDataset> {
    let mut sentences = Vec::new();
    let mut current_tokens = Vec::new();

    for line in content.lines() {
        let line = line.trim();

        // Skip DOCSTART lines
        if line.starts_with("-DOCSTART-") {
            continue;
        }

        if line.is_empty() {
            // End of sentence
            if !current_tokens.is_empty() {
                sentences.push(AnnotatedSentence {
                    tokens: std::mem::take(&mut current_tokens),
                    source_dataset: id,
                });
            }
            continue;
        }

        // Parse CoNLL line: WORD\tPOS\tCHUNK\tNER_TAG
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 4 {
            let word = parts[0].to_string();
            let ner_tag = parts[3].to_string();

            // Map Entity tags to BIO format
            let normalized_tag = if ner_tag.contains("Entity")
                || ner_tag.contains("CHEMICAL")
                || ner_tag.contains("DISEASE")
            {
                // Convert I-Entity to B-CHEMICAL or I-CHEMICAL based on context
                if ner_tag.starts_with("B-") {
                    "B-CHEMICAL".to_string()
                } else if ner_tag.starts_with("I-") {
                    "I-CHEMICAL".to_string()
                } else {
                    "O".to_string()
                }
            } else {
                ner_tag
            };

            current_tokens.push(AnnotatedToken {
                text: word,
                ner_tag: normalized_tag,
            });
        }
    }

    // Don't forget the last sentence
    if !current_tokens.is_empty() {
        sentences.push(AnnotatedSentence {
            tokens: current_tokens,
            source_dataset: id,
        });
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

/// Parse NCBI Disease dataset in CoNLL format from BioFLAIR.
///
/// Format: WORD\tPOS\tCHUNK\tNER_TAG
pub(crate) fn parse_ncbi_disease(content: &str, id: DatasetId) -> Result<LoadedDataset> {
    let mut sentences = Vec::new();
    let mut current_tokens = Vec::new();

    for line in content.lines() {
        let line = line.trim();

        if line.is_empty() {
            // End of sentence
            if !current_tokens.is_empty() {
                sentences.push(AnnotatedSentence {
                    tokens: std::mem::take(&mut current_tokens),
                    source_dataset: id,
                });
            }
            continue;
        }

        // Parse CoNLL line: WORD\tPOS\tCHUNK\tNER_TAG
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 4 {
            let word = parts[0].to_string();
            let ner_tag = parts[3].to_string();

            current_tokens.push(AnnotatedToken {
                text: word,
                ner_tag,
            });
        }
    }

    // Don't forget the last sentence
    if !current_tokens.is_empty() {
        sentences.push(AnnotatedSentence {
            tokens: current_tokens,
            source_dataset: id,
        });
    }

    if sentences.is_empty() {
        return Err(Error::InvalidInput(format!(
            "NCBI Disease file for {:?} contains no valid sentences",
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

/// Parse CoNLL-U format (Universal Dependencies).
///
/// Used by MasakhaPOS for African language POS tagging.
///
/// Source: <https://universaldependencies.org/format.html>
pub(crate) fn parse_conllu(content: &str, id: DatasetId) -> Result<LoadedDataset> {
    let mut sentences = Vec::new();
    let now = chrono::Utc::now().to_rfc3339();
    let mut current_tokens = Vec::new();

    for line in content.lines() {
        let line = line.trim();

        // Skip comments
        if line.starts_with('#') {
            continue;
        }

        // Blank line marks end of sentence
        if line.is_empty() {
            if !current_tokens.is_empty() {
                sentences.push(AnnotatedSentence {
                    tokens: std::mem::take(&mut current_tokens),
                    source_dataset: id,
                });
            }
            continue;
        }

        // Parse CoNLL-U columns
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() >= 4 {
            // Skip multi-word tokens (IDs with ranges like "1-2")
            let id_field = fields[0];
            if id_field.contains('-') || id_field.contains('.') {
                continue;
            }

            let form = fields[1]; // Word form
            let upos = fields[3]; // Universal POS tag

            current_tokens.push(AnnotatedToken {
                text: form.to_string(),
                ner_tag: format!("B-{}", upos), // Use POS tag as entity type
            });
        }
    }

    // Don't forget last sentence
    if !current_tokens.is_empty() {
        sentences.push(AnnotatedSentence {
            tokens: current_tokens,
            source_dataset: id,
        });
    }

    if sentences.is_empty() {
        return Err(Error::InvalidInput(format!(
            "CoNLL-U file for {:?} contains no valid sentences",
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
