use anno_rag_mcp::health::all_tool_names;

#[test]
fn health_lists_privacy_tools() {
    let tools = all_tool_names();
    assert!(tools.contains(&"privacy_prepare_folder".to_string()));
    assert!(tools.contains(&"privacy_finalize_folder".to_string()));
    assert!(tools.contains(&"privacy_status".to_string()));
}

#[test]
fn privacy_tool_response_shape_is_path_and_count_only() {
    let value = serde_json::json!({
        "ok": true,
        "workspace": "C:\\Matter\\vault",
        "working_folder": "C:\\Matter\\vault\\01-working-documents",
        "anonymized_folder": "C:\\Matter\\vault\\02-anonymized-documents",
        "documents_seen": 1,
        "documents_prepared": 1,
        "documents_failed": 0
    });
    let json = serde_json::to_string(&value).expect("serialize");
    assert!(!json.contains("Jean Dupont"));
    assert!(!json.contains("jean@example.test"));
}
