//! Property-based tests for anno-core types.
//!
//! These tests verify fundamental invariants of the Signal → Track → Identity hierarchy
//! and the Location type's geometric operations.

use anno_core::grounded::{
    Identity, IdentitySource, Location, Modality, Signal, SignalRef, Track, TrackRef,
};
use anno_core::{IdentityId, SignalId, TrackId};
use proptest::prelude::*;

// =============================================================================
// Arbitrary Generators
// =============================================================================

/// Generate a text location with valid offsets
fn arb_text_location() -> impl Strategy<Value = Location> {
    (0usize..10000, 1usize..1000).prop_map(|(start, len)| Location::text(start, start + len))
}

/// Generate a bounding box location with normalized coordinates
fn arb_bbox_location() -> impl Strategy<Value = Location> {
    (0.0f32..0.9, 0.0f32..0.9, 0.01f32..0.5, 0.01f32..0.5).prop_map(|(x, y, w, h)| {
        Location::BoundingBox {
            x,
            y,
            width: w,
            height: h,
            page: None,
        }
    })
}

/// Generate any location type
fn arb_location() -> impl Strategy<Value = Location> {
    prop_oneof![arb_text_location(), arb_bbox_location(),]
}

/// Generate a signal with text location
fn arb_signal() -> impl Strategy<Value = Signal<Location>> {
    (
        any::<u64>(),
        arb_text_location(),
        "[A-Za-z ]{1,50}",    // surface
        "(PER|ORG|LOC|MISC)", // label
        0.0f32..1.0,          // confidence
    )
        .prop_map(|(id, location, surface, label, confidence)| Signal {
            id: id.into(),
            location,
            surface,
            label,
            confidence,
            hierarchical: None,
            provenance: None,
            modality: Modality::Symbolic,
            normalized: None,
            negated: false,
            quantifier: None,
        })
}

/// Generate a track with valid structure
fn arb_track() -> impl Strategy<Value = Track> {
    (
        any::<u64>(),
        "[A-Za-z ]{1,30}",                      // canonical surface
        prop::option::of("(PER|ORG|LOC|MISC)"), // entity type
        0.0f32..1.0,                            // cluster confidence
        prop::collection::vec((any::<u64>(), 0u32..1000), 1..10), // signals
    )
        .prop_map(
            |(id, canonical_surface, entity_type, confidence, signal_specs)| {
                let mut track = Track::new(id, canonical_surface);
                track.entity_type = entity_type.map(|s| s.to_string());
                track.cluster_confidence = confidence;
                for (sig_id, pos) in signal_specs {
                    track.add_signal(sig_id, pos);
                }
                track
            },
        )
}

/// Generate an identity
fn arb_identity() -> impl Strategy<Value = Identity> {
    (
        any::<u64>(),
        "[A-Za-z ]{1,50}",                              // canonical name
        prop::option::of("(PER|ORG|LOC|MISC)"),         // entity type
        prop::option::of("Q[0-9]{1,10}"),               // kb_id
        0.0f32..1.0,                                    // confidence
        prop::collection::vec("[A-Za-z ]{1,20}", 0..5), // aliases
    )
        .prop_map(
            |(id, canonical_name, entity_type, kb_id, confidence, aliases)| {
                let mut identity = Identity::new(id, canonical_name);
                identity.entity_type = entity_type.map(|s| s.to_string());
                identity.kb_id = kb_id;
                identity.confidence = confidence;
                for alias in aliases {
                    identity.add_alias(alias);
                }
                identity
            },
        )
}

