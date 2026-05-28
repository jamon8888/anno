//! Deterministic value normalizers for local extraction.
//!
//! Each normalizer takes a raw extracted span and the column's [`CellType`]
//! and returns a `serde_json::Value` in the same shape the LLM would
//! produce for that cell type, or `None` when the span cannot be mapped
//! (e.g. an enum value not in the allowed list).

use crate::schema::CellType;
use regex::Regex;
use serde_json::{json, Value};

/// Normalise `raw` (a GLiNER-extracted text span) into the JSON value
/// shape expected for `cell_type`.  Returns `None` when the span does
/// not match the expected shape and should be discarded.
pub fn normalize_value(raw: &str, cell_type: &CellType) -> Option<Value> {
    let cleaned = raw.trim();
    match cell_type {
        CellType::Text | CellType::Verbatim => Some(json!(cleaned)),
        CellType::Number => normalize_number(cleaned).map(|n| json!(n)),
        CellType::Currency { code } => normalize_currency(cleaned, code),
        CellType::Enum { options } => options
            .iter()
            .find(|opt| opt.eq_ignore_ascii_case(cleaned))
            .map(|opt| json!(opt)),
        CellType::Boolean => None, // boolean requires reasoning, not span matching
        CellType::Date => normalize_date(cleaned),
    }
}

fn normalize_number(raw: &str) -> Option<f64> {
    // Accepts French number formats: "12 000", "12.000", "12 000,50"
    let re = Regex::new(r"(?P<num>\d+(?:[\s.]\d{3})*(?:,\d+)?)").ok()?;
    let cap = re.captures(raw)?;
    cap.name("num")?
        .as_str()
        .replace(' ', "")
        .replace('.', "")
        .replace(',', ".")
        .parse()
        .ok()
}

fn normalize_currency(raw: &str, code: &str) -> Option<Value> {
    let amount = normalize_number(raw)?;
    Some(json!({ "amount": amount, "code": code }))
}

/// Normalise a French date string like "1er janvier 2026" or
/// "15 mars 2025" to ISO 8601 ("2026-01-01").
fn normalize_date(raw: &str) -> Option<Value> {
    let lowered = raw.to_lowercase();
    let months: &[(&str, &str)] = &[
        ("janvier", "01"),
        ("fevrier", "02"),
        ("février", "02"),
        ("mars", "03"),
        ("avril", "04"),
        ("mai", "05"),
        ("juin", "06"),
        ("juillet", "07"),
        ("aout", "08"),
        ("août", "08"),
        ("septembre", "09"),
        ("octobre", "10"),
        ("novembre", "11"),
        ("decembre", "12"),
        ("décembre", "12"),
    ];
    // Match "1er janvier 2026", "15 mars 2025", etc.
    let re =
        Regex::new(r"(?P<day>\d{1,2}|1er)\s+(?P<month>[a-záàâéèêëîïôùûüœç]+)\s+(?P<year>\d{4})")
            .ok()?;
    let cap = re.captures(&lowered)?;
    let day_raw = cap.name("day")?.as_str();
    let day: u32 = if day_raw == "1er" { 1 } else { day_raw.parse().ok()? };
    let month_name = cap.name("month")?.as_str();
    let month = months.iter().find(|(name, _)| *name == month_name)?.1;
    let year = cap.name("year")?.as_str();
    Some(json!(format!("{year}-{month}-{day:02}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_eur_currency() {
        let value =
            normalize_value("12 000 €", &CellType::Currency { code: "EUR".into() }).expect("ok");
        assert_eq!(value["amount"], 12000.0_f64);
        assert_eq!(value["code"], "EUR");
    }

    #[test]
    fn normalizes_french_date_to_iso() {
        let value = normalize_value("1er janvier 2026", &CellType::Date).expect("ok");
        assert_eq!(value, json!("2026-01-01"));
    }

    #[test]
    fn normalizes_numeric_date() {
        let value = normalize_value("15 mars 2025", &CellType::Date).expect("ok");
        assert_eq!(value, json!("2025-03-15"));
    }

    #[test]
    fn rejects_invalid_enum() {
        let cell_type = CellType::Enum { options: vec!["FR".into(), "OTHER".into()] };
        assert!(normalize_value("Allemagne", &cell_type).is_none());
    }

    #[test]
    fn accepts_case_insensitive_enum() {
        let cell_type = CellType::Enum { options: vec!["FR".into(), "OTHER".into()] };
        let v = normalize_value("fr", &cell_type).expect("ok");
        assert_eq!(v, json!("FR"));
    }

    #[test]
    fn normalizes_plain_number() {
        let v = normalize_value("42 500", &CellType::Number).expect("ok");
        assert_eq!(v, json!(42500.0_f64));
    }

    #[test]
    fn boolean_returns_none() {
        assert!(normalize_value("oui", &CellType::Boolean).is_none());
    }
}
