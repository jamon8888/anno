# NER PII Confidence (Spec B) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Raise PII detection recall (privacy) without sacrificing precision (usability), measured by a new PII eval harness, by narrowing the model schema and unifying the query/ingest detection paths, with a selectable RGPD-strict vs cabinet-confidential masking scope.

**Architecture:** Measurement-first. Phase 1 builds a PII eval harness + non-regression gate (no behavior change). Phase 2 unifies `detect()` and `detect_for_ingest()` on one description-based, per-label-threshold core. Phase 3 splits the 24-label schema into focused passes (`identity`, `art9`) and merges, validated by the harness. Phase 4 adds a `MaskingScope` enum (RGPD-strict default vs cabinet-confidential). Phase 5 guarantees vault-lookup coverage on the query path.

**Tech Stack:** Rust, `anno::backends::gliner2_fastino` (ONNX GLiNER2), `cloakpipe_core` entity types, `serde_json` fixtures.

**Spec:** `docs/superpowers/specs/2026-06-24-ner-pii-confidence-design.md`

**Key grounding (verified):**
- `Detector::detect_inner` (detect.rs:653) uses `pii_ner.extract_with_label_descriptions(text, &described, 0.25)` then retains per `gdpr_label_thresholds()`.
- `Detector::detect_for_ingest` (detect.rs:743) uses `ner.extract_with_label_thresholds(text, &label_thresholds)`.
- `extract_with_label_descriptions(text, &[(&str,&str)], f32) -> Result<Vec<anno::Entity>>` (gliner2_fastino/mod.rs:547).
- `anno_entities_to_detected(text, entities) -> Result<Vec<DetectedEntity>>` translates char→byte offsets.
- `GDPR_NER_LABELS: &[(&str, &str, f32)]` = (label, description, threshold) (detect.rs:236).
- `is_pii_entity`, `IngestDetectionBundle { pii, legal, raw_model_spans }` (detect.rs:917/930).

**B2 refinement (decided):** unify on the **description-based** variant + per-label threshold retain (not the bare-label `extract_with_label_thresholds`). Descriptions improve span precision per the backend's own doc; B1 validates the choice.

---

## File Structure

| File | Responsibility | Phase |
|------|----------------|-------|
| `crates/anno-rag/tests/fixtures/pii_eval/short/*.json` | Short-text ground-truth fixtures | 1 |
| `crates/anno-rag/tests/fixtures/pii_eval/long/*.json` | Long-text ground-truth fixtures | 1 |
| `crates/anno-rag/src/detect_eval.rs` | Fixture loader + precision/recall/F1 + gate | 1 |
| `crates/anno-rag/src/detect.rs` | Unified detection core, label groups, multi-pass merge, scope | 2-5 |
| `crates/anno-rag/src/config.rs` | `MaskingScope` enum + default | 4 |

---

## Phase 1 — PII eval harness (measurement foundation, no behavior change)

### Task 1: Fixture format + loader

**Files:**
- Create: `crates/anno-rag/tests/fixtures/pii_eval/short/person_01.json`
- Create: `crates/anno-rag/src/detect_eval.rs`
- Modify: `crates/anno-rag/src/lib.rs` (add `pub mod detect_eval;`)

- [ ] **Step 1: Create one fixture**

`crates/anno-rag/tests/fixtures/pii_eval/short/person_01.json`:

```json
{
  "text": "Jean-Pierre Moreau, avocat chez Cabinet Legrand.",
  "spans": [
    { "category": "person", "start": 0, "end": 18 }
  ]
}
```

- [ ] **Step 2: Write the failing test (loader)**

Create `crates/anno-rag/src/detect_eval.rs`:

```rust
//! Ground-truth PII detection eval harness. Loads labeled fixtures and
//! computes per-category precision/recall/F1 against `Detector` output.
//! Fixtures contain only synthetic (fictitious) PII.

use serde::Deserialize;
use std::path::{Path, PathBuf};

/// One labeled PII span (byte offsets into `text`).
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct GoldSpan {
    pub category: String,
    pub start: usize,
    pub end: usize,
}

/// One eval fixture: a text and its gold PII spans.
#[derive(Debug, Clone, Deserialize)]
pub struct PiiFixture {
    pub text: String,
    pub spans: Vec<GoldSpan>,
}

/// Root of the eval fixtures (env override, else versioned dir).
#[must_use]
pub fn pii_eval_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("ANNO_PII_EVAL_DIR") {
        return PathBuf::from(dir);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/pii_eval")
}

/// Load every `*.json` fixture under `dir` (recursively into short/ and long/).
pub fn load_fixtures(dir: &Path) -> std::io::Result<Vec<PiiFixture>> {
    let mut out = Vec::new();
    for sub in ["short", "long"] {
        let subdir = dir.join(sub);
        if !subdir.exists() {
            continue;
        }
        for entry in std::fs::read_dir(subdir)? {
            let path = entry?.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                let raw = std::fs::read_to_string(&path)?;
                let fixture: PiiFixture = serde_json::from_str(&raw)
                    .unwrap_or_else(|e| panic!("bad fixture {}: {e}", path.display()));
                out.push(fixture);
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_at_least_one_fixture() {
        let fx = load_fixtures(&pii_eval_dir()).expect("load fixtures");
        assert!(!fx.is_empty(), "expected at least one fixture");
        assert!(fx.iter().any(|f| !f.spans.is_empty()), "fixtures carry spans");
    }
}
```

