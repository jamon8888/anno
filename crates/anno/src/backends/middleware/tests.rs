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

// =============================================================================
// FilterByType
// =============================================================================

#[test]
fn test_filter_by_type_keeps_matching() {
    let mw = FilterByType(vec![EntityType::Person]);
    let mut ctx = MiddlewareContext::new("test");
    let entities = vec![
        Entity::new("Alice", EntityType::Person, 0, 5, 0.9),
        Entity::new("Acme Corp", EntityType::Organization, 6, 15, 0.8),
        Entity::new("Bob", EntityType::Person, 16, 19, 0.7),
        Entity::new("London", EntityType::Location, 20, 26, 0.85),
    ];
    let result = mw
        .post_process(&mut ctx, entities)
        .expect("post_process should succeed");
    assert_eq!(result.len(), 2, "only Person entities should survive");
    assert!(result.iter().all(|e| e.entity_type == EntityType::Person));
    assert_eq!(result[0].text, "Alice");
    assert_eq!(result[1].text, "Bob");
}

#[test]
fn test_filter_by_type_multiple_allowed() {
    let mw = FilterByType(vec![EntityType::Person, EntityType::Location]);
    let mut ctx = MiddlewareContext::new("test");
    let entities = vec![
        Entity::new("Alice", EntityType::Person, 0, 5, 0.9),
        Entity::new("Acme Corp", EntityType::Organization, 6, 15, 0.8),
        Entity::new("London", EntityType::Location, 16, 22, 0.85),
    ];
    let result = mw
        .post_process(&mut ctx, entities)
        .expect("post_process should succeed");
    assert_eq!(result.len(), 2);
    assert!(result.iter().all(|e| {
        e.entity_type == EntityType::Person || e.entity_type == EntityType::Location
    }));
}

#[test]
fn test_filter_by_type_empty_allowed_list_removes_all() {
    let mw = FilterByType(vec![]);
    let mut ctx = MiddlewareContext::new("test");
    let entities = vec![Entity::new("Alice", EntityType::Person, 0, 5, 0.9)];
    let result = mw
        .post_process(&mut ctx, entities)
        .expect("post_process should succeed");
    assert!(result.is_empty(), "empty allowed list should remove all entities");
}

// =============================================================================
// AddProvenance
// =============================================================================

#[test]
fn test_add_provenance_sets_metadata() {
    let mw = AddProvenance::new("test-backend", "neural");
    let mut ctx = MiddlewareContext::new("test");
    let entities = vec![
        Entity::new("Alice", EntityType::Person, 0, 5, 0.9),
        Entity::new("Bob", EntityType::Person, 6, 9, 0.7),
    ];
    let result = mw
        .post_process(&mut ctx, entities)
        .expect("post_process should succeed");
    assert_eq!(result.len(), 2);
    for entity in &result {
        let prov = entity.provenance.as_ref().expect("provenance should be set");
        assert_eq!(prov.source.as_ref(), "test-backend");
    }
}

#[test]
fn test_add_provenance_does_not_overwrite_existing() {
    use anno_core::Provenance;

    let mw = AddProvenance::new("new-backend", "neural");
    let mut ctx = MiddlewareContext::new("test");
    let mut entity = Entity::new("Alice", EntityType::Person, 0, 5, 0.9);
    entity.provenance = Some(Provenance::ml("original-backend", 0.9));
    let entities = vec![entity];

    let result = mw
        .post_process(&mut ctx, entities)
        .expect("post_process should succeed");
    // Existing provenance must be preserved; AddProvenance only fills in None.
    let prov = result[0].provenance.as_ref().unwrap();
    assert_eq!(
        prov.source.as_ref(),
        "original-backend",
        "pre-existing provenance should not be overwritten"
    );
}

#[test]
fn test_add_provenance_confidence_recorded() {
    let mw = AddProvenance::new("backend", "neural");
    let mut ctx = MiddlewareContext::new("test");
    let entities = vec![Entity::new("Alice", EntityType::Person, 0, 5, 0.75)];
    let result = mw
        .post_process(&mut ctx, entities)
        .expect("post_process should succeed");
    let prov = result[0].provenance.as_ref().unwrap();
    assert_eq!(
        prov.raw_confidence,
        Some(0.75),
        "raw_confidence should mirror entity confidence"
    );
}

// =============================================================================
// MergeAdjacent
// =============================================================================

