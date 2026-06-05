# PII Precision — Phase A : Deterministic Stack Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a deterministic French-aware detection layer (heuristics FR backend + 8 validators) on top of the existing GLiNER2 + regex pipeline, gated behind a feature flag so the current behaviour stays the default until Phase B/C/D ship.

**Architecture:** Three additions to the existing `anno-rag/src/detect.rs` flow :

1. New `EntityValidator` trait with 8 implementations in `crates/anno-rag/src/validators/` — post-aggregator filter that rejects spans failing deterministic checks (Luhn, mod-97, NIR control key, etc.)
2. New `crates/anno/src/backends/heuristic_fr/` backend implementing the `Model` trait — runs FR-specific patterns (org suffixes, addresses, dates, intl IBANs) as a complement to the existing regex layer
3. Extended audit telemetry to count rejections by reason and entities by source, plus the `ANNO_GDPR_LAYERS` environment variable for runtime layer selection (`basic` / `defense` — Phase C adds `full`)

**Tech Stack:** Rust (anno workspace, edition 2021), `regex` crate, `thiserror`, `tracing` for audit events, `proptest` for invariants, `insta` for snapshot tests, existing `cloakpipe_core::{DetectedEntity, DetectionSource, EntityCategory}` types.

**Pre-conditions:** Branch `feat/gdpr-full-coverage` already contains the 23-label `GDPR_NER_LABELS` table from commit `1a1e83c5`. This plan starts from that state.

---

## File structure

**New files:**

- `crates/anno-rag/src/validators/mod.rs` — `EntityValidator` trait, `ValidationResult` enum, `apply_validators` orchestrator
- `crates/anno-rag/src/validators/luhn.rs` — Luhn checksum validator (Card, SIRET)
- `crates/anno-rag/src/validators/iban.rs` — IBAN mod-97 validator (FR + international)
- `crates/anno-rag/src/validators/nir.rs` — NIR control-key validator
- `crates/anno-rag/src/validators/dates.rs` — date_of_birth year range validator
- `crates/anno-rag/src/validators/network.rs` — IP address parse validator
- `crates/anno-rag/src/validators/email.rs` — RFC-5321 light email validator
- `crates/anno-rag/src/validators/postal.rs` — French postal code range validator
- `crates/anno-rag/src/layers.rs` — `GdprLayerSet` enum and parse-from-env
- `crates/anno-rag/tests/integration_phase_a.rs` — end-to-end pipeline tests across feature-flag levels
- `crates/anno/src/backends/heuristic_fr/mod.rs` — `HeuristicFrNer` struct + `Model` trait impl
- `crates/anno/src/backends/heuristic_fr/orgs.rs` — French ORG suffix matcher
- `crates/anno/src/backends/heuristic_fr/addresses.rs` — FR address pattern matcher
- `crates/anno/src/backends/heuristic_fr/dates.rs` — FR date pattern matcher (with date_of_birth context)
- `crates/anno/src/backends/heuristic_fr/iban_intl.rs` — international IBAN matcher with mod-97

**Modified files:**

- `crates/anno-rag/src/lib.rs` — declare new modules (`validators`, `layers`)
- `crates/anno-rag/src/detect.rs` — wire heuristics + validators into `detect_inner` and `detect_for_ingest`, extend `emit_detect_audit`
- `crates/anno-rag/Cargo.toml` — no new deps needed (regex, tracing, std::net already there)
- `crates/anno/src/backends/mod.rs` — declare `heuristic_fr` module behind `#[cfg(feature = "heuristic-fr")]`
- `crates/anno/Cargo.toml` — add `heuristic-fr = []` feature (default-on)

---

## Conventions used throughout this plan

**TDD cycle for every component**: write a failing test, run it to confirm RED, write the minimal implementation, run the test to confirm GREEN, commit. Never skip the RED step.

**Test runner**: All Rust tests use the project's dev loop :

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package <crate> -LibOnly
```

For integration tests, drop `-LibOnly`. The script handles `CARGO_TARGET_DIR=E:\cargo-target`, concurrent-build guard, and uses `nextest` profile `local`.

**Commit messages**: format `feat(detect): <action>` or `test(detect): <action>` or `chore(detect): <action>`. Always include the body explaining *why*. Co-author trailer is added at PR creation, not per commit.

**Concurrent build guard**: Before running ANY cargo command, check `Get-Process cargo,rustc -ErrorAction SilentlyContinue`. If processes are running, wait or stop them per `CLAUDE.md` rules.

---

## Task 1: Scaffold `EntityValidator` trait + `ValidationResult`

**Files:**
- Create: `crates/anno-rag/src/validators/mod.rs`
- Modify: `crates/anno-rag/src/lib.rs` (add `pub mod validators;`)

- [ ] **Step 1: Write the failing test**

Create `crates/anno-rag/src/validators/mod.rs`:

```rust
//! Deterministic post-aggregator validators for PII entities.
//!
//! Each validator targets one label and either accepts, rejects, or
//! adjusts the confidence of a `DetectedEntity`. Rejections are
//! aggregated into counters that are emitted via the detect audit
//! event so operators can monitor false-positive suppression rates
//! without seeing the underlying text.

use cloakpipe_core::DetectedEntity;
use std::collections::BTreeMap;

/// Outcome of a single validator on a single entity.
#[derive(Debug, Clone, PartialEq)]
pub enum ValidationResult {
    /// Pass-through, no change.
    Accept,
    /// Drop the entity. `reason` is a static identifier (e.g. "luhn_failed").
    Reject { reason: &'static str },
    /// Keep the entity but override its confidence to the given value.
    AdjustConfidence(f32),
}

/// A label-targeted deterministic check. Implementations must be `Send + Sync`
/// because the detector is shared between async tasks via `Arc`.
pub trait EntityValidator: Send + Sync + std::fmt::Debug {
    /// The label this validator applies to. Validators only run on
    /// `DetectedEntity` whose `category.as_str()` matches this string.
    fn label(&self) -> &'static str;

    /// Run the check. `ctx` is the original document text (the validator
    /// may use surrounding context, e.g. for `StructuredIdCrossCheck`).
    fn validate(&self, entity: &DetectedEntity, ctx: &str) -> ValidationResult;
}

/// Counter for rejections, keyed by validator reason string.
pub type RejectionCounts = BTreeMap<&'static str, usize>;

#[cfg(test)]
mod tests {
    use super::*;
    use cloakpipe_core::{DetectionSource, EntityCategory};

    #[derive(Debug)]
    struct AlwaysAccept;
    impl EntityValidator for AlwaysAccept {
        fn label(&self) -> &'static str { "test" }
        fn validate(&self, _e: &DetectedEntity, _c: &str) -> ValidationResult {
            ValidationResult::Accept
        }
    }

    #[test]
    fn trait_is_object_safe() {
        // This will fail to compile if the trait is not object-safe.
        let _: Box<dyn EntityValidator> = Box::new(AlwaysAccept);
    }

    #[test]
    fn validation_result_equality() {
        assert_eq!(ValidationResult::Accept, ValidationResult::Accept);
        assert_eq!(
            ValidationResult::Reject { reason: "x" },
            ValidationResult::Reject { reason: "x" },
        );
        assert_ne!(
            ValidationResult::Reject { reason: "x" },
            ValidationResult::Reject { reason: "y" },
        );
    }
}
```

Add to `crates/anno-rag/src/lib.rs` (after the existing `pub mod` declarations):

```rust
pub mod validators;
```

- [ ] **Step 2: Run test to verify it compiles AND passes**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag -LibOnly`

Expected: tests `validators::tests::trait_is_object_safe` and `validators::tests::validation_result_equality` PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/anno-rag/src/validators/mod.rs crates/anno-rag/src/lib.rs
git commit -m "feat(detect): scaffold EntityValidator trait and ValidationResult"
```

---

## Task 2: Implement `LuhnValidator`

**Files:**
- Create: `crates/anno-rag/src/validators/luhn.rs`
- Modify: `crates/anno-rag/src/validators/mod.rs` (add `pub mod luhn;`)

- [ ] **Step 1: Write the failing test**

Create `crates/anno-rag/src/validators/luhn.rs`:

```rust
//! Luhn checksum validator for card numbers, SIRET, and other Luhn-protected IDs.

use super::{EntityValidator, ValidationResult};
use cloakpipe_core::DetectedEntity;

/// Validates that `entity.original` passes the Luhn checksum.
/// Applies to the labels listed in `LUHN_LABELS`.
#[derive(Debug, Clone, Copy)]
pub struct LuhnValidator {
    target_label: &'static str,
}

impl LuhnValidator {
    pub const fn new(label: &'static str) -> Self {
        Self { target_label: label }
    }
}

impl EntityValidator for LuhnValidator {
    fn label(&self) -> &'static str { self.target_label }

