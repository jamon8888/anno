# Anno Tabular — Due Diligence Template Library Design

**Date:** 2026-06-06
**Status:** Design — pending user review
**Scope:** Bilingual (FR/EN), multi-jurisdiction due-diligence template library for `anno-rag-tabular`, covering M&A, LBO, and JV transactions.

---

## Goal

Ship a Legora-style tabular review library: a set of reusable TOML schema presets that let a lawyer drop a data room into `anno-rag review` and get traceable, citation-backed grids across every DD workstream — plus two transversal registers (data-room index + red-flags register) that turn a pile of grids into a deliverable.

## Positioning (why this maps to Legora)

Legora's core is **Tabular Review**: rows = documents, columns = extracted fields, with a source citation in every cell. The 15 abstraction grids reproduce that. What makes it a *DD product* rather than a contract abstracter is the **aggregation layer**: the data-room index (what we received / what's missing) and the red-flags register (the client deliverable). Workstream is a *pivot column*, not a separate grid — more flexible than the traditional workstream-structured DD report.

## Architecture

- **Format:** TOML presets in `crates/anno-rag-tabular/src/templates/`, following the existing `Template` / `TemplateColumn` / `CellTypeWire` shape (`schema/template.rs`).
- **No code changes to the engine** are required for the abstraction grids and registers — they use existing cell types (`text`, `date`, `verbatim`, `boolean`, `number`, `currency`, `enum`).
- **New `vertical` tag:** `"legal-intl"` (alongside existing `"legal-fr"`), so the picker can filter the international DD set.
- **Prompts in English**, column keys in `snake_case` English, for portability across cross-border data rooms.

## Cross-template conventions

### Evidence triplet (every abstraction grid ends with these three)

| Column | Type | Prompt intent |
|--------|------|---------------|
| `source_reference` | `verbatim` | Echo the single most material clause/provision verbatim (the citation). |
| `key_risk_flag` | `boolean` | Does this document warrant a red flag for the deal? |
| `risk_note` | `text` | One-line explanation of the risk; feeds the red-flags register. |

### Shared enums

```
workstream  = ["corporate","commercial","employment","ip_it","real_estate",
               "litigation","regulatory","finance_debt","tax",
               "data_protection","insurance","environmental"]
severity    = ["critical","high","medium","low","info"]
dr_status   = ["received","missing","partial","na"]
flag_status = ["open","mitigated","accepted","resolved"]
governing_law = ["FR","DE","UK","US","CH","BE","LU","NL","ES","IT","SE","OTHER"]
currency    = ["EUR","USD","GBP","CHF","OTHER"]
```

### Multi-currency amounts

Financial templates use a **pair**: `*_amount` (`number`) + `*_currency` (`enum: currency`) instead of a fixed-code `currency` cell, to handle cross-border data rooms.

### Cell-type usage

`date` → dates · `boolean` → clause presence · `verbatim` → clauses/citations · `number` → durations / % / amounts · `enum` → closed lists · `text` → free synthesis.

---

## Abstraction grids (15)

Each table below lists the template-specific columns. The **evidence triplet** (`source_reference`, `key_risk_flag`, `risk_note`) is appended to every grid and omitted from the tables for brevity.

### 1. `spa-v1` — Share Purchase Agreement (M&A / LBO)

| Column | Type | Notes |
|--------|------|-------|
| `target` | text | Target entity (legal name + form). |
| `seller` / `buyer` | text | Parties. |
| `signing_date` / `completion_date` | date | |
| `purchase_price_amount` / `purchase_price_currency` | number / enum | |
| `price_mechanism` | enum | `["locked_box","completion_accounts","fixed"]` |
| `earn_out` | boolean | |
| `earn_out_terms` | verbatim | If present. |
| `conditions_precedent` | verbatim | |
| `mac_clause` | boolean | Material Adverse Change. |
| `mac_terms` | verbatim | |
| `reps_warranties_scope` | verbatim | |
| `warranty_cap_amount` / `warranty_cap_currency` | number / enum | |
| `de_minimis_amount` / `basket_amount` | number / number | |
| `survival_period_months` | number | Claims survival. |
| `indemnities` | verbatim | Specific indemnities. |
| `restrictive_covenants` | verbatim | Non-compete / non-solicit. |
| `governing_law` | enum | governing_law |
| `jurisdiction` | verbatim | |