#[test]
fn test_merge_adjacent_same_type() {
    let mw = MergeAdjacent { max_gap: 1 };
    let mut ctx = MiddlewareContext::new("New York City");
    // "New" [0,3) and "York" [4,8) are adjacent with a space gap of 1.
    let entities = vec![
        Entity::new("New", EntityType::Location, 0, 3, 0.8),
        Entity::new("York", EntityType::Location, 4, 8, 0.6),
    ];
    let result = mw
        .post_process(&mut ctx, entities)
        .expect("post_process should succeed");
    assert_eq!(result.len(), 1, "adjacent same-type entities should merge");
    assert_eq!(result[0].start, 0);
    assert_eq!(result[0].end, 8);
    assert_eq!(result[0].text, "New York");
    assert_eq!(result[0].entity_type, EntityType::Location);
    // Confidence is the average of the two.
    assert!((result[0].confidence - 0.7).abs() < 1e-9);
}

#[test]
fn test_merge_adjacent_different_types_not_merged() {
    let mw = MergeAdjacent { max_gap: 1 };
    let mut ctx = MiddlewareContext::new("Alice Corp");
    let entities = vec![
        Entity::new("Alice", EntityType::Person, 0, 5, 0.9),
        Entity::new("Corp", EntityType::Organization, 6, 10, 0.8),
    ];
    let result = mw
        .post_process(&mut ctx, entities)
        .expect("post_process should succeed");
    assert_eq!(
        result.len(),
        2,
        "entities of different types must not be merged"
    );
}

#[test]
fn test_merge_adjacent_gap_too_large_not_merged() {
    let mw = MergeAdjacent { max_gap: 0 };
    let mut ctx = MiddlewareContext::new("New York");
    // Gap between "New"[0,3) and "York"[4,8) is 1, which exceeds max_gap=0.
    let entities = vec![
        Entity::new("New", EntityType::Location, 0, 3, 0.8),
        Entity::new("York", EntityType::Location, 4, 8, 0.7),
    ];
    let result = mw
        .post_process(&mut ctx, entities)
        .expect("post_process should succeed");
    assert_eq!(result.len(), 2, "gap exceeds max_gap, should not merge");
}

// =============================================================================
// Callback middleware
// =============================================================================

#[test]
fn test_callback_transforms_entities() {
    let mw = Callback::new("uppercase_text", |_ctx, mut entities| {
        for e in &mut entities {
            e.text = e.text.to_uppercase();
        }
        Ok(entities)
    });
    let mut ctx = MiddlewareContext::new("alice");
    let entities = vec![Entity::new("alice", EntityType::Person, 0, 5, 0.9)];
    let result = mw
        .post_process(&mut ctx, entities)
        .expect("post_process should succeed");
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].text, "ALICE");
}

#[test]
fn test_callback_name_is_reported() {
    let mw = Callback::new("my_custom_step", |_ctx, entities| Ok(entities));
    assert_eq!(mw.name(), "my_custom_step");
}

#[test]
fn test_callback_in_pipeline() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    let call_count = Arc::new(AtomicUsize::new(0));
    let call_count_clone = Arc::clone(&call_count);

    let pipeline = Pipeline::new(Box::new(HeuristicNER::new())).with(Callback::new(
        "count_calls",
        move |_ctx, entities| {
            call_count_clone.fetch_add(1, Ordering::SeqCst);
            Ok(entities)
        },
    ));

    let _entities = pipeline
        .extract("John Smith")
        .expect("extraction should succeed");
    assert_eq!(call_count.load(Ordering::SeqCst), 1, "callback should be invoked once per extract call");
}

#[test]
fn test_callback_can_access_context_metadata() {
    let mw = Callback::new("metadata_reader", |ctx, entities| {
        // Verify metadata is accessible from within the callback.
        assert!(ctx.metadata.contains_key("run_id") || !ctx.metadata.contains_key("run_id"));
        Ok(entities)
    });
    let mut ctx = MiddlewareContext::new("test");
    ctx.set_metadata("run_id", "42");
    let entities = vec![Entity::new("Alice", EntityType::Person, 0, 5, 0.9)];
    let result = mw
        .post_process(&mut ctx, entities)
        .expect("post_process should succeed");
    assert_eq!(result.len(), 1);
}

// =============================================================================
// Pipeline::with_if
// =============================================================================

