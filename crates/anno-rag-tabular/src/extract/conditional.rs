//! Conditional column DAG — topo-sort columns by their
//! [`crate::schema::ConditionalSpec::parent_col`] edge so the fan-out
//! runs them in dependency waves. Children only fire when their
//! parent's predicate passes.
//!
//! This is the scheduling half of "conditional column gating" — the
//! [`Predicate`](crate::schema::Predicate) evaluation itself lives on
//! the predicate type. The fan-out (`fanout::extract_and_upsert_one_row`)
//! consumes the waves: extract everything in wave 0 → read parents back
//! → decide which wave-1 children are live → extract those → etc.
//!
//! ## Why waves instead of one big DAG walk?
//!
//! Each wave is a single batched LLM round-trip (via
//! [`crate::extract::Extractor::extract_doc`]). That's the natural
//! unit: there's no benefit to streaming children mid-batch, and
//! waves keep the call-site logic linear.

use crate::error::{Error, Result};
use crate::ids::ColumnId;
use crate::schema::Column;
use std::collections::{HashMap, HashSet};

/// Topo-sort `columns` by their conditional parent.
///
/// Returns a `Vec<Vec<Column>>` where each inner Vec is a *wave* —
/// every column in wave `n` has its conditional parent (if any) in
/// some earlier wave `< n`, or no parent at all.
///
/// Within a wave, columns are sorted by `(order, name)` for
/// determinism (golden-test friendly).
///
/// ## Orphan parent handling
///
/// If a column's `conditional.parent_col` references a `ColumnId` that
/// is not in `columns`, the column is treated as if it had **no
/// parent** and placed in wave 0. The fanout caller is expected to
/// log a `tracing::warn` when this happens — typically a template was
/// edited to drop the parent column but forgot to clear the gate.
///
/// # Errors
///
/// Returns [`Error::ConditionalCycle`] if columns form a parent
/// cycle (e.g. `A.parent = B`, `B.parent = A`). The error's `path`
/// field carries the cycle as `name1 -> name2 -> ... -> name1`.
pub fn topo_waves(columns: &[Column]) -> Result<Vec<Vec<Column>>> {
    if columns.is_empty() {
        return Ok(Vec::new());
    }

    let id_set: HashSet<ColumnId> = columns.iter().map(|c| c.id).collect();

    // Effective parent: None if no conditional, or if the listed
    // parent isn't in this column set (orphan → degrade to no parent).
    let parent_of: HashMap<ColumnId, Option<ColumnId>> = columns
        .iter()
        .map(|c| {
            let eff = c
                .conditional
                .as_ref()
                .map(|s| s.parent_col)
                .filter(|pid| id_set.contains(pid));
            (c.id, eff)
        })
        .collect();

    let mut emitted: HashSet<ColumnId> = HashSet::new();
    let mut waves: Vec<Vec<Column>> = Vec::new();
    let mut remaining: Vec<&Column> = columns.iter().collect();

    while !remaining.is_empty() {
        let (ready, blocked): (Vec<&Column>, Vec<&Column>) =
            remaining.into_iter().partition(|c| match parent_of[&c.id] {
                None => true,
                Some(pid) => emitted.contains(&pid),
            });

        if ready.is_empty() {
            // Nothing emitted this pass → cycle among the survivors.
            let path = describe_cycle(&blocked, &parent_of);
            return Err(Error::ConditionalCycle { path });
        }

        let mut wave: Vec<Column> = ready.into_iter().cloned().collect();
        wave.sort_by(|a, b| a.order.cmp(&b.order).then_with(|| a.name.cmp(&b.name)));
        for c in &wave {
            emitted.insert(c.id);
        }
        waves.push(wave);
        remaining = blocked;
    }

    Ok(waves)
}

/// Decide whether `col` should be extracted given its parent cell's
/// current value (`None` if the parent cell was itself skipped or
/// hasn't been written).
///
/// Columns with no `conditional` always extract (`true`). Columns
/// with a conditional defer to [`crate::schema::Predicate::eval`].
#[must_use]
pub fn should_extract(col: &Column, parent_value: Option<&serde_json::Value>) -> bool {
    col.conditional
        .as_ref()
        .is_none_or(|c| c.predicate.eval(parent_value))
}

