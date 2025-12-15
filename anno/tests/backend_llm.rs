//! Tests for LLM-based NER backends (llm_prompt module)
//!
//! These tests verify the LLM prompt construction and BIO schema
//! without requiring actual API calls.

use anno::backends::llm_prompt::{BIOSchema, CodeNERPrompt};
use anno::EntityType;

#[test]
fn test_bio_schema_creation() {
    let types = [
        EntityType::Person,
        EntityType::Organization,
        EntityType::Location,
    ];
    let schema = BIOSchema::new(&types);

    assert_eq!(schema.entity_types.len(), 3);
    assert!(schema.descriptions.contains_key(&EntityType::Person));
}

#[test]
fn test_bio_schema_descriptions() {
    let types = [EntityType::Person];
    let schema = BIOSchema::new(&types);

    let desc = schema.descriptions.get(&EntityType::Person).unwrap();
    assert!(desc.contains("Person") || desc.to_lowercase().contains("person"));
}

#[test]
fn test_bio_schema_custom_description() {
    let types = [EntityType::Person];
    let schema =
        BIOSchema::new(&types).with_description(EntityType::Person, "Names of human individuals");

    let desc = schema.descriptions.get(&EntityType::Person).unwrap();
    assert!(desc.contains("human individuals"));
}

#[test]
fn test_code_ner_prompt_basic() {
    let schema = BIOSchema::new(&[EntityType::Person, EntityType::Organization]);

    let prompt = CodeNERPrompt::new(schema);
    let rendered = prompt.render("Apple Inc. CEO Tim Cook announced today.");

    // Should contain the input text
    assert!(rendered.contains("Apple Inc. CEO Tim Cook"));
}

#[test]
fn test_code_ner_prompt_with_demonstrations() {
    let schema = BIOSchema::new(&[EntityType::Person, EntityType::Organization]);

    let demos = vec![(
        "Steve Jobs founded Apple.",
        vec![
            ("Steve Jobs", "PER", 0usize, 10usize),
            ("Apple", "ORG", 19usize, 24usize),
        ],
    )];

    let prompt = CodeNERPrompt::new(schema).with_demonstrations(demos);
    let rendered = prompt.render("Test input");

    // Should include demonstration text
    assert!(rendered.contains("Steve Jobs") || rendered.contains("demonstration"));
}

#[test]
fn test_code_ner_prompt_chain_of_thought() {
    let schema = BIOSchema::new(&[EntityType::Person]);
    let prompt = CodeNERPrompt::new(schema).with_chain_of_thought(true);
    let rendered = prompt.render("Marie Curie was a scientist.");

    // CoT prompts are typically longer and include reasoning steps
    assert!(!rendered.is_empty());
}

#[test]
fn test_code_ner_prompt_output_format() {
    let schema = BIOSchema::new(&[EntityType::Person]);

    let prompt = CodeNERPrompt::new(schema);

    // Check that output format description is provided
    let format_desc = prompt.output_format();
    assert!(!format_desc.is_empty());
    assert!(format_desc.contains("text") || format_desc.contains("type"));
}

#[test]
fn test_bio_schema_all_standard_types() {
    let types = [
        EntityType::Person,
        EntityType::Organization,
        EntityType::Location,
        EntityType::Date,
        EntityType::Time,
        EntityType::Money,
        EntityType::Percent,
        EntityType::Email,
        EntityType::Phone,
        EntityType::Url,
    ];

    let schema = BIOSchema::new(&types);

    // All types should have descriptions
    for t in &types {
        assert!(
            schema.descriptions.contains_key(t),
            "Missing description for {:?}",
            t
        );
    }
}

#[test]
fn test_bio_schema_custom_type() {
    use anno::EntityCategory;

    let custom_type = EntityType::custom("GENE", EntityCategory::Misc);

    let schema = BIOSchema::new(std::slice::from_ref(&custom_type));
    assert!(schema.entity_types.contains(&custom_type));
}

#[test]
fn test_code_ner_prompt_empty_text() {
    let schema = BIOSchema::new(&[EntityType::Person]);
    let prompt = CodeNERPrompt::new(schema);

    // Should handle empty text gracefully
    let rendered = prompt.render("");
    assert!(!rendered.is_empty()); // Still produces a prompt, just with empty input
}

#[test]
fn test_code_ner_prompt_special_characters() {
    let schema = BIOSchema::new(&[EntityType::Person]);
    let prompt = CodeNERPrompt::new(schema);

    // Should handle special characters
    let text_with_special = "John's company \"Acme Corp\" made $1M.";
    let rendered = prompt.render(text_with_special);

    // Should preserve or properly escape the input
    assert!(rendered.contains("John") || rendered.contains("Acme"));
}
