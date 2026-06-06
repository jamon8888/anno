# DD Tabular Template Library — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship a bilingual (FR/EN), multi-jurisdiction due-diligence template library (17 templates: 15 contract-abstraction grids + 2 transversal registers) for `anno-rag-tabular`, plus the security wiring that makes extraction local-first by default with a mandatory PII gate on any remote call.

**Architecture:** TOML presets in `crates/anno-rag-tabular/src/templates/`, registered in `schema/template.rs` via `include_str!` + `match`. Each grid ends with a standard evidence triplet (`source_reference` verbatim, `key_risk_flag` boolean, `risk_note` text). Security: replace the bare `default_from_env()` (Anthropic-only, no PII gate) in CLI `cmd_extract` and the MCP extract handlers with a `RoutingLlmClient` that runs `LocalTabularClient` (GLiNER2/Fastino) first and only calls a remote fallback when explicitly opted in AND `fallback_prompt_is_safe()` passes.

**Tech Stack:** Rust, `anno-rag-tabular` (`schema::template::Template`, `schema::CellType`, `llm::{routing::RoutingLlmClient, local::client::{LocalTabularClient, Gliner2EntityExtractor}, default_from_env}`), TOML, `toml` crate, `tokio`/`async-trait`. Local dev: `scripts/test-local.ps1 -Package anno-rag-tabular`.

**Spec:** `docs/superpowers/specs/2026-06-06-anno-tabular-dd-templates-design.md`

---

## File Structure

**Create (templates):**
- `crates/anno-rag-tabular/src/templates/spa-v1.toml`
- `crates/anno-rag-tabular/src/templates/sha-v1.toml`
- `crates/anno-rag-tabular/src/templates/jva-v1.toml`
- `crates/anno-rag-tabular/src/templates/senior-facilities-v1.toml`
- `crates/anno-rag-tabular/src/templates/security-package-v1.toml`
- `crates/anno-rag-tabular/src/templates/intercreditor-v1.toml`
- `crates/anno-rag-tabular/src/templates/material-commercial-v1.toml`
- `crates/anno-rag-tabular/src/templates/commercial-lease-v1.toml`
- `crates/anno-rag-tabular/src/templates/employment-exec-v1.toml`
- `crates/anno-rag-tabular/src/templates/ip-portfolio-v1.toml`
- `crates/anno-rag-tabular/src/templates/litigation-v1.toml`
- `crates/anno-rag-tabular/src/templates/insurance-v1.toml`
- `crates/anno-rag-tabular/src/templates/data-protection-v1.toml`
- `crates/anno-rag-tabular/src/templates/debt-finance-v1.toml`
- `crates/anno-rag-tabular/src/templates/corporate-captable-v1.toml`
- `crates/anno-rag-tabular/src/templates/data-room-index-v1.toml`
- `crates/anno-rag-tabular/src/templates/red-flags-register-v1.toml`

**Modify:**
- `crates/anno-rag-tabular/src/schema/template.rs` — register 17 templates in `builtin()` + `list_builtin()`.
- `crates/anno-rag-tabular/src/llm/mod.rs` — add `routing_client_from_env(allow_remote: bool)` factory.
- `crates/anno-rag-tabular/Cargo.toml` — ensure `gliner2` feature reaches the local extractor in non-test builds.
- `crates/anno-rag-bin/src/review.rs` — use the routing factory; add `--allow-remote-llm` flag.
- `crates/anno-rag-mcp/src/lib.rs` — use the routing factory at the two `default_from_env()` call sites (~2752, ~4214).
- `crates/anno-rag/README.md` — document the DD library + local-first default.

**Test:**
- `crates/anno-rag-tabular/tests/dd_templates.rs` — load-all, enum-consistency, evidence-triplet, SPA snapshot.
- `crates/anno-rag-tabular/tests/security_routing.rs` — remote-not-taken-without-opt-in.

---

## Conventions every template TOML follows

- `version = "1.0.0"`, `vertical = "legal-intl"`.
- Prompts in English. Column keys `snake_case`.
- Every **abstraction grid** ends with exactly these three columns (the evidence triplet):

```toml
[[column]]
name   = "source_reference"
prompt = "Quote verbatim the single most material clause supporting this row."
type   = "verbatim"

[[column]]
name   = "key_risk_flag"
prompt = "Does this document warrant a red flag for the deal? true/false."
type   = "boolean"

[[column]]
name   = "risk_note"
prompt = "One sentence explaining the risk, if any. Otherwise 'none'."
type   = "text"
```

- Shared enum option lists (use verbatim where referenced):
  - `governing_law` → `["FR","DE","UK","US","CH","BE","LU","NL","ES","IT","SE","OTHER"]`
  - `currency` → `["EUR","USD","GBP","CHF","OTHER"]`
  - `workstream` → `["corporate","commercial","employment","ip_it","real_estate","litigation","regulatory","finance_debt","tax","data_protection","insurance","environmental"]`
  - `severity` → `["critical","high","medium","low","info"]`

---

## Task 1: Test harness + shared-enum constants

**Files:**
- Create: `crates/anno-rag-tabular/tests/dd_templates.rs`
- Reference: `crates/anno-rag-tabular/src/schema/template.rs:102` (`builtin`), `:117` (`list_builtin`)

- [ ] **Step 1: Write the failing load-all test**

```rust
// crates/anno-rag-tabular/tests/dd_templates.rs
use anno_rag_tabular::schema::template::Template;

const DD_TEMPLATES: &[&str] = &[
    "spa-v1","sha-v1","jva-v1","senior-facilities-v1","security-package-v1",
    "intercreditor-v1","material-commercial-v1","commercial-lease-v1",
    "employment-exec-v1","ip-portfolio-v1","litigation-v1","insurance-v1",
    "data-protection-v1","debt-finance-v1","corporate-captable-v1",
    "data-room-index-v1","red-flags-register-v1",
];

#[test]
fn all_dd_templates_load() {
    for id in DD_TEMPLATES {
        let t = Template::builtin(id).unwrap_or_else(|e| panic!("template {id} must load: {e}"));
        assert_eq!(t.id, *id, "template id mismatch in {id}");
        assert_eq!(t.vertical, "legal-intl", "{id} must be legal-intl");
        assert!(!t.columns.is_empty(), "{id} has no columns");
    }
}
```

- [ ] **Step 2: Run it to verify it fails**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag-tabular -- --test dd_templates all_dd_templates_load`
Expected: FAIL — `template spa-v1 must load: TemplateNotFound`.

- [ ] **Step 3: Commit the failing test**

```bash
git add crates/anno-rag-tabular/tests/dd_templates.rs
git commit -m "test(tabular): failing load-all test for DD template library"
```

---

## Task 2: M&A core grids — spa-v1, sha-v1

**Files:**
- Create: `crates/anno-rag-tabular/src/templates/spa-v1.toml`
- Create: `crates/anno-rag-tabular/src/templates/sha-v1.toml`
- Modify: `crates/anno-rag-tabular/src/schema/template.rs`

- [ ] **Step 1: Write `spa-v1.toml`**

```toml
id = "spa-v1"
name = "Share Purchase Agreement (DD)"
version = "1.0.0"
description = "SPA abstraction — price, conditions precedent, R&W, indemnities, MAC, restrictive covenants."
vertical = "legal-intl"

[[column]]
name = "target"
prompt = "Name of the target entity (full legal name and corporate form)."
type = "text"

[[column]]
name = "seller"
prompt = "Name(s) of the seller(s)."
type = "text"

[[column]]
name = "buyer"
prompt = "Name(s) of the buyer(s)."
type = "text"

[[column]]
name = "signing_date"
prompt = "Signing date. Return ISO 8601 (YYYY-MM-DD)."
type = "date"

[[column]]
name = "completion_date"
prompt = "Completion/closing date. Return ISO 8601 (YYYY-MM-DD)."
type = "date"

[[column]]
name = "purchase_price_amount"
prompt = "Total purchase price as a number, no currency symbol."
type = "number"

[[column]]
name = "purchase_price_currency"
prompt = "Currency of the purchase price."
type = "enum"
options = ["EUR","USD","GBP","CHF","OTHER"]

[[column]]
name = "price_mechanism"
prompt = "Price mechanism."
type = "enum"
options = ["locked_box","completion_accounts","fixed"]

[[column]]
name = "earn_out"
prompt = "Is there an earn-out? true/false."
type = "boolean"

[[column]]
name = "earn_out_terms"
prompt = "If earn-out present, summarise its terms verbatim. Otherwise 'none'."
type = "verbatim"

[[column]]
name = "conditions_precedent"
prompt = "List the conditions precedent to completion, verbatim."
type = "verbatim"

[[column]]
name = "mac_clause"
prompt = "Is there a Material Adverse Change clause? true/false."
type = "boolean"

[[column]]
name = "mac_terms"
prompt = "If MAC present, quote the clause verbatim. Otherwise 'none'."
type = "verbatim"

[[column]]
name = "reps_warranties_scope"
prompt = "Summarise the scope of representations and warranties, verbatim where possible."
type = "verbatim"

[[column]]
name = "warranty_cap_amount"
prompt = "Warranty/indemnity liability cap as a number. Null if uncapped."
type = "number"

[[column]]
name = "warranty_cap_currency"
prompt = "Currency of the warranty cap."
type = "enum"
options = ["EUR","USD","GBP","CHF","OTHER"]

[[column]]
name = "de_minimis_amount"
prompt = "De minimis threshold for claims as a number. Null if none."
type = "number"