### 2. `sha-v1` — Shareholders Agreement / Pacte d'actionnaires (M&A / JV)

| Column | Type | Notes |
|--------|------|-------|
| `parties` | text | |
| `effective_date` | date | |
| `share_classes` | verbatim | |
| `board_composition` | verbatim | Governance. |
| `reserved_matters` | verbatim | Veto/supermajority list. |
| `preemption_right` | boolean | |
| `tag_along` / `drag_along` | boolean / boolean | |
| `transfer_restrictions` | verbatim | Lock-up etc. |
| `deadlock_mechanism` | verbatim | |
| `exit_provisions` | verbatim | IPO / trade sale / put-call. |
| `non_compete` | boolean | |
| `change_of_control` | boolean | |
| `term` | verbatim | |
| `governing_law` | enum | governing_law |

### 3. `jva-v1` — Joint Venture Agreement (JV)

| Column | Type | Notes |
|--------|------|-------|
| `jv_parties` | text | |
| `jv_vehicle` | text | NewCo / contractual JV. |
| `effective_date` | date | |
| `scope_purpose` | verbatim | Business scope. |
| `contributions` | verbatim | Cash / assets / IP per party. |
| `ownership_split` | text | % per party. |
| `governance` | verbatim | Board / management. |
| `profit_sharing` | verbatim | |
| `funding_obligations` | verbatim | Future funding. |
| `non_compete` | boolean | |
| `non_compete_terms` | verbatim | |
| `exclusivity` | boolean | |
| `deadlock_mechanism` | verbatim | |
| `exit_terms` | verbatim | |
| `term` | verbatim | |
| `governing_law` | enum | governing_law |

### 4. `senior-facilities-v1` — Senior Facilities Agreement (LBO)

| Column | Type | Notes |
|--------|------|-------|
| `borrower` / `lenders` | text | |
| `agreement_date` | date | |
| `facility_tranches` | verbatim | Term A/B, RCF, capex. |
| `total_commitment_amount` / `total_commitment_currency` | number / enum | |
| `margin` | verbatim | Pricing grid. |
| `maturity_date` | date | |
| `financial_covenants` | verbatim | Leverage, ICR, etc. |
| `covenant_levels` | verbatim | Thresholds. |
| `mandatory_prepayment` | verbatim | Excess cashflow, disposals. |
| `change_of_control` | boolean | |
| `mac_clause` | boolean | |
| `events_of_default` | verbatim | Key EoDs. |
| `security_summary` | verbatim | Cross-ref to security package. |
| `governing_law` | enum | governing_law |

### 5. `security-package-v1` — Security / Sûretés (LBO)

| Column | Type | Notes |
|--------|------|-------|
| `grantor` / `secured_party` | text | |
| `security_type` | enum | `["share_pledge","asset_pledge","mortgage","dailly_assignment","fiducie","floating_charge","guarantee","other"]` |
| `secured_asset` | text | What is pledged. |
| `secured_amount` / `secured_currency` | number / enum | |
| `ranking` | enum | `["first","second","junior","pari_passu","other"]` |
| `perfection_status` | enum | `["registered","pending","unperfected","na"]` |
| `grant_date` | date | |
| `release_conditions` | verbatim | |
| `governing_law` | enum | governing_law |

### 6. `intercreditor-v1` — Intercreditor / Mezzanine (LBO)

| Column | Type | Notes |
|--------|------|-------|
| `parties` | text | Senior / mezz / hedge / shareholder. |
| `agreement_date` | date | |
| `ranking_waterfall` | verbatim | Priority of payments. |
| `subordination_terms` | verbatim | |
| `standstill_period` | verbatim | |
| `payment_blockage` | verbatim | |
| `enforcement_control` | verbatim | Who controls enforcement. |
| `turnover_provisions` | boolean | |
| `governing_law` | enum | governing_law |

### 7. `material-commercial-v1` — Material customer/supplier contracts (all)

| Column | Type | Notes |
|--------|------|-------|
| `counterparty` | text | |
| `contract_type` | enum | `["customer","supplier","distribution","framework","other"]` |
| `effective_date` / `expiry_date` | date / date | |
| `term` | verbatim | |
| `auto_renewal` | boolean | |
| `annual_value_amount` / `annual_value_currency` | number / enum | |
| `exclusivity` | boolean | |
| `minimum_commitments` | verbatim | |
| `change_of_control` | boolean | CoC termination/consent. |
| `termination_rights` | verbatim | |
| `liability_cap` | verbatim | |
| `governing_law` | enum | governing_law |

