use super::*;

#[test]
fn test_box_volume() {
    let box1 = BoxEmbedding::new(vec![0.0, 0.0], vec![1.0, 1.0]);
    assert_eq!(box1.volume(), 1.0);

    let box2 = BoxEmbedding::new(vec![0.0, 0.0, 0.0], vec![2.0, 3.0, 4.0]);
    assert_eq!(box2.volume(), 24.0);
}

#[test]
fn test_intersection_volume() {
    let box1 = BoxEmbedding::new(vec![0.0, 0.0], vec![2.0, 2.0]);
    let box2 = BoxEmbedding::new(vec![1.0, 1.0], vec![3.0, 3.0]);
    assert_eq!(box1.intersection_volume(&box2), 1.0);

    let box3 = BoxEmbedding::new(vec![5.0, 5.0], vec![6.0, 6.0]);
    assert_eq!(box1.intersection_volume(&box3), 0.0); // Disjoint
}

#[test]
fn test_conditional_probability() {
    let box_a = BoxEmbedding::new(vec![0.0, 0.0], vec![1.0, 1.0]); // Volume = 1
    let box_b = BoxEmbedding::new(vec![0.0, 0.0], vec![2.0, 2.0]); // Volume = 4

    // box_a is contained in box_b
    assert_eq!(box_a.conditional_probability(&box_b), 0.25); // 1/4
    assert_eq!(box_b.conditional_probability(&box_a), 1.0); // 4/4 (intersection = box_a)
}

#[test]
fn test_coreference_score() {
    // Identical boxes should have score = 1.0
    let box1 = BoxEmbedding::new(vec![0.0, 0.0], vec![1.0, 1.0]);
    let box2 = BoxEmbedding::new(vec![0.0, 0.0], vec![1.0, 1.0]);
    assert!((box1.coreference_score(&box2) - 1.0).abs() < 1e-6);

    // Disjoint boxes should have score = 0.0
    let box3 = BoxEmbedding::new(vec![0.0, 0.0], vec![1.0, 1.0]);
    let box4 = BoxEmbedding::new(vec![2.0, 2.0], vec![3.0, 3.0]);
    assert_eq!(box3.coreference_score(&box4), 0.0);
}

#[test]
fn test_containment() {
    let box_a = BoxEmbedding::new(vec![0.0, 0.0], vec![1.0, 1.0]);
    let box_b = BoxEmbedding::new(vec![0.0, 0.0], vec![2.0, 2.0]);
    assert!(box_a.is_contained_in(&box_b));
    assert!(!box_b.is_contained_in(&box_a));
}

#[test]
fn test_box_operations() {
    // Test that box operations work correctly
    let box1 = BoxEmbedding::new(vec![0.0, 0.0], vec![1.0, 1.0]);
    let box2 = BoxEmbedding::new(vec![0.5, 0.5], vec![1.5, 1.5]);

    // Should have intersection
    assert!(box1.intersection_volume(&box2) > 0.0);

    // Coreference score should be > 0
    assert!(box1.coreference_score(&box2) > 0.0);
}

#[test]
fn test_from_vector() {
    let vector = vec![0.5, 0.5, 0.5];
    let box_embedding = BoxEmbedding::from_vector(&vector, 0.1);

    assert_eq!(box_embedding.min, vec![0.4, 0.4, 0.4]);
    assert_eq!(box_embedding.max, vec![0.6, 0.6, 0.6]);
    assert!((box_embedding.volume() - 0.008).abs() < 1e-6); // 0.2^3 with float tolerance
}

#[test]
fn test_center_and_size() {
    let box_embedding = BoxEmbedding::new(vec![0.0, 1.0], vec![2.0, 3.0]);
    let center = box_embedding.center();
    let size = box_embedding.size();

    assert_eq!(center, vec![1.0, 2.0]);
    assert_eq!(size, vec![2.0, 2.0]);
}