[[column]]
name = "basket_amount"
prompt = "Basket/tipping threshold for claims as a number. Null if none."
type = "number"

[[column]]
name = "survival_period_months"
prompt = "Survival period for general warranty claims, in months. Null if not stated."
type = "number"

[[column]]
name = "indemnities"
prompt = "Specific indemnities granted, verbatim. Otherwise 'none'."
type = "verbatim"

[[column]]
name = "restrictive_covenants"
prompt = "Non-compete / non-solicit covenants binding the seller, verbatim. Otherwise 'none'."
type = "verbatim"

[[column]]
name = "governing_law"
prompt = "Governing law as an ISO 3166-1 alpha-2 country code."
type = "enum"
options = ["FR","DE","UK","US","CH","BE","LU","NL","ES","IT","SE","OTHER"]

[[column]]
name = "jurisdiction"
prompt = "Court or arbitration forum, verbatim."
type = "verbatim"

[[column]]
name = "source_reference"
prompt = "Quote verbatim the single most material clause supporting this row."
type = "verbatim"

[[column]]
name = "key_risk_flag"
prompt = "Does this document warrant a red flag for the deal? true/false."
type = "boolean"

[[column]]
name = "risk_note"
prompt = "One sentence explaining the risk, if any. Otherwise 'none'."
type = "text"
```

- [ ] **Step 2: Write `sha-v1.toml`**

```toml
id = "sha-v1"
name = "Shareholders Agreement (DD)"
version = "1.0.0"
description = "Shareholders agreement / pacte d'actionnaires — governance, transfer restrictions, exit."
vertical = "legal-intl"

[[column]]
name = "parties"
prompt = "Contracting parties (full legal names and corporate form)."
type = "text"

[[column]]
name = "effective_date"
prompt = "Effective date. Return ISO 8601 (YYYY-MM-DD)."
type = "date"

[[column]]
name = "share_classes"
prompt = "Share classes and their rights, verbatim."
type = "verbatim"

[[column]]
name = "board_composition"
prompt = "Board composition and appointment rights, verbatim."
type = "verbatim"

[[column]]
name = "reserved_matters"
prompt = "Reserved/veto matters requiring supermajority or specific consent, verbatim."
type = "verbatim"

[[column]]
name = "preemption_right"
prompt = "Is there a pre-emption right on transfers? true/false."
type = "boolean"

[[column]]
name = "tag_along"
prompt = "Is there a tag-along right? true/false."
type = "boolean"

[[column]]
name = "drag_along"
prompt = "Is there a drag-along right? true/false."
type = "boolean"

[[column]]
name = "transfer_restrictions"
prompt = "Share transfer restrictions and lock-up, verbatim."
type = "verbatim"

[[column]]
name = "deadlock_mechanism"
prompt = "Deadlock resolution mechanism, verbatim. Otherwise 'none'."
type = "verbatim"

[[column]]
name = "exit_provisions"
prompt = "Exit provisions (IPO, trade sale, put/call), verbatim."
type = "verbatim"

[[column]]
name = "non_compete"
prompt = "Is there a non-compete binding shareholders? true/false."
type = "boolean"

[[column]]
name = "change_of_control"
prompt = "Is there a change-of-control provision? true/false."
type = "boolean"

[[column]]
name = "term"
prompt = "Term/duration of the agreement, verbatim."
type = "verbatim"

[[column]]
name = "governing_law"
prompt = "Governing law as an ISO 3166-1 alpha-2 country code."
type = "enum"
options = ["FR","DE","UK","US","CH","BE","LU","NL","ES","IT","SE","OTHER"]

[[column]]
name = "source_reference"
prompt = "Quote verbatim the single most material clause supporting this row."
type = "verbatim"

[[column]]
name = "key_risk_flag"
prompt = "Does this document warrant a red flag for the deal? true/false."
type = "boolean"

[[column]]
name = "risk_note"
prompt = "One sentence explaining the risk, if any. Otherwise 'none'."
type = "text"
```

- [ ] **Step 3: Register both in `template.rs`**

In `Template::builtin()` (after the existing `"ip-v1" => …` arm, before `_ =>`):

```rust
            "spa-v1" => include_str!("../templates/spa-v1.toml"),
            "sha-v1" => include_str!("../templates/sha-v1.toml"),
```

In `Template::list_builtin()` array (append after `"ip-v1",`):

```rust
            "spa-v1",
            "sha-v1",
```

- [ ] **Step 4: Add an evidence-triplet assertion to the test**

Append to `crates/anno-rag-tabular/tests/dd_templates.rs`:

```rust
const GRIDS: &[&str] = &[
    "spa-v1","sha-v1","jva-v1","senior-facilities-v1","security-package-v1",
    "intercreditor-v1","material-commercial-v1","commercial-lease-v1",
    "employment-exec-v1","ip-portfolio-v1","litigation-v1","insurance-v1",
    "data-protection-v1","debt-finance-v1","corporate-captable-v1",
];

#[test]
fn grids_have_evidence_triplet() {
    for id in GRIDS {
        let Ok(t) = Template::builtin(id) else { continue };
        let names: Vec<&str> = t.columns.iter().map(|c| c.name.as_str()).collect();
        for must in ["source_reference","key_risk_flag","risk_note"] {
            assert!(names.contains(&must), "{id} missing evidence column {must}");
        }
    }
}
```

(The `let Ok(...) else { continue }` lets this test pass incrementally as grids land; Task 10 tightens it.)

- [ ] **Step 5: Run the templates tests**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag-tabular -- --test dd_templates`
Expected: `grids_have_evidence_triplet` PASS; `all_dd_templates_load` still FAIL (15 templates remain).

- [ ] **Step 6: Commit**

```bash
git add crates/anno-rag-tabular/src/templates/spa-v1.toml crates/anno-rag-tabular/src/templates/sha-v1.toml crates/anno-rag-tabular/src/schema/template.rs crates/anno-rag-tabular/tests/dd_templates.rs
git commit -m "feat(tabular): add spa-v1 + sha-v1 DD templates"
```

---

## Task 3: JV + LBO grids — jva-v1, senior-facilities-v1, security-package-v1, intercreditor-v1

**Files:**
- Create the four `.toml` files listed below.
- Modify: `crates/anno-rag-tabular/src/schema/template.rs`

- [ ] **Step 1: Write `jva-v1.toml`**

```toml
id = "jva-v1"
name = "Joint Venture Agreement (DD)"
version = "1.0.0"
description = "JV agreement — contributions, scope, governance, profit sharing, exit."
vertical = "legal-intl"

[[column]]
name = "jv_parties"
prompt = "JV parties (full legal names and corporate form)."
type = "text"

[[column]]
name = "jv_vehicle"
prompt = "JV vehicle (NewCo name) or 'contractual' if no entity."
type = "text"

[[column]]
name = "effective_date"
prompt = "Effective date. Return ISO 8601 (YYYY-MM-DD)."
type = "date"

[[column]]
name = "scope_purpose"
prompt = "Business scope and purpose of the JV, verbatim."
type = "verbatim"

[[column]]
name = "contributions"
prompt = "Contributions of each party (cash, assets, IP), verbatim."
type = "verbatim"

[[column]]
name = "ownership_split"
prompt = "Ownership percentage per party."
type = "text"

[[column]]
name = "governance"
prompt = "Governance structure (board, management), verbatim."
type = "verbatim"

[[column]]
name = "profit_sharing"
prompt = "Profit-sharing / distribution policy, verbatim."
type = "verbatim"

[[column]]
name = "funding_obligations"
prompt = "Future funding obligations, verbatim. Otherwise 'none'."
type = "verbatim"

[[column]]
name = "non_compete"
prompt = "Is there a non-compete binding the parties? true/false."
type = "boolean"

[[column]]
name = "non_compete_terms"
prompt = "If non-compete present, quote verbatim. Otherwise 'none'."
type = "verbatim"

[[column]]
name = "exclusivity"
prompt = "Is there exclusivity between the parties? true/false."
type = "boolean"

[[column]]
name = "deadlock_mechanism"
prompt = "Deadlock resolution mechanism, verbatim. Otherwise 'none'."
type = "verbatim"

[[column]]
name = "exit_terms"
prompt = "Exit terms (buy-out, put/call, dissolution), verbatim."
type = "verbatim"

[[column]]
name = "term"
prompt = "Term/duration of the JV, verbatim."
type = "verbatim"

[[column]]
name = "governing_law"
prompt = "Governing law as an ISO 3166-1 alpha-2 country code."
type = "enum"
options = ["FR","DE","UK","US","CH","BE","LU","NL","ES","IT","SE","OTHER"]

[[column]]
name = "source_reference"
prompt = "Quote verbatim the single most material clause supporting this row."
type = "verbatim"

[[column]]
name = "key_risk_flag"
prompt = "Does this document warrant a red flag for the deal? true/false."
type = "boolean"

[[column]]
name = "risk_note"
prompt = "One sentence explaining the risk, if any. Otherwise 'none'."
type = "text"
```

- [ ] **Step 2: Write `senior-facilities-v1.toml`**