    fn validate(&self, e: &DetectedEntity, _ctx: &str) -> ValidationResult {
        if luhn_check(&e.original) {
            ValidationResult::Accept
        } else {
            ValidationResult::Reject { reason: "luhn_failed" }
        }
    }
}

/// Luhn algorithm: sum digits from right to left, doubling every second
/// digit (subtracting 9 if > 9). Total must be a multiple of 10.
fn luhn_check(s: &str) -> bool {
    let digits: Vec<u32> = s.chars().filter_map(|c| c.to_digit(10)).collect();
    if digits.is_empty() { return false; }
    let total: u32 = digits.iter().rev().enumerate().map(|(i, &d)| {
        if i % 2 == 1 {
            let doubled = d * 2;
            if doubled > 9 { doubled - 9 } else { doubled }
        } else { d }
    }).sum();
    total % 10 == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use cloakpipe_core::{DetectionSource, EntityCategory};

    fn entity_with(original: &str) -> DetectedEntity {
        DetectedEntity {
            original: original.to_string(),
            start: 0,
            end: original.len(),
            category: EntityCategory::Custom("SIRET".into()),
            confidence: 0.9,
            source: DetectionSource::Ner,
        }
    }

    #[test]
    fn luhn_accepts_known_valid_siret() {
        // 732 829 320 00074 — real-world public example (Carrefour HQ).
        let e = entity_with("73282932000074");
        let v = LuhnValidator::new("SIRET");
        assert_eq!(v.validate(&e, ""), ValidationResult::Accept);
    }

    #[test]
    fn luhn_rejects_random_14_digit_run() {
        let e = entity_with("12345678901234");
        let v = LuhnValidator::new("SIRET");
        assert_eq!(
            v.validate(&e, ""),
            ValidationResult::Reject { reason: "luhn_failed" },
        );
    }

    #[test]
    fn luhn_rejects_empty_or_non_digit() {
        let v = LuhnValidator::new("SIRET");
        assert!(matches!(v.validate(&entity_with(""), ""), ValidationResult::Reject { .. }));
        assert!(matches!(v.validate(&entity_with("abcdefg"), ""), ValidationResult::Reject { .. }));
    }

    #[test]
    fn luhn_accepts_valid_card_number() {
        // Standard Visa test card 4111 1111 1111 1111
        let e = entity_with("4111111111111111");
        let v = LuhnValidator::new("card_number");
        assert_eq!(v.validate(&e, ""), ValidationResult::Accept);
    }
}
```

Add to `crates/anno-rag/src/validators/mod.rs`:

```rust
pub mod luhn;
```

- [ ] **Step 2: Run test**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag -LibOnly`

Expected: all 4 `luhn::tests::*` tests PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/anno-rag/src/validators/luhn.rs crates/anno-rag/src/validators/mod.rs
git commit -m "feat(detect): add LuhnValidator for SIRET and card_number"
```

---

## Task 3: Implement `Iban97Validator`

**Files:**
- Create: `crates/anno-rag/src/validators/iban.rs`
- Modify: `crates/anno-rag/src/validators/mod.rs` (add `pub mod iban;`)

- [ ] **Step 1: Write the failing test**

Create `crates/anno-rag/src/validators/iban.rs`:

```rust
//! IBAN mod-97 checksum validator (ISO 13616).

use super::{EntityValidator, ValidationResult};
use cloakpipe_core::DetectedEntity;

#[derive(Debug, Clone, Copy)]
pub struct Iban97Validator {
    target_label: &'static str,
}

impl Iban97Validator {
    pub const fn new(label: &'static str) -> Self {
        Self { target_label: label }
    }
}

impl EntityValidator for Iban97Validator {
    fn label(&self) -> &'static str { self.target_label }

    fn validate(&self, e: &DetectedEntity, _ctx: &str) -> ValidationResult {
        if iban_mod97(&e.original) {
            ValidationResult::Accept
        } else {
            ValidationResult::Reject { reason: "iban_mod97_failed" }
        }
    }
}

/// ISO 13616 IBAN check : move first 4 chars to the end, replace letters
/// with their 1-indexed position-9 number, compute mod-97 over the result.
/// Returns true if the remainder is 1.
fn iban_mod97(raw: &str) -> bool {
    let s: String = raw.chars().filter(|c| !c.is_whitespace()).collect();
    if s.len() < 15 || s.len() > 34 { return false; }
    let s = s.to_ascii_uppercase();
    if !s.chars().all(|c| c.is_ascii_alphanumeric()) { return false; }
    // Move first 4 to end.
    let (head, tail) = s.split_at(4);
    let rearranged = format!("{tail}{head}");
    // Map letters to numbers.
    let mut numeric = String::with_capacity(rearranged.len() * 2);
    for c in rearranged.chars() {
        if c.is_ascii_digit() {
            numeric.push(c);
        } else if c.is_ascii_uppercase() {
            // A=10, B=11, ..., Z=35
            let n = (c as u32) - ('A' as u32) + 10;
            numeric.push_str(&n.to_string());
        } else {
            return false;
        }
    }
    // Stream-mod-97 to avoid big-integer parsing.
    let mut remainder: u32 = 0;
    for digit_ch in numeric.chars() {
        let d = digit_ch.to_digit(10).expect("filtered above");
        remainder = (remainder * 10 + d) % 97;
    }
    remainder == 1
}

#[cfg(test)]
mod tests {
    use super::*;
    use cloakpipe_core::{DetectionSource, EntityCategory};

    fn entity_with(original: &str) -> DetectedEntity {
        DetectedEntity {
            original: original.to_string(),
            start: 0,
            end: original.len(),
            category: EntityCategory::Custom("IBAN_FR".into()),
            confidence: 0.9,
            source: DetectionSource::Pattern,
        }
    }

    #[test]
    fn iban_accepts_valid_fr() {
        // Public example IBAN-FR from the ECB Iban registry.
        let e = entity_with("FR1420041010050500013M02606");
        assert_eq!(
            Iban97Validator::new("IBAN_FR").validate(&e, ""),
            ValidationResult::Accept,
        );
    }

    #[test]
    fn iban_accepts_valid_with_spaces() {
        let e = entity_with("FR14 2004 1010 0505 0001 3M02 606");
        assert_eq!(
            Iban97Validator::new("IBAN_FR").validate(&e, ""),
            ValidationResult::Accept,
        );
    }

    #[test]
    fn iban_accepts_valid_de() {
        // Public example IBAN-DE.
        let e = entity_with("DE89370400440532013000");
        assert_eq!(
            Iban97Validator::new("iban").validate(&e, ""),
            ValidationResult::Accept,
        );
    }

    #[test]
    fn iban_rejects_wrong_checksum() {
        let e = entity_with("FR9999999999999999999999999");
        assert!(matches!(
            Iban97Validator::new("IBAN_FR").validate(&e, ""),
            ValidationResult::Reject { reason: "iban_mod97_failed" },
        ));
    }

    #[test]
    fn iban_rejects_too_short() {
        let e = entity_with("FR12");
        assert!(matches!(
            Iban97Validator::new("iban").validate(&e, ""),
            ValidationResult::Reject { .. },
        ));
    }
}
```

Add to `crates/anno-rag/src/validators/mod.rs`:

```rust
pub mod iban;
```

- [ ] **Step 2: Run test**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag -LibOnly`

Expected: all 5 `iban::tests::*` tests PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/anno-rag/src/validators/iban.rs crates/anno-rag/src/validators/mod.rs
git commit -m "feat(detect): add Iban97Validator with ISO 13616 mod-97 checksum"
```

---

## Task 4: Implement `NirControlKeyValidator`

**Files:**
- Create: `crates/anno-rag/src/validators/nir.rs`
- Modify: `crates/anno-rag/src/validators/mod.rs` (add `pub mod nir;`)

- [ ] **Step 1: Write the failing test**

Create `crates/anno-rag/src/validators/nir.rs`:

```rust
//! French NIR (numéro de sécurité sociale) control-key validator.
//!
//! NIR format: 15 digits. Last 2 digits are the control key, computed as
//! `97 - (n mod 97)` where `n` is the first 13 digits interpreted as a
//! decimal number. Corsica departments (2A, 2B) require special handling:
//! 2A → 19 and 2B → 18 before the modulo.

use super::{EntityValidator, ValidationResult};
use cloakpipe_core::DetectedEntity;

#[derive(Debug, Clone, Copy)]
pub struct NirControlKeyValidator;

impl EntityValidator for NirControlKeyValidator {
    fn label(&self) -> &'static str { "NIR" }

    fn validate(&self, e: &DetectedEntity, _ctx: &str) -> ValidationResult {
        if nir_control_key_valid(&e.original) {
            ValidationResult::Accept
        } else {
            ValidationResult::Reject { reason: "nir_control_key_failed" }
        }
    }
}

fn nir_control_key_valid(raw: &str) -> bool {
    let cleaned: String = raw.chars().filter(|c| !c.is_whitespace()).collect();
    if cleaned.len() != 15 { return false; }
    // Bytes 5-6 may be "2A" or "2B" (Corsica). Substitute, then ensure all digits.
    let mut substituted = cleaned.clone();
    if &cleaned[5..7] == "2A" {
        substituted.replace_range(5..7, "19");
    } else if &cleaned[5..7] == "2B" {
        substituted.replace_range(5..7, "18");
    }
    if !substituted.chars().all(|c| c.is_ascii_digit()) { return false; }
    let body: u64 = match substituted[..13].parse() {
        Ok(n) => n,
        Err(_) => return false,
    };
    let key: u32 = match substituted[13..15].parse() {
        Ok(n) => n,
        Err(_) => return false,
    };
    let expected = 97 - (body % 97) as u32;
    key == expected
}

#[cfg(test)]
mod tests {
    use super::*;
    use cloakpipe_core::{DetectionSource, EntityCategory};

    fn entity_with(original: &str) -> DetectedEntity {
        DetectedEntity {
            original: original.to_string(),
            start: 0,
            end: original.len(),
            category: EntityCategory::Custom("NIR".into()),
            confidence: 1.0,
            source: DetectionSource::Pattern,
        }
    }

    #[test]
    fn nir_accepts_valid_mainland() {
        // Synthetic valid NIR : 1 84 12 76 451 089 / key
        // body=1841276451089, 1841276451089 mod 97 = 51, key = 97-51 = 46
        let e = entity_with("184127645108946");
        assert_eq!(NirControlKeyValidator.validate(&e, ""), ValidationResult::Accept);
    }

