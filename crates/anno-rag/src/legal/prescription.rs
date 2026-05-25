//! French prescription (statute-of-limitations) engine.
//!
//! Implements the 8 seeded French prescription rules (Code civil 2224 et seq.)
//! with interruption logic. Interrupting events (mise en demeure, assignation,
//! reconnaissance de dette, etc.) restart the prescription period from their
//! date.
//!
//! # Prescription periods (Code civil)
//! | Category | Period | Article |
//! |---|---|---|
//! | contractuel | 5 ans | C.civ.:2224 |
//! | quasi_contrat | 5 ans | C.civ.:2224 |
//! | delictuel | 5 ans | C.civ.:2224 |
//! | responsabilite_decennale | 10 ans | C.civ.:1792-4-1 |
//! | biennale_consommation | 2 ans | C.conso.:L218-2 |
//! | garantie_vices | 2 ans | C.civ.:1648 |
//! | action_prud_homale | 2 ans | C.trav.:L1471-1 |
//! | prescription_penale_crime | 20 ans | CPP:7 |

use chrono::{DateTime, Datelike, Timelike, Utc};
use serde::{Deserialize, Serialize};

// ── Types ─────────────────────────────────────────────────────────────────────

/// An event that interrupts (restarts) the prescription period.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InterruptingEvent {
    /// Event kind, e.g. `"mise_en_demeure"`, `"assignation"`,
    /// `"reconnaissance_de_dette"`.
    pub kind: String,
    /// Date on which the interruption occurred.
    pub date: DateTime<Utc>,
}

/// Result of a prescription computation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PrescriptionResult {
    /// The prescription category supplied to the computation.
    pub category: String,
    /// Date on which the right becomes time-barred (exclusive).
    pub prescribes_on: DateTime<Utc>,
    /// Duration of the applicable prescription period, in years.
    pub years: u32,
    /// Code civil or code article reference.
    pub article_ref: String,
    /// Interrupting events that were taken into account. The last event
    /// determines the start of the running period.
    pub interrupted_by: Vec<InterruptingEvent>,
    /// True if the computed `prescribes_on` is already in the past relative
    /// to the `reference_date` supplied (default: now).
    pub is_prescribed: bool,
}

/// Rule entry in the static rule table.
struct Rule {
    category: &'static str,
    years: u32,
    article_ref: &'static str,
}

// ── Rule table ────────────────────────────────────────────────────────────────

static RULES: &[Rule] = &[
    Rule {
        category: "contractuel",
        years: 5,
        article_ref: "code_civil:2224",
    },
    Rule {
        category: "quasi_contrat",
        years: 5,
        article_ref: "code_civil:2224",
    },
    Rule {
        category: "delictuel",
        years: 5,
        article_ref: "code_civil:2224",
    },
    Rule {
        category: "responsabilite_decennale",
        years: 10,
        article_ref: "code_civil:1792-4-1",
    },
    Rule {
        category: "biennale_consommation",
        years: 2,
        article_ref: "code_consommation:L218-2",
    },
    Rule {
        category: "garantie_vices",
        years: 2,
        article_ref: "code_civil:1648",
    },
    Rule {
        category: "action_prud_homale",
        years: 2,
        article_ref: "code_travail:L1471-1",
    },
    Rule {
        category: "prescription_penale_crime",
        years: 20,
        article_ref: "cpp:7",
    },
];

// ── Computation ───────────────────────────────────────────────────────────────

/// Compute the prescription deadline for `category`, starting from
/// `event_date`, taking `interrupting_events` into account.
///
/// If any interrupting events are supplied, the period restarts from the
/// **latest** interrupting event's date. The `is_prescribed` flag is set
/// relative to `Utc::now()`.
///
/// Returns `None` when the `category` is not found in the rule table.
#[must_use]
pub fn compute_prescription(
    category: &str,
    event_date: DateTime<Utc>,
    interrupting_events: &[InterruptingEvent],
) -> Option<PrescriptionResult> {
    compute_prescription_at(category, event_date, interrupting_events, Utc::now())
}