```toml
id = "senior-facilities-v1"
name = "Senior Facilities Agreement (DD)"
version = "1.0.0"
description = "Senior debt facilities — tranches, pricing, covenants, prepayment, defaults."
vertical = "legal-intl"

[[column]]
name = "borrower"
prompt = "Borrower(s) (full legal names)."
type = "text"

[[column]]
name = "lenders"
prompt = "Lender(s) / agent."
type = "text"

[[column]]
name = "agreement_date"
prompt = "Agreement date. Return ISO 8601 (YYYY-MM-DD)."
type = "date"

[[column]]
name = "facility_tranches"
prompt = "Facility tranches (Term A/B, RCF, capex), verbatim."
type = "verbatim"

[[column]]
name = "total_commitment_amount"
prompt = "Total commitment as a number."
type = "number"

[[column]]
name = "total_commitment_currency"
prompt = "Currency of the total commitment."
type = "enum"
options = ["EUR","USD","GBP","CHF","OTHER"]

[[column]]
name = "margin"
prompt = "Margin / pricing grid, verbatim."
type = "verbatim"

[[column]]
name = "maturity_date"
prompt = "Final maturity date. Return ISO 8601 (YYYY-MM-DD)."
type = "date"

[[column]]
name = "financial_covenants"
prompt = "Financial covenants (leverage, ICR, etc.), verbatim."
type = "verbatim"

[[column]]
name = "covenant_levels"
prompt = "Covenant thresholds and step-downs, verbatim."
type = "verbatim"

[[column]]
name = "mandatory_prepayment"
prompt = "Mandatory prepayment events, verbatim."
type = "verbatim"

[[column]]
name = "change_of_control"
prompt = "Is there a change-of-control prepayment/default? true/false."
type = "boolean"

[[column]]
name = "mac_clause"
prompt = "Is there a Material Adverse Change clause? true/false."
type = "boolean"

[[column]]
name = "events_of_default"
prompt = "Key events of default, verbatim."
type = "verbatim"

[[column]]
name = "security_summary"
prompt = "Summary of security granted, verbatim."
type = "verbatim"

[[column]]
name = "governing_law"
prompt = "Governing law as an ISO 3166-1 alpha-2 country code."
type = "enum"
options = ["FR","DE","UK","US","CH","BE","LU","NL","ES","IT","SE","OTHER"]

[[column]]
name = "source_reference"
prompt = "Quote verbatim the single most material clause supporting this row."
type = "verbatim"

[[column]]
name = "key_risk_flag"
prompt = "Does this document warrant a red flag for the deal? true/false."
type = "boolean"

[[column]]
name = "risk_note"
prompt = "One sentence explaining the risk, if any. Otherwise 'none'."
type = "text"
```

- [ ] **Step 3: Write `security-package-v1.toml`**

```toml
id = "security-package-v1"
name = "Security Package (DD)"
version = "1.0.0"
description = "Security interests / sûretés — type, asset, ranking, perfection, amount secured."
vertical = "legal-intl"

[[column]]
name = "grantor"
prompt = "Security grantor (full legal name)."
type = "text"

[[column]]
name = "secured_party"
prompt = "Secured party / security agent."
type = "text"

[[column]]
name = "security_type"
prompt = "Type of security interest."
type = "enum"
options = ["share_pledge","asset_pledge","mortgage","dailly_assignment","fiducie","floating_charge","guarantee","other"]

[[column]]
name = "secured_asset"
prompt = "Asset over which security is granted."
type = "text"

[[column]]
name = "secured_amount"
prompt = "Maximum secured amount as a number. Null if all-monies."
type = "number"

[[column]]
name = "secured_currency"
prompt = "Currency of the secured amount."
type = "enum"
options = ["EUR","USD","GBP","CHF","OTHER"]

[[column]]
name = "ranking"
prompt = "Ranking of the security."
type = "enum"
options = ["first","second","junior","pari_passu","other"]

[[column]]
name = "perfection_status"
prompt = "Perfection/registration status."
type = "enum"
options = ["registered","pending","unperfected","na"]

[[column]]
name = "grant_date"
prompt = "Date the security was granted. Return ISO 8601 (YYYY-MM-DD)."
type = "date"

[[column]]
name = "release_conditions"
prompt = "Conditions for release of the security, verbatim. Otherwise 'none'."
type = "verbatim"

[[column]]
name = "governing_law"
prompt = "Governing law as an ISO 3166-1 alpha-2 country code."
type = "enum"
options = ["FR","DE","UK","US","CH","BE","LU","NL","ES","IT","SE","OTHER"]

[[column]]
name = "source_reference"
prompt = "Quote verbatim the single most material clause supporting this row."
type = "verbatim"

[[column]]
name = "key_risk_flag"
prompt = "Does this document warrant a red flag for the deal? true/false."
type = "boolean"

[[column]]
name = "risk_note"
prompt = "One sentence explaining the risk, if any. Otherwise 'none'."
type = "text"
```

- [ ] **Step 4: Write `intercreditor-v1.toml`**

```toml
id = "intercreditor-v1"
name = "Intercreditor Agreement (DD)"
version = "1.0.0"
description = "Intercreditor / mezzanine — ranking, subordination, standstill, enforcement."
vertical = "legal-intl"

[[column]]
name = "parties"
prompt = "Parties (senior, mezzanine, hedge, shareholder creditors)."
type = "text"

[[column]]
name = "agreement_date"
prompt = "Agreement date. Return ISO 8601 (YYYY-MM-DD)."
type = "date"

[[column]]
name = "ranking_waterfall"
prompt = "Priority of payments / waterfall, verbatim."
type = "verbatim"

[[column]]
name = "subordination_terms"
prompt = "Subordination terms, verbatim."
type = "verbatim"

[[column]]
name = "standstill_period"
prompt = "Standstill period(s), verbatim. Otherwise 'none'."
type = "verbatim"

[[column]]
name = "payment_blockage"
prompt = "Payment blockage provisions, verbatim. Otherwise 'none'."
type = "verbatim"

[[column]]
name = "enforcement_control"
prompt = "Who controls enforcement, verbatim."
type = "verbatim"

[[column]]
name = "turnover_provisions"
prompt = "Are there turnover provisions? true/false."
type = "boolean"

[[column]]
name = "governing_law"
prompt = "Governing law as an ISO 3166-1 alpha-2 country code."
type = "enum"
options = ["FR","DE","UK","US","CH","BE","LU","NL","ES","IT","SE","OTHER"]

[[column]]
name = "source_reference"
prompt = "Quote verbatim the single most material clause supporting this row."
type = "verbatim"

[[column]]
name = "key_risk_flag"
prompt = "Does this document warrant a red flag for the deal? true/false."
type = "boolean"

[[column]]
name = "risk_note"
prompt = "One sentence explaining the risk, if any. Otherwise 'none'."
type = "text"
```

- [ ] **Step 5: Register all four in `template.rs`**

`builtin()` arms:

```rust
            "jva-v1" => include_str!("../templates/jva-v1.toml"),
            "senior-facilities-v1" => include_str!("../templates/senior-facilities-v1.toml"),
            "security-package-v1" => include_str!("../templates/security-package-v1.toml"),
            "intercreditor-v1" => include_str!("../templates/intercreditor-v1.toml"),
```

`list_builtin()` entries:

```rust
            "jva-v1",
            "senior-facilities-v1",
            "security-package-v1",
            "intercreditor-v1",
```

- [ ] **Step 6: Run tests**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag-tabular -- --test dd_templates`
Expected: `grids_have_evidence_triplet` PASS for the 6 grids so far; `all_dd_templates_load` still FAIL (11 remain).

- [ ] **Step 7: Commit**

```bash
git add crates/anno-rag-tabular/src/templates/jva-v1.toml crates/anno-rag-tabular/src/templates/senior-facilities-v1.toml crates/anno-rag-tabular/src/templates/security-package-v1.toml crates/anno-rag-tabular/src/templates/intercreditor-v1.toml crates/anno-rag-tabular/src/schema/template.rs
git commit -m "feat(tabular): add jva-v1 + LBO DD templates (facilities, security, intercreditor)"
```

---

## Task 4: Commercial grids — material-commercial-v1, commercial-lease-v1

**Files:** create the two `.toml` files; modify `template.rs`.

- [ ] **Step 1: Write `material-commercial-v1.toml`**

```toml
id = "material-commercial-v1"
name = "Material Commercial Contract (DD)"
version = "1.0.0"
description = "Material customer/supplier contracts — term, exclusivity, change of control, termination."
vertical = "legal-intl"

[[column]]
name = "counterparty"
prompt = "Counterparty (full legal name)."
type = "text"

[[column]]
name = "contract_type"
prompt = "Type of commercial contract."
type = "enum"
options = ["customer","supplier","distribution","framework","other"]

[[column]]
name = "effective_date"
prompt = "Effective date. Return ISO 8601 (YYYY-MM-DD)."
type = "date"

[[column]]
name = "expiry_date"
prompt = "Expiry date. Return ISO 8601 (YYYY-MM-DD). Null if perpetual."
type = "date"

[[column]]
name = "term"
prompt = "Term/duration, verbatim."
type = "verbatim"

[[column]]
name = "auto_renewal"
prompt = "Does the contract auto-renew? true/false."
type = "boolean"

[[column]]
name = "annual_value_amount"
prompt = "Annual contract value as a number. Null if not stated."
type = "number"

[[column]]
name = "annual_value_currency"
prompt = "Currency of the annual value."
type = "enum"
options = ["EUR","USD","GBP","CHF","OTHER"]

[[column]]
name = "exclusivity"
prompt = "Is there an exclusivity obligation? true/false."
type = "boolean"

[[column]]
name = "minimum_commitments"
prompt = "Minimum purchase/volume commitments, verbatim. Otherwise 'none'."
type = "verbatim"

[[column]]
name = "change_of_control"
prompt = "Is there a change-of-control termination or consent right? true/false."
type = "boolean"

