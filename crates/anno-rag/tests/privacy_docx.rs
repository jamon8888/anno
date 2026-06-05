use anno_rag::privacy_docx::{write_normalized_docx, NormalizedDocx, NormalizedSection};
use std::io::Read;

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
