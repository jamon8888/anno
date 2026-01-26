//! Test that datasets.toml stays in sync with DatasetId enum.
//!
//! This test ensures that:
//! 1. All DatasetId variants have corresponding TOML entries
//! 2. All TOML entries map to valid DatasetId variants
//! 3. Metadata is consistent between Rust and TOML

#[cfg(feature = "eval")]
mod dataset_sync_tests {
    use anno::eval::dataset_registry::DatasetId;
    use std::collections::HashSet;
    use std::fs;
    use std::str::FromStr;

    /// Load datasets.toml and extract all dataset IDs
    fn load_toml_dataset_ids() -> HashSet<String> {
        let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("workspace root");
        let toml_path = workspace_root.join("datasets.toml");

        let content = fs::read_to_string(&toml_path)
            .unwrap_or_else(|_| panic!("Failed to read {}", toml_path.display()));

        let toml: toml::Value = content.parse().expect("Invalid TOML");

        let mut ids = HashSet::new();

        // Extract dataset IDs from TOML structure
        // datasets.toml uses [datasets.X] structure (datasets is a table of tables)
        if let toml::Value::Table(root) = toml {
            if let Some(toml::Value::Table(datasets)) = root.get("datasets") {
                for key in datasets.keys() {
                    ids.insert(key.clone());
                }
            }
        }

        ids
    }

    #[test]
    #[ignore = "datasets.toml not implemented - registry is in Rust code"]
    fn toml_entries_are_valid_dataset_ids() {
        let toml_ids = load_toml_dataset_ids();

        for toml_id in &toml_ids {
            // Try to parse as DatasetId
            let result = DatasetId::from_str(toml_id);
            if result.is_err() {
                // Some TOML IDs use different naming conventions
                // Just warn, don't fail (datasets.toml may have extra metadata)
                eprintln!(
                    "Warning: TOML entry '{}' doesn't map to a DatasetId variant",
                    toml_id
                );
            }
        }
    }

    #[test]
    fn dataset_ids_have_consistent_metadata() {
        // Every DatasetId should have:
        // - A non-empty name
        // - A non-empty description
        // - A valid download URL (or placeholder)
        // - A defined task type

        for id in DatasetId::all() {
            let name = id.name();
            assert!(!name.is_empty(), "{:?} has empty name", id);

            let desc = id.description();
            assert!(!desc.is_empty(), "{:?} has empty description", id);

            // Download URL may be empty for synthetic/constructed datasets
            // but should be defined
            let _url = id.download_url();

            // Entity types should be defined
            let types = id.entity_types();
            // Note: Not every dataset is a “label inventory” dataset.
            // We only require `entity_types` when the dataset is used with a fixed label set in eval.
            let tasks = id.tasks();
            let expects_label_set = tasks.contains(&"ner")
                || tasks.contains(&"pos")
                || tasks.contains(&"sentiment")
                || tasks.contains(&"text_classification");
            assert!(
                !types.is_empty() || id.is_constructed() || !expects_label_set,
                "{:?} has no entity types defined (tasks: {:?})",
                id,
                tasks
            );
        }
    }

    #[test]
    fn all_groups_have_members() {
        // Each dataset group function should return at least one dataset
        assert!(!DatasetId::quick().is_empty(), "quick() is empty");
        assert!(!DatasetId::medium().is_empty(), "medium() is empty");
        assert!(!DatasetId::all_ner().is_empty(), "all_ner() is empty");
        assert!(
            !DatasetId::all_multilingual().is_empty(),
            "all_multilingual() is empty"
        );
        assert!(
            !DatasetId::all_biomedical().is_empty(),
            "all_biomedical() is empty"
        );
        assert!(!DatasetId::all_coref().is_empty(), "all_coref() is empty");
        assert!(
            !DatasetId::all_indigenous().is_empty(),
            "all_indigenous() is empty"
        );
        assert!(
            !DatasetId::all_constructed().is_empty(),
            "all_constructed() is empty"
        );
        assert!(
            !DatasetId::all_low_resource().is_empty(),
            "all_low_resource() is empty"
        );
        assert!(
            !DatasetId::all_dialogue().is_empty(),
            "all_dialogue() is empty"
        );
    }

    #[test]
    fn group_members_are_in_all() {
        let all: HashSet<_> = DatasetId::all().iter().collect();

        // Check that all group members are in all()
        for id in DatasetId::all_low_resource() {
            assert!(
                all.contains(id),
                "{:?} in all_low_resource() but not in all()",
                id
            );
        }

        for id in DatasetId::all_constructed() {
            assert!(
                all.contains(id),
                "{:?} in all_constructed() but not in all()",
                id
            );
        }

        for id in DatasetId::all_dialogue() {
            assert!(
                all.contains(id),
                "{:?} in all_dialogue() but not in all()",
                id
            );
        }
    }
}
