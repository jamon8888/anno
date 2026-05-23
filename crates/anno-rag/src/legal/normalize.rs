//! French legal normalization. Pure, model-free.

use crate::legal::types::{ArticleRef, PartyKind};
use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};

/// Canonical form for a party reference.
#[must_use]
pub fn canonical_party_form(raw: &str) -> (PartyKind, String) {
    let trimmed = raw.trim();
    let lower = trimmed.to_lowercase();
    let is_person_marker = ["monsieur ", "madame ", "m. ", "mme ", "mlle "]
        .iter()
        .any(|prefix| lower.starts_with(prefix));

    if is_person_marker {
        let cleaned = strip_person_marker(trimmed);
        let (last, first) = split_person_name(&cleaned);
        let slug = format!(
            "{}{}",
            slugify(&last),
            if first.is_empty() {
                String::new()
            } else {
                format!("_{}", slugify(&first))
            }
        );
        return (PartyKind::Person, format!("person:{slug}"));
    }

    let mut value = trimmed.to_string();
    if value.to_lowercase().starts_with("societe ") {
        value = value[8..].to_string();
    }
    if value.to_lowercase().starts_with("société ") {
        value = value[9..].to_string();
    }
    for suffix in ORG_SUFFIXES {
        value = value.replacen(suffix, "", 1);
    }
    if let Some(idx) = value.to_lowercase().find(" au capital ") {
        value.truncate(idx);
    }
    let slug = slugify(value.trim());
    (PartyKind::Organization, format!("org:{slug}"))
}

const ORG_SUFFIXES: &[&str] = &[
    " SAS", " SARL", " SA", " SCI", " SCS", " SCP", " EURL", " SASU", " SNC", " sas", " sarl",
    " sa", " sci",
];

fn strip_person_marker(value: &str) -> String {
    for prefix in ["Monsieur ", "Madame ", "M. ", "Mme ", "Mlle "] {
        if let Some(rest) = value.strip_prefix(prefix) {
            return rest.to_string();
        }
    }
    value.to_string()
}

fn split_person_name(value: &str) -> (String, String) {
    let tokens: Vec<&str> = value.split_whitespace().collect();
    let last_all_caps_idx = tokens
        .iter()
        .enumerate()
        .filter(|(_, token)| {
            token
                .chars()
                .all(|ch| !ch.is_alphabetic() || ch.is_uppercase())
                && token.chars().any(char::is_alphabetic)
        })
        .map(|(idx, _)| idx)
        .last();

    if let Some(idx) = last_all_caps_idx {
        let last = tokens[idx].to_string();
        let first = tokens
            .iter()
            .enumerate()
            .filter(|(candidate_idx, _)| *candidate_idx != idx)
            .map(|(_, token)| *token)
            .collect::<Vec<_>>()
            .join(" ");
        (last, first)
    } else if tokens.len() == 1 {
        (tokens[0].to_string(), String::new())
    } else {
        let last = tokens.last().copied().unwrap_or_default().to_string();
        let first = tokens[..tokens.len() - 1].join(" ");
        (last, first)
    }
}

fn slugify(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut prev_sep = true;
    for ch in value.chars() {
        let lower = ch.to_ascii_lowercase();
        if lower.is_ascii_alphanumeric() {
            out.push(lower);
            prev_sep = false;
        } else if !prev_sep {
            out.push('_');
            prev_sep = true;
        }
    }
    out.trim_matches('_').to_string()
}

/// Extract the first SIREN from text.
#[must_use]
pub fn parse_siren(text: &str) -> Option<String> {
    use regex::Regex;

    static RE: once_cell::sync::Lazy<Regex> = once_cell::sync::Lazy::new(|| {
        Regex::new(r"\b(\d{3})\s?(\d{3})\s?(\d{3})\b").expect("valid SIREN regex")
    });

    RE.captures(text)
        .map(|captures| format!("{}{}{}", &captures[1], &captures[2], &captures[3]))
}

/// Parse all code-article references in a span.
#[must_use]
pub fn parse_code_reference(text: &str) -> Vec<ArticleRef> {
    crate::legal::codes::parse_all(text)
}

/// Parse a French EUR monetary amount into cents.
#[must_use]
pub fn parse_amount_eur(text: &str) -> Option<i64> {
    use regex::Regex;

    static RE: once_cell::sync::Lazy<Regex> = once_cell::sync::Lazy::new(|| {
        Regex::new(
            r"(?ix)
            (\d{1,3}(?:[\s.]\d{3})*(?:,\d{1,2})?|\d+(?:,\d{1,2})?)
            \s*
            (€|EUR|euros?)
        ",
        )
        .expect("valid EUR amount regex")
    });

    let captures = RE.captures(text)?;
    let number = captures.get(1)?.as_str();
    let normalized = number.replace([' ', '.'], "").replace(',', ".");
    let euros: f64 = normalized.parse().ok()?;
    Some((euros * 100.0).round() as i64)
}

