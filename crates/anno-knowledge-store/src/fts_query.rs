//! FTS5 query normalization for local user input.

const MAX_FTS_TERMS: usize = 8;

/// Convert user input into a conservative FTS5 expression.
///
/// Returns `None` if the input contains no usable terms after filtering.
/// Each term is double-quoted and inner quotes are escaped (FTS5 doubling).
/// At most [`MAX_FTS_TERMS`] terms are produced.
#[must_use]
pub fn build_fts_query(input: &str) -> Option<String> {
    let terms: Vec<String> = input
        .split(|c: char| !(c.is_alphanumeric() || c == '\'' || c == '-'))
        .filter_map(|raw| {
            let trimmed = raw.trim_matches(|c: char| c == '\'' || c == '-');
            if trimmed.is_empty() {
                None
            } else {
                Some(format!("\"{}\"", trimmed.replace('"', "\"\"")))
            }
        })
        .take(MAX_FTS_TERMS)
        .collect();

    if terms.is_empty() {
        None
    } else {
        Some(terms.join(" AND "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_quotes_terms_and_escapes_quotes() {
        let query = build_fts_query("contrat \"Dupont\" 2026").expect("query");

        assert_eq!(query, "\"contrat\" AND \"Dupont\" AND \"2026\"");
    }

    #[test]
    fn query_drops_only_punctuation_input() {
        assert_eq!(build_fts_query("!? /"), None);
    }

    #[test]
    fn query_caps_term_count() {
        let query = build_fts_query("a b c d e f g h i j k l").expect("query");

        assert_eq!(
            query,
            "\"a\" AND \"b\" AND \"c\" AND \"d\" AND \"e\" AND \"f\" AND \"g\" AND \"h\""
        );
    }
}
