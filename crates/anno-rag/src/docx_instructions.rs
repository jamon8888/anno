//! Read plain-language privacy instructions from Word comments.

use crate::error::{Error, Result};
use quick_xml::events::{BytesStart, BytesText, Event};
use quick_xml::Reader;
use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;

/// Supported user instruction actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstructionAction {
    /// Force anonymization for selected text.
    Mask,
    /// Keep selected text visible as a reviewed false positive.
    Keep,
}

/// One instruction extracted from a Word comment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocxInstruction {
    /// Word comment id.
    pub comment_id: String,
    /// Parsed action.
    pub action: InstructionAction,
    /// Text selected by the Word comment anchor.
    pub selected_text: String,
}

/// Read supported privacy instructions from a `.docx`.
///
/// # Errors
/// Returns [`Error::Privacy`] when the DOCX package or XML cannot be read.
pub fn read_docx_instructions(path: &Path) -> Result<Vec<DocxInstruction>> {
    let file = std::fs::File::open(path)?;
    let mut zip =
        zip::ZipArchive::new(file).map_err(|e| Error::Privacy(format!("open docx: {e}")))?;

    let mut document_xml = String::new();
    zip.by_name("word/document.xml")
        .map_err(|e| Error::Privacy(format!("missing word/document.xml: {e}")))?
        .read_to_string(&mut document_xml)?;

    let mut comments_xml = String::new();
    match zip.by_name("word/comments.xml") {
        Ok(mut comments) => {
            comments.read_to_string(&mut comments_xml)?;
        }
        Err(_) => return Ok(Vec::new()),
    }

    let comment_actions = parse_comment_actions(&comments_xml)?;
    let selected_text = parse_comment_ranges(&document_xml)?;

    let mut instructions = Vec::new();
    for (comment_id, action) in comment_actions {
        if let Some(text) = selected_text.get(&comment_id) {
            let selected = text.trim();
            if !selected.is_empty() {
                instructions.push(DocxInstruction {
                    comment_id,
                    action,
                    selected_text: selected.to_string(),
                });
            }
        }
    }
    Ok(instructions)
}

fn parse_comment_actions(xml: &str) -> Result<BTreeMap<String, InstructionAction>> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut current_id: Option<String> = None;
    let mut current_text = String::new();
    let mut out = BTreeMap::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) if e.name().as_ref() == b"w:comment" => {
                current_id = attr_value(&e, b"w:id");
                current_text.clear();
            }
            Ok(Event::Text(e)) if current_id.is_some() => {
                current_text.push_str(&decode_text(&e, "comment text")?);
            }
            Ok(Event::End(e)) if e.name().as_ref() == b"w:comment" => {
                if let Some(id) = current_id.take() {
                    if let Some(action) = normalize_instruction(&current_text) {
                        out.insert(id, action);
                    }
                }
                current_text.clear();
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(e) => return Err(Error::Privacy(format!("parse comments.xml: {e}"))),
        }
        buf.clear();
    }
    Ok(out)
}

fn parse_comment_ranges(xml: &str) -> Result<BTreeMap<String, String>> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut buf = Vec::new();
    let mut active: Vec<String> = Vec::new();
    let mut out: BTreeMap<String, String> = BTreeMap::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Empty(e)) if e.name().as_ref() == b"w:commentRangeStart" => {
                if let Some(id) = attr_value(&e, b"w:id") {
                    active.push(id);
                }
            }
            Ok(Event::Empty(e)) if e.name().as_ref() == b"w:commentRangeEnd" => {
                if let Some(id) = attr_value(&e, b"w:id") {
                    active.retain(|candidate| candidate != &id);
                }
            }
            Ok(Event::Text(e)) if !active.is_empty() => {
                let text = decode_text(&e, "document text")?;
                for id in &active {
                    out.entry(id.clone()).or_default().push_str(&text);
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(e) => return Err(Error::Privacy(format!("parse document.xml: {e}"))),
        }
        buf.clear();
    }
    Ok(out)
}

fn decode_text(e: &BytesText<'_>, context: &str) -> Result<String> {
    e.xml10_content()
        .map(|text| text.into_owned())
        .map_err(|err| Error::Privacy(format!("{context}: {err}")))
}

fn attr_value(e: &BytesStart<'_>, key: &[u8]) -> Option<String> {
    e.attributes().flatten().find_map(|attr| {
        if attr.key.as_ref() == key {
            Some(String::from_utf8_lossy(&attr.value).to_string())
        } else {
            None
        }
    })
}

/// Normalize a Word comment body into an action.
#[must_use]
pub fn normalize_instruction(text: &str) -> Option<InstructionAction> {
    let normalized = text
        .trim()
        .to_lowercase()
        .replace('à', "a")
        .replace('â', "a")
        .replace('é', "e")
        .replace('è', "e")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");

    match normalized.as_str() {
        "a masquer" => Some(InstructionAction::Mask),
        "a garder" => Some(InstructionAction::Keep),
        _ => None,
    }
}