// =============================================================================
// Location Properties
// =============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    // --- Modality ---

    /// Text locations have Symbolic modality
    #[test]
    fn text_location_is_symbolic(loc in arb_text_location()) {
        prop_assert_eq!(loc.modality(), Modality::Symbolic);
    }

    /// BBox locations have Iconic modality
    #[test]
    fn bbox_location_is_iconic(loc in arb_bbox_location()) {
        prop_assert_eq!(loc.modality(), Modality::Iconic);
    }

    // --- Text Offsets ---

    /// text_offsets() returns Some for text locations
    #[test]
    fn text_location_has_offsets(start in 0usize..10000, len in 1usize..1000) {
        let loc = Location::text(start, start + len);
        let offsets = loc.text_offsets();
        prop_assert!(offsets.is_some());
        let (s, e) = offsets.unwrap();
        prop_assert_eq!(s, start);
        prop_assert_eq!(e, start + len);
    }

    /// text_offsets() returns None for bbox locations
    #[test]
    fn bbox_has_no_text_offsets(loc in arb_bbox_location()) {
        prop_assert!(loc.text_offsets().is_none());
    }

    // --- Overlap ---

    /// A location always overlaps with itself
    #[test]
    fn location_overlaps_self(loc in arb_location()) {
        prop_assert!(loc.overlaps(&loc));
    }

    /// Overlap is symmetric
    #[test]
    fn overlap_symmetric(
        start1 in 0usize..5000,
        len1 in 1usize..500,
        start2 in 0usize..5000,
        len2 in 1usize..500
    ) {
        let loc1 = Location::text(start1, start1 + len1);
        let loc2 = Location::text(start2, start2 + len2);
        prop_assert_eq!(loc1.overlaps(&loc2), loc2.overlaps(&loc1));
    }

    /// Non-overlapping text spans don't overlap
    #[test]
    fn disjoint_text_no_overlap(
        start1 in 0usize..1000,
        len1 in 1usize..100
    ) {
        let loc1 = Location::text(start1, start1 + len1);
        let loc2 = Location::text(start1 + len1 + 100, start1 + len1 + 200);
        prop_assert!(!loc1.overlaps(&loc2));
    }

    /// Different modality locations don't overlap
    #[test]
    fn different_modality_no_overlap(
        text_loc in arb_text_location(),
        bbox_loc in arb_bbox_location()
    ) {
        prop_assert!(!text_loc.overlaps(&bbox_loc));
        prop_assert!(!bbox_loc.overlaps(&text_loc));
    }

    // --- IoU (Intersection over Union) ---

    /// IoU of identical locations is 1.0
    #[test]
    fn iou_identical_is_one(loc in arb_location()) {
        if let Some(iou) = loc.iou(&loc) {
            prop_assert!((iou - 1.0).abs() < 0.0001, "IoU of identical should be 1.0, got {}", iou);
        }
    }

    /// IoU is symmetric
    #[test]
    fn iou_symmetric(loc1 in arb_text_location(), loc2 in arb_text_location()) {
        let iou1 = loc1.iou(&loc2);
        let iou2 = loc2.iou(&loc1);
        prop_assert_eq!(iou1, iou2, "IoU should be symmetric");
    }

    /// IoU is bounded [0, 1]
    #[test]
    fn iou_bounded(loc1 in arb_text_location(), loc2 in arb_text_location()) {
        if let Some(iou) = loc1.iou(&loc2) {
            prop_assert!(iou >= 0.0 && iou <= 1.0, "IoU should be in [0,1], got {}", iou);
        }
    }

    /// Disjoint locations have IoU = 0
    #[test]
    fn disjoint_iou_zero(start in 0usize..1000, len in 1usize..100) {
        let loc1 = Location::text(start, start + len);
        let loc2 = Location::text(start + len + 100, start + len + 200);
        if let Some(iou) = loc1.iou(&loc2) {
            prop_assert!((iou - 0.0).abs() < 0.0001, "Disjoint IoU should be 0, got {}", iou);
        }
    }

    /// Contained location has IoU = inner_area / outer_area
    #[test]
    fn contained_iou(
        start in 100usize..1000,
        outer_len in 100usize..500,
        inner_offset in 10usize..50,
        inner_len in 10usize..50
    ) {
        // Ensure inner is actually inside outer
        prop_assume!(inner_offset + inner_len < outer_len);

        let outer = Location::text(start, start + outer_len);
        let inner = Location::text(start + inner_offset, start + inner_offset + inner_len);

        if let Some(iou) = outer.iou(&inner) {
            // IoU = intersection / union = inner_len / outer_len
            let expected = inner_len as f64 / outer_len as f64;
            prop_assert!(
                (iou - expected).abs() < 0.01,
                "Contained IoU should be {}, got {}",
                expected,
                iou
            );
        }
    }
}

// =============================================================================
// Signal Properties
// =============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    /// Signals have valid confidence bounds
    #[test]
    fn signal_confidence_bounded(signal in arb_signal()) {
        prop_assert!(signal.confidence >= 0.0 && signal.confidence <= 1.0);
    }

    /// Signals have non-empty surface
    #[test]
    fn signal_surface_non_empty(signal in arb_signal()) {
        prop_assert!(!signal.surface.is_empty());
    }

    /// Signals have non-empty label
    #[test]
    fn signal_label_non_empty(signal in arb_signal()) {
        prop_assert!(!signal.label.is_empty());
    }

    /// Signal location text offsets are valid (start < end)
    #[test]
    fn signal_text_offsets_valid(signal in arb_signal()) {
        if let Some((start, end)) = signal.location.text_offsets() {
            prop_assert!(start < end, "start ({}) should be < end ({})", start, end);
        }
    }
}