[[column]]
name = "termination_rights"
prompt = "Termination rights, verbatim."
type = "verbatim"

[[column]]
name = "liability_cap"
prompt = "Liability cap, verbatim. Otherwise 'none'."
type = "verbatim"

[[column]]
name = "governing_law"
prompt = "Governing law as an ISO 3166-1 alpha-2 country code."
type = "enum"
options = ["FR","DE","UK","US","CH","BE","LU","NL","ES","IT","SE","OTHER"]

[[column]]
name = "source_reference"
prompt = "Quote verbatim the single most material clause supporting this row."
type = "verbatim"

[[column]]
name = "key_risk_flag"
prompt = "Does this document warrant a red flag for the deal? true/false."
type = "boolean"

[[column]]
name = "risk_note"
prompt = "One sentence explaining the risk, if any. Otherwise 'none'."
type = "text"
```

- [ ] **Step 2: Write `commercial-lease-v1.toml`**

```toml
id = "commercial-lease-v1"
name = "Commercial Lease (DD)"
version = "1.0.0"
description = "Commercial leases / baux commerciaux — rent, term, use, assignment, restoration."
vertical = "legal-intl"

[[column]]
name = "landlord"
prompt = "Landlord (full legal name)."
type = "text"

[[column]]
name = "tenant"
prompt = "Tenant (full legal name)."
type = "text"

[[column]]
name = "premises"
prompt = "Premises address and surface area."
type = "text"

[[column]]
name = "lease_type"
prompt = "Type of lease."
type = "enum"
options = ["commercial_369","derogatory","professional","other"]

[[column]]
name = "commencement_date"
prompt = "Commencement date. Return ISO 8601 (YYYY-MM-DD)."
type = "date"

[[column]]
name = "term_years"
prompt = "Lease term in years as a number."
type = "number"

[[column]]
name = "annual_rent_amount"
prompt = "Annual rent as a number."
type = "number"

[[column]]
name = "annual_rent_currency"
prompt = "Currency of the annual rent."
type = "enum"
options = ["EUR","USD","GBP","CHF","OTHER"]

[[column]]
name = "permitted_use"
prompt = "Permitted use / destination, verbatim."
type = "verbatim"

[[column]]
name = "assignment_sublet"
prompt = "Assignment and sub-let rights, verbatim."
type = "verbatim"

[[column]]
name = "change_of_control"
prompt = "Is there a change-of-control restriction? true/false."
type = "boolean"

[[column]]
name = "restoration_obligations"
prompt = "Restoration/dilapidation obligations, verbatim. Otherwise 'none'."
type = "verbatim"

[[column]]
name = "break_options"
prompt = "Break options, verbatim. Otherwise 'none'."
type = "verbatim"

[[column]]
name = "governing_law"
prompt = "Governing law as an ISO 3166-1 alpha-2 country code."
type = "enum"
options = ["FR","DE","UK","US","CH","BE","LU","NL","ES","IT","SE","OTHER"]

[[column]]
name = "source_reference"
prompt = "Quote verbatim the single most material clause supporting this row."
type = "verbatim"

[[column]]
name = "key_risk_flag"
prompt = "Does this document warrant a red flag for the deal? true/false."
type = "boolean"

[[column]]
name = "risk_note"
prompt = "One sentence explaining the risk, if any. Otherwise 'none'."
type = "text"
```

- [ ] **Step 3: Register in `template.rs`**

`builtin()`:

```rust
            "material-commercial-v1" => include_str!("../templates/material-commercial-v1.toml"),
            "commercial-lease-v1" => include_str!("../templates/commercial-lease-v1.toml"),
```

`list_builtin()`:

```rust
            "material-commercial-v1",
            "commercial-lease-v1",
```

- [ ] **Step 4: Run tests**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag-tabular -- --test dd_templates`
Expected: `grids_have_evidence_triplet` PASS; `all_dd_templates_load` still FAIL (9 remain).

- [ ] **Step 5: Commit**

```bash
git add crates/anno-rag-tabular/src/templates/material-commercial-v1.toml crates/anno-rag-tabular/src/templates/commercial-lease-v1.toml crates/anno-rag-tabular/src/schema/template.rs
git commit -m "feat(tabular): add material-commercial + commercial-lease DD templates"
```

---

## Task 5: People + IP grids — employment-exec-v1, ip-portfolio-v1

**Files:** create the two `.toml` files; modify `template.rs`.

- [ ] **Step 1: Write `employment-exec-v1.toml`**

```toml
id = "employment-exec-v1"
name = "Executive Employment & Officers (DD)"
version = "1.0.0"
description = "Executives and corporate officers — comp, equity, severance, change of control."
vertical = "legal-intl"

[[column]]
name = "name_role"
prompt = "Name and role of the executive/officer."
type = "text"

[[column]]
name = "contract_type"
prompt = "Type of engagement."
type = "enum"
options = ["employment","mandate","consulting","other"]

[[column]]
name = "start_date"
prompt = "Start date. Return ISO 8601 (YYYY-MM-DD)."
type = "date"

[[column]]
name = "base_comp_amount"
prompt = "Annual base compensation as a number."
type = "number"

[[column]]
name = "base_comp_currency"
prompt = "Currency of the base compensation."
type = "enum"
options = ["EUR","USD","GBP","CHF","OTHER"]

[[column]]
name = "variable_comp"
prompt = "Variable compensation (bonus, LTIP), verbatim. Otherwise 'none'."
type = "verbatim"

[[column]]
name = "equity_incentives"
prompt = "Equity incentives (BSPCE, stock options, free shares), verbatim. Otherwise 'none'."
type = "verbatim"

[[column]]
name = "severance_terms"
prompt = "Severance / golden parachute terms, verbatim. Otherwise 'none'."
type = "verbatim"

[[column]]
name = "change_of_control_trigger"
prompt = "Is there a change-of-control acceleration or severance trigger? true/false."
type = "boolean"

[[column]]
name = "non_compete"
prompt = "Is there a post-termination non-compete? true/false."
type = "boolean"

[[column]]
name = "non_compete_indemnity"
prompt = "Non-compete indemnity terms, verbatim. Otherwise 'none'."
type = "verbatim"

[[column]]
name = "notice_period"
prompt = "Notice period, verbatim."
type = "verbatim"

[[column]]
name = "governing_law"
prompt = "Governing law as an ISO 3166-1 alpha-2 country code."
type = "enum"
options = ["FR","DE","UK","US","CH","BE","LU","NL","ES","IT","SE","OTHER"]

[[column]]
name = "source_reference"
prompt = "Quote verbatim the single most material clause supporting this row."
type = "verbatim"

[[column]]
name = "key_risk_flag"
prompt = "Does this document warrant a red flag for the deal? true/false."
type = "boolean"

[[column]]
name = "risk_note"
prompt = "One sentence explaining the risk, if any. Otherwise 'none'."
type = "text"
```

- [ ] **Step 2: Write `ip-portfolio-v1.toml`**

```toml
id = "ip-portfolio-v1"
name = "IP & IT Portfolio (DD)"
version = "1.0.0"
description = "IP/IT assets — type, ownership, registration, licenses, open source, encumbrances."
vertical = "legal-intl"

[[column]]
name = "asset_name"
prompt = "Name/identifier of the IP or IT asset."
type = "text"

[[column]]
name = "ip_type"
prompt = "Type of IP asset."
type = "enum"
options = ["patent","trademark","copyright","domain","trade_secret","software","other"]

[[column]]
name = "ownership"
prompt = "Ownership status of the asset."
type = "enum"
options = ["owned","licensed_in","licensed_out","joint","disputed"]

[[column]]
name = "registration_status"
prompt = "Registration status (filed, granted, pending), verbatim."
type = "verbatim"

[[column]]
name = "jurisdiction_coverage"
prompt = "Territories/jurisdictions covered."
type = "text"

[[column]]
name = "license_terms"
prompt = "License terms if licensed, verbatim. Otherwise 'none'."
type = "verbatim"

[[column]]
name = "open_source_usage"
prompt = "Does the asset use open-source components? true/false."
type = "boolean"

[[column]]
name = "open_source_terms"
prompt = "Copyleft/open-source obligations, verbatim. Otherwise 'none'."
type = "verbatim"

[[column]]
name = "change_of_control"
prompt = "Is there a change-of-control restriction in any license? true/false."
type = "boolean"

[[column]]
name = "encumbrances"
prompt = "Encumbrances or disputes affecting the asset, verbatim. Otherwise 'none'."
type = "verbatim"

[[column]]
name = "governing_law"
prompt = "Governing law as an ISO 3166-1 alpha-2 country code."
type = "enum"
options = ["FR","DE","UK","US","CH","BE","LU","NL","ES","IT","SE","OTHER"]

[[column]]
name = "source_reference"
prompt = "Quote verbatim the single most material clause supporting this row."
type = "verbatim"

[[column]]
name = "key_risk_flag"
prompt = "Does this document warrant a red flag for the deal? true/false."
type = "boolean"

[[column]]
name = "risk_note"
prompt = "One sentence explaining the risk, if any. Otherwise 'none'."
type = "text"
```

- [ ] **Step 3: Register in `template.rs`**

`builtin()`:

```rust
            "employment-exec-v1" => include_str!("../templates/employment-exec-v1.toml"),
            "ip-portfolio-v1" => include_str!("../templates/ip-portfolio-v1.toml"),
```

`list_builtin()`:

```rust
            "employment-exec-v1",
            "ip-portfolio-v1",
```

