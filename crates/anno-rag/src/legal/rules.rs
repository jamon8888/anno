//! French legal pattern rules. Deterministic, model-free.

use crate::legal::courts;
use crate::legal::normalize;
use crate::legal::types::{ArticleRef, CourtRef, LegalEntity};
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use uuid::Uuid;

/// One typed fact produced by the deterministic Layer 2 rules.
#[derive(Debug, Clone, PartialEq)]
pub enum TypedFact {
    /// A normalized party has a role in a document.
    PartyRole {
        /// Normalized party id.
        party: String,
        /// Role label, for example `plaintiff` or `defendant`.
        role: String,
    },
    /// A detected legal or contractual obligation.
    Obligation {
        /// Normalized obligor id, if known.
        obligor: Option<String>,
        /// Obligation kind.
        kind: String,
        /// Optional deadline.
        deadline: Option<DateTime<Utc>>,
        /// Optional normalized amount in cents.
        amount_cents: Option<i64>,
        /// Optional beneficiary party id.
        owed_to: Option<String>,
    },
    /// A code or article reference.
    Reference {
        /// Normalized article reference.
        article: ArticleRef,
    },
    /// A court routing fact.
    CourtRouting {
        /// Normalized court reference.
        court: CourtRef,
    },
    /// A procedural or legal event.
    Event {
        /// Event kind.
        kind: String,
        /// Optional event date.
        event_date: Option<DateTime<Utc>>,
    },
}

/// Forward map from vault aliases to legal canonical ids.
pub struct VaultForwardMap {
    /// Alias to canonical id, for example `ORG_1 -> org:acme`.
    pub alias_to_canonical: HashMap<String, String>,
}

/// Apply every Layer-2 rule to a chunk's pseudonymized text.
#[must_use]
pub fn apply_all(
    chunk_id: Uuid,
    pseudo_text: &str,
    entities: &[LegalEntity],
    fwd: &VaultForwardMap,
) -> Vec<TypedFact> {
    let mut out = Vec::new();
    out.extend(rule_party_role_litigation(pseudo_text, fwd));
    out.extend(rule_party_role_contract(pseudo_text, fwd));
    out.extend(rule_obligation_engagement(pseudo_text, fwd));
    out.extend(rule_code_reference(pseudo_text));
    out.extend(rule_court_routing(pseudo_text));
    out.extend(rule_procedural_event(pseudo_text));
    let _ = (chunk_id, entities);
    out
}

fn canonical_from_alias(fwd: &VaultForwardMap, alias: &str) -> Option<String> {
    fwd.alias_to_canonical.get(alias).cloned()
}

fn rule_party_role_litigation(text: &str, fwd: &VaultForwardMap) -> Vec<TypedFact> {
    use regex::Regex;

    static RE: once_cell::sync::Lazy<Regex> = once_cell::sync::Lazy::new(|| {
        Regex::new(r"(?i)(ORG_\d+|PERS_\d+)\s*(?:c\./|contre|/)\s*(ORG_\d+|PERS_\d+)")
            .expect("valid litigation party regex")
    });

    let mut out = Vec::new();
    for captures in RE.captures_iter(text) {
        let plaintiff_alias = &captures[1];
        let defendant_alias = &captures[2];
        if let Some(party) = canonical_from_alias(fwd, plaintiff_alias) {
            out.push(TypedFact::PartyRole {
                party,
                role: "plaintiff".into(),
            });
        }
        if let Some(party) = canonical_from_alias(fwd, defendant_alias) {
            out.push(TypedFact::PartyRole {
                party,
                role: "defendant".into(),
            });
        }
    }
    out
}

fn rule_party_role_contract(text: &str, fwd: &VaultForwardMap) -> Vec<TypedFact> {
    use regex::Regex;

    static RE: once_cell::sync::Lazy<Regex> = once_cell::sync::Lazy::new(|| {
        Regex::new(r"(?i)(ORG_\d+|PERS_\d+)[^.\n]{0,40}\b(acheteur|vendeur|bailleur|locataire|employeur|salari[ée]|preneur|donneur d'ordre|prestataire|client)\b")
            .expect("valid contract party role regex")
    });

    let mut out = Vec::new();
    for captures in RE.captures_iter(text) {
        if let Some(party) = canonical_from_alias(fwd, &captures[1]) {
            let role = captures[2].to_lowercase().replace('é', "e");
            out.push(TypedFact::PartyRole { party, role });
        }
    }
    out
}