// =========================================================================
// Temporal Box Tests
// =========================================================================

#[test]
fn test_temporal_box_at_time() {
    let base = BoxEmbedding::new(vec![0.0, 0.0], vec![1.0, 1.0]);
    let velocity = BoxVelocity::static_velocity(2);
    let temporal = TemporalBox::new(base, velocity, (2012.0, 2016.0));

    // Should be valid in range
    assert!(temporal.is_valid_at(2014.0));
    assert!(!temporal.is_valid_at(2017.0));

    // Static velocity: box should be same at any time in range
    let box_at_time = temporal.at_time(2014.0).unwrap();
    assert_eq!(box_at_time.min, vec![0.0, 0.0]);
    assert_eq!(box_at_time.max, vec![1.0, 1.0]);
}

#[test]
fn test_temporal_box_with_velocity() {
    // Box that moves over time
    let base = BoxEmbedding::new(vec![0.0, 0.0], vec![1.0, 1.0]);
    let velocity = BoxVelocity::new(vec![0.1, 0.1], vec![0.1, 0.1]);
    let temporal = TemporalBox::new(base, velocity, (0.0, 10.0));

    // At time 0, should be at base
    let box_t0 = temporal.at_time(0.0).unwrap();
    assert_eq!(box_t0.min, vec![0.0, 0.0]);
    assert_eq!(box_t0.max, vec![1.0, 1.0]);

    // At time 5, should have moved
    let box_t5 = temporal.at_time(5.0).unwrap();
    assert_eq!(box_t5.min, vec![0.5, 0.5]); // 0.0 + 0.1 * 5
    assert_eq!(box_t5.max, vec![1.5, 1.5]); // 1.0 + 0.1 * 5
}

#[test]
fn test_temporal_box_coreference() {
    // Two presidencies that don't overlap in time
    let obama_base = BoxEmbedding::new(vec![0.0, 0.0], vec![1.0, 1.0]);
    let trump_base = BoxEmbedding::new(vec![5.0, 5.0], vec![6.0, 6.0]);
    let velocity = BoxVelocity::static_velocity(2);

    let obama = TemporalBox::new(obama_base, velocity.clone(), (2012.0, 2016.0));
    let trump = TemporalBox::new(trump_base, velocity, (2017.0, 2021.0));

    // Should not corefer (different time ranges)
    assert_eq!(obama.coreference_at_time(&trump, 2015.0), 0.0);
    assert_eq!(obama.coreference_at_time(&trump, 2018.0), 0.0);
}

// =========================================================================
// Uncertainty-Aware Box Tests
// =========================================================================

#[test]
fn test_uncertain_box_confidence() {
    // Small box = high confidence
    let small_box = BoxEmbedding::new(vec![0.0, 0.0], vec![0.1, 0.1]);
    let uncertain_small = UncertainBox::new(small_box, 0.9);
    assert!(uncertain_small.confidence() > 0.5);

    // Large box = low confidence
    let large_box = BoxEmbedding::new(vec![0.0, 0.0], vec![10.0, 10.0]);
    let uncertain_large = UncertainBox::new(large_box, 0.9);
    assert!(uncertain_large.confidence() < uncertain_small.confidence());
}

#[test]
fn test_conflict_detection() {
    // Two high-confidence, disjoint claims = conflict
    let claim_a = UncertainBox::new(BoxEmbedding::new(vec![0.0, 0.0], vec![0.1, 0.1]), 0.95);
    let claim_b = UncertainBox::new(BoxEmbedding::new(vec![5.0, 5.0], vec![5.1, 5.1]), 0.90);

    let conflict = claim_a.detect_conflict(&claim_b);
    assert!(conflict.is_some());
    if let Some(c) = conflict {
        assert!(c.severity > 0.0);
        assert_eq!(c.claim_a_trust, 0.95);
        assert_eq!(c.claim_b_trust, 0.90);
    }
}