// =============================================================================
// Track Properties
// =============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    /// Tracks have valid cluster confidence bounds
    #[test]
    fn track_confidence_bounded(track in arb_track()) {
        prop_assert!(
            track.cluster_confidence >= 0.0 && track.cluster_confidence <= 1.0,
            "Cluster confidence should be in [0,1], got {}",
            track.cluster_confidence
        );
    }

    /// Tracks have non-empty canonical surface
    #[test]
    fn track_canonical_non_empty(track in arb_track()) {
        prop_assert!(!track.canonical_surface.is_empty());
    }

    /// Track len() matches signals count
    #[test]
    fn track_len_matches_signals(track in arb_track()) {
        prop_assert_eq!(track.len(), track.signals.len());
    }

    /// Track is_empty() is consistent with len()
    #[test]
    fn track_is_empty_consistent(track in arb_track()) {
        prop_assert_eq!(track.is_empty(), track.len() == 0);
    }

    /// Track is_singleton() is consistent with len()
    #[test]
    fn track_is_singleton_consistent(track in arb_track()) {
        prop_assert_eq!(track.is_singleton(), track.len() == 1);
    }

    /// New track has no identity by default
    #[test]
    fn new_track_no_identity(id in any::<u64>(), surface in "[A-Za-z]{1,20}") {
        let track = Track::new(id, surface);
        prop_assert!(track.identity_id.is_none());
    }

    /// with_identity sets identity_id
    #[test]
    fn with_identity_sets_id(id in any::<u64>(), surface in "[A-Za-z]{1,20}", identity_id in any::<u64>()) {
        let identity_id = IdentityId::new(identity_id);
        let track = Track::new(id, surface).with_identity(identity_id);
        prop_assert_eq!(track.identity_id, Some(identity_id));
    }

    /// with_type sets entity_type
    #[test]
    fn with_type_sets_type(id in any::<u64>(), surface in "[A-Za-z]{1,20}", etype in "(PER|ORG|LOC)") {
        let track = Track::new(id, surface).with_type(&etype);
        prop_assert_eq!(track.entity_type, Some(etype));
    }
}

// =============================================================================
// Identity Properties
// =============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    /// Identities have valid confidence bounds
    #[test]
    fn identity_confidence_bounded(identity in arb_identity()) {
        prop_assert!(
            identity.confidence >= 0.0 && identity.confidence <= 1.0,
            "Identity confidence should be in [0,1], got {}",
            identity.confidence
        );
    }

    /// Identities have non-empty canonical name
    #[test]
    fn identity_canonical_non_empty(identity in arb_identity()) {
        prop_assert!(!identity.canonical_name.is_empty());
    }

    /// New identity has empty aliases
    #[test]
    fn new_identity_empty_aliases(id in any::<u64>(), name in "[A-Za-z]{1,20}") {
        let identity = Identity::new(id, name);
        prop_assert!(identity.aliases.is_empty());
    }

    /// add_alias increases alias count
    #[test]
    fn add_alias_increases_count(id in any::<u64>(), name in "[A-Za-z]{1,20}", alias in "[A-Za-z]{1,10}") {
        let mut identity = Identity::new(id, name);
        let before = identity.aliases.len();
        identity.add_alias(alias);
        prop_assert_eq!(identity.aliases.len(), before + 1);
    }

    /// Identity serialization roundtrip
    #[test]
    fn identity_serde_roundtrip(identity in arb_identity()) {
        let json = serde_json::to_string(&identity).expect("Serialization failed");
        let deserialized: Identity = serde_json::from_str(&json).expect("Deserialization failed");

        prop_assert_eq!(identity.id, deserialized.id);
        prop_assert_eq!(identity.canonical_name, deserialized.canonical_name);
        prop_assert_eq!(identity.entity_type, deserialized.entity_type);
        prop_assert_eq!(identity.kb_id, deserialized.kb_id);
        prop_assert_eq!(identity.aliases, deserialized.aliases);
    }
}

