//! Registry of deprecated tool names hidden from `list_tools` unless
//! `ANNO_EXPOSE_DEPRECATED=1`. Handlers stay callable. Spec C §4 (U4) + §11 (D2).

/// Tool names hidden by default. Includes legacy tools (U4) and the bare names
/// superseded by canonical names (D2).
pub(crate) const DEPRECATED_TOOLS: &[&str] = &[
    // U4 — legacy duplicates superseded by unified `search`
    "legacy_search",
    "knowledge_search",
    "legal_ingest",
    "legal_search",
    // U4 — legacy knowledge status/sources superseded by unified `service_status`
    "knowledge_sources",
    "knowledge_status",
    // U4 — legacy knowledge management superseded by `index` / `forget`
    "knowledge_add_local_folder",
    "knowledge_sync",
    "knowledge_forget",
    // D2 — superseded bare names (canonical: `search`, `index`, `forget`)
    "forget",
    "rehydrate",
    "status",
];

/// True when deprecated tools should still be advertised in `list_tools`.
pub(crate) fn expose_deprecated() -> bool {
    std::env::var("ANNO_EXPOSE_DEPRECATED")
        .map(|v| v == "1")
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hides_deprecated_by_default() {
        // search and detect are NOT in DEPRECATED_TOOLS
        assert!(
            !DEPRECATED_TOOLS.contains(&"search"),
            "search must not be in deprecated list"
        );
        assert!(
            !DEPRECATED_TOOLS.contains(&"detect"),
            "detect must not be in deprecated list"
        );
        assert!(
            !DEPRECATED_TOOLS.contains(&"index"),
            "index must not be in deprecated list"
        );
    }

    #[test]
    fn deprecated_list_is_non_empty() {
        assert!(!DEPRECATED_TOOLS.is_empty());
        assert!(DEPRECATED_TOOLS.contains(&"legacy_search"));
        assert!(DEPRECATED_TOOLS.contains(&"forget"));
        assert!(DEPRECATED_TOOLS.contains(&"status"));
        assert!(DEPRECATED_TOOLS.contains(&"rehydrate"));
        assert!(DEPRECATED_TOOLS.contains(&"knowledge_search"));
        assert!(DEPRECATED_TOOLS.contains(&"knowledge_sources"));
        assert!(DEPRECATED_TOOLS.contains(&"knowledge_status"));
        assert!(DEPRECATED_TOOLS.contains(&"legal_ingest"));
        assert!(DEPRECATED_TOOLS.contains(&"legal_search"));
    }

    #[test]
    fn expose_deprecated_defaults_to_false() {
        // Can only assert the function is callable; env may vary in CI.
        // If ANNO_EXPOSE_DEPRECATED is not set, it must return false.
        // SAFETY: single-threaded test; no other thread reads ANNO_EXPOSE_DEPRECATED.
        unsafe { std::env::remove_var("ANNO_EXPOSE_DEPRECATED") };
        assert!(!expose_deprecated());
    }
}