- [ ] **Step 4: Run tests**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag-tabular -- --test dd_templates`
Expected: `grids_have_evidence_triplet` PASS; `all_dd_templates_load` still FAIL (7 remain).

- [ ] **Step 5: Commit**

```bash
git add crates/anno-rag-tabular/src/templates/employment-exec-v1.toml crates/anno-rag-tabular/src/templates/ip-portfolio-v1.toml crates/anno-rag-tabular/src/schema/template.rs
git commit -m "feat(tabular): add employment-exec + ip-portfolio DD templates"
```

---

## Task 6: Risk grids — litigation-v1, insurance-v1, data-protection-v1

**Files:** create the three `.toml` files; modify `template.rs`.

- [ ] **Step 1: Write `litigation-v1.toml`**

```toml
id = "litigation-v1"
name = "Litigation & Disputes (DD)"
version = "1.0.0"
description = "Pending and threatened disputes — type, status, exposure, provisions."
vertical = "legal-intl"

[[column]]
name = "matter_reference"
prompt = "Matter reference or case number."
type = "text"

[[column]]
name = "parties"
prompt = "Parties to the dispute."
type = "text"

[[column]]
name = "dispute_type"
prompt = "Type of dispute."
type = "enum"
options = ["commercial","employment","ip","tax","regulatory","product","other"]

[[column]]
name = "status"
prompt = "Status of the dispute."
type = "enum"
options = ["pending","threatened","settled","appeal","closed"]

[[column]]
name = "forum"
prompt = "Court or arbitration forum, verbatim."
type = "verbatim"

[[column]]
name = "amount_at_stake_amount"
prompt = "Amount at stake as a number. Null if unquantified."
type = "number"

[[column]]
name = "amount_at_stake_currency"
prompt = "Currency of the amount at stake."
type = "enum"
options = ["EUR","USD","GBP","CHF","OTHER"]

[[column]]
name = "provision_booked_amount"
prompt = "Accounting provision booked as a number. Null if none."
type = "number"

[[column]]
name = "provision_booked_currency"
prompt = "Currency of the booked provision."
type = "enum"
options = ["EUR","USD","GBP","CHF","OTHER"]

[[column]]
name = "commenced_date"
prompt = "Date the dispute commenced. Return ISO 8601 (YYYY-MM-DD)."
type = "date"

[[column]]
name = "summary"
prompt = "Short summary of the dispute."
type = "text"

[[column]]
name = "governing_law"
prompt = "Governing law as an ISO 3166-1 alpha-2 country code."
type = "enum"
options = ["FR","DE","UK","US","CH","BE","LU","NL","ES","IT","SE","OTHER"]

[[column]]
name = "source_reference"
prompt = "Quote verbatim the single most material passage supporting this row."
type = "verbatim"

[[column]]
name = "key_risk_flag"
prompt = "Does this matter warrant a red flag for the deal? true/false."
type = "boolean"

[[column]]
name = "risk_note"
prompt = "One sentence explaining the risk, if any. Otherwise 'none'."
type = "text"
```

- [ ] **Step 2: Write `insurance-v1.toml`**

```toml
id = "insurance-v1"
name = "Insurance Policies (DD)"
version = "1.0.0"
description = "Insurance policies — coverage, limits, deductibles, exclusions, claims history."
vertical = "legal-intl"

[[column]]
name = "insurer"
prompt = "Insurer name."
type = "text"

[[column]]
name = "policy_type"
prompt = "Type of policy."
type = "enum"
options = ["liability","property","d_and_o","cyber","credit","w_and_i","other"]

[[column]]
name = "policy_number"
prompt = "Policy number."
type = "text"

[[column]]
name = "coverage_limit_amount"
prompt = "Coverage limit as a number."
type = "number"

[[column]]
name = "coverage_limit_currency"
prompt = "Currency of the coverage limit."
type = "enum"
options = ["EUR","USD","GBP","CHF","OTHER"]

[[column]]
name = "deductible_amount"
prompt = "Deductible/excess as a number. Null if none."
type = "number"

[[column]]
name = "deductible_currency"
prompt = "Currency of the deductible."
type = "enum"
options = ["EUR","USD","GBP","CHF","OTHER"]

[[column]]
name = "period_start"
prompt = "Policy period start. Return ISO 8601 (YYYY-MM-DD)."
type = "date"

[[column]]
name = "period_end"
prompt = "Policy period end. Return ISO 8601 (YYYY-MM-DD)."
type = "date"

[[column]]
name = "key_exclusions"
prompt = "Key exclusions, verbatim."
type = "verbatim"

[[column]]
name = "claims_history"
prompt = "Claims history, verbatim. Otherwise 'none'."
type = "verbatim"

[[column]]
name = "change_of_control"
prompt = "Does the policy survive a change of control? true/false."
type = "boolean"

[[column]]
name = "source_reference"
prompt = "Quote verbatim the single most material clause supporting this row."
type = "verbatim"

[[column]]
name = "key_risk_flag"
prompt = "Does this policy warrant a red flag for the deal? true/false."
type = "boolean"

[[column]]
name = "risk_note"
prompt = "One sentence explaining the risk, if any. Otherwise 'none'."
type = "text"
```

- [ ] **Step 3: Write `data-protection-v1.toml`**

```toml
id = "data-protection-v1"
name = "Data Protection / GDPR (DD)"
version = "1.0.0"
description = "GDPR / DPA posture — roles, transfers, subprocessors, breaches, DPIA."
vertical = "legal-intl"

[[column]]
name = "counterparty"
prompt = "Counterparty (full legal name)."
type = "text"

[[column]]
name = "role"
prompt = "Data-protection role under GDPR."
type = "enum"
options = ["controller","processor","joint_controller","na"]

[[column]]
name = "dpa_in_place"
prompt = "Is a Data Processing Agreement in place? true/false."
type = "boolean"

[[column]]
name = "processing_purpose"
prompt = "Purpose of processing, verbatim."
type = "verbatim"

[[column]]
name = "data_categories"
prompt = "Categories of personal data, including special categories, verbatim."
type = "verbatim"

[[column]]
name = "international_transfers"
prompt = "Are there international transfers outside the EEA? true/false."
type = "boolean"

[[column]]
name = "transfer_mechanism"
prompt = "Transfer mechanism."
type = "enum"
options = ["sccs","adequacy","bcr","derogation","none","na"]

[[column]]
name = "subprocessors"
prompt = "Sub-processors used, verbatim. Otherwise 'none'."
type = "verbatim"

[[column]]
name = "breach_history"
prompt = "Personal-data breach history, verbatim. Otherwise 'none'."
type = "verbatim"

[[column]]
name = "dpia_done"
prompt = "Has a DPIA (AIPD) been performed? true/false."
type = "boolean"

[[column]]
name = "retention_terms"
prompt = "Data retention terms, verbatim. Otherwise 'none'."
type = "verbatim"

[[column]]
name = "source_reference"
prompt = "Quote verbatim the single most material clause supporting this row."
type = "verbatim"

[[column]]
name = "key_risk_flag"
prompt = "Does this warrant a red flag for the deal? true/false."
type = "boolean"

[[column]]
name = "risk_note"
prompt = "One sentence explaining the risk, if any. Otherwise 'none'."
type = "text"
```

- [ ] **Step 4: Register in `template.rs`**

`builtin()`:

```rust
            "litigation-v1" => include_str!("../templates/litigation-v1.toml"),
            "insurance-v1" => include_str!("../templates/insurance-v1.toml"),
            "data-protection-v1" => include_str!("../templates/data-protection-v1.toml"),
```

`list_builtin()`:

```rust
            "litigation-v1",
            "insurance-v1",
            "data-protection-v1",
```

- [ ] **Step 5: Run tests**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag-tabular -- --test dd_templates`
Expected: `grids_have_evidence_triplet` PASS; `all_dd_templates_load` still FAIL (4 remain).

- [ ] **Step 6: Commit**

```bash
git add crates/anno-rag-tabular/src/templates/litigation-v1.toml crates/anno-rag-tabular/src/templates/insurance-v1.toml crates/anno-rag-tabular/src/templates/data-protection-v1.toml crates/anno-rag-tabular/src/schema/template.rs
git commit -m "feat(tabular): add litigation + insurance + data-protection DD templates"
```

---

## Task 7: Finance + corporate grids — debt-finance-v1, corporate-captable-v1

**Files:** create the two `.toml` files; modify `template.rs`.

- [ ] **Step 1: Write `debt-finance-v1.toml`**