/// Parse a French date in common formats.
#[must_use]
pub fn parse_french_date(text: &str) -> Option<DateTime<Utc>> {
    use regex::Regex;

    static RE_DMY_TEXT: once_cell::sync::Lazy<Regex> = once_cell::sync::Lazy::new(|| {
        Regex::new(
            r"(?i)(\d{1,2})\s+(janvier|février|fevrier|mars|avril|mai|juin|juillet|août|aout|septembre|octobre|novembre|décembre|decembre)\s+(\d{4})",
        )
        .expect("valid French date regex")
    });
    static RE_DMY_SLASH: once_cell::sync::Lazy<Regex> = once_cell::sync::Lazy::new(|| {
        Regex::new(r"(\d{1,2})[/.\-](\d{1,2})[/.\-](\d{4})").expect("valid numeric date regex")
    });

    if let Some(captures) = RE_DMY_TEXT.captures(text) {
        let day: u32 = captures[1].parse().ok()?;
        let month = french_month_to_num(&captures[2])?;
        let year: i32 = captures[3].parse().ok()?;
        return naive_to_utc(year, month, day);
    }

    if let Some(captures) = RE_DMY_SLASH.captures(text) {
        let day: u32 = captures[1].parse().ok()?;
        let month: u32 = captures[2].parse().ok()?;
        let year: i32 = captures[3].parse().ok()?;
        return naive_to_utc(year, month, day);
    }

    None
}

fn french_month_to_num(value: &str) -> Option<u32> {
    Some(match value.to_lowercase().as_str() {
        "janvier" => 1,
        "février" | "fevrier" => 2,
        "mars" => 3,
        "avril" => 4,
        "mai" => 5,
        "juin" => 6,
        "juillet" => 7,
        "août" | "aout" => 8,
        "septembre" => 9,
        "octobre" => 10,
        "novembre" => 11,
        "décembre" | "decembre" => 12,
        _ => return None,
    })
}

fn naive_to_utc(year: i32, month: u32, day: u32) -> Option<DateTime<Utc>> {
    let date = NaiveDate::from_ymd_opt(year, month, day)?;
    let dt = NaiveDateTime::new(date, NaiveTime::from_hms_opt(0, 0, 0)?);
    Some(Utc.from_utc_datetime(&dt))
}

/// Parse a relative delay expression and add it to `anchor`.
#[must_use]
pub fn parse_delay(text: &str, anchor: DateTime<Utc>) -> Option<DateTime<Utc>> {
    use regex::Regex;

    static RE: once_cell::sync::Lazy<Regex> = once_cell::sync::Lazy::new(|| {
        Regex::new(r"(?i)(?:dans un d[ée]lai de\s+|sous\s+)(\d+)\s+(jours?|mois|ans?)")
            .expect("valid delay regex")
    });

    let captures = RE.captures(text)?;
    let count: i64 = captures[1].parse().ok()?;
    let unit = captures[2].to_lowercase();
    let days = match unit.as_str() {
        "jour" | "jours" => count,
        "mois" => count * 30,
        "an" | "ans" => count * 365,
        _ => return None,
    };
    Some(anchor + chrono::Duration::days(days))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Datelike, TimeZone};

    #[test]
    fn canonical_party_org_strips_legal_suffixes() {
        let (kind, norm) = canonical_party_form("Société ACME SAS au capital de 50 000 euros");
        assert_eq!(kind, PartyKind::Organization);
        assert_eq!(norm, "org:acme");
    }

    #[test]
    fn canonical_party_person_extracts_lastname_firstname() {
        let (kind, norm) = canonical_party_form("Monsieur Martin DUPONT");
        assert_eq!(kind, PartyKind::Person);
        assert_eq!(norm, "person:dupont_martin");
    }

    #[test]
    fn parse_siren_extracts_9_digit_number_with_spaces() {
        assert_eq!(
            parse_siren("RCS Paris 123 456 789"),
            Some("123456789".to_string())
        );
        assert_eq!(parse_siren("aucun siren ici"), None);
    }

    #[test]
    fn parse_code_reference_handles_common_aliases() {
        let refs = parse_code_reference("art. 1240 c. civ. et article L210-2 du Code de commerce");
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0].normalized_ref(), "code_civil:1240");
        assert_eq!(refs[1].normalized_ref(), "code_commerce:L210-2");
    }

    #[test]
    fn parse_amount_eur_handles_french_formats() {
        assert_eq!(parse_amount_eur("10 000 €"), Some(1_000_000));
        assert_eq!(parse_amount_eur("10.000 EUR"), Some(1_000_000));
        assert_eq!(parse_amount_eur("1 234,56 €"), Some(123_456));
    }

    #[test]
    fn parse_french_date_handles_common_formats() {
        let dmy = parse_french_date("le 12 mars 2024").unwrap();
        assert_eq!((dmy.year(), dmy.month(), dmy.day()), (2024, 3, 12));
        let slash = parse_french_date("12/03/2024").unwrap();
        assert_eq!((slash.year(), slash.month(), slash.day()), (2024, 3, 12));
    }

    #[test]
    fn parse_delay_adds_calendar_days_to_anchor() {
        let anchor = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let due = parse_delay(
            "dans un délai de 30 jours à compter de la notification",
            anchor,
        )
        .unwrap();
        assert_eq!(due.day(), 31);
        assert_eq!(due.month(), 1);
        assert_eq!(due.year(), 2024);
    }
}
