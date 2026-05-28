//! Parse the `[CHUNK::uuid]...[/CHUNK]` and `[COLUMN::name]...[/COLUMN]`
//! sections produced by `build_user_prompt()` into typed structs for
//! the local extraction engine.

use crate::error::{Error, Result};
use uuid::Uuid;

/// Result of parsing a structured user prompt containing chunks and columns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedPrompt {
    /// Document text chunks extracted from `[CHUNK::uuid]...[/CHUNK]` markers.
    pub chunks: Vec<ParsedChunk>,
    /// Column descriptors extracted from `[COLUMN::name]...[/COLUMN]` markers.
    pub columns: Vec<ParsedColumn>,
}

/// A single text chunk with its stable UUID identifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedChunk {
    /// Stable chunk identifier (UUIDv7).
    pub id: Uuid,
    /// Raw text content of the chunk.
    pub text: String,
}

/// A column descriptor parsed from the user prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedColumn {
    /// Column key (machine-readable name).
    pub name: String,
    /// Human-readable extraction prompt for this column.
    pub prompt: String,
}

/// Parse a structured user prompt into chunks and columns.
///
/// Expects `[CHUNK::uuid]...[/CHUNK]` and `[COLUMN::name]...[/COLUMN]`
/// markers as produced by `build_user_prompt()`.
///
/// # Errors
///
/// Returns [`Error::Extract`] if a marker is malformed or a chunk UUID
/// cannot be parsed.
pub fn parse_user_prompt(input: &str) -> Result<ParsedPrompt> {
    let mut chunks = Vec::new();
    let mut columns = Vec::new();

    // --- chunks ---
    let mut rest = input;
    while let Some(start) = rest.find("[CHUNK::") {
        rest = &rest[start + "[CHUNK::".len()..];
        let end_id = rest
            .find(']')
            .ok_or_else(|| parse_error("missing chunk id terminator"))?;
        let id = rest[..end_id]
            .parse::<Uuid>()
            .map_err(|e| parse_error(e.to_string()))?;
        rest = &rest[end_id + 1..];
        let end = rest
            .find("[/CHUNK]")
            .ok_or_else(|| parse_error("missing chunk close marker"))?;
        chunks.push(ParsedChunk {
            id,
            text: rest[..end].to_string(),
        });
        rest = &rest[end + "[/CHUNK]".len()..];
    }

    // --- columns ---
    rest = input;
    while let Some(start) = rest.find("[COLUMN::") {
        rest = &rest[start + "[COLUMN::".len()..];
        let end_name = rest
            .find(']')
            .ok_or_else(|| parse_error("missing column name terminator"))?;
        let name = rest[..end_name].to_string();
        rest = &rest[end_name + 1..];
        let end = rest
            .find("[/COLUMN]")
            .ok_or_else(|| parse_error("missing column close marker"))?;
        columns.push(ParsedColumn {
            name,
            prompt: rest[..end].to_string(),
        });
        rest = &rest[end + "[/COLUMN]".len()..];
    }

    Ok(ParsedPrompt { chunks, columns })
}

fn parse_error(msg: impl Into<String>) -> Error {
    Error::Extract {
        doc: "prompt".into(),
        col: "*".into(),
        source: msg.into().into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_chunks_and_columns_from_user_prompt() {
        let chunk_id = uuid::Uuid::now_v7();
        let user = format!(
            "[CHUNK::{chunk_id}]Le bailleur est ACME SAS.[/CHUNK]\n\
             [COLUMN::landlord]Landlord legal name[/COLUMN]\n"
        );

        let parsed = parse_user_prompt(&user).expect("parse");

        assert_eq!(parsed.chunks.len(), 1);
        assert_eq!(parsed.chunks[0].id, chunk_id);
        assert_eq!(parsed.chunks[0].text, "Le bailleur est ACME SAS.");
        assert_eq!(parsed.columns[0].name, "landlord");
        assert_eq!(parsed.columns[0].prompt, "Landlord legal name");
    }

    #[test]
    fn parses_multiple_chunks() {
        let id1 = uuid::Uuid::now_v7();
        let id2 = uuid::Uuid::now_v7();
        let user =
            format!("[CHUNK::{id1}]First chunk.[/CHUNK]\n[CHUNK::{id2}]Second chunk.[/CHUNK]\n");
        let parsed = parse_user_prompt(&user).expect("parse");
        assert_eq!(parsed.chunks.len(), 2);
        assert_eq!(parsed.chunks[0].text, "First chunk.");
        assert_eq!(parsed.chunks[1].text, "Second chunk.");
    }

    #[test]
    fn empty_input_gives_empty_parsed() {
        let parsed = parse_user_prompt("").expect("parse");
        assert!(parsed.chunks.is_empty());
        assert!(parsed.columns.is_empty());
    }
}