```toml
id = "debt-finance-v1"
name = "Existing Debt (DD)"
version = "1.0.0"
description = "Existing debt instruments — amount, maturity, covenants, change of control, security."
vertical = "legal-intl"

[[column]]
name = "lender"
prompt = "Lender name."
type = "text"

[[column]]
name = "instrument_type"
prompt = "Type of debt instrument."
type = "enum"
options = ["term_loan","revolving","bond","leasing","factoring","shareholder_loan","other"]

[[column]]
name = "principal_amount"
prompt = "Original principal as a number."
type = "number"

[[column]]
name = "principal_currency"
prompt = "Currency of the principal."
type = "enum"
options = ["EUR","USD","GBP","CHF","OTHER"]

[[column]]
name = "outstanding_amount"
prompt = "Outstanding balance as a number. Null if unknown."
type = "number"

[[column]]
name = "outstanding_currency"
prompt = "Currency of the outstanding balance."
type = "enum"
options = ["EUR","USD","GBP","CHF","OTHER"]

[[column]]
name = "interest_terms"
prompt = "Interest rate / margin terms, verbatim."
type = "verbatim"

[[column]]
name = "maturity_date"
prompt = "Maturity date. Return ISO 8601 (YYYY-MM-DD)."
type = "date"

[[column]]
name = "financial_covenants"
prompt = "Financial covenants, verbatim. Otherwise 'none'."
type = "verbatim"

[[column]]
name = "change_of_control"
prompt = "Is there a change-of-control prepayment/default? true/false."
type = "boolean"

[[column]]
name = "security_granted"
prompt = "Security granted for this debt, verbatim. Otherwise 'none'."
type = "verbatim"

[[column]]
name = "governing_law"
prompt = "Governing law as an ISO 3166-1 alpha-2 country code."
type = "enum"
options = ["FR","DE","UK","US","CH","BE","LU","NL","ES","IT","SE","OTHER"]

[[column]]
name = "source_reference"
prompt = "Quote verbatim the single most material clause supporting this row."
type = "verbatim"

[[column]]
name = "key_risk_flag"
prompt = "Does this instrument warrant a red flag for the deal? true/false."
type = "boolean"

[[column]]
name = "risk_note"
prompt = "One sentence explaining the risk, if any. Otherwise 'none'."
type = "text"
```

- [ ] **Step 2: Write `corporate-captable-v1.toml`**

```toml
id = "corporate-captable-v1"
name = "Capital Structure (DD)"
version = "1.0.0"
description = "Cap table — share classes, ownership, options, encumbrances, voting rights."
vertical = "legal-intl"

[[column]]
name = "holder"
prompt = "Shareholder / holder name."
type = "text"

[[column]]
name = "share_class"
prompt = "Share class (ordinary, preferred, etc.)."
type = "text"

[[column]]
name = "shares_held"
prompt = "Number of shares held as a number."
type = "number"

[[column]]
name = "ownership_pct"
prompt = "Ownership percentage as a number (0-100)."
type = "number"

[[column]]
name = "fully_diluted_pct"
prompt = "Fully diluted percentage as a number (0-100)."
type = "number"

[[column]]
name = "instrument_type"
prompt = "Type of instrument held."
type = "enum"
options = ["shares","options","warrants","bsa","bspce","convertible","other"]

[[column]]
name = "vesting_terms"
prompt = "Vesting terms for options/equity, verbatim. Otherwise 'none'."
type = "verbatim"

[[column]]
name = "encumbrances"
prompt = "Pledges or encumbrances over the holding, verbatim. Otherwise 'none'."
type = "verbatim"

[[column]]
name = "transfer_restrictions"
prompt = "Transfer restrictions on the holding, verbatim. Otherwise 'none'."
type = "verbatim"

[[column]]
name = "voting_rights"
prompt = "Voting rights attached, verbatim."
type = "verbatim"

[[column]]
name = "source_reference"
prompt = "Quote verbatim the single most material passage supporting this row."
type = "verbatim"

[[column]]
name = "key_risk_flag"
prompt = "Does this holding warrant a red flag for the deal? true/false."
type = "boolean"

[[column]]
name = "risk_note"
prompt = "One sentence explaining the risk, if any. Otherwise 'none'."
type = "text"
```

- [ ] **Step 3: Register in `template.rs`**

`builtin()`:

```rust
            "debt-finance-v1" => include_str!("../templates/debt-finance-v1.toml"),
            "corporate-captable-v1" => include_str!("../templates/corporate-captable-v1.toml"),
```

`list_builtin()`:

```rust
            "debt-finance-v1",
            "corporate-captable-v1",
```

- [ ] **Step 4: Run tests**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag-tabular -- --test dd_templates`
Expected: `grids_have_evidence_triplet` PASS for all 15 grids; `all_dd_templates_load` still FAIL (2 registers remain).

- [ ] **Step 5: Commit**

```bash
git add crates/anno-rag-tabular/src/templates/debt-finance-v1.toml crates/anno-rag-tabular/src/templates/corporate-captable-v1.toml crates/anno-rag-tabular/src/schema/template.rs
git commit -m "feat(tabular): add debt-finance + corporate-captable DD templates"
```

---

## Task 8: Transversal registers — data-room-index-v1, red-flags-register-v1

**Files:** create the two `.toml` files; modify `template.rs`.

> Registers are NOT grids: they have no evidence triplet. `red-flags-register-v1` carries its own `source_citation` verbatim column instead.

- [ ] **Step 1: Write `data-room-index-v1.toml`**

```toml
id = "data-room-index-v1"
name = "Data Room Index (DD)"
version = "1.0.0"
description = "Data-room inventory — one row per document, surfaces gaps and priorities."
vertical = "legal-intl"

[[column]]
name = "document_title"
prompt = "Title of the document."
type = "text"

[[column]]
name = "doc_type"
prompt = "Document type."
type = "enum"
options = ["spa","sha","jva","facilities","security","intercreditor","commercial","lease","employment","ip","litigation","insurance","data_protection","debt","corporate","other"]

[[column]]
name = "workstream"
prompt = "DD workstream this document belongs to."
type = "enum"
options = ["corporate","commercial","employment","ip_it","real_estate","litigation","regulatory","finance_debt","tax","data_protection","insurance","environmental"]

[[column]]
name = "language"
prompt = "Primary language of the document."
type = "enum"
options = ["fr","en","de","other"]

[[column]]
name = "document_date"
prompt = "Document date. Return ISO 8601 (YYYY-MM-DD). Null if undated."
type = "date"

[[column]]
name = "parties"
prompt = "Parties named in the document."
type = "text"

[[column]]
name = "status"
prompt = "Completeness status in the data room."
type = "enum"
options = ["received","missing","partial","na"]

[[column]]
name = "priority"
prompt = "Review priority."
type = "enum"
options = ["high","medium","low"]

[[column]]
name = "page_count"
prompt = "Number of pages as a number. Null if unknown."
type = "number"

[[column]]
name = "notes"
prompt = "Reviewer notes."
type = "text"
```

- [ ] **Step 2: Write `red-flags-register-v1.toml`**

```toml
id = "red-flags-register-v1"
name = "Red Flags Register (DD)"
version = "1.0.0"
description = "Issues register — one row per risk, the client deliverable."
vertical = "legal-intl"

[[column]]
name = "issue_title"
prompt = "Short title of the issue."
type = "text"

[[column]]
name = "workstream"
prompt = "DD workstream the issue belongs to."
type = "enum"
options = ["corporate","commercial","employment","ip_it","real_estate","litigation","regulatory","finance_debt","tax","data_protection","insurance","environmental"]

[[column]]
name = "severity"
prompt = "Severity of the issue."
type = "enum"
options = ["critical","high","medium","low","info"]

[[column]]
name = "description"
prompt = "Description of the risk."
type = "text"

[[column]]
name = "source_citation"
prompt = "Quote verbatim the clause or passage evidencing the issue."
type = "verbatim"

[[column]]
name = "source_document"
prompt = "Document/exhibit the citation comes from."
type = "text"

[[column]]
name = "impact"
prompt = "Impact of the issue on the deal."
type = "text"

[[column]]
name = "recommendation"
prompt = "Recommended mitigation or SPA protection."
type = "text"

[[column]]
name = "status"
prompt = "Status of the issue."
type = "enum"
options = ["open","mitigated","accepted","resolved"]

[[column]]
name = "owner"
prompt = "Advisor responsible for the issue."
type = "text"
```

- [ ] **Step 3: Register in `template.rs`**

`builtin()`:

```rust
            "data-room-index-v1" => include_str!("../templates/data-room-index-v1.toml"),
            "red-flags-register-v1" => include_str!("../templates/red-flags-register-v1.toml"),
```

`list_builtin()`:

```rust
            "data-room-index-v1",
            "red-flags-register-v1",
```

- [ ] **Step 4: Run tests — now load-all passes**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag-tabular -- --test dd_templates`
Expected: BOTH `all_dd_templates_load` and `grids_have_evidence_triplet` PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/anno-rag-tabular/src/templates/data-room-index-v1.toml crates/anno-rag-tabular/src/templates/red-flags-register-v1.toml crates/anno-rag-tabular/src/schema/template.rs
git commit -m "feat(tabular): add data-room-index + red-flags-register DD templates"
```

---

## Task 9: Enum-consistency + SPA snapshot tests

**Files:**
- Modify: `crates/anno-rag-tabular/tests/dd_templates.rs`

- [ ] **Step 1: Write the enum-consistency test**

Append:

```rust
use anno_rag_tabular::schema::CellType;

fn enum_options<'a>(t: &'a Template, col: &str) -> Option<Vec<String>> {
    let c = t.columns.iter().find(|c| c.name == col)?;
    match &c.cell_type {
        anno_rag_tabular::schema::template::CellTypeWire::Enum { options } => Some(options.clone()),
        _ => None,
    }
}

#[test]
fn governing_law_enum_is_consistent() {
    let expected = vec!["FR","DE","UK","US","CH","BE","LU","NL","ES","IT","SE","OTHER"];
    for id in GRIDS {
        let t = Template::builtin(id).expect("loads");
        if let Some(opts) = enum_options(&t, "governing_law") {
            assert_eq!(opts, expected, "{id} governing_law enum drifted");
        }
    }
}

