//! Binary: generate config-schema.json, config.toml.example, docs/reference/configuration.md
//!
//! Run:  cargo run -p anno-rag --bin schema-gen --features generate-schema
//! The three output files are committed and kept in sync by CI.

fn main() {
    let schema = anno_rag::AnnoRagConfig::config_schema();
    let defaults = anno_rag::AnnoRagConfig::default();
    let defaults_json: serde_json::Value =
        serde_json::to_value(&defaults).expect("default config must serialize");

    // 1. config-schema.json
    let schema_entries: Vec<serde_json::Value> = schema
        .iter()
        .map(|f| {
            let default_val = defaults_json
                .get(f.name)
                .map(|v| v.to_string())
                .unwrap_or_default();
            serde_json::json!({
                "name":          f.name,
                "env_var":       f.env_var,
                "cli_flag":      f.cli_flag,
                "doc":           f.doc,
                "default_value": default_val,
                "since":         f.since,
                "type_name":     f.type_name,
            })
        })
        .collect();

    let schema_json = serde_json::to_string_pretty(&schema_entries)
        .expect("schema must serialize");

    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()  // crates/
        .unwrap()
        .parent()  // repo root
        .unwrap();

    let schema_path = root.join("crates/anno-rag/config-schema.json");
    std::fs::write(&schema_path, &schema_json).expect("write config-schema.json");
    eprintln!("wrote {}", schema_path.display());

    // 2. config.toml.example
    let mut toml_lines = vec![
        "# anno-rag configuration file".to_string(),
        "# Copy to ~/.anno-rag/config.toml and uncomment the fields you want to change.".to_string(),
        "# All values shown are the defaults.".to_string(),
        String::new(),
    ];
    for f in schema {
        let default_val = defaults_json
            .get(f.name)
            .map(|v| v.to_string())
            .unwrap_or_default();
        toml_lines.push(format!("# {} (env: {})", f.doc, f.env_var));
        toml_lines.push(format!("# {} = {}", f.name, default_val));
        toml_lines.push(String::new());
    }

    let toml_path = root.join("crates/anno-rag/config.toml.example");
    std::fs::write(&toml_path, toml_lines.join("\n")).expect("write config.toml.example");
    eprintln!("wrote {}", toml_path.display());

    // 3. docs/reference/configuration.md
    let mut md_lines = vec![
        "# Configuration Reference".to_string(),
        String::new(),
        "> Auto-generated from `AnnoRagConfig`. Do not edit by hand — run `cargo run -p anno-rag --bin schema-gen --features generate-schema`.".to_string(),
        String::new(),
        "Precedence (lowest → highest): defaults → `~/.anno-rag/config.toml` → env vars → CLI flags.".to_string(),
        String::new(),
        "| Field | Env var | CLI flag | Default | Since | Description |".to_string(),
        "|-------|---------|----------|---------|-------|-------------|".to_string(),
    ];
    for f in schema {
        let default_val = defaults_json
            .get(f.name)
            .map(|v| v.to_string())
            .unwrap_or_default();
        md_lines.push(format!(
            "| `{}` | `{}` | `{}` | `{}` | {} | {} |",
            f.name, f.env_var, f.cli_flag, default_val, f.since, f.doc
        ));
    }
    md_lines.push(String::new());
    md_lines.push("> **Runtime-only env vars** (not in `config.toml`): `ANNO_MODELS_DIR` (model weights override), `ANNO_RAG_VAULT_PASSPHRASE`, `ANNO_RAG_VAULT_KMS_PROVIDER`, `ANNO_RAG_VAULT_KMS_KEY_ID`.".to_string());

    let docs_path = root.join("docs/reference/configuration.md");
    std::fs::create_dir_all(docs_path.parent().unwrap()).expect("create docs/reference");
    std::fs::write(&docs_path, md_lines.join("\n")).expect("write configuration.md");
    eprintln!("wrote {}", docs_path.display());

    println!("Schema generation complete.");
}
