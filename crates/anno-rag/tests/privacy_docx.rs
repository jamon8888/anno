use anno_rag::docx_instructions::{read_docx_instructions, InstructionAction};
use anno_rag::privacy_docx::{write_normalized_docx, NormalizedDocx, NormalizedSection};
use std::io::Read;
use std::io::Write;

#[test]
fn writes_minimal_docx_with_metadata_and_sections() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = dir.path().join("working.docx");
    let doc = NormalizedDocx {
        title: "contracts/contract.pdf".to_string(),
        metadata: vec![
            ("Source".to_string(), "contracts/contract.pdf".to_string()),
            ("Extraction".to_string(), "Kreuzberg".to_string()),
        ],
        sections: vec![NormalizedSection {
            heading: "Page 1".to_string(),
            body: "Jean Dupont signe le contrat.".to_string(),
        }],
    };

    write_normalized_docx(&out, &doc).expect("write docx");

    let file = std::fs::File::open(&out).expect("open docx");
    let mut zip = zip::ZipArchive::new(file).expect("zip");
    let mut xml = String::new();
    zip.by_name("word/document.xml")
        .expect("document.xml")
        .read_to_string(&mut xml)
        .expect("read xml");

    assert!(xml.contains("contracts/contract.pdf"));
    assert!(xml.contains("Jean Dupont signe le contrat."));
    assert!(!xml.contains("à masquer"));
}

#[test]
fn escapes_xml_text() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = dir.path().join("escaped.docx");
    let doc = NormalizedDocx {
        title: "A & B".to_string(),
        metadata: Vec::new(),
        sections: vec![NormalizedSection {
            heading: "Clause <1>".to_string(),
            body: "A < B & C > D".to_string(),
        }],
    };

    write_normalized_docx(&out, &doc).expect("write docx");
    let mut zip = zip::ZipArchive::new(std::fs::File::open(&out).expect("open")).expect("zip");
    let mut xml = String::new();
    zip.by_name("word/document.xml")
        .expect("document.xml")
        .read_to_string(&mut xml)
        .expect("read xml");

    assert!(xml.contains("A &amp; B"));
    assert!(xml.contains("Clause &lt;1&gt;"));
    assert!(xml.contains("A &lt; B &amp; C &gt; D"));
}

#[test]
fn reads_a_masquer_and_a_garder_comments_from_docx() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("comments.docx");
    write_commented_docx(&path);

    let instructions = read_docx_instructions(&path).expect("read instructions");

    assert_eq!(instructions.len(), 2);
    assert_eq!(instructions[0].action, InstructionAction::Mask);
    assert_eq!(instructions[0].selected_text, "Jean Dupont");
    assert_eq!(instructions[1].action, InstructionAction::Keep);
    assert_eq!(instructions[1].selected_text, "Orange");
}

fn write_commented_docx(path: &std::path::Path) {
    let file = std::fs::File::create(path).expect("create");
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    zip.start_file("[Content_Types].xml", options)
        .expect("content types");
    zip.write_all(
        br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
  <Override PartName="/word/comments.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.comments+xml"/>
</Types>"#,
    )
    .expect("write content types");

    zip.add_directory("_rels/", options).expect("rels dir");
    zip.start_file("_rels/.rels", options).expect("rels");
    zip.write_all(
        br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"#,
    )
    .expect("write rels");

    zip.add_directory("word/", options).expect("word dir");
    zip.start_file("word/document.xml", options)
        .expect("document");
    zip.write_all(
        br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:r><w:t>Client </w:t></w:r>
      <w:commentRangeStart w:id="0"/>
      <w:r><w:t>Jean Dupont</w:t></w:r>
      <w:commentRangeEnd w:id="0"/>
      <w:r><w:commentReference w:id="0"/></w:r>
    </w:p>
    <w:p>
      <w:r><w:t>Marque </w:t></w:r>
      <w:commentRangeStart w:id="1"/>
      <w:r><w:t>Orange</w:t></w:r>
      <w:commentRangeEnd w:id="1"/>
      <w:r><w:commentReference w:id="1"/></w:r>
    </w:p>
  </w:body>
</w:document>"#,
    )
    .expect("write document");

    zip.start_file("word/comments.xml", options)
        .expect("comments");
    zip.write_all(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:comments xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:comment w:id="0"><w:p><w:r><w:t>à masquer</w:t></w:r></w:p></w:comment>
  <w:comment w:id="1"><w:p><w:r><w:t>a garder</w:t></w:r></w:p></w:comment>
</w:comments>"#
            .as_bytes(),
    )
    .expect("write comments");

    zip.finish().expect("finish");
}