#[test]
fn workstream_enum_is_consistent() {
    let expected = vec![
        "corporate","commercial","employment","ip_it","real_estate","litigation",
        "regulatory","finance_debt","tax","data_protection","insurance","environmental",
    ];
    for id in ["data-room-index-v1","red-flags-register-v1"] {
        let t = Template::builtin(id).expect("loads");
        let opts = enum_options(&t, "workstream").expect("has workstream enum");
        assert_eq!(opts, expected, "{id} workstream enum drifted");
    }
}
```

> NOTE: if `CellTypeWire` is not `pub`, make it `pub` in `schema/template.rs` (it is already `pub enum CellTypeWire`). The `cell_type` field on `TemplateColumn` is `pub`.

- [ ] **Step 2: Write the SPA snapshot test**

Append:

```rust
#[test]
fn spa_v1_has_expected_shape() {
    let t = Template::builtin("spa-v1").expect("loads");
    // 24 domain columns + 3 evidence-triplet columns = 27.
    assert_eq!(t.columns.len(), 27, "spa-v1 column count changed");
    let names: Vec<&str> = t.columns.iter().map(|c| c.name.as_str()).collect();
    for must in ["purchase_price_amount","mac_clause","warranty_cap_amount","governing_law"] {
        assert!(names.contains(&must), "spa-v1 missing {must}");
    }
    // last three columns are the evidence triplet, in order
    assert_eq!(&names[names.len()-3..], &["source_reference","key_risk_flag","risk_note"]);
}
```

- [ ] **Step 3: Run tests**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag-tabular -- --test dd_templates`
Expected: all four tests PASS. If `spa_v1_has_expected_shape` fails on count, recount the `[[column]]` blocks in `spa-v1.toml` and fix the assertion to match (24 domain + 3 triplet).

- [ ] **Step 4: Commit**

```bash
git add crates/anno-rag-tabular/tests/dd_templates.rs
git commit -m "test(tabular): enum-consistency + spa-v1 shape assertions"
```

---

## Task 10: Tighten the evidence-triplet test

**Files:**
- Modify: `crates/anno-rag-tabular/tests/dd_templates.rs`

- [ ] **Step 1: Replace the lenient guard with a strict one**

In `grids_have_evidence_triplet`, replace:

```rust
        let Ok(t) = Template::builtin(id) else { continue };
```

with:

```rust
        let t = Template::builtin(id).expect("grid must load");
```

- [ ] **Step 2: Run tests**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag-tabular -- --test dd_templates`
Expected: all tests PASS (every grid now loads, so the strict guard holds).

- [ ] **Step 3: Commit**

```bash
git add crates/anno-rag-tabular/tests/dd_templates.rs
git commit -m "test(tabular): make evidence-triplet test strict now all grids ship"
```

---

## Task 11: Routing-client factory (security core)

**Files:**
- Modify: `crates/anno-rag-tabular/src/llm/mod.rs`
- Modify: `crates/anno-rag-tabular/Cargo.toml` (feature plumbing)
- Reference: `crates/anno-rag-tabular/src/llm/routing.rs:21` (`RoutingLlmClient::new`), `src/llm/local/client.rs:53` (`LocalTabularClient::new`), `:263` (`Gliner2EntityExtractor::from_pretrained`)

- [ ] **Step 1: Add a failing unit test for the factory**

Append to `crates/anno-rag-tabular/src/llm/mod.rs` `#[cfg(test)] mod tests`:

```rust
    #[test]
    fn routing_factory_local_only_when_remote_denied() {
        // With allow_remote = false and no API key, we must still get a
        // client (local-only), never an error about a missing key.
        unsafe { std::env::remove_var("ANTHROPIC_API_KEY"); }
        let c = routing_client_from_env(false);
        assert!(c.is_ok(), "local-only routing must not require an API key");
        assert_eq!(c.unwrap().model_id(), "routing-local-tabular");
    }
```

- [ ] **Step 2: Run it to verify it fails**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag-tabular -- llm::tests::routing_factory_local_only_when_remote_denied`
Expected: FAIL — `routing_client_from_env` not found.

- [ ] **Step 3: Implement the factory**

Add to `crates/anno-rag-tabular/src/llm/mod.rs` (after `default_from_env`):

```rust
/// Build the production extraction client: local-first, with an optional
/// remote fallback.
///
/// Security policy:
/// - The local extractor (GLiNER2/Fastino) always runs first.
/// - A remote LLM fallback is attached ONLY when `allow_remote` is true
///   AND an API key resolves via [`default_from_env`]. When `allow_remote`
///   is false the returned client never makes a network call.
/// - The remote call itself is additionally gated at runtime by
///   [`routing::RoutingLlmClient`] via `fallback_prompt_is_safe`.
///
/// # Errors
///
/// Returns [`crate::error::Error::Extract`] only if the local extractor
/// cannot be constructed. A missing API key is NOT an error — it simply
/// yields a local-only client.
#[cfg(feature = "gliner2")]
pub fn routing_client_from_env(allow_remote: bool) -> crate::error::Result<Box<dyn LlmClient>> {
    use crate::llm::local::client::{Gliner2EntityExtractor, LocalTabularClient};
    use crate::llm::routing::RoutingLlmClient;

    let extractor = Gliner2EntityExtractor::from_pretrained(crate::llm::local::DEFAULT_LOCAL_MODEL)?;
    let local: Box<dyn LlmClient> = Box::new(LocalTabularClient::new(Box::new(extractor)));

    let fallback: Option<Box<dyn LlmClient>> = if allow_remote {
        default_from_env().ok()
    } else {
        None
    };

    Ok(Box::new(RoutingLlmClient::new(local, fallback)))
}

/// Fallback build used when the `gliner2` feature is OFF: no local model
/// is available, so this errors unless remote is explicitly allowed and a
/// key resolves. This keeps non-gliner2 builds compiling while making the
/// "no silent remote" policy explicit.
#[cfg(not(feature = "gliner2"))]
pub fn routing_client_from_env(allow_remote: bool) -> crate::error::Result<Box<dyn LlmClient>> {
    use crate::error::Error;
    if allow_remote {
        return default_from_env();
    }
    Err(Error::Extract {
        doc: "config".into(),
        col: "?".into(),
        source: "local extraction requires the `gliner2` feature; \
                 enable it or pass --allow-remote-llm".into(),
    })
}
```

- [ ] **Step 4: Add `DEFAULT_LOCAL_MODEL` constant**

In `crates/anno-rag-tabular/src/llm/local/mod.rs`, add near the top (model id matches the one used elsewhere — `crates/anno-rag/src/download_models.rs:13` uses `fastino/gliner2-multi-v1`):

```rust
/// Default local extraction model (GLiNER2/Fastino multi-v1).
pub const DEFAULT_LOCAL_MODEL: &str = "fastino/gliner2-multi-v1";
```

- [ ] **Step 5: Plumb the `gliner2` feature in `Cargo.toml`**

In `crates/anno-rag-tabular/Cargo.toml`, ensure a `gliner2` feature exists and that the default build enables local extraction. Add/confirm:

```toml
[features]
default = ["gliner2"]
gliner2 = ["anno/gliner2-fastino"]
```

(If `default` already lists features, append `"gliner2"` to it instead of replacing.)

- [ ] **Step 6: Run the factory test (cfg(gliner2) path)**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag-tabular -Features gliner2 -- llm::tests::routing_factory_local_only_when_remote_denied`
Expected: PASS. (The test asserts a local-only client is returned without an API key.)

- [ ] **Step 7: Commit**

```bash
git add crates/anno-rag-tabular/src/llm/mod.rs crates/anno-rag-tabular/src/llm/local/mod.rs crates/anno-rag-tabular/Cargo.toml
git commit -m "feat(tabular): routing_client_from_env — local-first with opt-in remote"
```

---

## Task 12: Wire routing into the CLI `extract` path

**Files:**
- Modify: `crates/anno-rag-bin/src/review.rs:98-106` (add flag), `:282-284` (swap client)

- [ ] **Step 1: Add `--allow-remote-llm` to the `Extract` subcommand**

In `ReviewCmd::Extract { … }` (around line 98), add a field:

```rust
        /// Allow a remote LLM fallback for columns the local model cannot
        /// fill. OFF by default: extraction stays 100% local unless set.
        /// The remote prompt is still gated by the PII safety check.
        #[arg(long, default_value_t = false)]
        allow_remote_llm: bool,
```

Update the `ReviewCmd::Extract` match arm in `run()` (around line 140):

```rust
        ReviewCmd::Extract { review, force, allow_remote_llm } => {
            cmd_extract(cfg, ReviewId(review), force, allow_remote_llm).await
        }
```

- [ ] **Step 2: Update `cmd_extract` signature + client construction**

Change `async fn cmd_extract(cfg: &AnnoRagConfig, review_id: ReviewId, force: bool)` to take the flag:

```rust
async fn cmd_extract(
    cfg: &AnnoRagConfig,
    review_id: ReviewId,
    force: bool,
    allow_remote_llm: bool,
) -> anyhow::Result<()> {
```

Replace the two lines (currently 282-283):

```rust
    let llm_box = default_from_env().map_err(|e| anyhow::anyhow!("LLM client init failed: {e}"))?;
    let llm: Arc<dyn anno_rag_tabular::llm::LlmClient> = Arc::from(llm_box);
```

with:

```rust
    if allow_remote_llm {
        eprintln!("⚠  Remote LLM fallback ENABLED — pseudonymized prompts may transit to the remote API (PII-gated).");
    } else {
        eprintln!("🔒 Local-only extraction (no remote LLM). Pass --allow-remote-llm to enable a PII-gated fallback.");
    }
    let llm_box = anno_rag_tabular::llm::routing_client_from_env(allow_remote_llm)
        .map_err(|e| anyhow::anyhow!("LLM client init failed: {e}"))?;
    let llm: Arc<dyn anno_rag_tabular::llm::LlmClient> = Arc::from(llm_box);
```