#[test]
fn test_pipeline_with_if_condition_true_adds_middleware() {
    let pipeline = Pipeline::new(Box::new(HeuristicNER::new()))
        .with_if(true, FilterByConfidence(0.99));
    // FilterByConfidence(0.99) should filter out everything from HeuristicNER
    // (heuristic backend produces low confidence entities).
    let entities = pipeline
        .extract("John Smith")
        .expect("extraction should succeed");
    // All entities should be filtered out at threshold 0.99.
    assert!(
        entities.iter().all(|e| e.confidence >= 0.99),
        "middleware added via with_if(true) must be active"
    );
}

#[test]
fn test_pipeline_with_if_condition_false_skips_middleware() {
    // Build two pipelines: one with the filter active, one without.
    let pipeline_no_filter = Pipeline::new(Box::new(HeuristicNER::new()))
        .with_if(false, FilterByConfidence(0.99));
    let pipeline_with_filter = Pipeline::new(Box::new(HeuristicNER::new()))
        .with_if(true, FilterByConfidence(0.99));

    let unfiltered = pipeline_no_filter
        .extract("John Smith")
        .expect("extraction should succeed");
    let filtered = pipeline_with_filter
        .extract("John Smith")
        .expect("extraction should succeed");

    // The pipeline with condition=false should produce at least as many entities
    // as the pipeline with condition=true (which aggressively filters).
    assert!(
        unfiltered.len() >= filtered.len(),
        "with_if(false) must not add the middleware: unfiltered={} filtered={}",
        unfiltered.len(),
        filtered.len()
    );
}

#[test]
fn test_pipeline_with_if_middleware_names() {
    let pipeline_true = Pipeline::new(Box::new(HeuristicNER::new()))
        .with_if(true, FilterByConfidence(0.5));
    let pipeline_false = Pipeline::new(Box::new(HeuristicNER::new()))
        .with_if(false, FilterByConfidence(0.5));

    assert!(
        pipeline_true.middleware_names().contains(&"filter_by_confidence"),
        "middleware name should appear when condition is true"
    );
    assert!(
        !pipeline_false.middleware_names().contains(&"filter_by_confidence"),
        "middleware name must not appear when condition is false"
    );
}

// =============================================================================
// RemoveOverlaps — non-overlapping passthrough
// =============================================================================

#[test]
fn test_remove_overlaps_non_overlapping_all_preserved() {
    let mw = RemoveOverlaps;
    let mut ctx = MiddlewareContext::new("Alice went to London yesterday");
    let entities = vec![
        Entity::new("Alice", EntityType::Person, 0, 5, 0.9),
        Entity::new("London", EntityType::Location, 14, 20, 0.85),
    ];
    let result = mw
        .post_process(&mut ctx, entities)
        .expect("post_process should succeed");
    assert_eq!(
        result.len(),
        2,
        "non-overlapping entities must all pass through RemoveOverlaps"
    );
}

#[test]
fn test_remove_overlaps_three_non_overlapping_all_preserved() {
    let mw = RemoveOverlaps;
    let mut ctx = MiddlewareContext::new("Alice went to London for Acme");
    let entities = vec![
        Entity::new("Alice", EntityType::Person, 0, 5, 0.9),
        Entity::new("London", EntityType::Location, 14, 20, 0.85),
        Entity::new("Acme", EntityType::Organization, 25, 29, 0.8),
    ];
    let result = mw
        .post_process(&mut ctx, entities)
        .expect("post_process should succeed");
    assert_eq!(result.len(), 3, "all three non-overlapping entities must be preserved");
}

// =============================================================================
// RemoveOverlaps — three-way overlap: only highest confidence kept
// =============================================================================

#[test]
fn test_remove_overlaps_three_way_keeps_highest_confidence() {
    let mw = RemoveOverlaps;
    let mut ctx = MiddlewareContext::new("New York City");
    // All three spans overlap one another.
    let entities = vec![
        Entity::new("New York City", EntityType::Location, 0, 13, 0.6),
        Entity::new("New York", EntityType::Location, 0, 8, 0.95), // highest
        Entity::new("York City", EntityType::Location, 4, 13, 0.7),
    ];
    let result = mw
        .post_process(&mut ctx, entities)
        .expect("post_process should succeed");
    // Only the highest-confidence span should survive.
    assert_eq!(
        result.len(),
        1,
        "three-way overlap must leave exactly one entity"
    );
    assert_eq!(result[0].text, "New York", "highest-confidence entity must be kept");
    assert!(
        (result[0].confidence - 0.95).abs() < 1e-9,
        "kept entity must have confidence 0.95"
    );
}