/// Walk the parent chain starting from the first blocked column until
/// we revisit a node — that loop is the cycle to report. Best-effort:
/// when names can't be resolved we fall back to stringified ids.
fn describe_cycle(blocked: &[&Column], parent_of: &HashMap<ColumnId, Option<ColumnId>>) -> String {
    let by_id: HashMap<ColumnId, &Column> = blocked.iter().map(|c| (c.id, *c)).collect();
    let start = blocked[0].id;
    let mut seen: Vec<ColumnId> = vec![start];
    let mut cursor = start;
    while let Some(Some(next)) = parent_of.get(&cursor) {
        if let Some(pos) = seen.iter().position(|id| id == next) {
            // Cycle closed: emit just the loop portion.
            let loop_part = &seen[pos..];
            let mut names: Vec<String> = loop_part
                .iter()
                .map(|id| {
                    by_id
                        .get(id)
                        .map(|c| c.name.clone())
                        .unwrap_or_else(|| format!("{id:?}"))
                })
                .collect();
            // Close the loop visually.
            names.push(names[0].clone());
            return names.join(" -> ");
        }
        seen.push(*next);
        cursor = *next;
    }
    // Shouldn't happen — caller only invokes describe_cycle when a
    // cycle exists — but degrade to listing the survivors.
    blocked
        .iter()
        .map(|c| c.name.clone())
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::ReviewId;
    use crate::schema::column::ColumnBuilder;
    use crate::schema::{CellType, ConditionalSpec, Predicate};
    use serde_json::json;

    fn col(review: ReviewId, name: &str, order: u32) -> Column {
        ColumnBuilder::new(review, name, "q?", CellType::Text)
            .order(order)
            .build()
    }

    fn col_gated(
        review: ReviewId,
        name: &str,
        order: u32,
        parent: ColumnId,
        pred: Predicate,
    ) -> Column {
        ColumnBuilder::new(review, name, "q?", CellType::Text)
            .order(order)
            .conditional(ConditionalSpec {
                parent_col: parent,
                predicate: pred,
            })
            .build()
    }

    #[test]
    fn columns_with_no_gates_form_one_wave() {
        let r = ReviewId::new();
        let a = col(r, "a", 0);
        let b = col(r, "b", 1);
        let c = col(r, "c", 2);
        let waves = topo_waves(&[a, b, c]).expect("ok");
        assert_eq!(waves.len(), 1);
        assert_eq!(waves[0].len(), 3);
        let names: Vec<&str> = waves[0].iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["a", "b", "c"], "sorted by order");
    }

    #[test]
    fn child_gated_on_parent_forms_two_waves() {
        let r = ReviewId::new();
        let parent = col(r, "parent", 0);
        let child = col_gated(
            r,
            "child",
            1,
            parent.id,
            Predicate::Equals { value: json!("X") },
        );
        let waves = topo_waves(&[child.clone(), parent.clone()]).expect("ok");
        assert_eq!(waves.len(), 2, "parent and child must be in separate waves");
        assert_eq!(waves[0][0].name, "parent");
        assert_eq!(waves[1][0].name, "child");
    }

    #[test]
    fn cycle_detected_and_errors() {
        let r = ReviewId::new();
        // Build A and B referring to each other — needs the ids
        // ahead of time so we can wire the conditional both ways.
        let a_id = crate::ids::ColumnId::for_name(r, "a");
        let b_id = crate::ids::ColumnId::for_name(r, "b");
        let a = col_gated(r, "a", 0, b_id, Predicate::NonNull);
        let b = col_gated(r, "b", 1, a_id, Predicate::NonNull);
        let err = topo_waves(&[a, b]).expect_err("cycle must error");
        match err {
            Error::ConditionalCycle { path } => {
                assert!(path.contains("a") && path.contains("b"), "path: {path}");
                assert!(path.contains("->"), "should describe a path: {path}");
            }
            other => panic!("expected ConditionalCycle, got {other:?}"),
        }
    }

    #[test]
    fn should_extract_returns_true_when_no_conditional() {
        let r = ReviewId::new();
        let c = col(r, "a", 0);
        assert!(should_extract(&c, None));
        assert!(should_extract(&c, Some(&json!("anything"))));
    }

    #[test]
    fn should_extract_evaluates_predicate_when_present() {
        let r = ReviewId::new();
        let parent = col(r, "p", 0);
        let child = col_gated(
            r,
            "c",
            1,
            parent.id,
            Predicate::Equals { value: json!("FR") },
        );
        assert!(should_extract(&child, Some(&json!("FR"))));
        assert!(!should_extract(&child, Some(&json!("DE"))));
        assert!(!should_extract(&child, None));
    }

    #[test]
    fn orphan_parent_treated_as_no_parent() {
        let r = ReviewId::new();
        let fake_parent = crate::ids::ColumnId::for_name(r, "missing");
        let lonely = col_gated(r, "lonely", 0, fake_parent, Predicate::NonNull);
        let waves = topo_waves(&[lonely]).expect("orphan parent must not cycle");
        assert_eq!(waves.len(), 1);
        assert_eq!(waves[0][0].name, "lonely");
    }
}
