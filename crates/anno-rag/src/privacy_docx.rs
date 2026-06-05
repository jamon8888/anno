//! Minimal DOCX writers for privacy workspace artifacts.

use crate::error::{Error, Result};
use std::io::Write;
use std::path::Path;
use zip::write::SimpleFileOptions;

/// One normalized `.docx` section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedSection {
    /// Section heading.
    pub heading: String,
    /// Section body.
    pub body: String,
}

/// Normalized `.docx` content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedDocx {
    /// Document title.
    pub title: String,
    /// Metadata rows rendered near the top.
    pub metadata: Vec<(String, String)>,
    /// Body sections.
    pub sections: Vec<NormalizedSection>,
}

/// Write a minimal Word-compatible `.docx`.
///
/// # Errors
/// Returns [`Error::Privacy`] or IO errors when the package cannot be written.
pub fn write_normalized_docx(path: &Path, doc: &NormalizedDocx) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let file = std::fs::File::create(path)?;
    let mut zip = zip::ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    zip.start_file("[Content_Types].xml", options)
        .map_err(|e| Error::Privacy(format!("docx content types: {e}")))?;
    zip.write_all(CONTENT_TYPES.as_bytes())?;

    zip.add_directory("_rels/", options)
        .map_err(|e| Error::Privacy(format!("docx rels dir: {e}")))?;
    zip.start_file("_rels/.rels", options)
        .map_err(|e| Error::Privacy(format!("docx root rels: {e}")))?;
    zip.write_all(ROOT_RELS.as_bytes())?;

    zip.add_directory("word/", options)
        .map_err(|e| Error::Privacy(format!("docx word dir: {e}")))?;
    zip.start_file("word/document.xml", options)
        .map_err(|e| Error::Privacy(format!("docx document: {e}")))?;
    zip.write_all(document_xml(doc).as_bytes())?;

    zip.add_directory("word/_rels/", options)
        .map_err(|e| Error::Privacy(format!("docx word rels dir: {e}")))?;
    zip.start_file("word/_rels/document.xml.rels", options)
        .map_err(|e| Error::Privacy(format!("docx document rels: {e}")))?;
    zip.write_all(DOCUMENT_RELS.as_bytes())?;

    zip.finish()
        .map_err(|e| Error::Privacy(format!("docx finish: {e}")))?;
    Ok(())
}

fn document_xml(doc: &NormalizedDocx) -> String {
    let mut body = String::new();
    body.push_str(&paragraph(&doc.title, "Title"));

    for (key, value) in &doc.metadata {
        body.push_str(&paragraph(&format!("{key}: {value}"), "Normal"));
    }

    for section in &doc.sections {
        body.push_str(&paragraph(&section.heading, "Heading1"));
        for line in section.body.lines() {
            body.push_str(&paragraph(line, "Normal"));
        }
    }

    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    {body}
    <w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440"/></w:sectPr>
  </w:body>
</w:document>"#
    )
}

fn paragraph(text: &str, style: &str) -> String {
    format!(
        r#"<w:p><w:pPr><w:pStyle w:val="{style}"/></w:pPr><w:r><w:t xml:space="preserve">{}</w:t></w:r></w:p>"#,
        escape_xml(text)
    )
}

fn escape_xml(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

const CONTENT_TYPES: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
</Types>"#;

const ROOT_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"#;

const DOCUMENT_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"/>"#;
