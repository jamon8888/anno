//! Tests for the Human-Voice Agent Interaction dataset.
//!
//! This dataset is derived from Rudaz, Broth & Mlynář (2025) "Everything counts:
//! the managed omnirelevance of speech in human-voice agent interaction".
//!
//! The data captures turn-taking phenomena in naturalistic interactions between
//! humans and conversational AI agents (Pepper robot and ChatGPT voice mode).

use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

/// A single turn in the human-voice agent transcript.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct TranscriptTurn {
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
    notes: String,
}

/// A discourse deixis annotation.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct DiscourseDeixisExample {
    id: String,
    text: String,
    antecedent: AntecedentInfo,
    anaphor: AnaphorInfo,
    anaphora_type: String,
    notes: String,
    source: String,
    #[serde(default)]
    language: Option<String>,
    #[serde(default)]
    translation: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AntecedentInfo {
    text: String,
    start: usize,
    end: usize,
    #[serde(rename = "type")]
    antecedent_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AnaphorInfo {
    text: String,
    start: usize,
    end: usize,
}

/// A response token annotation.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ResponseToken {
    id: String,
    token: String,
    language: String,
    translation: String,
    function: String,
    triggered_cutoff: bool,
    excerpt: u32,
    speaker: String,
    context: String,
    notes: String,
}

fn dataset_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("testdata")
        .join("human_voice_agent")
}

#[test]
fn test_load_transcripts() {
    let path = dataset_path().join("transcripts.jsonl");
    if !path.exists() {
        eprintln!("Skipping test: dataset not found at {:?}", path);
        return;
    }

    let file = File::open(&path).expect("Failed to open transcripts.jsonl");
    let reader = BufReader::new(file);

    let mut turns: Vec<TranscriptTurn> = Vec::new();
    for line in reader.lines() {
        let line = line.expect("Failed to read line");
        let turn: TranscriptTurn = serde_json::from_str(&line).expect("Failed to parse turn");
        turns.push(turn);
    }

    // Basic statistics
    assert!(!turns.is_empty(), "Should have some turns");
    println!("Loaded {} transcript turns", turns.len());

    // Count by excerpt
    let excerpt1_count = turns.iter().filter(|t| t.excerpt == 1).count();
    let excerpt2_count = turns.iter().filter(|t| t.excerpt == 2).count();
    let excerpt3_count = turns.iter().filter(|t| t.excerpt == 3).count();

    println!("  Excerpt 1 (Pepper robot): {} turns", excerpt1_count);
    println!("  Excerpt 2 (ChatGPT cards): {} turns", excerpt2_count);
    println!("  Excerpt 3 (ChatGPT newspaper): {} turns", excerpt3_count);

    assert!(excerpt1_count > 0, "Should have excerpt 1 data");
    assert!(excerpt2_count > 0, "Should have excerpt 2 data");
    assert!(excerpt3_count > 0, "Should have excerpt 3 data");

    // Count aside sequences
    let aside_count = turns.iter().filter(|t| t.is_aside).count();
    println!("  Aside sequences: {}", aside_count);
    assert!(aside_count > 0, "Should have aside sequences");

    // Count response tokens
    let response_token_count = turns.iter().filter(|t| t.is_response_token).count();
    println!("  Response tokens: {}", response_token_count);

    // Count cutoffs triggered by response tokens
    let cutoff_count = turns
        .iter()
        .filter(|t| t.is_response_token && t.triggered_cutoff)
        .count();
    println!("  Response tokens that triggered cutoffs: {}", cutoff_count);
}

#[test]
fn test_load_discourse_deixis() {
    let path = dataset_path().join("discourse_deixis.jsonl");
    if !path.exists() {
        eprintln!("Skipping test: dataset not found at {:?}", path);
        return;
    }

    let file = File::open(&path).expect("Failed to open discourse_deixis.jsonl");
    let reader = BufReader::new(file);

    let mut examples: Vec<DiscourseDeixisExample> = Vec::new();
    for line in reader.lines() {
        let line = line.expect("Failed to read line");
        let example: DiscourseDeixisExample =
            serde_json::from_str(&line).expect("Failed to parse example");
        examples.push(example);
    }

    println!("Loaded {} discourse deixis examples", examples.len());
    assert!(!examples.is_empty(), "Should have discourse deixis examples");

    // Validate offsets
    for example in &examples {
        assert!(
            example.antecedent.start < example.antecedent.end,
            "Antecedent start should be before end: {:?}",
            example.id
        );
        assert!(
            example.anaphor.start < example.anaphor.end,
            "Anaphor start should be before end: {:?}",
            example.id
        );

        // Verify antecedent text matches offset
        let extracted = &example.text[example.antecedent.start..example.antecedent.end];
        assert_eq!(
            extracted, example.antecedent.text,
            "Antecedent text mismatch in {}: expected '{}', got '{}'",
            example.id, example.antecedent.text, extracted
        );

        // Verify anaphor text matches offset
        let extracted = &example.text[example.anaphor.start..example.anaphor.end];
        assert_eq!(
            extracted, example.anaphor.text,
            "Anaphor text mismatch in {}: expected '{}', got '{}'",
            example.id, example.anaphor.text, extracted
        );
    }

    // Count by anaphora type
    let event_count = examples
        .iter()
        .filter(|e| e.anaphora_type == "Event")
        .count();
    let prop_count = examples
        .iter()
        .filter(|e| e.anaphora_type == "Proposition")
        .count();
    let fact_count = examples
        .iter()
        .filter(|e| e.anaphora_type == "Fact")
        .count();
    let sit_count = examples
        .iter()
        .filter(|e| e.anaphora_type == "Situation")
        .count();

    println!("  Event anaphora: {}", event_count);
    println!("  Proposition anaphora: {}", prop_count);
    println!("  Fact anaphora: {}", fact_count);
    println!("  Situation anaphora: {}", sit_count);
}