Add to `crates/anno-rag/src/lib.rs`:

```rust
pub mod detect_eval;
```

- [ ] **Step 3: Run test to verify it passes**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag`
Expected: PASS (`loads_at_least_one_fixture`).

- [ ] **Step 4: Commit**

```bash
git add crates/anno-rag/src/detect_eval.rs crates/anno-rag/src/lib.rs crates/anno-rag/tests/fixtures/pii_eval/
git commit -m "feat(detect-eval): PII fixture format + loader"
```

---

### Task 2: Per-category precision/recall/F1

**Files:**
- Modify: `crates/anno-rag/src/detect_eval.rs`

- [ ] **Step 1: Write the failing test**

Add to `detect_eval.rs`:

```rust
    #[test]
    fn scores_overlap_match_by_category() {
        // gold: one person span 0..18
        let gold = vec![GoldSpan { category: "person".into(), start: 0, end: 18 }];
        // predicted: overlapping person span 0..11 (partial overlap, same category)
        let pred = vec![PredSpan { category: "person".into(), start: 0, end: 11 }];
        let s = score_one(&gold, &pred);
        assert_eq!(s.get("person").map(|c| c.true_positive), Some(1));
        assert_eq!(s.get("person").map(|c| c.false_negative), Some(0));
        assert_eq!(s.get("person").map(|c| c.false_positive), Some(0));
    }

    #[test]
    fn scores_count_false_positive_and_negative() {
        let gold = vec![GoldSpan { category: "person".into(), start: 0, end: 5 }];
        let pred = vec![PredSpan { category: "organization".into(), start: 10, end: 15 }];
        let s = score_one(&gold, &pred);
        assert_eq!(s.get("person").map(|c| c.false_negative), Some(1));
        assert_eq!(s.get("organization").map(|c| c.false_positive), Some(1));
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag`
Expected: FAIL — `PredSpan`/`score_one` not defined.

- [ ] **Step 3: Implement scoring**

Add to `detect_eval.rs`:

```rust
use std::collections::BTreeMap;

/// A predicted PII span (byte offsets), category lowercased to match gold.
#[derive(Debug, Clone)]
pub struct PredSpan {
    pub category: String,
    pub start: usize,
    pub end: usize,
}

/// Per-category confusion counts.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CategoryCounts {
    pub true_positive: usize,
    pub false_positive: usize,
    pub false_negative: usize,
}

impl CategoryCounts {
    #[must_use]
    pub fn recall(&self) -> f64 {
        let denom = self.true_positive + self.false_negative;
        if denom == 0 { 1.0 } else { self.true_positive as f64 / denom as f64 }
    }
    #[must_use]
    pub fn precision(&self) -> f64 {
        let denom = self.true_positive + self.false_positive;
        if denom == 0 { 1.0 } else { self.true_positive as f64 / denom as f64 }
    }
    #[must_use]
    pub fn f1(&self) -> f64 {
        let (p, r) = (self.precision(), self.recall());
        if p + r == 0.0 { 0.0 } else { 2.0 * p * r / (p + r) }
    }
}

fn overlaps(a: &GoldSpan, b: &PredSpan) -> bool {
    a.category.eq_ignore_ascii_case(&b.category) && a.start < b.end && b.start < a.end
}

/// Score one fixture: greedy 1:1 overlap match per category.
#[must_use]
pub fn score_one(gold: &[GoldSpan], pred: &[PredSpan]) -> BTreeMap<String, CategoryCounts> {
    let mut counts: BTreeMap<String, CategoryCounts> = BTreeMap::new();
    let mut matched_pred = vec![false; pred.len()];
    for g in gold {
        let entry = counts.entry(g.category.to_ascii_lowercase()).or_default();
        match pred.iter().enumerate().position(|(i, p)| !matched_pred[i] && overlaps(g, p)) {
            Some(i) => { matched_pred[i] = true; entry.true_positive += 1; }
            None => entry.false_negative += 1,
        }
    }
    for (i, p) in pred.iter().enumerate() {
        if !matched_pred[i] {
            counts.entry(p.category.to_ascii_lowercase()).or_default().false_positive += 1;
        }
    }
    counts
}

