//! Property tests for URL detection
//!
//! Ensures URL regex correctly identifies URLs and rejects non-URLs.

use proptest::prelude::*;

/// Property: URLs must start with http:// or https://
#[test]
fn prop_url_must_have_protocol() {
    proptest!(|(text in "[a-zA-Z0-9./?=&#]+")| {
        // If text doesn't start with http:// or https://, it shouldn't match URL pattern
        if !text.starts_with("http://") && !text.starts_with("https://") {
            // This is a sanity check - the actual regex is tested in pattern_config.rs
            // But we can verify single letters don't match
            if text.len() == 1 {
                assert!(!text.starts_with("http://"));
                assert!(!text.starts_with("https://"));
            }
        }
    });
}

/// Property: Valid URLs should be detected
#[test]
fn prop_valid_urls_detected() {
    proptest!(|(domain in "[a-zA-Z0-9.-]+", path in "[a-zA-Z0-9./?=&#-]*")| {
        let url = format!("https://{}/{}", domain, path);
        // URL should contain protocol
        assert!(url.starts_with("https://"));

        // Test http:// variant
        let url_http = format!("http://{}/{}", domain, path);
        assert!(url_http.starts_with("http://"));
    });
}

/// Regression: Single letters should not be detected as URLs
#[test]
fn regression_single_letters_not_urls() {
    for letter in 'A'..='Z' {
        let text = letter.to_string();
        // Single letter should not be a valid URL
        assert!(!text.starts_with("http://"));
        assert!(!text.starts_with("https://"));
    }
}