fn rule_obligation_engagement(text: &str, fwd: &VaultForwardMap) -> Vec<TypedFact> {
    use regex::Regex;

    static RE: once_cell::sync::Lazy<Regex> = once_cell::sync::Lazy::new(|| {
        Regex::new(r"(?i)(ORG_\d+|PERS_\d+)\s+(?:s'engage à|devra|s'oblige à|s'engage)\b")
            .expect("valid obligation regex")
    });

    let mut out = Vec::new();
    for captures in RE.captures_iter(text) {
        let obligor = canonical_from_alias(fwd, &captures[1]);
        let deadline = normalize::parse_delay(text, Utc::now());
        let amount_cents = normalize::parse_amount_eur(text);
        out.push(TypedFact::Obligation {
            obligor,
            kind: "performance".into(),
            deadline,
            amount_cents,
            owed_to: None,
        });
    }
    out
}

fn rule_code_reference(text: &str) -> Vec<TypedFact> {
    normalize::parse_code_reference(text)
        .into_iter()
        .map(|article| TypedFact::Reference { article })
        .collect()
}

fn rule_court_routing(text: &str) -> Vec<TypedFact> {
    courts::resolve(text)
        .map(|court| vec![TypedFact::CourtRouting { court }])
        .unwrap_or_default()
}

fn rule_procedural_event(text: &str) -> Vec<TypedFact> {
    use regex::Regex;

    static RE: once_cell::sync::Lazy<Regex> = once_cell::sync::Lazy::new(|| {
        Regex::new(r"(?i)\b(mise en demeure|assignation|jugement|arr[êe]t|audience)\b\s+(?:du|rendu(?:e)? le|d[ée]livr[ée](?:e)? le|fix[ée](?:e)? au)?\s*([0-9]{1,2}[ /.\-][0-9]{1,2}[ /.\-][0-9]{4}|\d{1,2}\s+(?:janvier|février|fevrier|mars|avril|mai|juin|juillet|août|aout|septembre|octobre|novembre|décembre|decembre)\s+\d{4})?")
            .expect("valid procedural event regex")
    });

    let mut out = Vec::new();
    for captures in RE.captures_iter(text) {
        let kind = captures[1]
            .to_lowercase()
            .replace(' ', "_")
            .replace('â', "a")
            .replace('ê', "e");
        let event_date = captures
            .get(2)
            .and_then(|date| normalize::parse_french_date(date.as_str()));
        out.push(TypedFact::Event { kind, event_date });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fwd(map: &[(&str, &str)]) -> VaultForwardMap {
        VaultForwardMap {
            alias_to_canonical: map
                .iter()
                .map(|(alias, canonical)| (alias.to_string(), canonical.to_string()))
                .collect(),
        }
    }

    #[test]
    fn party_role_litigation_extracts_plaintiff_and_defendant() {
        let fwd = fwd(&[("ORG_1", "org:acme"), ("ORG_2", "org:beta")]);
        let facts = apply_all(Uuid::nil(), "ORG_1 c./ ORG_2", &[], &fwd);
        assert!(facts.iter().any(|fact| {
            matches!(fact, TypedFact::PartyRole { role, party } if role == "plaintiff" && party == "org:acme")
        }));
        assert!(facts.iter().any(|fact| {
            matches!(fact, TypedFact::PartyRole { role, party } if role == "defendant" && party == "org:beta")
        }));
    }

    #[test]
    fn obligation_engagement_binds_party_to_obligation() {
        let fwd = fwd(&[("ORG_1", "org:acme")]);
        let facts = apply_all(
            Uuid::nil(),
            "ORG_1 s'engage à livrer dans un délai de 30 jours.",
            &[],
            &fwd,
        );
        assert!(facts.iter().any(|fact| {
            matches!(fact, TypedFact::Obligation { obligor: Some(party), .. } if party == "org:acme")
        }));
    }

    #[test]
    fn code_reference_rule_adds_reference_fact() {
        let fwd = fwd(&[]);
        let facts = apply_all(Uuid::nil(), "Vu l'article 1240 du Code civil", &[], &fwd);
        assert!(facts.iter().any(|fact| {
            matches!(fact, TypedFact::Reference { article } if article.normalized_ref() == "code_civil:1240")
        }));
    }

    #[test]
    fn court_routing_rule_handles_aliases() {
        let fwd = fwd(&[]);
        let facts = apply_all(Uuid::nil(), "Tribunal de commerce de Paris", &[], &fwd);
        assert!(facts.iter().any(|fact| {
            matches!(fact, TypedFact::CourtRouting { court } if court.id == "trib_com_paris")
        }));
    }
}