### 8. `commercial-lease-v1` — Commercial leases / baux commerciaux (all)

| Column | Type | Notes |
|--------|------|-------|
| `landlord` / `tenant` | text | |
| `premises` | text | Address + surface. |
| `lease_type` | enum | `["commercial_369","derogatory","professional","other"]` |
| `commencement_date` | date | |
| `term_years` | number | |
| `annual_rent_amount` / `annual_rent_currency` | number / enum | |
| `permitted_use` | verbatim | Destination. |
| `assignment_sublet` | verbatim | Cession/sous-location. |
| `change_of_control` | boolean | |
| `restoration_obligations` | verbatim | |
| `break_options` | verbatim | |
| `governing_law` | enum | governing_law |

### 9. `employment-exec-v1` — Executives & corporate officers (all)

| Column | Type | Notes |
|--------|------|-------|
| `name_role` | text | Pseudonymized via vault. |
| `contract_type` | enum | `["employment","mandate","consulting","other"]` |
| `start_date` | date | |
| `base_comp_amount` / `base_comp_currency` | number / enum | |
| `variable_comp` | verbatim | Bonus / LTIP. |
| `equity_incentives` | verbatim | BSPCE / stock options / shares. |
| `severance_terms` | verbatim | Golden parachute. |
| `change_of_control_trigger` | boolean | CoC acceleration/severance. |
| `non_compete` | boolean | |
| `non_compete_indemnity` | verbatim | |
| `notice_period` | verbatim | |
| `governing_law` | enum | governing_law |

### 10. `ip-portfolio-v1` — IP / IT (all)

| Column | Type | Notes |
|--------|------|-------|
| `asset_name` | text | |
| `ip_type` | enum | `["patent","trademark","copyright","domain","trade_secret","software","other"]` |
| `ownership` | enum | `["owned","licensed_in","licensed_out","joint","disputed"]` |
| `registration_status` | verbatim | |
| `jurisdiction_coverage` | text | Territories. |
| `license_terms` | verbatim | If licensed. |
| `open_source_usage` | boolean | |
| `open_source_terms` | verbatim | Copyleft exposure. |
| `change_of_control` | boolean | CoC in licenses. |
| `encumbrances` | verbatim | Security / disputes. |
| `governing_law` | enum | governing_law |

### 11. `litigation-v1` — Litigation & disputes (all)

| Column | Type | Notes |
|--------|------|-------|
| `matter_reference` | text | |
| `parties` | text | |
| `dispute_type` | enum | `["commercial","employment","ip","tax","regulatory","product","other"]` |
| `status` | enum | `["pending","threatened","settled","appeal","closed"]` |
| `forum` | verbatim | Court / arbitration. |
| `amount_at_stake_amount` / `amount_at_stake_currency` | number / enum | |
| `provision_booked_amount` / `provision_booked_currency` | number / enum | Accounting provision. |
| `commenced_date` | date | |
| `summary` | text | |
| `governing_law` | enum | governing_law |

### 12. `insurance-v1` — Insurance policies (all)

| Column | Type | Notes |
|--------|------|-------|
| `insurer` | text | |
| `policy_type` | enum | `["liability","property","d_and_o","cyber","credit","w_and_i","other"]` |
| `policy_number` | text | |
| `coverage_limit_amount` / `coverage_limit_currency` | number / enum | |
| `deductible_amount` / `deductible_currency` | number / enum | |
| `period_start` / `period_end` | date / date | |
| `key_exclusions` | verbatim | |
| `claims_history` | verbatim | |
| `change_of_control` | boolean | Policy continuity on CoC. |

### 13. `data-protection-v1` — GDPR / DPA (all)

| Column | Type | Notes |
|--------|------|-------|
| `counterparty` | text | |
| `role` | enum | `["controller","processor","joint_controller","na"]` |
| `dpa_in_place` | boolean | |
| `processing_purpose` | verbatim | |
| `data_categories` | verbatim | Incl. special categories. |
| `international_transfers` | boolean | |
| `transfer_mechanism` | enum | `["sccs","adequacy","bcr","derogation","none","na"]` |
| `subprocessors` | verbatim | |
| `breach_history` | verbatim | |
| `dpia_done` | boolean | AIPD. |
| `retention_terms` | verbatim | |

