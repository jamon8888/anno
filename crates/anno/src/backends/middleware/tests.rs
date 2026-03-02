use super::*;
use crate::HeuristicNER;

#[test]
fn test_normalize_whitespace() {
    let mw = NormalizeWhitespace;
    let mut ctx = MiddlewareContext::new("  hello   world  ");
    let text = ctx.original_text.clone();
    let result = mw
        .pre_process(&mut ctx, &text)
        .expect("pre_process should succeed");
    assert_eq!(result, "hello world");
}

#[test]
fn test_filter_by_confidence() {
    let mw = FilterByConfidence(0.5);
    let mut ctx = MiddlewareContext::new("test");
    let entities = vec![
        Entity::new("high", EntityType::Person, 0, 4, 0.8),
        Entity::new("low", EntityType::Person, 5, 8, 0.3),
    ];
    let result = mw
        .post_process(&mut ctx, entities)
        .expect("post_process should succeed");
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].text, "high");
}

#[test]
fn test_pipeline_basic() {
    let pipeline = Pipeline::new(Box::new(HeuristicNER::new()))
        .with(NormalizeWhitespace)
        .with(FilterByConfidence(0.3));

    let _entities = pipeline
        .extract("Hello  World")
        .expect("extraction should succeed");
    // Just verify it runs without error
}

#[test]
fn test_remove_overlaps() {
    let mw = RemoveOverlaps;
    let mut ctx = MiddlewareContext::new("New York City");
    let entities = vec![
        Entity::new("New York", EntityType::Location, 0, 8, 0.9),
        Entity::new("York City", EntityType::Location, 4, 13, 0.7),
    ];
    let result = mw
        .post_process(&mut ctx, entities)
        .expect("post_process should succeed");
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].text, "New York"); // Higher confidence wins
}

#[test]
fn test_hooked_pipeline_basic() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    let pipeline = HookedPipeline::new(Box::new(HeuristicNER::new())).with(NormalizeWhitespace);

    // Track hook invocations
    let before_count = Arc::new(AtomicUsize::new(0));
    let after_count = Arc::new(AtomicUsize::new(0));

    let before_count_clone = Arc::clone(&before_count);
    pipeline.on(HookEvent::BeforeExtraction, move |_, _, _| {
        before_count_clone.fetch_add(1, Ordering::SeqCst);
    });

    let after_count_clone = Arc::clone(&after_count);
    pipeline.on(HookEvent::AfterExtraction, move |_, _, _| {
        after_count_clone.fetch_add(1, Ordering::SeqCst);
    });

    let _entities = pipeline.extract("Hello World").unwrap();

    assert_eq!(before_count.load(Ordering::SeqCst), 1);
    assert_eq!(after_count.load(Ordering::SeqCst), 1);
}

#[test]
fn test_hooked_pipeline_entity_found_hook() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    let pipeline = HookedPipeline::new(Box::new(HeuristicNER::new()));

    let entity_count = Arc::new(AtomicUsize::new(0));
    let entity_count_clone = Arc::clone(&entity_count);

    pipeline.on(HookEvent::EntityFound, move |_, _, entities| {
        if entities.is_some() {
            entity_count_clone.fetch_add(1, Ordering::SeqCst);
        }
    });

    // HeuristicNER should find capitalized words
    let _entities = pipeline.extract("John Smith went to New York").unwrap();

    // EntityFound should be called for each entity
    assert!(entity_count.load(Ordering::SeqCst) > 0);
}

#[test]
fn test_hooked_pipeline_with_middleware() {
    let pipeline = HookedPipeline::new(Box::new(HeuristicNER::new()))
        .with(NormalizeWhitespace)
        .with(FilterByConfidence(0.3));

    let entities = pipeline
        .extract("  John   Smith  ")
        .expect("extraction should succeed");
    // Should normalize whitespace and filter by confidence
    // Just verify it runs without error
    let _ = entities;
}

#[test]
fn test_hooked_pipeline_hook_count() {
    let pipeline = HookedPipeline::new(Box::new(HeuristicNER::new()));

    assert_eq!(pipeline.hook_count(), 0);

    pipeline.on(HookEvent::BeforeExtraction, |_, _, _| {});
    pipeline.on(HookEvent::AfterExtraction, |_, _, _| {});
    pipeline.on(HookEvent::EntityFound, |_, _, _| {});

    assert_eq!(pipeline.hook_count(), 3);
}

#[test]
fn test_pipeline_stage_adapter() {
    use crate::backends::streaming::PipelineStage;

    /// A simple PipelineStage that filters entities below a confidence threshold.
    struct MinConfidenceStage(f64);

    impl PipelineStage for MinConfidenceStage {
        fn process(&self, entities: Vec<Entity>, _text: &str) -> Vec<Entity> {
            entities
                .into_iter()
                .filter(|e| e.confidence >= self.0)
                .collect()
        }

        fn name(&self) -> &'static str {
            "min_confidence_stage"
        }
    }

    // Use with_stage on Pipeline
    let pipeline = Pipeline::new(Box::new(HeuristicNER::new()))
        .with_stage(MinConfidenceStage(0.5));

    assert_eq!(pipeline.middleware_names(), vec!["min_confidence_stage"]);

    let _entities = pipeline
        .extract("John Smith")
        .expect("extraction through adapted stage should succeed");
}

#[test]
fn test_pipeline_stage_adapter_on_hooked_pipeline() {
    use crate::backends::streaming::PipelineStage;

    struct UpperCaseText;

    impl PipelineStage for UpperCaseText {
        fn process(&self, mut entities: Vec<Entity>, _text: &str) -> Vec<Entity> {
            for e in &mut entities {
                e.text = e.text.to_uppercase();
            }
            entities
        }

        fn name(&self) -> &'static str {
            "uppercase_text"
        }
    }

    let pipeline = HookedPipeline::new(Box::new(HeuristicNER::new()))
        .with_stage(UpperCaseText);

    assert_eq!(pipeline.middleware_names(), vec!["uppercase_text"]);

    let entities = pipeline
        .extract("John Smith")
        .expect("extraction should succeed");

    for e in &entities {
        assert_eq!(e.text, e.text.to_uppercase(), "entity text should be uppercased");
    }
}
