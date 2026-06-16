use crate::eval::loader::types::{AnnotatedSentence, AnnotatedToken, DataSource, LoadedDataset};
use crate::eval::loader::DatasetId;
use crate::eval::loader::DatasetLoader;
use anno::{Error, Result};

/// Parse AfriSenti sentiment TSV format.
///
/// Source: <https://github.com/afrisenti-semeval/afrisent-semeval-2023>
pub(crate) fn parse_afrisenti(content: &str, id: DatasetId) -> Result<LoadedDataset> {
    let mut sentences = Vec::new();
    let now = chrono::Utc::now().to_rfc3339();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 2 {
            let text = parts[0].to_string();
            let label = parts[1].to_string();

            // For sentiment, we create a single token annotation with the sentiment label
            let tokens = vec![AnnotatedToken {
                text: text.clone(),
                ner_tag: format!("B-{}", label),
            }];

            sentences.push(AnnotatedSentence {
                tokens,
                source_dataset: id,
            });
        }
    }

    if sentences.is_empty() {
        return Err(Error::InvalidInput(format!(
            "AfriSenti file for {:?} contains no valid sentences",
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

/// Parse AfriQA question answering format.
///
/// AfriQA uses JSON format with fields: context, question, answers
/// Each answer has text and answer_start
///
/// Source: <https://github.com/masakhane-io/afriqa>
pub(crate) fn parse_afriqa(content: &str, id: DatasetId) -> Result<LoadedDataset> {
    let mut sentences = Vec::new();
    let now = chrono::Utc::now().to_rfc3339();

    // AfriQA data can be JSON array or JSONL
    let docs: Vec<serde_json::Value> = if content.trim().starts_with('[') {
        serde_json::from_str(content)
            .map_err(|e| Error::InvalidInput(format!("Failed to parse AfriQA JSON: {}", e)))?
    } else {
        // JSONL format
        content
            .lines()
            .filter(|l| !l.trim().is_empty())
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect()
    };

    for doc in docs {
        // Get context text
        let context = doc.get("context").and_then(|v| v.as_str()).unwrap_or("");

        // Get answers
        if let Some(answers) = doc.get("answers").and_then(|v| v.as_object()) {
            let texts = answers.get("text").and_then(|v| v.as_array());
            let starts = answers.get("answer_start").and_then(|v| v.as_array());

            if let (Some(texts), Some(starts)) = (texts, starts) {
                // Create tokens for context (word-level tokenization)
                let words: Vec<&str> = context.split_whitespace().collect();
                let mut tokens: Vec<AnnotatedToken> = words
                    .iter()
                    .map(|w| AnnotatedToken {
                        text: w.to_string(),
                        ner_tag: "O".to_string(),
                    })
                    .collect();

                // Mark answer spans
                for (text_val, start_val) in texts.iter().zip(starts.iter()) {
                    if let (Some(answer_text), Some(start)) =
                        (text_val.as_str(), start_val.as_u64())
                    {
                        let start = start as usize;
                        let answer_words: Vec<&str> = answer_text.split_whitespace().collect();

                        // Find word index by counting words before start position
                        let prefix: String = context.chars().take(start).collect();
                        let word_idx = prefix.split_whitespace().count();

                        // Apply BIO tags
                        for (i, _) in answer_words.iter().enumerate() {
                            let idx = word_idx + i;
                            if idx < tokens.len() {
                                tokens[idx].ner_tag = if i == 0 {
                                    "B-ANSWER".to_string()
                                } else {
                                    "I-ANSWER".to_string()
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
        }
    }

    if sentences.is_empty() {
        return Err(Error::InvalidInput(format!(
            "AfriQA file for {:?} contains no valid sentences",
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

/// Parse MasakhaNEWS topic classification format.
///
/// MasakhaNEWS uses TSV format: headline\tbody\tcategory
/// Categories: business, entertainment, health, politics, religion, sports, technology
///
/// Source: <https://github.com/masakhane-io/masakhane-news>
pub(crate) fn parse_masakhanews(content: &str, id: DatasetId) -> Result<LoadedDataset> {
    let mut sentences = Vec::new();
    let now = chrono::Utc::now().to_rfc3339();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with("headline\t") {
            continue;
        }

        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 2 {
            // Use headline as text, category as label
            let text = parts[0].to_string();
            let category = if parts.len() >= 3 {
                parts[2].to_string()
            } else {
                parts[1].to_string()
            };

            // For classification, create single token with category
            let tokens = vec![AnnotatedToken {
                text,
                ner_tag: format!("B-{}", category),
            }];

            sentences.push(AnnotatedSentence {
                tokens,
                source_dataset: id,
            });
        }
    }

    if sentences.is_empty() {
        return Err(Error::InvalidInput(format!(
            "MasakhaNEWS file for {:?} contains no valid sentences",
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

/// Parse TREC question classification format.
///
/// TREC format: `COARSE:fine question text`
/// E.g.: `NUM:dist How far is it from Denver to Aspen ?`
///
/// 6 coarse classes: ABBR, DESC, ENTY, HUM, LOC, NUM
/// 50 fine-grained classes.
///
/// Source: <https://cogcomp.seas.upenn.edu/Data/QA/QC/>
pub(crate) fn parse_trec(content: &str, id: DatasetId) -> Result<LoadedDataset> {
    let mut sentences = Vec::new();
    let now = chrono::Utc::now().to_rfc3339();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // Format: COARSE:fine question text
        // Find the first space to separate label from question
        if let Some(space_idx) = line.find(' ') {
            let label = &line[..space_idx];
            let question = line[space_idx + 1..].trim();

            // Extract coarse label (before colon)
            let coarse_label = label.split(':').next().unwrap_or(label);

            // Create single token with the question and classification label
            let tokens = vec![AnnotatedToken {
                text: question.to_string(),
                ner_tag: format!("B-{}", coarse_label),
            }];

            sentences.push(AnnotatedSentence {
                tokens,
                source_dataset: id,
            });
        }
    }

    if sentences.is_empty() {
        return Err(Error::InvalidInput(format!(
            "TREC file for {:?} contains no valid sentences",
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

/// Parse AG News classification format (parquet or CSV).
///
/// AG News has 4 classes: World (0), Sports (1), Business (2), Sci/Tech (3)
///
/// Source: <https://huggingface.co/datasets/ag_news>
pub(crate) fn parse_agnews(content: &str, id: DatasetId) -> Result<LoadedDataset> {
    let mut sentences = Vec::new();
    let now = chrono::Utc::now().to_rfc3339();

    let label_map = ["World", "Sports", "Business", "Sci/Tech"];

    // Try parsing as JSON (converted from parquet)
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Try JSON format
        if let Ok(obj) = serde_json::from_str::<serde_json::Value>(line) {
            let text = obj.get("text").and_then(|v| v.as_str()).unwrap_or_default();
            let label_idx = obj.get("label").and_then(|v| v.as_i64()).unwrap_or(0) as usize;

            let label = label_map.get(label_idx).unwrap_or(&"Unknown");

            let tokens = vec![AnnotatedToken {
                text: text.to_string(),
                ner_tag: format!("B-{}", label),
            }];

            sentences.push(AnnotatedSentence {
                tokens,
                source_dataset: id,
            });
        }
    }

    if sentences.is_empty() {
        return Err(Error::InvalidInput(format!(
            "AG News file for {:?} contains no valid sentences",
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

/// Parse DBPedia-14 classification format.
///
/// 14 classes from DBpedia ontology.
///
/// Source: <https://huggingface.co/datasets/dbpedia_14>
pub(crate) fn parse_dbpedia14(content: &str, id: DatasetId) -> Result<LoadedDataset> {
    let mut sentences = Vec::new();
    let now = chrono::Utc::now().to_rfc3339();

    let label_map = [
        "Company",
        "EducationalInstitution",
        "Artist",
        "Athlete",
        "OfficeHolder",
        "MeanOfTransportation",
        "Building",
        "NaturalPlace",
        "Village",
        "Animal",
        "Plant",
        "Album",
        "Film",
        "WrittenWork",
    ];

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if let Ok(obj) = serde_json::from_str::<serde_json::Value>(line) {
            let content_text = obj
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let label_idx = obj.get("label").and_then(|v| v.as_i64()).unwrap_or(0) as usize;

            let label = label_map.get(label_idx).unwrap_or(&"Unknown");

            let tokens = vec![AnnotatedToken {
                text: content_text.to_string(),
                ner_tag: format!("B-{}", label),
            }];

            sentences.push(AnnotatedSentence {
                tokens,
                source_dataset: id,
            });
        }
    }

    if sentences.is_empty() {
        return Err(Error::InvalidInput(format!(
            "DBPedia-14 file for {:?} contains no valid sentences",
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

/// Parse Yahoo Answers topic classification format.
///
/// 10 topics: Society, Science, Health, Education, Computers,
/// Sports, Business, Entertainment, Family, Politics
///
/// Source: <https://huggingface.co/datasets/yahoo_answers_topics>
pub(crate) fn parse_yahoo_answers(content: &str, id: DatasetId) -> Result<LoadedDataset> {
    let mut sentences = Vec::new();
    let now = chrono::Utc::now().to_rfc3339();

    let label_map = [
        "Society",
        "Science",
        "Health",
        "Education",
        "Computers",
        "Sports",
        "Business",
        "Entertainment",
        "Family",
        "Politics",
    ];

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if let Ok(obj) = serde_json::from_str::<serde_json::Value>(line) {
            // Yahoo Answers has question_title, question_content, best_answer
            let question = obj
                .get("question_title")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let label_idx = obj.get("topic").and_then(|v| v.as_i64()).unwrap_or(0) as usize;

            let label = label_map.get(label_idx).unwrap_or(&"Unknown");

            let tokens = vec![AnnotatedToken {
                text: question.to_string(),
                ner_tag: format!("B-{}", label),
            }];

            sentences.push(AnnotatedSentence {
                tokens,
                source_dataset: id,
            });
        }
    }

    if sentences.is_empty() {
        return Err(Error::InvalidInput(format!(
            "Yahoo Answers file for {:?} contains no valid sentences",
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

/// Parse TweetTopic format (JSONL with text and labels).
///
/// Format: {"text": "...", "label": 0, "label_name": "sports_&_gaming", ...}
///
/// Source: <https://huggingface.co/datasets/cardiffnlp/tweet_topic_single>
pub(crate) fn parse_tweettopic(content: &str, id: DatasetId) -> Result<LoadedDataset> {
    let mut sentences = Vec::new();
    let now = chrono::Utc::now().to_rfc3339();

    // Label mapping if label_name not present
    let label_map = [
        "arts_&_culture",
        "business_&_entrepreneurs",
        "pop_culture",
        "daily_life",
        "sports_&_gaming",
        "science_&_technology",
    ];

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if let Ok(obj) = serde_json::from_str::<serde_json::Value>(line) {
            let text = obj.get("text").and_then(|v| v.as_str()).unwrap_or_default();

            // Try label_name first, then map from label index
            let label = obj
                .get("label_name")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .or_else(|| {
                    obj.get("label")
                        .and_then(|v| v.as_i64())
                        .and_then(|idx| label_map.get(idx as usize))
                        .map(|s| s.to_string())
                })
                .unwrap_or_else(|| "topic".to_string());

            let tokens = vec![AnnotatedToken {
                text: text.to_string(),
                ner_tag: format!("B-{}", label),
            }];

            sentences.push(AnnotatedSentence {
                tokens,
                source_dataset: id,
            });
        }
    }

    if sentences.is_empty() {
        return Err(Error::InvalidInput(format!(
            "TweetTopic file for {:?} contains no valid sentences",
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
