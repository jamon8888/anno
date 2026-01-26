//! Regression tests for URL detection false positives
//!
//! Ensures single letters and non-URL text are not detected as URLs.

use anno::{Model, RegexNER};

#[test]
fn regression_single_letters_not_urls() {
    let ner = RegexNER::new();
    let entities = ner.extract_entities("A, B, C are letters", None).unwrap();

    // Should not detect single letters as URLs
    let url_entities: Vec<_> = entities
        .iter()
        .filter(|e| e.entity_type.as_label() == "URL")
        .filter(|e| e.text.len() == 1)
        .collect();

    assert_eq!(
        url_entities.len(),
        0,
        "Single letters should not be detected as URLs. Found: {:?}",
        url_entities
    );
}

#[test]
fn regression_urls_require_protocol() {
    let ner = RegexNER::new();

    // These should NOT be detected as URLs (no protocol)
    let non_urls = vec![
        "example.com",
        "www.example.com",
        "subdomain.example.org/path",
    ];

    for text in non_urls {
        let entities = ner.extract_entities(text, None).unwrap();
        let url_entities: Vec<_> = entities
            .iter()
            .filter(|e| e.entity_type.as_label() == "URL")
            .collect();
        assert_eq!(
            url_entities.len(),
            0,
            "Text without protocol should not be detected as URL: '{}'",
            text
        );
    }

    // These SHOULD be detected as URLs (have protocol)
    let valid_urls = vec![
        "https://example.com",
        "http://www.example.com",
        "https://subdomain.example.org/path?query=1",
    ];

    for text in valid_urls {
        let entities = ner.extract_entities(text, None).unwrap();
        let url_entities: Vec<_> = entities
            .iter()
            .filter(|e| e.entity_type.as_label() == "URL")
            .collect();
        assert!(
            !url_entities.is_empty(),
            "Valid URL should be detected: '{}'",
            text
        );
    }
}
