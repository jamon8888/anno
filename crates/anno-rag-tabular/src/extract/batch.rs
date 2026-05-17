//! Single-batch extraction primitive.
//!
//! Assembles the system + user prompts for one LLM call covering a set
//! of chunks and a set of columns, dispatches to the [`LlmClient`],
//! parses the structured response into [`Cell`]s.
//!
//! For docs with many columns the caller (`Extractor::extract_doc`)
//! splits the column list via [`crate::extract::budget::split_columns`]
//! and invokes [`extract_batch`] once per sub-batch, then concatenates
//! the resulting cells. This module stays single-batch on purpose —
//! see `extract::budget` for the splitter.

use crate::error::{Error, Result};
use crate::extract::ChunkRef;
use crate::ids::{ReviewId, RowId};
use crate::llm::LlmClient;
use crate::schema::{json_schema, Column};
use crate::storage::cells::{Author, Cell, Citation, Confidence};
use chrono::Utc;
use serde_json::Value;

/// Hardcoded extractor playbook. The Anthropic backend marks this string
/// as cacheable, so keep it stable across calls — every prompt-text
/// edit invalidates the prompt cache.
const SYSTEM_PROMPT: &str = "You are a legal-document extractor. Use the `emit_cells` tool to return one envelope per requested column.\n\n\
Rules:\n\
- Every citation MUST reference a `chunk_id` that appears verbatim in the user message (between [CHUNK::<id>] and [/CHUNK] markers).\n\
- `byte_start` and `byte_end` MUST be valid byte offsets into that chunk's text, with `byte_start < byte_end`.\n\
- `quoted_text` MUST equal the substring `chunk_text[byte_start..byte_end]` exactly.\n\
- If a field cannot be supported by at least one citation from the supplied chunks, omit that column from the response.\n\
- `reasoning` is a short justification (one or two sentences). Do not restate the value.";

/// Run a single LLM extraction call for `columns` over `chunks` and
/// return the parsed cells, ready for the verifier.
///
/// The returned cells are pre-verifier: `support_score = 0.0`,
/// `confidence = Confidence::Medium`, `locked = false`, `version = 1`,
/// `author = Author::System { extractor_version: llm.model_id() }`. The
/// verifier (Phase 6) overwrites `support_score` and `confidence` once
/// it has scored each citation.
///
/// Manual columns are defensively skipped — the JSON schema generator
/// already excludes them, so the LLM should never emit a value for one,
/// but this keeps the contract local.
///
/// # Errors
///
/// Returns [`Error::Extract`] with the offending column name when:
/// - The LLM call itself fails.
/// - The response is not a JSON object.
/// - A column envelope is missing `value` / `reasoning` / `citations`.
/// - A citation is malformed (non-UUID `chunk_id`, missing offsets, …).
///
/// A column entirely missing from an otherwise-well-formed response is
/// **not** an error here — Phase 7 conditional-skip handling needs the
/// freedom for the LLM to drop a column when the parent gate is false.
/// We just don't emit a cell for it.
pub(crate) async fn extract_batch(
    llm: &dyn LlmClient,
    review_id: ReviewId,
    doc_id: uuid::Uuid,
    chunks: &[ChunkRef],
    columns: &[Column],
) -> Result<Vec<Cell>> {
    let user = build_user_prompt(chunks, columns);
    let schema = json_schema::for_columns(columns);

    // Any failure inside the LLM call is reported against the first
    // non-manual column — it's the closest thing we have to a "scope"
    // when we don't yet know which column choked the model. Tests rely
    // on this column-name presence to assert `Error::Extract`.
    let scope_col = columns
        .iter()
        .find(|c| !c.manual)
        .map(|c| c.name.clone())
        .unwrap_or_else(|| "*".into());

    let out = llm
        .generate_structured(SYSTEM_PROMPT, &user, &schema)
        .await
        .map_err(|e| wrap_extract(&doc_id, &scope_col, e))?;

    let obj = out.value.as_object().ok_or_else(|| Error::Extract {
        doc: doc_id.to_string(),
        col: scope_col.clone(),
        source: format!(
            "LLM returned non-object JSON: {}",
            short_preview(&out.value)
        )
        .into(),
    })?;

    let row_id = RowId::for_doc(review_id, doc_id);
    let extractor_version = llm.model_id().to_string();
    let now = Utc::now();

    let mut cells = Vec::with_capacity(columns.len());
    for col in columns {
        if col.manual {
            continue;
        }
        let Some(envelope) = obj.get(&col.name) else {
            // Missing column: see method-level rustdoc. Phase 7 handles
            // conditional gates; for v1 we silently skip.
            // TODO(T26+): assert presence when the column has no
            // conditional gate (LLM is violating contract).
            continue;
        };
        let cell = parse_cell_envelope(
            envelope,
            col,
            review_id,
            row_id,
            doc_id,
            &extractor_version,
            now,
        )?;
        cells.push(cell);
    }

    Ok(cells)
}