- [ ] **Step 3: Update the import**

In the `use anno_rag_tabular::{…}` block, remove `llm::default_from_env,` (no longer used here) — leave the rest. If `default_from_env` is referenced nowhere else in the file, dropping it avoids an unused-import warning.

- [ ] **Step 4: Build the binary crate**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag-bin -Mode check`
Expected: compiles with no errors. Fix any signature/import mismatch reported.

- [ ] **Step 5: Commit**

```bash
git add crates/anno-rag-bin/src/review.rs
git commit -m "feat(cli): local-first extraction; --allow-remote-llm opt-in"
```

---

## Task 13: Wire routing into the MCP `extract` handlers

**Files:**
- Modify: `crates/anno-rag-mcp/src/lib.rs` (call sites ~2752 and ~4214)

- [ ] **Step 1: Replace both `default_from_env()` call sites**

At each site currently reading:

```rust
        let llm_client = match anno_rag_tabular::llm::default_from_env() {
```

(and the second one `let llm = match anno_rag_tabular::llm::default_from_env() {`)

replace the `default_from_env()` call with the local-first factory. The MCP server has no per-call flag, so default to local-only (no remote) — the privacy-by-default posture for the server:

```rust
        let llm_client = match anno_rag_tabular::llm::routing_client_from_env(false) {
```

and:

```rust
        let llm = match anno_rag_tabular::llm::routing_client_from_env(false) {
```

Leave the surrounding `match` error handling unchanged.

- [ ] **Step 2: Confirm the `gliner2` feature reaches anno-rag-mcp**

In `crates/anno-rag-mcp/Cargo.toml`, ensure the `anno-rag-tabular` dependency enables `gliner2` (its default). If `anno-rag-tabular` is pulled with `default-features = false` anywhere, add `features = ["gliner2"]`. Check with:

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag-mcp -Mode check`
Expected: compiles. If it errors that `routing_client_from_env` is the `not(gliner2)` variant returning an error path, that's still valid compilation — the feature wiring is what matters at runtime.

- [ ] **Step 3: Commit**

```bash
git add crates/anno-rag-mcp/src/lib.rs crates/anno-rag-mcp/Cargo.toml
git commit -m "feat(mcp): local-first extraction by default (no silent remote LLM)"
```

---

## Task 14: Security regression test

**Files:**
- Create: `crates/anno-rag-tabular/tests/security_routing.rs`
- Reference: `crates/anno-rag-tabular/src/llm/routing.rs`, `src/llm/mock.rs`

- [ ] **Step 1: Write the test — remote is never called when not opted in**

```rust
//! Security regression: the local-first routing client must NOT call the
//! remote fallback unless one was explicitly attached (allow_remote=true).

use anno_rag_tabular::llm::local::client::{LocalEntity, LocalEntityExtractor, LocalTabularClient};
use anno_rag_tabular::llm::routing::RoutingLlmClient;
use anno_rag_tabular::llm::{LlmClient, StructuredOutput, Usage};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

struct CountingRemote(Arc<AtomicUsize>);

#[async_trait]
impl LlmClient for CountingRemote {
    async fn generate_structured(
        &self,
        _s: &str,
        _u: &str,
        _schema: &Value,
    ) -> anno_rag_tabular::error::Result<StructuredOutput> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(StructuredOutput { value: json!({}), usage: Usage::default() })
    }
    fn model_id(&self) -> &str { "counting-remote" }
}

struct NoopExtractor;
impl LocalEntityExtractor for NoopExtractor {
    fn extract(&self, _t: &str, _l: &[(&str, &str)], _th: f32)
        -> anno_rag_tabular::error::Result<Vec<LocalEntity>> {
        Ok(vec![])
    }
}

#[tokio::test]
async fn no_remote_call_when_fallback_absent() {
    let local: Box<dyn LlmClient> = Box::new(LocalTabularClient::new(Box::new(NoopExtractor)));
    let router = RoutingLlmClient::new(local, None); // allow_remote == false equivalent
    let _ = router
        .generate_structured("sys", "[CHUNK::x]hello[/CHUNK]", &json!({}))
        .await
        .expect("local-only call succeeds");
    // No remote attached → nothing to count, but the call must not panic
    // or attempt network IO. Reaching here is the assertion.
}

#[tokio::test]
async fn remote_called_only_when_attached_and_safe() {
    let counter = Arc::new(AtomicUsize::new(0));
    let local: Box<dyn LlmClient> = Box::new(LocalTabularClient::new(Box::new(NoopExtractor)));
    let remote: Box<dyn LlmClient> = Box::new(CountingRemote(Arc::clone(&counter)));
    let router = RoutingLlmClient::new(local, Some(remote));
    // A PII-free prompt passes the safety gate → remote IS consulted.
    let _ = router
        .generate_structured("sys", "[CHUNK::x]governing law is France[/CHUNK]", &json!({}))
        .await
        .expect("call succeeds");
    assert_eq!(counter.load(Ordering::SeqCst), 1, "remote must be consulted once when attached + safe");
}
```

- [ ] **Step 2: Run the security tests**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag-tabular -- --test security_routing`
Expected: both tests PASS. If `remote_called_only_when_attached_and_safe` fails because the prompt trips the PII gate, change the chunk text to a clearly PII-free string (e.g. `"the term is 24 months"`).

- [ ] **Step 3: Commit**

```bash
git add crates/anno-rag-tabular/tests/security_routing.rs
git commit -m "test(tabular): security regression for local-first routing"
```

---

## Task 15: Documentation

**Files:**
- Modify: `crates/anno-rag/README.md`

- [ ] **Step 1: Add a DD library section to the README**

Insert after the "## CLI" table:

```markdown
## Due-Diligence Template Library (legal-intl)

Bilingual (FR/EN) M&A / LBO / JV due-diligence presets. Each grid ends with an
evidence triplet (`source_reference`, `key_risk_flag`, `risk_note`); two
transversal registers aggregate findings.

Grids: `spa-v1`, `sha-v1`, `jva-v1`, `senior-facilities-v1`, `security-package-v1`,
`intercreditor-v1`, `material-commercial-v1`, `commercial-lease-v1`,
`employment-exec-v1`, `ip-portfolio-v1`, `litigation-v1`, `insurance-v1`,
`data-protection-v1`, `debt-finance-v1`, `corporate-captable-v1`.
Registers: `data-room-index-v1`, `red-flags-register-v1`.

```sh
# Ingest with maximum PII recall before extraction:
ANNO_GDPR_LAYERS=defense anno-rag ingest ~/deal-acme --recursive

# Create a review from a template, add documents, extract (LOCAL by default):
anno-rag review create --name "Acme SPA" --template spa-v1
anno-rag review add-rows --review <id> --folder-path "Acme/01_SPA" --doc-ids <uuid,uuid>
anno-rag review extract --review <id>          # 100% local, no network
anno-rag review extract --review <id> --allow-remote-llm   # PII-gated remote fallback
anno-rag review export --review <id> --format xlsx --output acme-spa.xlsx
```

**Privacy:** extraction is local-first by default. Chunks are pseudonymized at
ingest; a remote fallback transits pseudonymized text ONLY with
`--allow-remote-llm`, and even then every remote prompt passes a PII safety
gate. The MCP server is always local-only.
```

- [ ] **Step 2: Commit**

```bash
git add crates/anno-rag/README.md
git commit -m "docs(readme): document DD template library + local-first extraction"
```

---

## Task 16: Final verification

- [ ] **Step 1: Full tabular test suite**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag-tabular -Features gliner2`
Expected: all tests PASS, including `dd_templates` and `security_routing`.

- [ ] **Step 2: Check the dependent binaries compile**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag-bin -Mode check`
Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag-mcp -Mode check`
Expected: both compile cleanly.

- [ ] **Step 3: GitNexus scope check before final commit**

Run: `npx gitnexus detect-change` (or `npx gitnexus status` + `git diff --name-status`)
Expected: changes confined to `crates/anno-rag-tabular/**`, `crates/anno-rag-bin/src/review.rs`, `crates/anno-rag-mcp/src/lib.rs` (+ Cargo.toml), `crates/anno-rag/README.md`, and the spec/plan docs.

- [ ] **Step 4: Final commit (if anything uncommitted)**

```bash
git add -A
git commit -m "chore(tabular): DD template library complete (17 templates + local-first security)"
```

---

## Notes for the implementer

- **CellTypeWire enum variants** the templates use: `text`, `date`, `verbatim`, `boolean`, `number`, `currency { code }`, `enum { options }`. The DD templates avoid `currency` in favour of `number` + `enum` currency pairs — do not introduce `currency` cells.
- **Do not** run `cargo test --workspace` or `cargo build --release` locally (see CLAUDE.md build-isolation rules). Use the `scripts/*.ps1` helpers, which isolate `CARGO_TARGET_DIR`.
- **Guard concurrent builds**: before any build, `Get-Process cargo,rustc -ErrorAction SilentlyContinue`.
- The `extract` runtime requires downloaded models (`anno-rag download-models`) and a populated index; the tests in this plan do NOT require either (they test template loading + routing logic with doubles).
- If `RoutingLlmClient` token-usage aggregation or merge semantics change, re-check Task 14 expectations.
