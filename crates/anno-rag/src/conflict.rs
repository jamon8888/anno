//! v0.2 conflict resolver — decides when a fresh save invalidates a prior
//! memory of the same kind.
//!
//! Rules:
//! - **`Fact`** and **`Context`** are append-only. A new fact about an
//!   entity adds to history; it does not retract prior facts. Context is
//!   transient and short-lived by design.
//! - **`Preference`** and **`Reference`** auto-invalidate the prior row
//!   when:
//!     1. The new and prior rows share at least one `entity_ref`.
//!     2. Their embeddings have cosine similarity ≥ the configured
//!        threshold (`conflict_cosine_threshold`, default 0.85).
//!     3. The prior row is still valid (`valid_to IS NULL`).
//!
//! The cosine + shared-entity guard avoids two failure modes:
//! - Pure cosine: would invalidate semantically-similar but topically-
//!   distinct memories ("préférence PDF pour Cabinet Dupont" vs
//!   "préférence PDF pour Cabinet Martin").
//! - Pure entity overlap: would invalidate unrelated preferences sharing
//!   only a common organisation reference.

use crate::memory::{Memory, MemoryKind};

/// Cosine similarity over two equal-dimensional `f32` vectors.
/// Returns 0.0 when either vector is the zero vector (vacuously dissimilar).
#[must_use]
pub fn cosine_sim(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if na == 0.0 || nb == 0.0 {
        0.0
    } else {
        dot / (na * nb)
    }
}

/// True iff slices `a` and `b` share at least one element under `PartialEq`.
#[must_use]
pub fn shares_any<T: PartialEq>(a: &[T], b: &[T]) -> bool {
    a.iter().any(|x| b.iter().any(|y| x == y))
}

/// True iff saving `new` should invalidate `prior` per the v0.2 rules.
#[must_use]
pub fn resolves_conflict(new: &Memory, prior: &Memory, threshold: f32) -> bool {
    if new.kind != prior.kind {
        return false;
    }
    if !matches!(new.kind, MemoryKind::Preference | MemoryKind::Reference) {
        return false;
    }
    if prior.valid_to.is_some() {
        return false;
    }
    if !shares_any(&new.entity_refs, &prior.entity_refs) {
        return false;
    }
    cosine_sim(&new.embedding, &prior.embedding) >= threshold
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{MemoryId, MemoryKind};
    use chrono::Utc;

    fn mk(
        kind: MemoryKind,
        entities: Vec<String>,
        vec: Vec<f32>,
        valid_to: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Memory {
        let now = Utc::now();
        Memory {
            id: MemoryId::new(),
            session_id: None,
            kind,
            text: String::new(),
            created_at: now,
            accessed_at: now,
            valid_from: now,
            valid_to,
            embedding: vec,
            token_refs: vec![],
            entity_refs: entities,
        }
    }

    #[test]
    fn cosine_zero_for_zero_vector() {
        assert_eq!(cosine_sim(&[0.0, 0.0], &[1.0, 1.0]), 0.0);
    }

    #[test]
    fn cosine_one_for_identical_vectors() {
        let v = vec![0.6f32, 0.8];
        assert!((cosine_sim(&v, &v) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn fact_never_conflicts() {
        let a = mk(
            MemoryKind::Fact,
            vec!["ent:ORG:cabinet dupont".into()],
            vec![1.0, 0.0],
            None,
        );
        let b = mk(
            MemoryKind::Fact,
            vec!["ent:ORG:cabinet dupont".into()],
            vec![1.0, 0.0],
            None,
        );
        assert!(!resolves_conflict(&b, &a, 0.85));
    }

    #[test]
    fn context_never_conflicts() {
        let a = mk(
            MemoryKind::Context,
            vec!["ent:ORG:cabinet dupont".into()],
            vec![1.0, 0.0],
            None,
        );
        let b = mk(
            MemoryKind::Context,
            vec!["ent:ORG:cabinet dupont".into()],
            vec![1.0, 0.0],
            None,
        );
        assert!(!resolves_conflict(&b, &a, 0.85));
    }

    #[test]
    fn preference_with_shared_entity_and_high_sim_conflicts() {
        let a = mk(
            MemoryKind::Preference,
            vec!["ent:ORG:cabinet dupont".into()],
            vec![1.0, 0.0],
            None,
        );
        let b = mk(
            MemoryKind::Preference,
            vec!["ent:ORG:cabinet dupont".into()],
            vec![0.95, 0.05],
            None,
        );
        assert!(resolves_conflict(&b, &a, 0.85));
    }

    #[test]
    fn preference_with_no_shared_entity_does_not_conflict() {
        let a = mk(
            MemoryKind::Preference,
            vec!["ent:ORG:a".into()],
            vec![1.0, 0.0],
            None,
        );
        let b = mk(
            MemoryKind::Preference,
            vec!["ent:ORG:b".into()],
            vec![1.0, 0.0],
            None,
        );
        assert!(!resolves_conflict(&b, &a, 0.85));
    }

    #[test]
    fn already_invalidated_prior_does_not_count() {
        let a = mk(
            MemoryKind::Preference,
            vec!["ent:ORG:a".into()],
            vec![1.0, 0.0],
            Some(Utc::now()),
        );
        let b = mk(
            MemoryKind::Preference,
            vec!["ent:ORG:a".into()],
            vec![1.0, 0.0],
            None,
        );
        assert!(!resolves_conflict(&b, &a, 0.85));
    }

    #[test]
    fn reference_kind_also_resolves() {
        let a = mk(
            MemoryKind::Reference,
            vec!["ent:ORG:a".into()],
            vec![1.0, 0.0],
            None,
        );
        let b = mk(
            MemoryKind::Reference,
            vec!["ent:ORG:a".into()],
            vec![0.95, 0.05],
            None,
        );
        assert!(resolves_conflict(&b, &a, 0.85));
    }

    #[test]
    fn threshold_gates_low_similarity() {
        let a = mk(
            MemoryKind::Preference,
            vec!["ent:ORG:a".into()],
            vec![1.0, 0.0],
            None,
        );
        let b = mk(
            MemoryKind::Preference,
            vec!["ent:ORG:a".into()],
            vec![0.5, 0.866],
            None,
        );
        // cosine ≈ 0.5 — below 0.85 threshold.
        assert!(!resolves_conflict(&b, &a, 0.85));
    }
}