/// Build the user message: chunks first (so the model "sees" the
/// material before being told what to extract), then per-column
/// instructions. Format is intentionally bracket-tagged so the prompt
/// is human-debuggable and string-searchable in fixtures.
fn build_user_prompt(chunks: &[ChunkRef], columns: &[Column]) -> String {
    let mut s = String::with_capacity(chunks.iter().map(|c| c.content.len() + 64).sum::<usize>());
    for ch in chunks {
        s.push_str("[CHUNK::");
        s.push_str(&ch.id.to_string());
        s.push(']');
        s.push_str(&ch.content);
        s.push_str("[/CHUNK]\n");
    }
    s.push('\n');
    for col in columns {
        if col.manual {
            continue;
        }
        s.push_str("[COLUMN::");
        s.push_str(&col.name);
        s.push(']');
        s.push_str(&col.prompt);
        s.push_str("[/COLUMN]\n");
    }
    s
}

/// Decode one `{ value, reasoning, citations }` envelope into a [`Cell`].
fn parse_cell_envelope(
    envelope: &Value,
    col: &Column,
    review_id: ReviewId,
    row_id: RowId,
    doc_id: uuid::Uuid,
    extractor_version: &str,
    now: chrono::DateTime<Utc>,
) -> Result<Cell> {
    let obj = envelope.as_object().ok_or_else(|| Error::Extract {
        doc: doc_id.to_string(),
        col: col.name.clone(),
        source: format!(
            "column envelope is not a JSON object: {}",
            short_preview(envelope)
        )
        .into(),
    })?;

    let value = obj.get("value").cloned().ok_or_else(|| Error::Extract {
        doc: doc_id.to_string(),
        col: col.name.clone(),
        source: "column envelope missing `value`".into(),
    })?;

    let reasoning = obj
        .get("reasoning")
        .and_then(|v| v.as_str())
        .map(str::to_string);

    let citations_val = obj.get("citations").ok_or_else(|| Error::Extract {
        doc: doc_id.to_string(),
        col: col.name.clone(),
        source: "column envelope missing `citations`".into(),
    })?;
    let citations_arr = citations_val.as_array().ok_or_else(|| Error::Extract {
        doc: doc_id.to_string(),
        col: col.name.clone(),
        source: "`citations` is not an array".into(),
    })?;
    let mut citations = Vec::with_capacity(citations_arr.len());
    for c in citations_arr {
        citations.push(parse_citation(c, doc_id, &col.name)?);
    }

    Ok(Cell {
        review_id,
        row_id,
        col_id: col.id,
        value,
        reasoning,
        citations,
        support_score: 0.0,
        confidence: Confidence::Medium,
        locked: false,
        version: 1,
        author: Author::System {
            extractor_version: extractor_version.to_string(),
        },
        updated_at: now,
    })
}

fn parse_citation(v: &Value, doc_id: uuid::Uuid, col: &str) -> Result<Citation> {
    let obj = v.as_object().ok_or_else(|| Error::Extract {
        doc: doc_id.to_string(),
        col: col.into(),
        source: "citation is not a JSON object".into(),
    })?;

    let chunk_id_str = obj
        .get("chunk_id")
        .and_then(|x| x.as_str())
        .ok_or_else(|| Error::Extract {
            doc: doc_id.to_string(),
            col: col.into(),
            source: "citation missing string `chunk_id`".into(),
        })?;
    let chunk_id = uuid::Uuid::parse_str(chunk_id_str).map_err(|e| Error::Extract {
        doc: doc_id.to_string(),
        col: col.into(),
        source: format!("citation `chunk_id` not a UUID: {e}").into(),
    })?;

    let byte_start = u32_field(obj, "byte_start", doc_id, col)?;
    let byte_end = u32_field(obj, "byte_end", doc_id, col)?;
    let quoted_text = obj
        .get("quoted_text")
        .and_then(|x| x.as_str())
        .ok_or_else(|| Error::Extract {
            doc: doc_id.to_string(),
            col: col.into(),
            source: "citation missing string `quoted_text`".into(),
        })?
        .to_string();

    // `page` is not in the v1 JSON schema; tolerate it if a future
    // schema starts emitting it but don't require it here.
    let page = obj.get("page").and_then(|x| x.as_u64()).and_then(|n| {
        let n: u32 = n.try_into().ok()?;
        Some(n)
    });

    Ok(Citation {
        chunk_id,
        byte_start,
        byte_end,
        quoted_text,
        page,
    })
}

fn u32_field(
    obj: &serde_json::Map<String, Value>,
    key: &str,
    doc_id: uuid::Uuid,
    col: &str,
) -> Result<u32> {
    let n = obj
        .get(key)
        .and_then(|x| x.as_u64())
        .ok_or_else(|| Error::Extract {
            doc: doc_id.to_string(),
            col: col.into(),
            source: format!("citation missing integer `{key}`").into(),
        })?;
    u32::try_from(n).map_err(|_| Error::Extract {
        doc: doc_id.to_string(),
        col: col.into(),
        source: format!("citation `{key}` does not fit in u32: {n}").into(),
    })
}

fn wrap_extract(doc_id: &uuid::Uuid, col: &str, e: Error) -> Error {
    Error::Extract {
        doc: doc_id.to_string(),
        col: col.into(),
        source: Box::new(e),
    }
}

fn short_preview(v: &Value) -> String {
    let s = v.to_string();
    if s.len() > 120 {
        format!("{}…", &s[..120])
    } else {
        s
    }
}