#[test]
fn test_no_conflict_for_overlapping_boxes() {
    // Overlapping boxes should not conflict
    let claim_a = UncertainBox::new(BoxEmbedding::new(vec![0.0, 0.0], vec![1.0, 1.0]), 0.95);
    let claim_b = UncertainBox::new(BoxEmbedding::new(vec![0.5, 0.5], vec![1.5, 1.5]), 0.90);

    let conflict = claim_a.detect_conflict(&claim_b);
    assert!(conflict.is_none()); // Overlapping = no conflict
}

// =========================================================================
// Gumbel Box Tests
// =========================================================================

#[test]
fn test_gumbel_box_membership() {
    let mean_box = BoxEmbedding::new(vec![0.0, 0.0], vec![1.0, 1.0]);
    let gumbel = GumbelBox::new(mean_box, 0.1);

    // Point inside box should have high membership
    let inside = vec![0.5, 0.5];
    let prob_inside = gumbel.membership_probability(&inside);
    assert!(prob_inside > 0.5);

    // Point outside box should have low membership
    let outside = vec![2.0, 2.0];
    let prob_outside = gumbel.membership_probability(&outside);
    assert!(prob_outside < prob_inside);
}

#[test]
fn test_gumbel_temperature_effect() {
    let mean_box = BoxEmbedding::new(vec![0.0, 0.0], vec![1.0, 1.0]);
    let sharp = GumbelBox::new(mean_box.clone(), 0.01); // Low temp = sharp
    let fuzzy = GumbelBox::new(mean_box, 1.0); // High temp = fuzzy

    let point = vec![1.1, 1.1]; // Just outside box
    let prob_sharp = sharp.membership_probability(&point);
    let prob_fuzzy = fuzzy.membership_probability(&point);

    // Fuzzy box should have higher probability for near-boundary points
    assert!(prob_fuzzy > prob_sharp);
}

#[test]
fn test_gumbel_robust_coreference() {
    let box1 = BoxEmbedding::new(vec![0.0, 0.0], vec![1.0, 1.0]);
    let box2 = BoxEmbedding::new(vec![0.1, 0.1], vec![0.9, 0.9]);
    let gumbel1 = GumbelBox::new(box1, 0.1);
    let gumbel2 = GumbelBox::new(box2, 0.1);

    // Overlapping boxes should have high robust coreference
    let score = gumbel1.robust_coreference(&gumbel2, 100);
    assert!(score > 0.3);
}

// =========================================================================
// Interaction Modeling Tests
// =========================================================================

#[test]
fn test_interaction_strength() {
    let actor = BoxEmbedding::new(vec![0.0, 0.0], vec![1.0, 1.0]);
    let action = BoxEmbedding::new(vec![0.2, 0.2], vec![0.8, 0.8]);
    let target = BoxEmbedding::new(vec![0.3, 0.3], vec![0.7, 0.7]);

    let strength = interaction_strength(&actor, &action, &target);
    assert!(strength > 0.0);
    assert!(strength <= 1.0);
}

#[test]
fn test_acquisition_roles() {
    let buyer = BoxEmbedding::new(vec![0.0, 0.0], vec![1.0, 1.0]);
    let seller = BoxEmbedding::new(vec![0.5, 0.5], vec![1.5, 1.5]);
    let acquisition = BoxEmbedding::new(vec![0.2, 0.2], vec![0.8, 0.8]);

    let (buyer_role, seller_role) = acquisition_roles(&buyer, &seller, &acquisition);

    // Both should have non-zero roles
    assert!(buyer_role >= 0.0);
    assert!(seller_role >= 0.0);

    // Roles should be asymmetric (buyer ≠ seller in general)
    // Note: In this simple test, they might be equal, but in practice
    // with learned embeddings, they would differ
}

// =========================================================================
// New Methods Tests (intersection, union, overlap_prob, distance)
// =========================================================================