// =============================================================================
// Hierarchy Invariants
// =============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    /// Signal → Track: signals can be added to track
    #[test]
    fn signals_can_join_track(
        track_id in any::<u64>(),
        surface in "[A-Za-z]{1,20}",
        signal_ids in prop::collection::vec(any::<u64>(), 1..10)
    ) {
        let mut track = Track::new(track_id, surface);
        let signal_ids: Vec<SignalId> = signal_ids.into_iter().map(SignalId::new).collect();
        for (pos, sig_id) in signal_ids.iter().enumerate() {
            track.add_signal(*sig_id, pos as u32);
        }

        prop_assert_eq!(track.len(), signal_ids.len());
        for (pos, sig_id) in signal_ids.iter().enumerate() {
            prop_assert_eq!(track.signals[pos].signal_id, *sig_id);
        }
    }

    /// Track → Identity: track can link to identity
    #[test]
    fn track_can_link_identity(
        track_id in any::<u64>(),
        surface in "[A-Za-z]{1,20}",
        identity_id in any::<u64>()
    ) {
        let identity_id = IdentityId::new(identity_id);
        let track = Track::new(track_id, surface).with_identity(identity_id);
        prop_assert_eq!(track.identity_id, Some(identity_id));
    }

    /// Identity can track multiple cross-doc tracks
    #[test]
    fn identity_cross_doc_tracks(
        identity_id in any::<u64>(),
        name in "[A-Za-z]{1,20}",
        track_refs in prop::collection::vec(
            ("[a-z]{3,8}", any::<u64>()),
            1..5
        )
    ) {
        let refs: Vec<TrackRef> = track_refs
            .into_iter()
            .map(|(doc_id, track_id)| TrackRef { doc_id, track_id: track_id.into() })
            .collect();

        let mut identity = Identity::new(identity_id, name);
        identity.source = Some(IdentitySource::CrossDocCoref {
            track_refs: refs.clone(),
        });

        match &identity.source {
            Some(IdentitySource::CrossDocCoref { track_refs }) => {
                prop_assert_eq!(track_refs.len(), refs.len());
            }
            _ => prop_assert!(false, "Expected CrossDocCoref source"),
        }
    }
}

// =============================================================================
// Edge Cases
// =============================================================================

#[test]
fn test_zero_length_text_span() {
    let loc = Location::text(100, 100);
    // Zero-length spans are technically valid but unusual
    assert_eq!(loc.text_offsets(), Some((100, 100)));
}

#[test]
fn test_location_default() {
    let loc = Location::default();
    assert_eq!(loc, Location::Text { start: 0, end: 0 });
}

#[test]
fn test_track_empty_after_creation() {
    let track = Track::new(1, "Test");
    assert!(track.is_empty());
    assert!(!track.is_singleton());
}

#[test]
fn test_signal_ref_equality() {
    let ref1 = SignalRef {
        signal_id: 42.into(),
        position: 5,
    };
    let ref2 = SignalRef {
        signal_id: 42.into(),
        position: 5,
    };
    assert_eq!(ref1, ref2);
}

#[test]
fn test_track_ref_equality() {
    let ref1 = TrackRef {
        doc_id: "doc1".to_string(),
        track_id: 42.into(),
    };
    let ref2 = TrackRef {
        doc_id: "doc1".to_string(),
        track_id: 42.into(),
    };
    assert_eq!(ref1, ref2);
}

#[test]
fn test_identity_source_variants_all() {
    let kb = IdentitySource::KnowledgeBase {
        kb_name: "wikidata".to_string(),
        kb_id: "Q42".to_string(),
    };
    assert!(matches!(kb, IdentitySource::KnowledgeBase { .. }));

    let coref = IdentitySource::CrossDocCoref {
        track_refs: vec![TrackRef {
            doc_id: "doc1".to_string(),
            track_id: 1.into(),
        }],
    };
    assert!(matches!(coref, IdentitySource::CrossDocCoref { .. }));

    let hybrid = IdentitySource::Hybrid {
        track_refs: vec![],
        kb_name: "wikidata".to_string(),
        kb_id: "Q42".to_string(),
    };
    assert!(matches!(hybrid, IdentitySource::Hybrid { .. }));
}

#[test]
fn test_bbox_iou_partial_overlap() {
    let box1 = Location::BoundingBox {
        x: 0.0,
        y: 0.0,
        width: 0.5,
        height: 0.5,
        page: None,
    };
    let box2 = Location::BoundingBox {
        x: 0.25,
        y: 0.25,
        width: 0.5,
        height: 0.5,
        page: None,
    };

    let iou = box1.iou(&box2).expect("Should compute IoU for bboxes");

    // Intersection: 0.25 x 0.25 = 0.0625
    // Union: 0.25 + 0.25 - 0.0625 = 0.4375
    // IoU ≈ 0.0625 / 0.4375 ≈ 0.143
    assert!(
        (iou - 0.143).abs() < 0.01,
        "IoU should be ~0.143, got {}",
        iou
    );
}

#[test]
fn test_different_page_bboxes_no_overlap() {
    let box1 = Location::BoundingBox {
        x: 0.0,
        y: 0.0,
        width: 0.5,
        height: 0.5,
        page: Some(1),
    };
    let box2 = Location::BoundingBox {
        x: 0.0,
        y: 0.0,
        width: 0.5,
        height: 0.5,
        page: Some(2),
    };

    assert!(!box1.overlaps(&box2));
    assert_eq!(box1.iou(&box2), Some(0.0));
}
