//! Schema/offset validation for human_voice_agent datasets.
//!
//! These JSONL files are outside the NER loaders, so validate them explicitly:
//! - required fields
//! - language tag present
//! - no empty text
//! - offsets (where present) are within bounds

use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct TranscriptRow {
    id: String,
    text: String,
    speaker: String,
    speaker_type: String,
    language: String,
    translation: Option<String>,
    is_aside: bool,
    is_response_token: bool,
    triggered_cutoff: bool,
    excerpt: u32,
    line: u32,
    notes: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DiscourseDeixisRow {
    id: String,
    text: String,
    antecedent: SpanWithType,
    anaphor: Span,
    anaphora_type: String,
    notes: Option<String>,
    source: Option<String>,
    language: Option<String>,
    translation: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SpanWithType {
    text: String,
    start: usize,
    end: usize,
    r#type: String,
}

#[derive(Debug, Deserialize)]
struct Span {
    text: String,
    start: usize,
    end: usize,
}

#[derive(Debug, Deserialize)]
struct ResponseTokenRow {
    id: String,
    token: String,
    language: String,
    translation: Option<String>,
    function: String,
    triggered_cutoff: bool,
    excerpt: u32,
    speaker: String,
    context: Option<String>,
    notes: Option<String>,
}

fn read_lines(relative_path: &str) -> Vec<String> {
    // Handle running from either workspace root or anno/ crate directory
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir.parent().unwrap_or(manifest_dir);
    let path = workspace_root.join(relative_path);

    let content = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {}", path.display(), e));
    content
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.to_string())
        .collect()
}

fn assert_span_within(text: &str, start: usize, end: usize, label: &str) {
    let char_len = text.chars().count();
    assert!(
        start < end && end <= char_len,
        "{} span [{}, {}) exceeds text length {} in: {}",
        label,
        start,
        end,
        char_len,
        text
    );
}

#[test]
fn validate_transcripts_schema() {
    for line in read_lines("testdata/human_voice_agent/transcripts.jsonl") {
        let row: TranscriptRow = serde_json::from_str(&line).expect("valid transcript JSON object");
        assert!(!row.text.trim().is_empty(), "empty text in {}", row.id);
        assert!(
            !row.speaker.trim().is_empty(),
            "empty speaker in {}",
            row.id
        );
        assert!(
            row.language.len() >= 2,
            "missing/short language tag in {}",
            row.id
        );
    }
}

#[test]
fn validate_discourse_deixis_schema_and_spans() {
    for line in read_lines("testdata/human_voice_agent/discourse_deixis.jsonl") {
        let row: DiscourseDeixisRow =
            serde_json::from_str(&line).expect("valid discourse_deixis JSON object");
        assert!(!row.text.trim().is_empty(), "empty text in {}", row.id);
        assert_span_within(
            &row.text,
            row.antecedent.start,
            row.antecedent.end,
            "antecedent",
        );
        assert_span_within(&row.text, row.anaphor.start, row.anaphor.end, "anaphor");
    }
}

#[test]
fn validate_response_tokens_schema() {
    for line in read_lines("testdata/human_voice_agent/response_tokens.jsonl") {
        let row: ResponseTokenRow =
            serde_json::from_str(&line).expect("valid response_token JSON object");
        assert!(!row.token.trim().is_empty(), "empty token in {}", row.id);
        assert!(
            row.language.len() >= 2,
            "missing/short language tag in {}",
            row.id
        );
    }
}