/// Like [`compute_prescription`] but uses `reference_date` instead of
/// `Utc::now()` for the `is_prescribed` flag. Useful in tests.
#[must_use]
pub fn compute_prescription_at(
    category: &str,
    event_date: DateTime<Utc>,
    interrupting_events: &[InterruptingEvent],
    reference_date: DateTime<Utc>,
) -> Option<PrescriptionResult> {
    let rule = RULES.iter().find(|r| r.category == category)?;

    // The start of the running period is the latest interrupting event, or
    // the original event_date when there are none.
    let mut sorted = interrupting_events.to_vec();
    sorted.sort_by_key(|e| e.date);

    let start = sorted.last().map(|e| e.date).unwrap_or(event_date);

    let prescribes_on = add_years(start, rule.years);
    let is_prescribed = prescribes_on <= reference_date;

    Some(PrescriptionResult {
        category: category.to_string(),
        prescribes_on,
        years: rule.years,
        article_ref: rule.article_ref.to_string(),
        interrupted_by: sorted,
        is_prescribed,
    })
}

/// Add `years` to `dt`, handling February-29 gracefully by clamping to
/// February-28 on non-leap years.
fn add_years(dt: DateTime<Utc>, years: u32) -> DateTime<Utc> {
    use chrono::TimeZone;
    let target_year = dt.year() + years as i32;
    // Try exact date first; fall back to -1 day (Feb 28 for leap-day input).
    Utc.with_ymd_and_hms(
        target_year,
        dt.month(),
        dt.day(),
        dt.hour(),
        dt.minute(),
        dt.second(),
    )
    .single()
    .unwrap_or_else(|| {
        Utc.with_ymd_and_hms(
            target_year,
            dt.month(),
            dt.day() - 1,
            dt.hour(),
            dt.minute(),
            dt.second(),
        )
        .unwrap()
    })
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn d(year: i32, month: u32, day: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(year, month, day, 0, 0, 0).unwrap()
    }

    #[test]
    fn breach_contractuel_prescribes_5_years_from_event() {
        let event_date = d(2020, 1, 1);
        let r = compute_prescription("contractuel", event_date, &[]).unwrap();
        assert_eq!(r.prescribes_on.year(), 2025);
        assert_eq!(r.article_ref, "code_civil:2224");
        assert_eq!(r.years, 5);
    }

    #[test]
    fn mise_en_demeure_interrupts_prescription() {
        let event = d(2020, 1, 1);
        let mise = d(2024, 6, 1);
        let r = compute_prescription(
            "contractuel",
            event,
            &[InterruptingEvent {
                kind: "mise_en_demeure".into(),
                date: mise,
            }],
        )
        .unwrap();
        // New 5-year period starts from mise en demeure date.
        assert_eq!(r.prescribes_on.year(), 2029);
        assert!(r.interrupted_by.iter().any(|e| e.kind == "mise_en_demeure"));
    }

    #[test]
    fn multiple_interruptions_use_latest_date() {
        let event = d(2020, 1, 1);
        let first_mise = d(2022, 3, 1);
        let second_mise = d(2023, 9, 15);
        let r = compute_prescription(
            "contractuel",
            event,
            &[
                InterruptingEvent {
                    kind: "mise_en_demeure".into(),
                    date: first_mise,
                },
                InterruptingEvent {
                    kind: "assignation".into(),
                    date: second_mise,
                },
            ],
        )
        .unwrap();
        // Period restarts from 2023-09-15 → prescribes 2028-09-15.
        assert_eq!(r.prescribes_on.year(), 2028);
    }

    #[test]
    fn decennale_prescribes_after_10_years() {
        let event = d(2015, 6, 1);
        let r = compute_prescription("responsabilite_decennale", event, &[]).unwrap();
        assert_eq!(r.prescribes_on.year(), 2025);
        assert_eq!(r.article_ref, "code_civil:1792-4-1");
    }

    #[test]
    fn biennale_consommation_prescribes_after_2_years() {
        let event = d(2023, 1, 15);
        let r = compute_prescription("biennale_consommation", event, &[]).unwrap();
        assert_eq!(r.prescribes_on.year(), 2025);
    }

    #[test]
    fn unknown_category_returns_none() {
        let r = compute_prescription("invalid_category", d(2020, 1, 1), &[]);
        assert!(r.is_none());
    }

    #[test]
    fn is_prescribed_true_for_past_deadline() {
        let event = d(2010, 1, 1);
        let reference = d(2026, 1, 1);
        let r = compute_prescription_at("contractuel", event, &[], reference).unwrap();
        // 2010 + 5 = 2015, which is before 2026.
        assert!(r.is_prescribed);
    }

    #[test]
    fn is_prescribed_false_for_future_deadline() {
        let event = d(2024, 1, 1);
        let reference = d(2026, 1, 1);
        let r = compute_prescription_at("contractuel", event, &[], reference).unwrap();
        // 2024 + 5 = 2029, which is after 2026.
        assert!(!r.is_prescribed);
    }
}