/// Merge per-fixture counts into an aggregate.
#[must_use]
pub fn merge(mut acc: BTreeMap<String, CategoryCounts>, one: BTreeMap<String, CategoryCounts>)
    -> BTreeMap<String, CategoryCounts> {
    for (cat, c) in one {
        let e = acc.entry(cat).or_default();
        e.true_positive += c.true_positive;
        e.false_positive += c.false_positive;
        e.false_negative += c.false_negative;
    }
    acc
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag`
Expected: PASS (both scoring tests).

- [ ] **Step 5: Commit**

```bash
git add crates/anno-rag/src/detect_eval.rs
git commit -m "feat(detect-eval): per-category precision/recall/F1 scoring"
```

---

### Task 3: Wire `Detector` → predicted spans + baseline gate (ignored until measured)

**Files:**
- Modify: `crates/anno-rag/src/detect_eval.rs`
- Create: 4–6 more fixtures under `short/` and `long/` (person, organization, health_data, address, email)

- [ ] **Step 1: Add fixtures**

Create at least these (synthetic PII only). Example `long/health_01.json`:

```json
{
  "text": "Le patient Marc Dubois souffre d'un diabète de type 2 diagnostiqué en 2019 par le Dr Lefèvre.",
  "spans": [
    { "category": "person", "start": 11, "end": 22 },
    { "category": "health_data", "start": 36, "end": 53 },
    { "category": "person", "start": 82, "end": 92 }
  ]
}
```

> Verify each `start`/`end` is a byte offset by slicing the string in a scratch test if unsure. Add `short/email_01.json`, `long/org_01.json`, `short/address_01.json` similarly.

- [ ] **Step 2: Add a model-backed eval runner behind `#[ignore]`**

Add to `detect_eval.rs` tests (ignored because it loads the ONNX model, slow):

```rust
    fn detector_pred_spans(det: &crate::detect::Detector, text: &str) -> Vec<PredSpan> {
        det.detect(text)
            .expect("detect")
            .into_iter()
            .map(|e| PredSpan {
                category: match &e.category {
                    cloakpipe_core::EntityCategory::Custom(s) => s.to_ascii_lowercase(),
                    other => format!("{other:?}").to_ascii_lowercase(),
                },
                start: e.start,
                end: e.end,
            })
            .collect()
    }

    #[test]
    #[ignore = "loads ONNX PII model; run explicitly to (re)measure"]
    fn pii_eval_report() {
        let det = crate::detect::Detector::new(&crate::config::AnnoRagConfig::default())
            .expect("detector");
        let fx = load_fixtures(&pii_eval_dir()).expect("fixtures");
        let mut agg = std::collections::BTreeMap::new();
        for f in &fx {
            let pred = detector_pred_spans(&det, &f.text);
            agg = merge(agg, score_one(&f.spans, &pred));
        }
        for (cat, c) in &agg {
            println!(
                "{cat}: P={:.2} R={:.2} F1={:.2} (tp={} fp={} fn={})",
                c.precision(), c.recall(), c.f1(),
                c.true_positive, c.false_positive, c.false_negative
            );
        }
    }
```

- [ ] **Step 3: Run the report explicitly and record the baseline**

Run: `cargo test -p anno-rag detect_eval::tests::pii_eval_report -- --ignored --nocapture`
Expected: prints per-category P/R/F1. **Record these numbers in the spec's B1 section as the baseline** (commit message references them).

- [ ] **Step 4: Add the gate with floors derived from the baseline**

Add (ignored, model-loading) — floors set from the Step 3 measurement, Art.9 forced to ≥0.90:

```rust
    #[test]
    #[ignore = "loads ONNX PII model; non-regression gate"]
    fn pii_eval_meets_floors() {
        let art9 = ["health_data","genetic_data","biometric_data","sexual_orientation",
                    "political_opinion","religious_belief","trade_union_membership","racial_ethnic_origin"];
        let det = crate::detect::Detector::new(&crate::config::AnnoRagConfig::default()).expect("detector");
        let fx = load_fixtures(&pii_eval_dir()).expect("fixtures");
        let mut agg = std::collections::BTreeMap::new();
        for f in &fx { agg = merge(agg, score_one(&f.spans, &detector_pred_spans(&det, &f.text))); }
        for cat in art9 {
            if let Some(c) = agg.get(cat) {
                assert!(c.recall() >= 0.90, "{cat} recall {:.2} < 0.90", c.recall());
            }
        }
    }
```

- [ ] **Step 5: Commit**

```bash
git add crates/anno-rag/src/detect_eval.rs crates/anno-rag/tests/fixtures/pii_eval/
git commit -m "feat(detect-eval): model-backed report + Art.9 recall gate (baseline recorded)"
```

---

## Phase 2 — Unify query and ingest detection

### Task 4: Structured label groups (single source of truth)

**Files:**
- Modify: `crates/anno-rag/src/detect.rs`

- [ ] **Step 1: Write the failing test**

Add to `detect.rs` tests:

```rust
    #[test]
    fn label_groups_cover_all_gdpr_labels() {
        let grouped: std::collections::HashSet<&str> =
            label_groups().iter().flat_map(|g| g.labels.iter().map(|l| l.name)).collect();
        for (name, _, _) in GDPR_NER_LABELS {
            assert!(grouped.contains(name), "label {name} missing from any group");
        }
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag`
Expected: FAIL — `label_groups` not defined.

- [ ] **Step 3: Define label groups derived from `GDPR_NER_LABELS`**

Add to `detect.rs`:

```rust
/// One GDPR label with its model description and retain threshold.
#[derive(Debug, Clone, Copy)]
pub(crate) struct GdprLabel {
    pub name: &'static str,
    pub description: &'static str,
    pub threshold: f32,
}

/// A focused detection pass: a narrow set of labels sent together.
pub(crate) struct LabelGroup {
    pub key: &'static str,
    pub labels: Vec<GdprLabel>,
}

fn label_by_name(name: &str) -> GdprLabel {
    let (n, d, t) = GDPR_NER_LABELS.iter().find(|(l, _, _)| *l == name)
        .expect("known GDPR label");
    GdprLabel { name: n, description: d, threshold: *t }
}

/// Focused groups. `identifiers_model` labels are also covered by the regex
/// layer; they ride along in a third group only when the model adds value.
pub(crate) fn label_groups() -> Vec<LabelGroup> {
    let pick = |names: &[&str]| names.iter().map(|n| label_by_name(n)).collect::<Vec<_>>();
    vec![
        LabelGroup { key: "identity", labels: pick(&[
            "person","address","date_of_birth","age","nationality","profession",
            "organization","location",
        ]) },
        LabelGroup { key: "art9", labels: pick(&[
            "racial_ethnic_origin","political_opinion","religious_belief",
            "trade_union_membership","health_data","genetic_data","biometric_data",
            "sexual_orientation","criminal_record",
        ]) },
        LabelGroup { key: "identifiers_model", labels: pick(&[
            "national_id","tax_id","bank_account","ip_address","username","device_id",
        ]) },
    ]
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag`
Expected: PASS (`label_groups_cover_all_gdpr_labels`).

- [ ] **Step 5: Commit**

```bash
git add crates/anno-rag/src/detect.rs
git commit -m "feat(detect): structured GDPR label groups (identity/art9/identifiers)"
```

---

### Task 5: Single description-based detection core

**Files:**
- Modify: `crates/anno-rag/src/detect.rs`

Goal: one private method `pii_ner_spans(text, groups) -> Vec<DetectedEntity>` that both `detect_inner` and `detect_for_ingest` call. It runs `extract_with_label_descriptions` per group and retains by per-label threshold.

- [ ] **Step 1: Write the failing test**

Add to `detect.rs` tests (uses the stubbed `pii_ner` test harness already present — mirror `detects_iban_fr`):

```rust
    #[test]
    fn unified_core_runs_without_panic_on_empty() {
        let d = Detector::new(&crate::config::AnnoRagConfig::default()).expect("detector builds");
        // Empty text → no spans, no panic.
        let out = d.pii_ner_spans("", &label_groups());
        assert!(out.expect("ok").is_empty());
    }
```

> This test loads the model; if the suite already gates model tests behind a feature/ignore (see existing `detects_iban_fr`), match that convention (add `#[ignore]` if the others are ignored locally).

- [ ] **Step 2: Run test to verify it fails**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag`
Expected: FAIL — `no method named pii_ner_spans`.

- [ ] **Step 3: Implement the unified core**

Add to `impl Detector`:

```rust
    /// Run the PII model over each label group (description-based) and retain
    /// spans meeting their per-label threshold. Shared by query + ingest paths.
    pub(crate) fn pii_ner_spans(
        &self,
        text: &str,
        groups: &[LabelGroup],
    ) -> Result<Vec<DetectedEntity>> {
        let mut all: Vec<DetectedEntity> = Vec::new();
        for group in groups {
            if group.labels.is_empty() {
                continue;
            }
            let described: Vec<(&str, &str)> =
                group.labels.iter().map(|l| (l.name, l.description)).collect();
            // Floor 0.25 passes everything; per-label thresholds applied below.
            let entities = self
                .pii_ner
                .extract_with_label_descriptions(text, &described, 0.25)
                .map_err(|e| Error::Detect(e.to_string()))?;
            let mut detected = anno_entities_to_detected(text, entities)?;
            detected.retain(|e| {
                let label = match &e.category {
                    cloakpipe_core::EntityCategory::Custom(s) => s.as_str(),
                    other => return f64::from(e.confidence) >= 0.40, // Person/Org/Loc
                };
                let thr = group.labels.iter().find(|l| l.name == label)
                    .map(|l| l.threshold).unwrap_or(0.50);
                f64::from(e.confidence) >= f64::from(thr)
            });
            all.extend(detected);
        }
        Ok(all)
    }
```

> Verify `e.confidence`'s type (f32/f64) against `DetectedEntity` in `cloakpipe_core`; adjust the `f64::from` casts to compile. The `other =>` arm handles the built-in Person/Organization/Location categories that aren't `Custom`.

- [ ] **Step 4: Run test to verify it passes**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/anno-rag/src/detect.rs
git commit -m "feat(detect): unified description-based per-label PII core"
```

---

### Task 6: Route `detect_inner` and `detect_for_ingest` through the core

**Files:**
- Modify: `crates/anno-rag/src/detect.rs` (`detect_inner`, `detect_for_ingest`)

- [ ] **Step 1: Write the failing parity test**

Add to `detect.rs` tests (model-loading; match local ignore convention):

```rust
    #[test]
    #[ignore = "loads ONNX PII model; query/ingest parity"]
    fn query_and_ingest_agree_on_pii() {
        let d = Detector::new(&crate::config::AnnoRagConfig::default()).expect("detector");
        let text = "Le patient Marc Dubois souffre d'un diabète de type 2.";
        let from_query = d.detect(text).expect("query");
        let bundle = d
            .detect_for_ingest(text, &crate::legal::default_legal_labels(), &crate::legal::default_thresholds())
            .expect("ingest");
        let q: std::collections::HashSet<(usize, usize)> =
            from_query.iter().filter(|e| crate::detect::is_pii_entity(e)).map(|e| (e.start, e.end)).collect();
        let i: std::collections::HashSet<(usize, usize)> =
            bundle.pii.iter().map(|e| (e.start, e.end)).collect();
        assert_eq!(q, i, "query and ingest must agree on PII spans");
    }
```

- [ ] **Step 2: Re-implement `detect_inner` over the core**

Replace the model section of `detect_inner` (the `extract_with_label_descriptions` block, detect.rs:672-682) with:

```rust
        // 2. NER over focused label groups (shared core).
        all.extend(self.pii_ner_spans(text, &label_groups())?);
```

Keep steps 1 (regex), 1b (heuristics), 3 (sort+dedup), 4 (validators) unchanged.

- [ ] **Step 3: Re-implement the PII branch of `detect_for_ingest` over the core**

In `detect_for_ingest`, replace the `extract_with_label_thresholds` call + `is_pii_entity` push loop (detect.rs:762-779) so the **PII** spans come from `pii_ner_spans`, while the **legal** spans keep using the existing `ner`/legal-label path. Concretely:

```rust
        // PII spans via the shared focused-group core.
        let mut pii_model = self.pii_ner_spans(text, &label_groups())?;
        pii.append(&mut pii_model);
        pii.sort_by(|a, b| {
            a.start.cmp(&b.start)
                .then_with(|| (b.end - b.start).cmp(&(a.end - a.start)))
                .then_with(|| pattern_priority(&a.source).cmp(&pattern_priority(&b.source)))
        });
        dedup_overlaps(&mut pii, text);

        // Legal spans keep using the generalist `ner` with legal labels.
        let mut legal_label_thresholds: Vec<(&str, f32)> = Vec::new();
        for label in legal_labels {
            legal_label_thresholds.push((label.name, legal_thresholds.get(label.name).copied().unwrap_or(0.5)));
        }
        let raw_model_spans = anno_entities_to_detected(
            text,
            self.ner.extract_with_label_thresholds(text, &legal_label_thresholds)
                .map_err(|e| Error::Detect(e.to_string()))?,
        )?;
        let mut legal: Vec<LegalEntity> = raw_model_spans.iter().filter_map(|entity| {
            let EntityCategory::Custom(label) = &entity.category else { return None; };
            if !legal_labels.iter().any(|c| c.name == label) { return None; }
            Some(LegalEntity {
                label: label.clone(), text: entity.original.clone(),
                byte_start: entity.start as u32, byte_end: entity.end as u32,
                confidence: entity.confidence as f32,
            })
        }).collect();
        dedup_legal_overlaps(&mut legal);
```

> This changes `raw_model_spans` to mean "legal model spans" — confirm no downstream consumer relies on it containing PII spans (grep `raw_model_spans`). If one does, build it as the union before returning.

- [ ] **Step 4: Run the parity test**

Run: `cargo test -p anno-rag detect::tests::query_and_ingest_agree_on_pii -- --ignored --nocapture`
Expected: PASS — identical PII spans from both paths.

- [ ] **Step 5: Commit**

```bash
git add crates/anno-rag/src/detect.rs
git commit -m "refactor(detect): query + ingest share the focused-group PII core"
```

---

## Phase 3 — Validate the narrowing (measurement)

### Task 7: Measure narrowing vs single-pass; settle group count

**Files:**
- Modify: `crates/anno-rag/src/detect_eval.rs` (add a single-pass comparison helper)

- [ ] **Step 1: Add a single-pass baseline detector path for comparison**

Add a test-only helper that runs all 24 labels in ONE described pass (the old behavior) to compare against `pii_ner_spans(groups)`:

```rust
    #[ignore = "loads ONNX PII model; narrowing comparison"]
    #[test]
    fn narrowing_improves_or_holds_recall() {
        use crate::detect::{Detector, label_groups, gdpr_described_for_test};
        let det = Detector::new(&crate::config::AnnoRagConfig::default()).expect("detector");
        let fx = load_fixtures(&pii_eval_dir()).expect("fixtures");
        let (mut grouped, mut single) = (std::collections::BTreeMap::new(), std::collections::BTreeMap::new());
        for f in &fx {
            grouped = merge(grouped, score_one(&f.spans, &det_pred_grouped(&det, &f.text)));
            single = merge(single, score_one(&f.spans, &det_pred_single(&det, &f.text)));
        }
        for cat in ["person","organization","health_data"] {
            let (rg, rs) = (recall_of(&grouped, cat), recall_of(&single, cat));
            println!("{cat}: grouped R={rg:.2} vs single R={rs:.2}");
            assert!(rg + 1e-9 >= rs, "{cat} grouped recall regressed: {rg:.2} < {rs:.2}");
        }
    }
```

> Implement `det_pred_grouped`, `det_pred_single`, `recall_of`, and the test-only exports (`gdpr_described_for_test`) as small helpers. `det_pred_single` calls a test-only `Detector` method that runs one described pass over all `GDPR_NER_LABELS`.

- [ ] **Step 2: Run and record**

Run: `cargo test -p anno-rag detect_eval::tests::narrowing_improves_or_holds_recall -- --ignored --nocapture`
Expected: prints grouped vs single recall. **Decision rule:** if `art9` group adds no recall over folding its labels into one pass, keep it (Art.9 semantics differ); if `identifiers_model` group adds nothing over regex, drop that group from `label_groups()` (YAGNI) and re-run Task 4's coverage test (it will fail — update it to assert identifiers are covered by regex, not a group).

- [ ] **Step 3: Apply the decision**

Edit `label_groups()` per the measured outcome. If a group is dropped, update `label_groups_cover_all_gdpr_labels` to exclude regex-covered identifier labels with an explicit comment.

- [ ] **Step 4: Run tests**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/anno-rag/src/detect.rs crates/anno-rag/src/detect_eval.rs
git commit -m "perf(detect): settle label-group count from measured recall"
```

---

## Phase 4 — Dual masking scope

### Task 8: `MaskingScope` enum + default

**Files:**
- Modify: `crates/anno-rag/src/config.rs`

- [ ] **Step 1: Write the failing test**

Add to `config.rs` tests:

```rust
    #[test]
    fn masking_scope_defaults_to_rgpd_strict() {
        let cfg = AnnoRagConfig::default();
        assert_eq!(cfg.masking_scope, MaskingScope::RgpdStrict);
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag`
Expected: FAIL — `MaskingScope` / `masking_scope` not defined.

- [ ] **Step 3: Add the enum + field**

In `config.rs`:

```rust
/// Masking perimeter for PII detection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MaskingScope {
    /// GDPR personal-data scope (organization only when tied to a person).
    #[default]
    RgpdStrict,
    /// Cabinet confidentiality: mask all organizations and named parties.
    CabinetConfidential,
}
```

Add to `AnnoRagConfig` (with the crate's serde-default pattern):

```rust
    #[serde(default)]
    pub masking_scope: MaskingScope,
```

Set it in the `Default for AnnoRagConfig` impl: `masking_scope: MaskingScope::default(),`.

- [ ] **Step 4: Run test to verify it passes**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/anno-rag/src/config.rs
git commit -m "feat(config): MaskingScope enum (rgpd_strict default / cabinet_confidential)"
```

---

### Task 9: Scope-aware `identity` group

**Files:**
- Modify: `crates/anno-rag/src/detect.rs`

- [ ] **Step 1: Write the failing test**

Add to `detect.rs` tests:

```rust
    #[test]
    fn cabinet_scope_lowers_org_threshold() {
        let strict = label_groups_for(crate::config::MaskingScope::RgpdStrict);
        let cabinet = label_groups_for(crate::config::MaskingScope::CabinetConfidential);
        let org_strict = strict.iter().flat_map(|g| &g.labels).find(|l| l.name == "organization").unwrap().threshold;
        let org_cabinet = cabinet.iter().flat_map(|g| &g.labels).find(|l| l.name == "organization").unwrap().threshold;
        assert!(org_cabinet < org_strict, "cabinet scope must lower org threshold");
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag`
Expected: FAIL — `label_groups_for` not defined.

- [ ] **Step 3: Add `label_groups_for(scope)`**

Refactor `label_groups()` to delegate to `label_groups_for(MaskingScope::RgpdStrict)`. Add:

```rust
pub(crate) fn label_groups_for(scope: crate::config::MaskingScope) -> Vec<LabelGroup> {
    let mut groups = label_groups(); // base = RGPD strict
    if matches!(scope, crate::config::MaskingScope::CabinetConfidential) {
        for g in &mut groups {
            for l in &mut g.labels {
                if l.name == "organization" {
                    // Broaden + lower threshold: mask every named org/party.
                    l.description = "toute organisation, cabinet, société, administration ou partie nommée";
                    l.threshold = 0.30;
                }
            }
        }
    }
    groups
}
```

- [ ] **Step 4: Thread scope into the detector core**

Change `pii_ner_spans(text, groups)` callers in `detect_inner` / `detect_for_ingest` to pass `&label_groups_for(self.masking_scope)`. Add a `masking_scope: crate::config::MaskingScope` field to `Detector`, set from `cfg.masking_scope` in every `Detector::new*` constructor.

- [ ] **Step 5: Run tests**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag`
Expected: PASS (`cabinet_scope_lowers_org_threshold`).

- [ ] **Step 6: Commit**

```bash
git add crates/anno-rag/src/detect.rs
git commit -m "feat(detect): scope-aware identity group (cabinet vs rgpd)"
```

---

### Task 10: Validate scope behavior with eval

**Files:**
- Create: `crates/anno-rag/tests/fixtures/pii_eval/long/org_cabinet_01.json`

- [ ] **Step 1: Add a fixture where cabinet masks an org rgpd leaves**

`org_cabinet_01.json`:

```json
{
  "text": "Le litige oppose la société Durand SAS au cabinet Legrand & Associés.",
  "spans": [
    { "category": "organization", "start": 21, "end": 31 },
    { "category": "organization", "start": 42, "end": 67 }
  ]
}
```

- [ ] **Step 2: Add an ignored scope-comparison test**

```rust
    #[test]
    #[ignore = "loads ONNX PII model; scope comparison"]
    fn cabinet_scope_catches_more_orgs() {
        use crate::config::{AnnoRagConfig, MaskingScope};
        let mut strict_cfg = AnnoRagConfig::default(); strict_cfg.masking_scope = MaskingScope::RgpdStrict;
        let mut cab_cfg = AnnoRagConfig::default(); cab_cfg.masking_scope = MaskingScope::CabinetConfidential;
        let strict = crate::detect::Detector::new(&strict_cfg).expect("strict");
        let cabinet = crate::detect::Detector::new(&cab_cfg).expect("cabinet");
        let text = std::fs::read_to_string(
            pii_eval_dir().join("long/org_cabinet_01.json")).expect("read");
        let f: PiiFixture = serde_json::from_str(&text).expect("parse");
        let rs = score_one(&f.spans, &detector_pred_spans(&strict, &f.text));
        let rc = score_one(&f.spans, &detector_pred_spans(&cabinet, &f.text));
        assert!(recall_of(&rc, "organization") >= recall_of(&rs, "organization"),
            "cabinet must catch >= orgs than strict");
    }
```

- [ ] **Step 3: Run explicitly**

Run: `cargo test -p anno-rag detect_eval::tests::cabinet_scope_catches_more_orgs -- --ignored --nocapture`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/anno-rag/tests/fixtures/pii_eval/long/org_cabinet_01.json crates/anno-rag/src/detect_eval.rs
git commit -m "test(detect-eval): cabinet scope catches orgs rgpd-strict skips"
```

---

## Phase 5 — Query-path leak coverage + finalize

### Task 11: Vault-lookup-first coverage on the query path

**Files:**
- Modify: `crates/anno-rag/src/pipeline.rs` (`search`, line 754)

- [ ] **Step 1: Confirm current behavior**

Read `Pipeline::search` (pipeline.rs:754) and `vault.pseudonymize`. Verify whether `pseudonymize` already replaces known vault tokens regardless of NER. Document the finding in the commit message.

- [ ] **Step 2: Write the failing test**

Add an integration test (in `anno-rag` tests) that:
1. ingests a doc containing a name → vault assigns it a token;
2. issues a query containing that exact name;
3. asserts the embedded/pseudonymized query no longer contains the cleartext name.

```rust
    // Adapt to the existing Pipeline test harness. The key assertion:
    // let pseudo = pipeline.pseudonymize_query_for_test("…Marc Dubois…").await?;
    // assert!(!pseudo.contains("Marc Dubois"));
```

> If no `pseudonymize_query_for_test` seam exists, extract the query-pseudonymization step of `search` into a small testable method `async fn pseudonymize_query(&self, q: &str) -> Result<String>` and call it from `search`.

- [ ] **Step 3: Ensure vault lookup runs on the query**

If Step 1 showed a gap, make `pseudonymize_query` apply vault token replacement for all known tokens before/independent of NER confidence. If no gap exists, the test passes as-is and this step is a no-op (note it in the commit).

- [ ] **Step 4: Run tests**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag`
Expected: PASS — cleartext name absent from pseudonymized query.

- [ ] **Step 5: Commit**

```bash
git add crates/anno-rag/src/pipeline.rs
git commit -m "feat(pipeline): guarantee vault-lookup pseudonymization on query path"
```

---

### Task 12: Re-measure, fmt, clippy

**Files:** none (verification)

- [ ] **Step 1: Re-run the eval report and gate**

Run: `cargo test -p anno-rag detect_eval::tests::pii_eval_report -- --ignored --nocapture`
Then: `cargo test -p anno-rag detect_eval::tests::pii_eval_meets_floors -- --ignored`
Expected: per-category recall ≥ recorded baseline; Art.9 ≥ 0.90. If any category regressed, adjust only that category's threshold (recall deficit only) and re-run.

- [ ] **Step 2: Format**

Run: `cargo fmt`
Commit separately: `git add -A && git commit -m "style: cargo fmt"`

- [ ] **Step 3: Clippy (jobs 2)**

Run: `cargo clippy --package anno-rag --jobs 2 -- -D warnings`
Fix lints inline.

- [ ] **Step 4: Targeted check**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag -Mode check`
Expected: PASS.

- [ ] **Step 5: Commit any fixes**

```bash
git add -A
git commit -m "chore: finalize NER PII confidence (eval re-measured)"
```

---

## Self-Review notes

- **Spec coverage:** B1 → Tasks 1–3 (+7 measurement); B2 → Tasks 4–6; B3 → Tasks 4,5,7; B4 → Tasks 8–10; B5 → Task 11; gate/finalize → Tasks 3,12. All covered.
- **Measurement-first preserved:** Task 3 records the baseline before any behavior change (Tasks 4–6 are refactors validated by parity test; Task 7 gates narrowing on measured recall).
- **Flagged verifications (not placeholders):** `e.confidence` numeric type (Task 5 Step 3); `raw_model_spans` downstream consumers (Task 6 Step 3); existing model-test ignore convention (Tasks 5,6); vault-lookup seam in `search` (Task 11). Each names the exact grep/read to do first.
- **Type consistency:** `GdprLabel { name, description, threshold }`, `LabelGroup { key, labels }`, `pii_ner_spans(&self, &str, &[LabelGroup]) -> Result<Vec<DetectedEntity>>`, `label_groups_for(MaskingScope) -> Vec<LabelGroup>`, `MaskingScope::{RgpdStrict, CabinetConfidential}`, `score_one`/`merge`/`CategoryCounts` — consistent across Tasks 1–10.
- **Decision rule, not guess:** Task 7 settles the group count from measured recall; identifiers_model group is dropped if regex already covers it.
```