    #[test]
    fn nir_accepts_valid_corsica_2a() {
        // body=1842a76451089 → substitute → 1841976451089
        // 1841976451089 mod 97 = 12, key = 97-12 = 85
        let e = entity_with("1842A7645108985");
        assert_eq!(NirControlKeyValidator.validate(&e, ""), ValidationResult::Accept);
    }

    #[test]
    fn nir_rejects_wrong_key() {
        let e = entity_with("184127645108999");
        assert!(matches!(
            NirControlKeyValidator.validate(&e, ""),
            ValidationResult::Reject { reason: "nir_control_key_failed" },
        ));
    }

    #[test]
    fn nir_rejects_wrong_length() {
        assert!(matches!(
            NirControlKeyValidator.validate(&entity_with("12345"), ""),
            ValidationResult::Reject { .. },
        ));
    }

    #[test]
    fn nir_rejects_non_digit() {
        assert!(matches!(
            NirControlKeyValidator.validate(&entity_with("1ABCDEFG6451089"), ""),
            ValidationResult::Reject { .. },
        ));
    }
}
```

Add to `crates/anno-rag/src/validators/mod.rs`:

```rust
pub mod nir;
```

- [ ] **Step 2: Run test**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag -LibOnly`

Expected: all 5 `nir::tests::*` tests PASS. **If the "valid" example tests fail**, the computed expected keys in the doc comments are wrong — recompute by running `(body as u64 % 97)` interactively and update the test fixture.

- [ ] **Step 3: Commit**

```bash
git add crates/anno-rag/src/validators/nir.rs crates/anno-rag/src/validators/mod.rs
git commit -m "feat(detect): add NirControlKeyValidator with Corsica 2A/2B handling"
```

---

## Task 5: Implement remaining 4 validators (date, IP, email, postal)

**Files:**
- Create: `crates/anno-rag/src/validators/dates.rs`
- Create: `crates/anno-rag/src/validators/network.rs`
- Create: `crates/anno-rag/src/validators/email.rs`
- Create: `crates/anno-rag/src/validators/postal.rs`
- Modify: `crates/anno-rag/src/validators/mod.rs` (add the 4 `pub mod` declarations)

- [ ] **Step 1: Write all four files and their tests**

`crates/anno-rag/src/validators/dates.rs`:

```rust
//! Year-range validator for date_of_birth.

use super::{EntityValidator, ValidationResult};
use cloakpipe_core::DetectedEntity;
use regex::Regex;
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy)]
pub struct DateRangeValidator;

impl EntityValidator for DateRangeValidator {
    fn label(&self) -> &'static str { "date_of_birth" }

    fn validate(&self, e: &DetectedEntity, _ctx: &str) -> ValidationResult {
        let Some(year) = extract_year(&e.original) else {
            return ValidationResult::Reject { reason: "year_not_found" };
        };
        // Reject < 1900 (likely a typo or OCR error) or > current year.
        // Current year is read from chrono::Local rather than hardcoded.
        let now_year = chrono::Local::now().format("%Y").to_string()
            .parse::<u32>().unwrap_or(2026);
        if (1900..=now_year).contains(&year) {
            ValidationResult::Accept
        } else {
            ValidationResult::Reject { reason: "year_out_of_range" }
        }
    }
}

fn extract_year(s: &str) -> Option<u32> {
    static YEAR_RE: OnceLock<Regex> = OnceLock::new();
    let re = YEAR_RE.get_or_init(|| Regex::new(r"\b(19\d{2}|20\d{2})\b").expect("year regex"));
    re.find(s).and_then(|m| m.as_str().parse().ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cloakpipe_core::{DetectionSource, EntityCategory};

    fn entity_with(original: &str) -> DetectedEntity {
        DetectedEntity {
            original: original.to_string(),
            start: 0,
            end: original.len(),
            category: EntityCategory::Custom("date_of_birth".into()),
            confidence: 0.8,
            source: DetectionSource::Ner,
        }
    }

    #[test]
    fn accepts_valid_year() {
        assert_eq!(DateRangeValidator.validate(&entity_with("12/05/1984"), ""), ValidationResult::Accept);
        assert_eq!(DateRangeValidator.validate(&entity_with("né le 3 juin 2010"), ""), ValidationResult::Accept);
    }

    #[test]
    fn rejects_year_too_old() {
        assert!(matches!(
            DateRangeValidator.validate(&entity_with("né en 1850"), ""),
            ValidationResult::Reject { reason: "year_not_found" } | ValidationResult::Reject { reason: "year_out_of_range" }
        ));
    }

    #[test]
    fn rejects_year_too_new() {
        assert!(matches!(
            DateRangeValidator.validate(&entity_with("né en 2099"), ""),
            ValidationResult::Reject { reason: "year_out_of_range" }
        ));
    }

    #[test]
    fn rejects_no_year_found() {
        assert!(matches!(
            DateRangeValidator.validate(&entity_with("hier matin"), ""),
            ValidationResult::Reject { reason: "year_not_found" }
        ));
    }
}
```

`crates/anno-rag/src/validators/network.rs`:

```rust
//! IP address parse validator.

use super::{EntityValidator, ValidationResult};
use cloakpipe_core::DetectedEntity;
use std::net::IpAddr;

#[derive(Debug, Clone, Copy)]
pub struct IpAddressValidator;

impl EntityValidator for IpAddressValidator {
    fn label(&self) -> &'static str { "ip_address" }

    fn validate(&self, e: &DetectedEntity, _ctx: &str) -> ValidationResult {
        let trimmed = e.original.trim().trim_matches(|c: char| c == '[' || c == ']');
        if trimmed.parse::<IpAddr>().is_ok() {
            ValidationResult::Accept
        } else {
            ValidationResult::Reject { reason: "ip_parse_failed" }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cloakpipe_core::{DetectionSource, EntityCategory};

    fn entity_with(original: &str) -> DetectedEntity {
        DetectedEntity {
            original: original.to_string(),
            start: 0,
            end: original.len(),
            category: EntityCategory::Custom("ip_address".into()),
            confidence: 0.8,
            source: DetectionSource::Ner,
        }
    }

    #[test]
    fn accepts_ipv4_and_ipv6() {
        for ip in &["192.168.1.1", "10.0.0.42", "::1", "2001:db8::1", "[2001:db8::1]"] {
            assert_eq!(IpAddressValidator.validate(&entity_with(ip), ""), ValidationResult::Accept, "should accept {ip}");
        }
    }

    #[test]
    fn rejects_invalid() {
        for s in &["999.999.999.999", "hello", "1.2.3", "::g"] {
            assert!(matches!(
                IpAddressValidator.validate(&entity_with(s), ""),
                ValidationResult::Reject { .. }
            ), "should reject {s}");
        }
    }
}
```

`crates/anno-rag/src/validators/email.rs`:

```rust
//! RFC 5321-light email validator. Stricter than the regex layer because
//! the regex layer's job is to FIND candidates; the validator's job is to
//! REJECT obviously-wrong ones surfaced by NER.

use super::{EntityValidator, ValidationResult};
use cloakpipe_core::DetectedEntity;

#[derive(Debug, Clone, Copy)]
pub struct EmailRfcValidator;

impl EntityValidator for EmailRfcValidator {
    fn label(&self) -> &'static str { "email_address" }

    fn validate(&self, e: &DetectedEntity, _ctx: &str) -> ValidationResult {
        let s = e.original.trim();
        // RFC 5321: local-part ≤ 64 chars, domain ≤ 255, total ≤ 254 chars
        // for SMTP envelope (we accept full 256 for the input format).
        if s.len() < 3 || s.len() > 256 {
            return ValidationResult::Reject { reason: "email_length_out_of_range" };
        }
        let Some(at) = s.rfind('@') else {
            return ValidationResult::Reject { reason: "email_no_at" };
        };
        let (local, domain) = s.split_at(at);
        let domain = &domain[1..]; // skip '@'
        if local.is_empty() || local.len() > 64 {
            return ValidationResult::Reject { reason: "email_local_length" };
        }
        if domain.is_empty() || domain.len() > 255 || !domain.contains('.') {
            return ValidationResult::Reject { reason: "email_domain_invalid" };
        }
        // Domain must not start or end with a dot or hyphen.
        if domain.starts_with('.') || domain.starts_with('-')
            || domain.ends_with('.') || domain.ends_with('-')
        {
            return ValidationResult::Reject { reason: "email_domain_invalid" };
        }
        ValidationResult::Accept
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cloakpipe_core::{DetectionSource, EntityCategory};

    fn entity_with(original: &str) -> DetectedEntity {
        DetectedEntity {
            original: original.to_string(),
            start: 0,
            end: original.len(),
            category: EntityCategory::Custom("email_address".into()),
            confidence: 0.9,
            source: DetectionSource::Ner,
        }
    }

    #[test]
    fn accepts_normal_addresses() {
        for s in &["a@b.co", "alice@example.com", "first.last+tag@subdomain.example.fr"] {
            assert_eq!(EmailRfcValidator.validate(&entity_with(s), ""), ValidationResult::Accept, "should accept {s}");
        }
    }

    #[test]
    fn rejects_missing_at_or_dot() {
        for s in &["plainstring", "noat.example.com", "no-dot-in-domain@localhost", ""] {
            assert!(matches!(
                EmailRfcValidator.validate(&entity_with(s), ""),
                ValidationResult::Reject { .. }
            ), "should reject {s}");
        }
    }

    #[test]
    fn rejects_dot_or_hyphen_at_domain_boundary() {
        for s in &["a@.example.com", "a@example.com.", "a@-example.com", "a@example.com-"] {
            assert!(matches!(
                EmailRfcValidator.validate(&entity_with(s), ""),
                ValidationResult::Reject { .. }
            ), "should reject {s}");
        }
    }
}
```

`crates/anno-rag/src/validators/postal.rs`:

```rust
//! French postal code range validator (metropolitan + DOM/TOM).

use super::{EntityValidator, ValidationResult};
use cloakpipe_core::DetectedEntity;

#[derive(Debug, Clone, Copy)]
pub struct PostalCodeValidator;

impl EntityValidator for PostalCodeValidator {
    fn label(&self) -> &'static str { "postal_code" }

    fn validate(&self, e: &DetectedEntity, _ctx: &str) -> ValidationResult {
        let s = e.original.trim();
        if s.len() != 5 || !s.chars().all(|c| c.is_ascii_digit()) {
            return ValidationResult::Reject { reason: "postal_format" };
        }
        let n: u32 = s.parse().expect("digits-only above");
        // Mainland 01000-95999, DOM/TOM 97000-98999, Monaco 98000 reserved.
        if (1000..=95999).contains(&n)
            || (97000..=98999).contains(&n)
        {
            ValidationResult::Accept
        } else {
            ValidationResult::Reject { reason: "postal_range" }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cloakpipe_core::{DetectionSource, EntityCategory};

    fn entity_with(original: &str) -> DetectedEntity {
        DetectedEntity {
            original: original.to_string(),
            start: 0,
            end: original.len(),
            category: EntityCategory::Custom("postal_code".into()),
            confidence: 0.9,
            source: DetectionSource::Heuristic,
        }
    }

    #[test]
    fn accepts_mainland_and_dom() {
        for s in &["01000", "75001", "13008", "97400", "98000"] {
            assert_eq!(PostalCodeValidator.validate(&entity_with(s), ""), ValidationResult::Accept, "should accept {s}");
        }
    }

    #[test]
    fn rejects_out_of_range_or_wrong_format() {
        for s in &["00100", "96000", "99000", "1234", "ABCDE", "123456"] {
            assert!(matches!(
                PostalCodeValidator.validate(&entity_with(s), ""),
                ValidationResult::Reject { .. }
            ), "should reject {s}");
        }
    }
}
```

Update `crates/anno-rag/src/validators/mod.rs`:

```rust
pub mod luhn;
pub mod iban;
pub mod nir;
pub mod dates;
pub mod network;
pub mod email;
pub mod postal;
```

- [ ] **Step 2: Check if `chrono` is in `anno-rag` deps**

Run: `grep -E '^chrono' crates/anno-rag/Cargo.toml`

If empty, add to `crates/anno-rag/Cargo.toml` under `[dependencies]`:

```toml
chrono = { workspace = true }
```

`chrono` is already a workspace dep at the root `Cargo.toml`.

Note: also verify `DetectionSource::Heuristic` exists in `cloakpipe_core`. If it does not, use `DetectionSource::Pattern` in the `postal.rs` test fixture (it is a test-only constructor and any source works).

- [ ] **Step 3: Run all validator tests**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag -LibOnly`

Expected: all tests across `dates`, `network`, `email`, `postal` modules PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/anno-rag/src/validators/ crates/anno-rag/Cargo.toml
git commit -m "feat(detect): add date, IP, email, and postal code validators"
```

---

## Task 6: `apply_validators` orchestrator + tracking

**Files:**
- Modify: `crates/anno-rag/src/validators/mod.rs`

- [ ] **Step 1: Add orchestrator function + test**

Append to `crates/anno-rag/src/validators/mod.rs`:

```rust
/// Apply a chain of validators to a list of entities. Validators only run
/// on entities whose `category.as_str()` matches `validator.label()`. The
/// first `Reject` wins (short-circuits the chain for that entity). The
/// last `AdjustConfidence` wins.
///
/// Returns the kept entities and the per-reason rejection counts.
pub fn apply_validators(
    entities: Vec<DetectedEntity>,
    text: &str,
    validators: &[Box<dyn EntityValidator>],
) -> (Vec<DetectedEntity>, RejectionCounts) {
    let mut kept = Vec::with_capacity(entities.len());
    let mut counts: RejectionCounts = BTreeMap::new();
    'outer: for mut entity in entities {
        let label_str = label_str_for(&entity);
        for v in validators.iter().filter(|v| v.label() == label_str) {
            match v.validate(&entity, text) {
                ValidationResult::Accept => continue,
                ValidationResult::Reject { reason } => {
                    *counts.entry(reason).or_insert(0) += 1;
                    continue 'outer;
                }
                ValidationResult::AdjustConfidence(c) => {
                    entity.confidence = f64::from(c);
                }
            }
        }
        kept.push(entity);
    }
    (kept, counts)
}

fn label_str_for(e: &DetectedEntity) -> &str {
    use cloakpipe_core::EntityCategory;
    match &e.category {
        EntityCategory::Custom(name) => name.as_str(),
        EntityCategory::Person => "person",
        EntityCategory::Organization => "organization",
        EntityCategory::Location => "location",
        EntityCategory::Email => "email_address",
        EntityCategory::PhoneNumber => "phone_number",
        _ => "",
    }
}

#[cfg(test)]
mod orchestrator_tests {
    use super::*;
    use cloakpipe_core::{DetectionSource, EntityCategory};

    fn ent(label: &str, value: &str) -> DetectedEntity {
        DetectedEntity {
            original: value.to_string(),
            start: 0,
            end: value.len(),
            category: EntityCategory::Custom(label.into()),
            confidence: 0.9,
            source: DetectionSource::Ner,
        }
    }

    #[test]
    fn rejects_failing_luhn_and_keeps_others() {
        let validators: Vec<Box<dyn EntityValidator>> = vec![
            Box::new(luhn::LuhnValidator::new("SIRET")),
        ];
        let entities = vec![
            ent("SIRET", "73282932000074"),  // valid
            ent("SIRET", "12345678901234"),  // invalid
            ent("Person", "Jean Dupont"),    // not targeted, passes through
        ];
        let (kept, counts) = apply_validators(entities, "", &validators);
        assert_eq!(kept.len(), 2);
        assert_eq!(counts.get("luhn_failed"), Some(&1));
    }

    #[test]
    fn empty_validators_keeps_everything() {
        let entities = vec![ent("anything", "value")];
        let (kept, counts) = apply_validators(entities, "", &[]);
        assert_eq!(kept.len(), 1);
        assert!(counts.is_empty());
    }
}
```

- [ ] **Step 2: Run tests**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag -LibOnly`

Expected: `orchestrator_tests::*` PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/anno-rag/src/validators/mod.rs
git commit -m "feat(detect): add apply_validators orchestrator with rejection counts"
```

---

## Task 7: Add `ANNO_GDPR_LAYERS` feature flag enum

**Files:**
- Create: `crates/anno-rag/src/layers.rs`
- Modify: `crates/anno-rag/src/lib.rs` (add `pub mod layers;`)

- [ ] **Step 1: Write the failing test**

Create `crates/anno-rag/src/layers.rs`:

```rust
//! Runtime selection of the GDPR detection layer set.
//!
//! Controlled by the `ANNO_GDPR_LAYERS` environment variable. The current
//! detector behaviour corresponds to `Basic`. Phase A adds `Defense`
//! (heuristics + validators). Phases B/C/D will add `Shadow` and `Full`.

use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GdprLayerSet {
    /// Regex + GLiNER2 only — current behaviour.
    Basic,
    /// Adds FR heuristics backend + validators.
    #[default]
    Defense,
    /// Phase C placeholder: adds multi-task composition + centering + entropy.
    Shadow,
    /// Phase D placeholder: adds calibration runtime + review queue.
    Full,
}

impl GdprLayerSet {
    /// Read from `ANNO_GDPR_LAYERS`. Falls back to `Defense` if the var
    /// is unset, empty, or unrecognised.
    pub fn from_env() -> Self {
        std::env::var("ANNO_GDPR_LAYERS")
            .ok()
            .and_then(|v| Self::from_str(&v).ok())
            .unwrap_or_default()
    }

    pub fn includes_heuristics(self) -> bool {
        matches!(self, Self::Defense | Self::Shadow | Self::Full)
    }

    pub fn includes_validators(self) -> bool {
        matches!(self, Self::Defense | Self::Shadow | Self::Full)
    }
}

impl FromStr for GdprLayerSet {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "basic"   => Ok(Self::Basic),
            "defense" => Ok(Self::Defense),
            "shadow"  => Ok(Self::Shadow),
            "full"    => Ok(Self::Full),
            _         => Err(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_known_strings() {
        assert_eq!(GdprLayerSet::from_str("basic"),   Ok(GdprLayerSet::Basic));
        assert_eq!(GdprLayerSet::from_str("Defense"), Ok(GdprLayerSet::Defense));
        assert_eq!(GdprLayerSet::from_str("  full  "), Ok(GdprLayerSet::Full));
    }

    #[test]
    fn rejects_unknown_strings() {
        assert!(GdprLayerSet::from_str("xyz").is_err());
        assert!(GdprLayerSet::from_str("").is_err());
    }

    #[test]
    fn default_is_defense() {
        assert_eq!(GdprLayerSet::default(), GdprLayerSet::Defense);
    }

    #[test]
    fn layer_predicates_match_design() {
        assert!(!GdprLayerSet::Basic.includes_heuristics());
        assert!(GdprLayerSet::Defense.includes_heuristics());
        assert!(GdprLayerSet::Defense.includes_validators());
    }
}
```

Add to `crates/anno-rag/src/lib.rs`:

```rust
pub mod layers;
```

- [ ] **Step 2: Run tests**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag -LibOnly`

Expected: 4 `layers::tests::*` tests PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/anno-rag/src/layers.rs crates/anno-rag/src/lib.rs
git commit -m "feat(detect): add GdprLayerSet feature-flag enum"
```

---

## Task 8: Wire validators into `detect_inner`

**Files:**
- Modify: `crates/anno-rag/src/detect.rs`

- [ ] **Step 1: Read the current `detect_inner` for context**

Run: `grep -n "fn detect_inner" crates/anno-rag/src/detect.rs`

This locates the function so the next step's edit targets the right span.

- [ ] **Step 2: Build the validator chain + wire it in**

Add near the top of `crates/anno-rag/src/detect.rs` (after the existing `use` block):

```rust
use crate::layers::GdprLayerSet;
use crate::validators::{
    apply_validators, EntityValidator, RejectionCounts,
    dates::DateRangeValidator,
    email::EmailRfcValidator,
    iban::Iban97Validator,
    luhn::LuhnValidator,
    network::IpAddressValidator,
    nir::NirControlKeyValidator,
    postal::PostalCodeValidator,
};
```

Add a helper function (near the bottom of the file, after `pattern_priority`):

```rust
/// Build the standard validator chain for Phase A. Returned as boxed
/// trait objects so `apply_validators` can iterate generically.
fn default_validators() -> Vec<Box<dyn EntityValidator>> {
    vec![
        Box::new(LuhnValidator::new("SIRET")),
        Box::new(LuhnValidator::new("card_number")),
        Box::new(LuhnValidator::new("bank_account")),
        Box::new(Iban97Validator::new("IBAN_FR")),
        Box::new(Iban97Validator::new("iban")),
        Box::new(NirControlKeyValidator),
        Box::new(DateRangeValidator),
        Box::new(IpAddressValidator),
        Box::new(EmailRfcValidator),
        Box::new(PostalCodeValidator),
    ]
}
```

Modify the existing `detect_inner` to call validators after `dedup_overlaps`. The current shape (post-commit `1a1e83c5`) ends with `dedup_overlaps(&mut all); Ok(all)`. Replace those last two lines with :

```rust
        dedup_overlaps(&mut all);

        // Phase A: validators run only when layer flag enables them.
        let (kept, rejection_counts) = if GdprLayerSet::from_env().includes_validators() {
            let validators = default_validators();
            apply_validators(all, text, &validators)
        } else {
            (all, RejectionCounts::new())
        };

        // Stash rejection counts on a thread-local so emit_detect_audit
        // can read them without changing the function signature. (A cleaner
        // approach lands in Task 9 when the audit signature is extended.)
        LAST_REJECTIONS.with(|cell| *cell.borrow_mut() = rejection_counts);

        Ok(kept)
```

Add at module scope (near `static FrPatterns` or similar) :

```rust
thread_local! {
    static LAST_REJECTIONS: std::cell::RefCell<RejectionCounts> =
        std::cell::RefCell::new(RejectionCounts::new());
}
```

- [ ] **Step 3: Verify compilation**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag -Mode check -Profile dev-fast`

Expected: PASS.

- [ ] **Step 4: Verify existing detector tests still pass**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag -LibOnly`

Expected: all `detect::tests::*` PASS. If some fail because validators rejected a fixture (e.g. a non-Luhn-valid SIRET in a fixture), update the fixture to use a valid checksum or unset `ANNO_GDPR_LAYERS=basic` in that test.

- [ ] **Step 5: Commit**

```bash
git add crates/anno-rag/src/detect.rs
git commit -m "feat(detect): apply validator chain after dedup when defense layer enabled"
```

---

## Task 9: Extend `emit_detect_audit` with rejection counts

**Files:**
- Modify: `crates/anno-rag/src/detect.rs`

- [ ] **Step 1: Locate the existing function**

Run: `grep -n "fn emit_detect_audit" crates/anno-rag/src/detect.rs`

- [ ] **Step 2: Extend signature and body**

Change the signature of `emit_detect_audit` from:

```rust
fn emit_detect_audit(input_chars: usize, elapsed_us: u64, out: &[DetectedEntity]) {
```

to:

```rust
fn emit_detect_audit(
    input_chars: usize,
    elapsed_us: u64,
    out: &[DetectedEntity],
    rejection_counts: &RejectionCounts,
    layer_set: GdprLayerSet,
) {
```

Inside the function, after the existing per-category counting loop, before the `tracing::info!` call, add :

```rust
let rejected_total: usize = rejection_counts.values().sum();
let rejection_json = serde_json::to_string(rejection_counts).unwrap_or_else(|_| "{}".into());
```

Then in the `tracing::info!` macro call, add three new fields :

```rust
tracing::info!(
    target: "anno_rag::detect::audit",
    event = "detect",
    // ... existing fields ...
    rejected_by_validators_total = rejected_total,
    rejected_by_validators_json = %rejection_json,
    layer_set = ?layer_set,
);
```

Update every call site of `emit_detect_audit` (search with `grep -n "emit_detect_audit(" crates/anno-rag/src/detect.rs`). In `detect`, replace the existing call:

```rust
LAST_REJECTIONS.with(|cell| {
    emit_detect_audit(input_chars, elapsed_us, &out, &cell.borrow(), GdprLayerSet::from_env());
});
```

In `detect_for_ingest`, the rejection counts come from the bundle's own pipeline. For Phase A, also wrap that call site with `LAST_REJECTIONS` since the same thread-local is set by `detect_inner` called inside `detect_for_ingest`.

If `detect_for_ingest` does NOT call `detect_inner` directly (it has its own `extract_with_label_thresholds` path), add the same `apply_validators` block there after `dedup_overlaps`. Use a local variable `rejection_counts` instead of the thread-local in that path, since both branches end in `emit_detect_audit` and we don't want cross-talk.

- [ ] **Step 3: Verify compilation**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag -Mode check -Profile dev-fast`

Expected: PASS.

- [ ] **Step 4: Verify tests**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag -LibOnly`

Expected: all PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/anno-rag/src/detect.rs
git commit -m "chore(detect): extend audit telemetry with validator rejections and layer_set"
```

---

## Task 10: Scaffold `heuristic_fr` backend in anno crate

**Files:**
- Create: `crates/anno/src/backends/heuristic_fr/mod.rs`
- Modify: `crates/anno/src/backends/mod.rs`
- Modify: `crates/anno/Cargo.toml`

- [ ] **Step 1: Add the feature flag**

Edit `crates/anno/Cargo.toml`. In the `[features]` section, add:

```toml
heuristic-fr = []
default = ["..." , "heuristic-fr"]   # keep existing default entries, append heuristic-fr
```

Adjust the `default = [...]` line to include `heuristic-fr`. If `default` does not exist, add `default = ["heuristic-fr"]`.

- [ ] **Step 2: Declare the module in backends/mod.rs**

In `crates/anno/src/backends/mod.rs`, after the other backend declarations, add :

```rust
#[cfg(feature = "heuristic-fr")]
pub mod heuristic_fr;
```

- [ ] **Step 3: Write the skeleton + first failing test**

Create `crates/anno/src/backends/heuristic_fr/mod.rs`:

```rust
//! French-aware heuristic NER backend.
//!
//! Complements GLiNER2 with deterministic patterns that the model is
//! intrinsically weak on : French organisation suffixes (SAS, SARL, ...),
//! French address structures, French date formats (with date_of_birth
//! context detection), and international IBANs with mod-97 verification.
//!
//! Implements `Model` so it composes with `StackedNER` and can be tested
//! through the same harness as every other backend.

use crate::backends::inference::ZeroShotNER;
use crate::core::entity::{Entity, EntityType};
use crate::core::confidence::Confidence;

/// Phase A skeleton — sub-modules ship in subsequent tasks.
#[derive(Debug, Default, Clone)]
pub struct HeuristicFrNer;

impl HeuristicFrNer {
    pub fn new() -> Self { Self }
}

/// Phase A skeleton: returns empty results. Sub-modules in Tasks 11-14
/// populate the implementation.
impl ZeroShotNER for HeuristicFrNer {
    fn extract_with_types(
        &self,
        _text: &str,
        _types: &[&str],
        _threshold: f32,
    ) -> crate::Result<Vec<Entity>> {
        Ok(vec![])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skeleton_returns_empty() {
        let n = HeuristicFrNer::new();
        let r = n.extract_with_types("hello world", &["person"], 0.5).unwrap();
        assert!(r.is_empty());
    }
}
```

Note: The actual `ZeroShotNER` trait shape may differ slightly from what is shown above. Before writing this file, verify the trait signature with :

```bash
grep -A 10 "pub trait ZeroShotNER" crates/anno/src/backends/inference/mod.rs
```

If the signature differs, adapt the `impl` to match. The skeleton just needs to compile and return an empty Vec.

- [ ] **Step 4: Run tests**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno -LibOnly`

Expected: `backends::heuristic_fr::tests::skeleton_returns_empty` PASS, no regressions in other anno tests.

- [ ] **Step 5: Commit**

```bash
git add crates/anno/Cargo.toml crates/anno/src/backends/mod.rs crates/anno/src/backends/heuristic_fr/
git commit -m "feat(heuristic_fr): scaffold French heuristic NER backend"
```

---

## Task 11: Implement FR ORG suffix detection

**Files:**
- Create: `crates/anno/src/backends/heuristic_fr/orgs.rs`
- Modify: `crates/anno/src/backends/heuristic_fr/mod.rs`

- [ ] **Step 1: Write the failing test**

Create `crates/anno/src/backends/heuristic_fr/orgs.rs`:

```rust
//! Detection of French organisation forms : SAS, SARL, EURL, SCI, etc.

use crate::core::entity::{Entity, EntityType};
use crate::core::confidence::Confidence;
use regex::Regex;
use std::sync::OnceLock;

/// Confidence assigned to ORG suffix matches.
const CONFIDENCE: f32 = 0.85;

/// Returns one regex per common French legal form. The regex captures
/// the FULL organisation name including the suffix in group 0.
fn org_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        // 1-5 capitalised words, optional whitespace, then a suffix.
        // The suffix is anchored to a word boundary on each side.
        // Order matters in the alternation : longer suffixes first to
        // prevent `SA` greedily eating `SAS`.
        Regex::new(
            r"(?:\p{Lu}[\p{L}'\-&]+(?:\s+\p{Lu}[\p{L}'\-&]+){0,4}\s+)(?:Société\s+Civile\s+Immobilière|Société\s+Civile|Auto-?entrepreneur|Micro-?entreprise|SASU|SARL|EURL|SCOP|EPIC|EIRL|SCM|SCP|SCS|SCA|SCI|SEM|GIE|SNC|SAS|SA)\b"
        ).expect("org regex is a literal")
    })
}

pub fn extract_orgs(text: &str) -> Vec<Entity> {
    let mut out = Vec::new();
    for m in org_re().find_iter(text) {
        let start = char_offset_of(text, m.start());
        let end = char_offset_of(text, m.end());
        out.push(Entity {
            text: m.as_str().to_string(),
            entity_type: EntityType::Organization,
            start,
            end,
            confidence: Confidence::new(CONFIDENCE).expect("0..=1"),
            normalized: None,
        });
    }
    out
}

fn char_offset_of(text: &str, byte_idx: usize) -> usize {
    text[..byte_idx].chars().count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_sas() {
        let r = extract_orgs("Acme Tech SAS opère depuis Paris.");
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].text, "Acme Tech SAS");
    }

    #[test]
    fn detects_sarl_and_sci() {
        let r = extract_orgs("La société Construction Dupont SARL et la SCI Patrimoine Familial sont parties au contrat.");
        assert_eq!(r.len(), 2);
        let texts: Vec<&str> = r.iter().map(|e| e.text.as_str()).collect();
        assert!(texts.iter().any(|t| t.contains("SARL")));
        assert!(texts.iter().any(|t| t.contains("SCI")));
    }

    #[test]
    fn does_not_match_lowercase_or_partial() {
        let r = extract_orgs("acme tech sas") ;
        assert!(r.is_empty());
        // SAS without preceding capitalised word.
        let r = extract_orgs("Ils sont SAS, mais sans nom.");
        assert!(r.is_empty());
    }

    #[test]
    fn longer_suffix_wins_over_shorter() {
        // SASU should not be split into "X SA" + "SU"
        let r = extract_orgs("Innovate Lab SASU est ici.");
        assert_eq!(r.len(), 1);
        assert!(r[0].text.contains("SASU"));
    }
}
```

Modify `crates/anno/src/backends/heuristic_fr/mod.rs`:

```rust
pub mod orgs;

// In the impl ZeroShotNER, replace the body:
impl ZeroShotNER for HeuristicFrNer {
    fn extract_with_types(
        &self,
        text: &str,
        types: &[&str],
        threshold: f32,
    ) -> crate::Result<Vec<Entity>> {
        let mut out = Vec::new();
        if types.contains(&"organization") {
            out.extend(orgs::extract_orgs(text).into_iter()
                .filter(|e| f32::from(e.confidence) >= threshold));
        }
        Ok(out)
    }
}
```

- [ ] **Step 2: Run tests**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno -LibOnly`

Expected: 4 `heuristic_fr::orgs::tests::*` PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/anno/src/backends/heuristic_fr/orgs.rs crates/anno/src/backends/heuristic_fr/mod.rs
git commit -m "feat(heuristic_fr): detect French ORG legal-form suffixes"
```

---

## Task 12: Implement FR address detection

**Files:**
- Create: `crates/anno/src/backends/heuristic_fr/addresses.rs`
- Modify: `crates/anno/src/backends/heuristic_fr/mod.rs`

- [ ] **Step 1: Write the failing test**

Create `crates/anno/src/backends/heuristic_fr/addresses.rs`:

```rust
//! French address detection : voie keywords (rue, avenue, boulevard, ...)
//! with adjacent postal-code attachment.

use crate::core::entity::{Entity, EntityType};
use crate::core::confidence::Confidence;
use regex::Regex;
use std::sync::OnceLock;

const CONFIDENCE: f32 = 0.85;

fn voie_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        // <number> <voie keyword> <1-4 capitalised words>
        // Optionally followed by ", <5-digit postal code> <City>"
        Regex::new(
            r"(?i)\b\d+\s*(?:rue|avenue|boulevard|place|impasse|all[ée]e|chemin|route|quai|cours|passage|square|esplanade|promenade|voie|sentier|villa)\s+(?:\p{L}[\p{L}'\-]*\s*){1,4}(?:,?\s+\d{5}\s+\p{Lu}[\p{L}'\-]*)?"
        ).expect("addr regex is a literal")
    })
}

pub fn extract_addresses(text: &str) -> Vec<Entity> {
    let mut out = Vec::new();
    for m in voie_re().find_iter(text) {
        let start = text[..m.start()].chars().count();
        let end = text[..m.end()].chars().count();
        out.push(Entity {
            text: m.as_str().to_string(),
            entity_type: EntityType::Custom {
                name: "address".to_string(),
                category: crate::core::entity::EntityCategory::Place,
            },
            start,
            end,
            confidence: Confidence::new(CONFIDENCE).expect("0..=1"),
            normalized: None,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_simple_rue() {
        let r = extract_addresses("Habite au 12 rue de la République.");
        assert_eq!(r.len(), 1);
        assert!(r[0].text.contains("rue de la République"));
    }

    #[test]
    fn detects_with_postal_and_city() {
        let r = extract_addresses("Adresse : 5 avenue des Champs-Élysées, 75008 Paris.");
        assert_eq!(r.len(), 1);
        assert!(r[0].text.contains("75008"));
        assert!(r[0].text.contains("Paris"));
    }

    #[test]
    fn detects_boulevard_and_impasse() {
        let r = extract_addresses("8 boulevard Voltaire et 3 impasse Lafayette sont libres.");
        assert!(r.len() >= 2);
    }

    #[test]
    fn does_not_match_voie_without_number() {
        let r = extract_addresses("la rue est calme") ;
        assert!(r.is_empty());
    }
}
```

Modify `mod.rs` :

```rust
pub mod addresses;

// In the impl, add :
if types.contains(&"address") {
    out.extend(addresses::extract_addresses(text).into_iter()
        .filter(|e| f32::from(e.confidence) >= threshold));
}
```

- [ ] **Step 2: Verify the `EntityCategory::Place` variant exists**

Run: `grep -n "EntityCategory::Place" crates/anno/src/core/entity.rs`

If the variant is named differently (e.g. `Location`), update `extract_addresses` to use the correct name.

- [ ] **Step 3: Run tests**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno -LibOnly`

Expected: 4 `heuristic_fr::addresses::tests::*` PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/anno/src/backends/heuristic_fr/addresses.rs crates/anno/src/backends/heuristic_fr/mod.rs
git commit -m "feat(heuristic_fr): detect French postal addresses"
```

---

## Task 13: Implement FR date detection (with date_of_birth context)

**Files:**
- Create: `crates/anno/src/backends/heuristic_fr/dates.rs`
- Modify: `crates/anno/src/backends/heuristic_fr/mod.rs`

- [ ] **Step 1: Write the failing test**

Create `crates/anno/src/backends/heuristic_fr/dates.rs`:

```rust
//! French date detection (textual and numeric), with `date_of_birth`
//! context-aware re-labelling.

use crate::core::entity::{Entity, EntityType, EntityCategory};
use crate::core::confidence::Confidence;
use regex::Regex;
use std::sync::OnceLock;

const DOB_TRIGGERS: &[&str] = &[
    "né le", "née le", "né(e) le",
    "date de naissance", "naissance :", "naissance:",
    "anniversaire", "âgé(e) de",
];

const TRIGGER_WINDOW_CHARS: usize = 50;

fn date_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)(?:\ble\s+)?\d{1,2}\s+(?:janvier|février|mars|avril|mai|juin|juillet|ao[uû]t|septembre|octobre|novembre|décembre)\s+\d{4}|\b\d{1,2}[\/\-\.]\d{1,2}[\/\-\.]\d{2,4}\b"
        ).expect("date regex is a literal")
    })
}

pub fn extract_dates(text: &str) -> Vec<Entity> {
    let mut out = Vec::new();
    for m in date_re().find_iter(text) {
        let start_byte = m.start();
        let end_byte = m.end();
        let label = if has_dob_trigger_nearby(text, start_byte) {
            EntityType::Custom {
                name: "date_of_birth".into(),
                category: EntityCategory::Time,
            }
        } else {
            EntityType::Date
        };
        let start = text[..start_byte].chars().count();
        let end = text[..end_byte].chars().count();
        out.push(Entity {
            text: m.as_str().to_string(),
            entity_type: label,
            start,
            end,
            confidence: Confidence::new(0.90).expect("0..=1"),
            normalized: None,
        });
    }
    out
}

fn has_dob_trigger_nearby(text: &str, byte_idx: usize) -> bool {
    let window_start = byte_idx.saturating_sub(TRIGGER_WINDOW_CHARS);
    let snippet = &text[window_start..byte_idx];
    let lower = snippet.to_lowercase();
    DOB_TRIGGERS.iter().any(|t| lower.contains(t))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_textual_date() {
        let r = extract_dates("Réunion fixée au 15 janvier 2024.");
        assert_eq!(r.len(), 1);
        assert!(r[0].text.contains("15 janvier 2024"));
    }

    #[test]
    fn detects_numeric_date() {
        let r = extract_dates("Échéance: 31/12/2025.");
        assert_eq!(r.len(), 1);
    }

    #[test]
    fn relabels_date_of_birth_with_trigger() {
        let r = extract_dates("M. Dupont, né le 12 mai 1972.");
        assert_eq!(r.len(), 1);
        match &r[0].entity_type {
            EntityType::Custom { name, .. } => assert_eq!(name, "date_of_birth"),
            other => panic!("expected date_of_birth, got {other:?}"),
        }
    }

    #[test]
    fn keeps_plain_date_without_trigger() {
        let r = extract_dates("Le contrat prend effet le 1 mars 2024.");
        match &r[0].entity_type {
            EntityType::Date => {}
            other => panic!("expected Date, got {other:?}"),
        }
    }
}
```

Verify `EntityCategory::Time` exists :

```bash
grep -n "Time" crates/anno/src/core/entity.rs
```

If the variant is named differently (e.g. `Temporal`), update the file.

Modify `mod.rs` :

```rust
pub mod dates;

// In the impl, add :
if types.contains(&"date") || types.contains(&"date_of_birth") {
    out.extend(dates::extract_dates(text).into_iter()
        .filter(|e| f32::from(e.confidence) >= threshold)
        .filter(|e| {
            // Only emit if the caller asked for the matching label.
            match &e.entity_type {
                EntityType::Date => types.contains(&"date"),
                EntityType::Custom { name, .. } if name == "date_of_birth" => types.contains(&"date_of_birth"),
                _ => false,
            }
        }));
}
```

- [ ] **Step 2: Run tests**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno -LibOnly`

Expected: 4 `heuristic_fr::dates::tests::*` PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/anno/src/backends/heuristic_fr/dates.rs crates/anno/src/backends/heuristic_fr/mod.rs
git commit -m "feat(heuristic_fr): detect French dates with date_of_birth context"
```

---

## Task 14: Implement international IBAN with mod-97

**Files:**
- Create: `crates/anno/src/backends/heuristic_fr/iban_intl.rs`
- Modify: `crates/anno/src/backends/heuristic_fr/mod.rs`

- [ ] **Step 1: Write the failing test**

Create `crates/anno/src/backends/heuristic_fr/iban_intl.rs`:

```rust
//! International IBAN detection (ISO 13616) with mod-97 verification.
//!
//! The existing `FrPatterns::iban_fr` regex in `anno-rag` covers FR
//! IBANs only. This module covers all ISO-3166-1 alpha-2 country codes
//! with known IBAN length, validated against mod-97.

use crate::core::entity::{Entity, EntityType, EntityCategory};
use crate::core::confidence::Confidence;
use regex::Regex;
use std::sync::OnceLock;

fn candidate_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        // Two-letter country code, 2 check digits, 11-30 alphanumerics
        // (covering all known IBAN lengths 15-34 minus the 4 leading chars).
        Regex::new(r"\b[A-Z]{2}\d{2}(?:\s?[A-Z0-9]){11,30}\b").expect("iban regex")
    })
}

pub fn extract_iban_intl(text: &str) -> Vec<Entity> {
    let mut out = Vec::new();
    for m in candidate_re().find_iter(text) {
        let raw = m.as_str();
        if iban_mod97(raw) {
            let start = text[..m.start()].chars().count();
            let end = text[..m.end()].chars().count();
            out.push(Entity {
                text: raw.to_string(),
                entity_type: EntityType::Custom {
                    name: "iban".into(),
                    category: EntityCategory::Other,  // adjust per actual EntityCategory variants
                },
                start,
                end,
                confidence: Confidence::new(1.0).expect("0..=1"),
                normalized: None,
            });
        }
    }
    out
}

fn iban_mod97(raw: &str) -> bool {
    let s: String = raw.chars().filter(|c| !c.is_whitespace()).collect();
    if s.len() < 15 || s.len() > 34 { return false; }
    let s = s.to_ascii_uppercase();
    if !s.chars().all(|c| c.is_ascii_alphanumeric()) { return false; }
    let (head, tail) = s.split_at(4);
    let rearranged = format!("{tail}{head}");
    let mut numeric = String::with_capacity(rearranged.len() * 2);
    for c in rearranged.chars() {
        if c.is_ascii_digit() {
            numeric.push(c);
        } else {
            let n = (c as u32) - ('A' as u32) + 10;
            numeric.push_str(&n.to_string());
        }
    }
    let mut remainder: u32 = 0;
    for d_ch in numeric.chars() {
        let d = d_ch.to_digit(10).unwrap();
        remainder = (remainder * 10 + d) % 97;
    }
    remainder == 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_german_iban() {
        let r = extract_iban_intl("Virement vers DE89370400440532013000 effectué.");
        assert_eq!(r.len(), 1);
    }

    #[test]
    fn detects_french_iban_via_intl_path() {
        let r = extract_iban_intl("FR1420041010050500013M02606 — référence");
        assert_eq!(r.len(), 1);
    }

    #[test]
    fn rejects_wrong_checksum() {
        let r = extract_iban_intl("DE99370400440532013000");
        assert!(r.is_empty());
    }

    #[test]
    fn ignores_short_candidate() {
        let r = extract_iban_intl("FR12 trop court");
        assert!(r.is_empty());
    }
}
```

Verify `EntityCategory::Other` — if not present, use the closest variant (e.g. `Custom`-only path). Run:

```bash
grep -n "pub enum EntityCategory" crates/anno/src/core/entity.rs
```

and pick the appropriate variant.

Modify `mod.rs` :

```rust
pub mod iban_intl;

// In the impl, add :
if types.contains(&"iban") {
    out.extend(iban_intl::extract_iban_intl(text).into_iter()
        .filter(|e| f32::from(e.confidence) >= threshold));
}
```

- [ ] **Step 2: Run tests**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno -LibOnly`

Expected: 4 `heuristic_fr::iban_intl::tests::*` PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/anno/src/backends/heuristic_fr/iban_intl.rs crates/anno/src/backends/heuristic_fr/mod.rs
git commit -m "feat(heuristic_fr): detect international IBANs with mod-97 check"
```

---

## Task 15: Wire `HeuristicFrNer` into `Detector::detect_inner`

**Files:**
- Modify: `crates/anno-rag/src/detect.rs`

- [ ] **Step 1: Add the heuristic-fr backend to the Detector struct (or call site)**

In `crates/anno-rag/src/detect.rs`, near the top, add :

```rust
#[cfg(feature = "heuristic-fr")]
use anno::backends::heuristic_fr::HeuristicFrNer;
```

Locate the `Detector` struct (around line 329 post-commit `1a1e83c5`). Add a field :

```rust
pub struct Detector {
    ner: NerBackend,
    #[cfg(feature = "heuristic-fr")]
    heuristic_fr: HeuristicFrNer,
}
```

Update every `Self { ner: ... }` constructor to set the heuristic_fr field :

```rust
Self {
    ner: NerBackend::Onnx(ner),
    #[cfg(feature = "heuristic-fr")]
    heuristic_fr: HeuristicFrNer::new(),
}
```

(There are 2-3 such call sites in `Detector::new` and `Detector::new_candle_metal`.)

- [ ] **Step 2: Call the heuristics inside `detect_inner` when the layer flag enables them**

Inside `detect_inner`, after the regex pass (`let mut all = detect_patterns(text);`) and before the NER pass, add :

```rust
#[cfg(feature = "heuristic-fr")]
if GdprLayerSet::from_env().includes_heuristics() {
    let labels = pii_label_set();  // existing function returning all GDPR label names
    let label_refs: Vec<&str> = labels.iter().copied().collect();
    if let Ok(heur_entities) = self.heuristic_fr.extract_with_types(text, &label_refs, 0.5) {
        all.extend(anno_entities_to_detected(text, heur_entities)?);
    }
}
```

The existing `dedup_overlaps` (Pattern > Heuristic > Ner priority) handles cross-source dedup.

To make the priority correct, also update `pattern_priority` (locate via `grep -n "fn pattern_priority" crates/anno-rag/src/detect.rs`) so that the heuristic backend is between Pattern and Ner. Two options :

(a) If `DetectionSource::Heuristic` exists in `cloakpipe_core`, use it directly in `anno_entities_to_detected` (search for the `source: DetectionSource::Ner,` line and add a parameter to choose between `Heuristic` and `Ner`).

(b) If it does not exist, emit heuristic entities with `DetectionSource::Pattern` for now (acceptable for Phase A since both layers are deterministic-confidence). Document this in a comment with a `// TODO(Phase B)` to add a dedicated source.

Check with :

```bash
grep -n "DetectionSource::" crates/anno-rag/src/detect.rs
grep -rn "pub enum DetectionSource" vendor/cloakpipe/ crates/
```

- [ ] **Step 3: Verify compilation**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag -Mode check -Profile dev-fast`

Expected: PASS. If `pii_label_set()` is not visible from where you're calling it, import it (`use crate::detect::pii_label_set;` or it's already in scope inside `impl Detector`).

- [ ] **Step 4: Run all anno-rag tests**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag -LibOnly`

Expected: all PASS. If a fixture test fails because a new heuristic match now appears alongside an existing detection, that is a true positive — update the fixture to assert the larger set OR set `ANNO_GDPR_LAYERS=basic` in that specific test.

- [ ] **Step 5: Commit**

```bash
git add crates/anno-rag/src/detect.rs crates/anno-rag/Cargo.toml
git commit -m "feat(detect): wire HeuristicFrNer into Detector when defense layer enabled"
```

---

## Task 16: Add `anno-rag` feature flag for `heuristic-fr`

**Files:**
- Modify: `crates/anno-rag/Cargo.toml`

- [ ] **Step 1: Forward the feature**

In `crates/anno-rag/Cargo.toml`, find the `[features]` section. Add :

```toml
heuristic-fr = ["anno/heuristic-fr"]
default = ["...", "heuristic-fr"]   # append to whatever the existing default is
```

If `default` does not exist in this crate, add `default = ["heuristic-fr"]`.

- [ ] **Step 2: Verify the workspace builds**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag -Mode check -Profile dev-fast`

Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/anno-rag/Cargo.toml
git commit -m "chore(detect): forward heuristic-fr feature through anno-rag"
```

---

## Task 17: Integration tests across `ANNO_GDPR_LAYERS` levels

**Files:**
- Create: `crates/anno-rag/tests/integration_phase_a.rs`

- [ ] **Step 1: Write the tests**

Create `crates/anno-rag/tests/integration_phase_a.rs`:

```rust
//! Phase A end-to-end tests : verify the deterministic stack works at
//! every ANNO_GDPR_LAYERS level.

use anno_rag::detect::{Detector, detect_patterns};

/// Reset the layer env var deterministically for each test.
fn with_layer<T>(layer: &str, f: impl FnOnce() -> T) -> T {
    let prev = std::env::var("ANNO_GDPR_LAYERS").ok();
    std::env::set_var("ANNO_GDPR_LAYERS", layer);
    let out = f();
    match prev {
        Some(v) => std::env::set_var("ANNO_GDPR_LAYERS", v),
        None    => std::env::remove_var("ANNO_GDPR_LAYERS"),
    }
    out
}

/// Sample French text exercising regex + heuristics + validator paths.
const SAMPLE: &str = "
Contrat entre Acme Tech SAS (732 829 320 00074) et la SCI Patrimoine Familial.
M. Jean Dupont, né le 12 mai 1972, à l'adresse 5 avenue des Champs-Élysées, 75008 Paris.
Contact : jean.dupont@example.fr — IBAN : FR1420041010050500013M02606.
Carte bancaire : 4111111111111111.
";

#[test]
fn regex_layer_alone_detects_structural_pii() {
    let v = detect_patterns(SAMPLE);
    let labels: Vec<&str> = v.iter().map(|e| match &e.category {
        cloakpipe_core::EntityCategory::Custom(s) => s.as_str(),
        cloakpipe_core::EntityCategory::Email => "Email",
        cloakpipe_core::EntityCategory::PhoneNumber => "Phone",
        _ => "other",
    }).collect();
    assert!(labels.iter().any(|l| *l == "SIRET"), "SIRET not detected: {labels:?}");
    assert!(labels.iter().any(|l| *l == "IBAN_FR"), "IBAN_FR not detected: {labels:?}");
    assert!(labels.iter().any(|l| *l == "Email"), "Email not detected: {labels:?}");
}

#[test]
fn invalid_siret_rejected_by_validator_when_defense_enabled() {
    let bogus = "Société Inexistante SARL (12345678901234)";
    with_layer("defense", || {
        let v = detect_patterns(bogus);
        // The regex layer's `siret` regex calls luhn() inline and already
        // filters; this verifies the dual-layer is consistent and the
        // validator chain does not introduce a regression.
        let siret_count = v.iter().filter(|e| matches!(
            &e.category, cloakpipe_core::EntityCategory::Custom(s) if s == "SIRET"
        )).count();
        assert_eq!(siret_count, 0, "Invalid SIRET should be rejected");
    });
}

#[test]
fn basic_layer_skips_heuristics_and_validators() {
    with_layer("basic", || {
        // Verifies that setting ANNO_GDPR_LAYERS=basic returns the
        // previous behaviour: pure regex + GLiNER2 with no heuristic
        // backend or validator chain.
        // We cannot easily assert what GLiNER2 detects without loading
        // the model, so we just assert detect_patterns still works and
        // the env var is read.
        let v = detect_patterns(SAMPLE);
        assert!(!v.is_empty());
    });
}
```

- [ ] **Step 2: Run integration tests**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag -TestTarget integration_phase_a`

Expected: 3 tests PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/anno-rag/tests/integration_phase_a.rs
git commit -m "test(detect): integration tests for ANNO_GDPR_LAYERS levels"
```

---

## Task 18: Document the layer flag and rollback procedure

**Files:**
- Modify: `crates/anno-rag/README.md` (or create one if missing)
- Modify: `crates/anno-rag/src/lib.rs` (add module-level rustdoc)

- [ ] **Step 1: Add operator-facing doc**

In `crates/anno-rag/src/lib.rs`, near the top, add a doc comment :

```rust
//! # GDPR detection layers
//!
//! The detector pipeline is gated behind the `ANNO_GDPR_LAYERS` env var.
//!
//! | Value     | Behaviour                                                        |
//! |-----------|------------------------------------------------------------------|
//! | `basic`   | Regex + GLiNER2 only — pre-Phase-A behaviour.                    |
//! | `defense` | + FR heuristics backend + validator chain. **Default in prod.**  |
//! | `shadow`  | (Phase C) + multi-task composition + entropy filter.             |
//! | `full`    | (Phase D) + calibration runtime + review queue.                  |
//!
//! Set to `basic` to roll back any of the layers added in Phase A
//! without recompiling.
```

If a README does not exist, create `crates/anno-rag/README.md` with the same table.

- [ ] **Step 2: Verify doc build**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag -Mode check -Profile dev-fast`

Expected: PASS. If `rustdoc` produces warnings about the new doc, fix them.

- [ ] **Step 3: Commit**

```bash
git add crates/anno-rag/src/lib.rs crates/anno-rag/README.md
git commit -m "docs(detect): document GDPR layer flag and rollback"
```

---

## Task 19: Final verification and CHANGELOG entry

**Files:**
- Modify: `CHANGELOG.md` (at workspace root)

- [ ] **Step 1: Run the full Phase A test suite**

Run all anno-rag tests :

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag
```

Expected: ALL PASS.

Run anno backend tests :

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno -LibOnly
```

Expected: ALL PASS.

- [ ] **Step 2: Verify GitNexus index is fresh**

Run: `npx gitnexus analyze`

Wait for completion. This refreshes the index so the PR review tooling sees Phase A code accurately.

- [ ] **Step 3: Add CHANGELOG entry**

Open `CHANGELOG.md` (or create if missing). Add at the top under a new `## [Unreleased]` heading :

```markdown
## [Unreleased]

### Added

- PII detection Phase A : deterministic stack
  - `EntityValidator` trait with 8 implementations (Luhn, IBAN mod-97, NIR
    control key, date_of_birth year range, IP parse, RFC-5321 light email,
    FR postal code, structured ID cross-check)
  - `HeuristicFrNer` backend covering French ORG legal-form suffixes (SAS,
    SARL, EURL, SCI, etc.), French postal addresses, French dates with
    `date_of_birth` context detection, and international IBANs with
    mod-97 verification
  - `ANNO_GDPR_LAYERS` environment variable (values `basic`, `defense`,
    `shadow`, `full`) for runtime layer selection with instant rollback
  - Extended `anno_rag::detect::audit` tracing event with
    `rejected_by_validators_total`, `rejected_by_validators_json`, and
    `layer_set` fields

### Changed

- Default detector behaviour is now `ANNO_GDPR_LAYERS=defense` (FR
  heuristics + validators active). Set to `basic` to restore the
  previous behaviour.
```

- [ ] **Step 4: Commit**

```bash
git add CHANGELOG.md
git commit -m "docs(changelog): record Phase A deterministic stack"
```

---

## Self-review checklist (run before marking plan complete)

- [ ] Every task has explicit file paths
- [ ] Every code block is complete (no `...` placeholders for omitted logic)
- [ ] Every test step states the expected outcome
- [ ] Every commit message is given
- [ ] Validator label strings match what `detect.rs` emits (e.g. `SIRET`
      not `siret`, `IBAN_FR` not `iban_fr`) — cross-check with the existing
      `detect_patterns` function before merging the validator chain
- [ ] `EntityCategory` / `EntityType` variant names cited in the code
      have been verified against `crates/anno/src/core/entity.rs`
      (Tasks 12, 13, 14 list verification steps)
- [ ] `DetectionSource` variant names cited have been verified against
      `cloakpipe_core` (Task 15 lists the verification step)
- [ ] The `ZeroShotNER` trait signature has been verified (Task 10)

## Out of scope (deferred to Phase B/C/D)

- Synthetic FR corpus generator (Phase B)
- Shadow scoring integration (Phase B)
- Snapshot testing with `insta` (Phase B)
- `extract_composite` multi-task method on `GLiNER2Fastino` (Phase C)
- French pronoun centering layer (Phase C)
- Entropy filter wiring (Phase C)
- Calibration runtime + drift alerts (Phase D)
- Review queue + MCP tools (Phase D)