### 14. `debt-finance-v1` — Existing debt (all)

| Column | Type | Notes |
|--------|------|-------|
| `lender` | text | |
| `instrument_type` | enum | `["term_loan","revolving","bond","leasing","factoring","shareholder_loan","other"]` |
| `principal_amount` / `principal_currency` | number / enum | |
| `outstanding_amount` / `outstanding_currency` | number / enum | |
| `interest_terms` | verbatim | |
| `maturity_date` | date | |
| `financial_covenants` | verbatim | |
| `change_of_control` | boolean | CoC / acceleration. |
| `security_granted` | verbatim | |
| `governing_law` | enum | governing_law |

### 15. `corporate-captable-v1` — Capital structure (all)

| Column | Type | Notes |
|--------|------|-------|
| `holder` | text | Shareholder. |
| `share_class` | text | Ordinary / preferred / etc. |
| `shares_held` | number | |
| `ownership_pct` | number | %. |
| `fully_diluted_pct` | number | |
| `instrument_type` | enum | `["shares","options","warrants","bsa","bspce","convertible","other"]` |
| `vesting_terms` | verbatim | For options/equity. |
| `encumbrances` | verbatim | Pledges over the shares. |
| `transfer_restrictions` | verbatim | |
| `voting_rights` | verbatim | |

---

## Transversal registers (2)

### 16. `data-room-index-v1` — Data-room inventory (one row per document)

| Column | Type | Notes |
|--------|------|-------|
| `document_title` | text | |
| `doc_type` | enum | Maps to the 15 grids: `["spa","sha","jva","facilities","security","intercreditor","commercial","lease","employment","ip","litigation","insurance","data_protection","debt","corporate","other"]` |
| `workstream` | enum | workstream |
| `language` | enum | `["fr","en","de","other"]` |
| `document_date` | date | |
| `parties` | text | |
| `status` | enum | dr_status (`received/missing/partial/na`) |
| `priority` | enum | `["high","medium","low"]` |
| `page_count` | number | |
| `notes` | text | |

### 17. `red-flags-register-v1` — Red-flags register (one row per issue)

| Column | Type | Notes |
|--------|------|-------|
| `issue_title` | text | |
| `workstream` | enum | workstream |
| `severity` | enum | severity (`critical/high/medium/low/info`) |
| `description` | text | The risk. |
| `source_citation` | verbatim | The exact clause (Legora "show me where"). |
| `source_document` | text | Which exhibit. |
| `impact` | text | Deal impact. |
| `recommendation` | text | Mitigation / SPA protection. |
| `status` | enum | flag_status (`open/mitigated/accepted/resolved`) |
| `owner` | text | Responsible advisor. |

---

## Data flow

1. Lawyer ingests the data room (`anno-rag ingest`), PII pseudonymized via the local vault.
2. For each document type, lawyer runs `anno-rag review --template <id>` → grid with the evidence triplet per row.
3. Rows with `key_risk_flag = true` are transcribed into `red-flags-register-v1` (manual or future automation).
4. `data-room-index-v1` is filled across all documents to surface gaps (`status = missing/partial`).
5. Export to XLSX/CSV/Markdown for the client deliverable.

## Privacy

All extraction stays local; raw PII remains in the encrypted vault. Grids, registers, and exports use pseudonymized text unless explicitly rehydrated on the local machine. The `name_role` column in `employment-exec-v1` is pseudonymized like any other PII.

## Testing

- **Schema validation test:** load all 17 TOML templates via `Template` deserialization; assert no parse errors, unique ids, valid `vertical`, every abstraction grid contains the evidence triplet.
- **Enum consistency test:** assert shared enums (`workstream`, `severity`, `governing_law`, `currency`) are identical across templates that use them.
- **Snapshot test:** one representative grid (`spa-v1`) extracted against a fixture contract, asserting column count and types.

## Out of scope (v1)

- Automatic flag → register propagation (manual transcription in v1; automation is a follow-up).
- Per-cell automatic citation rendering in the UI (relies on the `verbatim`/`source_reference` columns for now).
- Workstream-structured DD report generator (the register pivots cover this).
- Additional jurisdictions beyond the `governing_law` enum (extend the enum as needed).