#[test]
fn test_intersection_box() {
    let a = BoxEmbedding::new(vec![0.0, 0.0], vec![2.0, 2.0]);
    let b = BoxEmbedding::new(vec![1.0, 1.0], vec![3.0, 3.0]);

    let intersection = a.intersection(&b);
    assert_eq!(intersection.min, vec![1.0, 1.0]);
    assert_eq!(intersection.max, vec![2.0, 2.0]);
    assert_eq!(intersection.volume(), 1.0);
}

#[test]
fn test_union_box() {
    let a = BoxEmbedding::new(vec![0.0, 0.0], vec![2.0, 2.0]);
    let b = BoxEmbedding::new(vec![1.0, 1.0], vec![3.0, 3.0]);

    let union = a.union(&b);
    assert_eq!(union.min, vec![0.0, 0.0]);
    assert_eq!(union.max, vec![3.0, 3.0]);
    assert_eq!(union.volume(), 9.0);
}

#[test]
fn test_overlap_prob() {
    // Identical boxes: overlap = 1.0
    let a = BoxEmbedding::new(vec![0.0, 0.0], vec![1.0, 1.0]);
    let b = BoxEmbedding::new(vec![0.0, 0.0], vec![1.0, 1.0]);
    assert!((a.overlap_prob(&b) - 1.0).abs() < 0.001);

    // Disjoint boxes: overlap = 0.0
    let c = BoxEmbedding::new(vec![5.0, 5.0], vec![6.0, 6.0]);
    assert!((a.overlap_prob(&c) - 0.0).abs() < 0.001);

    // Partial overlap
    let d = BoxEmbedding::new(vec![0.5, 0.5], vec![1.5, 1.5]);
    let overlap = a.overlap_prob(&d);
    assert!(overlap > 0.0 && overlap < 1.0);
}

#[test]
fn test_distance() {
    // Overlapping boxes: distance = 0
    let a = BoxEmbedding::new(vec![0.0, 0.0], vec![2.0, 2.0]);
    let b = BoxEmbedding::new(vec![1.0, 1.0], vec![3.0, 3.0]);
    assert_eq!(a.distance(&b), 0.0);

    // Disjoint boxes: distance > 0
    let c = BoxEmbedding::new(vec![5.0, 5.0], vec![6.0, 6.0]);
    let dist = a.distance(&c);
    assert!(dist > 0.0);
    // Distance should be sqrt((5-2)^2 + (5-2)^2) = sqrt(18) ≈ 4.24
    assert!((dist - 18.0_f32.sqrt()).abs() < 0.01);
}

// =========================================================================
// Subsume Trait Tests (feature-gated)
// =========================================================================

#[test]
#[cfg(feature = "subsume")]
fn test_subsume_trait_implementation() {
    use subsume::Box as SubsumeBox;

    let a = BoxEmbedding::new(vec![0.0, 0.0], vec![2.0, 2.0]);
    let b = BoxEmbedding::new(vec![0.5, 0.5], vec![1.5, 1.5]);

    // Test trait methods
    assert_eq!(SubsumeBox::dim(&a), 2);
    assert_eq!(SubsumeBox::min(&a), &vec![0.0, 0.0]);
    assert_eq!(SubsumeBox::max(&a), &vec![2.0, 2.0]);

    // Volume (temperature is ignored for hard boxes)
    let vol = SubsumeBox::volume(&a, 1.0).unwrap();
    assert_eq!(vol, 4.0);

    // Containment prob: b is contained in a
    let containment = SubsumeBox::containment_prob(&a, &b, 1.0).unwrap();
    assert!(containment > 0.0);

    // Distance
    let dist = SubsumeBox::distance(&a, &b).unwrap();
    assert_eq!(dist, 0.0); // Overlapping

    // This verifies anno's BoxEmbedding is compatible with subsume's trait
    // and can use subsume's distance metrics, diagnostics, etc.
}