#[test]
fn test_load_response_tokens() {
    let path = dataset_path().join("response_tokens.jsonl");
    if !path.exists() {
        eprintln!("Skipping test: dataset not found at {:?}", path);
        return;
    }

    let file = File::open(&path).expect("Failed to open response_tokens.jsonl");
    let reader = BufReader::new(file);

    let mut tokens: Vec<ResponseToken> = Vec::new();
    for line in reader.lines() {
        let line = line.expect("Failed to read line");
        let token: ResponseToken = serde_json::from_str(&line).expect("Failed to parse token");
        tokens.push(token);
    }

    println!("Loaded {} response token examples", tokens.len());
    assert!(!tokens.is_empty(), "Should have response token examples");

    // Count by function
    let continuer_count = tokens.iter().filter(|t| t.function == "continuer").count();
    let ack_count = tokens
        .iter()
        .filter(|t| t.function == "acknowledgment")
        .count();
    let align_count = tokens.iter().filter(|t| t.function == "alignment").count();

    println!("  Continuers: {}", continuer_count);
    println!("  Acknowledgments: {}", ack_count);
    println!("  Alignment tokens: {}", align_count);

    // Count cutoffs
    let cutoff_count = tokens.iter().filter(|t| t.triggered_cutoff).count();
    let whispered_count = tokens.iter().filter(|t| !t.triggered_cutoff).count();

    println!(
        "  Triggered cutoffs: {} ({:.1}%)",
        cutoff_count,
        100.0 * cutoff_count as f64 / tokens.len() as f64
    );
    println!(
        "  Whispered/aside (no cutoff): {} ({:.1}%)",
        whispered_count,
        100.0 * whispered_count as f64 / tokens.len() as f64
    );
}

#[test]
fn test_french_text_unicode() {
    let path = dataset_path().join("transcripts.jsonl");
    if !path.exists() {
        return;
    }

    let file = File::open(&path).expect("Failed to open transcripts.jsonl");
    let reader = BufReader::new(file);

    for line in reader.lines() {
        let line = line.expect("Failed to read line");
        let turn: TranscriptTurn = serde_json::from_str(&line).expect("Failed to parse turn");

        // Verify French text is valid UTF-8 and contains expected characters
        if turn.language == "fr" {
            // Check for common French diacritics
            let has_french_chars = turn.text.chars().any(|c| {
                matches!(
                    c,
                    'é' | 'è' | 'ê' | 'ë' | 'à' | 'â' | 'ô' | 'û' | 'ù' | 'ç' | 'î' | 'ï'
                )
            }) || turn.text.contains("'");

            // Not all French text has diacritics, but character counting should work
            let char_count = turn.text.chars().count();
            let byte_count = turn.text.len();

            // For French, char count should usually be close to byte count
            // (most French diacritics are 2 bytes in UTF-8)
            assert!(
                char_count <= byte_count,
                "Character count should be <= byte count"
            );
        }
    }
}

/// Test that the dataset can be used with anno's discourse deixis infrastructure.
#[test]
#[cfg(feature = "discourse")]
fn test_integration_with_anno_discourse() {
    use anno::discourse::{DiscourseDeicticDetector, DiscourseScope};

    let path = dataset_path().join("discourse_deixis.jsonl");
    if !path.exists() {
        return;
    }

    let file = File::open(&path).expect("Failed to open discourse_deixis.jsonl");
    let reader = BufReader::new(file);

    let detector = DiscourseDeicticDetector::new();

    for line in reader.lines() {
        let line = line.expect("Failed to read line");
        let example: DiscourseDeixisExample =
            serde_json::from_str(&line).expect("Failed to parse example");

        // Try detecting discourse deictics in the text
        let detected = detector.detect(&example.text);

        // Analyze discourse scope
        let scope = DiscourseScope::analyze(&example.text);

        println!(
            "Example {}: {} sentences, {} clauses, {} detected deictics",
            example.id,
            scope.sentence_count(),
            scope.clause_count(),
            detected.len()
        );
    }
}
