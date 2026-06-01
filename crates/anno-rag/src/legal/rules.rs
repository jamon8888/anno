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
    /// A detected legal risk in the text.
    Risk {
        /// Risk category, e.g. "clause_penale", "non_concurrence".
        category: String,
        /// Severity: "high", "medium", or "low".
        severity: String,
        /// Pseudonymized text of the risky segment.
        text_pseudo: String,
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
    // Group A — Droit commun des contrats
    out.extend(rule_risk_clause_penale(pseudo_text));
    out.extend(rule_risk_responsabilite_illimitee(pseudo_text));
    out.extend(rule_risk_desequilibre_significatif(pseudo_text));
    out.extend(rule_risk_tacite_reconduction(pseudo_text));
    out.extend(rule_risk_clause_resolutoire(pseudo_text));
    out.extend(rule_risk_renonciation_recours(pseudo_text));
    out.extend(rule_risk_indexation_interdite(pseudo_text));
    out.extend(rule_risk_clause_leonine(pseudo_text));
    // Group B — Droit commercial
    out.extend(rule_risk_delai_paiement_excessif(pseudo_text));
    out.extend(rule_risk_rupture_brutale(pseudo_text));
    out.extend(rule_risk_exclusivite_sans_duree(pseudo_text));
    out.extend(rule_risk_non_sollicitation(pseudo_text));
    // Group C — Droit du travail
    out.extend(rule_risk_non_concurrence_sans_contrepartie(pseudo_text));
    out.extend(rule_risk_periode_essai_excessive(pseudo_text));
    out.extend(rule_risk_mobilite_illimitee(pseudo_text));
    out.extend(rule_risk_dedit_formation(pseudo_text));
    out.extend(rule_risk_forfait_jours_sans_suivi(pseudo_text));
    // Group D — Baux
    out.extend(rule_risk_solidarite_cessionnaire(pseudo_text));
    out.extend(rule_risk_charges_locatives_illimitees(pseudo_text));
    out.extend(rule_risk_bail_derogatoire_excessif(pseudo_text));
    // Group E — RGPD
    out.extend(rule_risk_transfert_hors_ue(pseudo_text));
    out.extend(rule_risk_sous_traitance_sans_art28(pseudo_text));
    out.extend(rule_risk_conservation_illimitee(pseudo_text));
    // Group F — Propriété intellectuelle
    out.extend(rule_risk_cession_pi_totale(pseudo_text));
    out.extend(rule_risk_cession_oeuvres_futures(pseudo_text));
    // GLiNER risk_indicator deduplication
    let _ = chunk_id;
    let regex_risks: Vec<_> = out
        .iter()
        .filter(|f| matches!(f, TypedFact::Risk { .. }))
        .cloned()
        .collect();
    out.extend(merge_gliner_risks(&regex_risks, entities, pseudo_text));
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

// ── Risk rules — Group A: Droit commun des contrats ─────────────────────────

fn rule_risk_clause_penale(text: &str) -> Vec<TypedFact> {
    use regex::Regex;
    static RE: once_cell::sync::Lazy<Regex> = once_cell::sync::Lazy::new(|| {
        Regex::new(r"(?i)clause\s+p[ée]nale|p[ée]nalit[ée]\s+forfaitaire|indemnit[ée]\s+forfaitaire\s+de\b.*\br[ée]siliation")
            .expect("valid clause_penale regex")
    });
    RE.find_iter(text)
        .map(|m| TypedFact::Risk {
            category: "clause_penale".into(),
            severity: "medium".into(),
            text_pseudo: m.as_str().to_string(),
        })
        .collect()
}

fn rule_risk_responsabilite_illimitee(text: &str) -> Vec<TypedFact> {
    use regex::Regex;
    static RE: once_cell::sync::Lazy<Regex> = once_cell::sync::Lazy::new(|| {
        Regex::new(r"(?i)responsabilit[ée]\s+(?:illimit[ée]e|sans\s+(?:limite|plafond))|exclusion\s+(?:totale\s+)?de\s+(?:toute\s+)?responsabilit[ée]|exon[ée]r[ée]e?\s+de\s+(?:toute\s+)?responsabilit[ée]|ne\s+pourra\s+[êe]tre\s+tenu\s+(?:d'aucune\s+)?responsabilit[ée]")
            .expect("valid responsabilite_illimitee regex")
    });
    RE.find_iter(text)
        .map(|m| TypedFact::Risk {
            category: "responsabilite_illimitee".into(),
            severity: "high".into(),
            text_pseudo: m.as_str().to_string(),
        })
        .collect()
}

fn rule_risk_desequilibre_significatif(text: &str) -> Vec<TypedFact> {
    use regex::Regex;
    static RE: once_cell::sync::Lazy<Regex> = once_cell::sync::Lazy::new(|| {
        Regex::new(r"(?i)d[ée]s[ée]quilibre\s+significatif|avantage\s+(?:excessif|manifestement\s+disproportionn[ée])")
            .expect("valid desequilibre_significatif regex")
    });
    RE.find_iter(text)
        .map(|m| TypedFact::Risk {
            category: "desequilibre_significatif".into(),
            severity: "high".into(),
            text_pseudo: m.as_str().to_string(),
        })
        .collect()
}

fn rule_risk_tacite_reconduction(text: &str) -> Vec<TypedFact> {
    use regex::Regex;
    static RE: once_cell::sync::Lazy<Regex> = once_cell::sync::Lazy::new(|| {
        Regex::new(r"(?i)tacite(?:ment)?\s+reconduit|renouvellement\s+tacite|reconduction\s+tacite|reconduit\s+(?:tacitement|automatiquement|de\s+plein\s+droit)")
            .expect("valid tacite_reconduction regex")
    });
    RE.find_iter(text)
        .map(|m| TypedFact::Risk {
            category: "tacite_reconduction".into(),
            severity: "medium".into(),
            text_pseudo: m.as_str().to_string(),
        })
        .collect()
}

fn rule_risk_clause_resolutoire(text: &str) -> Vec<TypedFact> {
    use regex::Regex;
    static RE: once_cell::sync::Lazy<Regex> = once_cell::sync::Lazy::new(|| {
        Regex::new(r"(?i)r[ée]solu(?:tion)?\s+de\s+plein\s+droit|clause\s+r[ée]solutoire|r[ée]siliation\s+(?:automatique|imm[ée]diate|de\s+plein\s+droit)\s+sans\s+(?:pr[ée]avis|mise\s+en\s+demeure)")
            .expect("valid clause_resolutoire regex")
    });
    RE.find_iter(text)
        .map(|m| TypedFact::Risk {
            category: "clause_resolutoire".into(),
            severity: "high".into(),
            text_pseudo: m.as_str().to_string(),
        })
        .collect()
}

fn rule_risk_renonciation_recours(text: &str) -> Vec<TypedFact> {
    use regex::Regex;
    static RE: once_cell::sync::Lazy<Regex> = once_cell::sync::Lazy::new(|| {
        Regex::new(r"(?i)renonce\s+(?:irr[ée]vocablement\s+)?[àa]\s+(?:tout\s+)?recours|renonciation\s+[àa]\s+(?:tout\s+)?recours")
            .expect("valid renonciation_recours regex")
    });
    RE.find_iter(text)
        .map(|m| TypedFact::Risk {
            category: "renonciation_recours".into(),
            severity: "high".into(),
            text_pseudo: m.as_str().to_string(),
        })
        .collect()
}

fn rule_risk_indexation_interdite(text: &str) -> Vec<TypedFact> {
    use regex::Regex;
    static RE: once_cell::sync::Lazy<Regex> = once_cell::sync::Lazy::new(|| {
        Regex::new(r"(?i)index[ée](?:e)?\s+sur\s+(?:le\s+)?(?:smic|smig|salaire\s+minimum|niveau\s+g[ée]n[ée]ral\s+des\s+(?:prix|salaires))")
            .expect("valid indexation_interdite regex")
    });
    RE.find_iter(text)
        .map(|m| TypedFact::Risk {
            category: "indexation_interdite".into(),
            severity: "high".into(),
            text_pseudo: m.as_str().to_string(),
        })
        .collect()
}

fn rule_risk_clause_leonine(text: &str) -> Vec<TypedFact> {
    use regex::Regex;
    static RE: once_cell::sync::Lazy<Regex> = once_cell::sync::Lazy::new(|| {
        Regex::new(r"(?i)clause\s+l[ée]onine|exon[ée]r[ée](?:e)?\s+de\s+toute\s+(?:perte|contribution\s+aux\s+pertes)|attribut(?:ion)?\s+(?:de\s+)?(?:la\s+)?totalit[ée]\s+des\s+(?:b[ée]n[ée]fices|profits)")
            .expect("valid clause_leonine regex")
    });
    RE.find_iter(text)
        .map(|m| TypedFact::Risk {
            category: "clause_leonine".into(),
            severity: "high".into(),
            text_pseudo: m.as_str().to_string(),
        })
        .collect()
}

// ── Risk rules — Group B: Droit commercial ──────────────────────────────────

fn rule_risk_delai_paiement_excessif(text: &str) -> Vec<TypedFact> {
    use regex::Regex;
    static RE: once_cell::sync::Lazy<Regex> = once_cell::sync::Lazy::new(|| {
        Regex::new(
            r"(?i)d[ée]lai\s+de\s+(?:paiement|r[èe]glement)\s+(?:est\s+)?(?:de\s+)?(\d+)\s*jours",
        )
        .expect("valid delai_paiement regex")
    });
    RE.captures_iter(text)
        .filter_map(|cap| {
            let days: u32 = cap[1].parse().ok()?;
            if days > 60 {
                Some(TypedFact::Risk {
                    category: "delai_paiement_excessif".into(),
                    severity: "high".into(),
                    text_pseudo: cap[0].to_string(),
                })
            } else {
                None
            }
        })
        .collect()
}

fn rule_risk_rupture_brutale(text: &str) -> Vec<TypedFact> {
    use regex::Regex;
    static RE: once_cell::sync::Lazy<Regex> = once_cell::sync::Lazy::new(|| {
        Regex::new(r"(?i)r[ée]sili(?:ation|er|[ée])\s+(?:sans\s+(?:pr[ée]avis|motif)|[àa]\s+(?:tout\s+)?moment\s+sans\s+(?:pr[ée]avis|indemnit[ée]))|rupture\s+(?:brutale|sans\s+pr[ée]avis)")
            .expect("valid rupture_brutale regex")
    });
    RE.find_iter(text)
        .map(|m| TypedFact::Risk {
            category: "rupture_brutale".into(),
            severity: "high".into(),
            text_pseudo: m.as_str().to_string(),
        })
        .collect()
}

fn rule_risk_exclusivite_sans_duree(text: &str) -> Vec<TypedFact> {
    use regex::Regex;
    static RE: once_cell::sync::Lazy<Regex> = once_cell::sync::Lazy::new(|| {
        Regex::new(r"(?i)exclusivit[ée]\b.{0,80}(?:sans\s+(?:limite|dur[ée]e)|pour\s+une\s+dur[ée]e\s+ind[ée]termin[ée]e|\bperp[ée]tu)")
            .expect("valid exclusivite_sans_duree regex")
    });
    RE.find_iter(text)
        .map(|m| TypedFact::Risk {
            category: "exclusivite_sans_duree".into(),
            severity: "high".into(),
            text_pseudo: m.as_str().to_string(),
        })
        .collect()
}

fn rule_risk_non_sollicitation(text: &str) -> Vec<TypedFact> {
    use regex::Regex;
    static RE: once_cell::sync::Lazy<Regex> = once_cell::sync::Lazy::new(|| {
        Regex::new(r"(?i)non[- ]sollicitation|interdiction\s+de\s+sollicit(?:er|ation)\s+(?:du\s+)?personnel|d[ée]bauchage")
            .expect("valid non_sollicitation regex")
    });
    RE.find_iter(text)
        .map(|m| TypedFact::Risk {
            category: "non_sollicitation".into(),
            severity: "medium".into(),
            text_pseudo: m.as_str().to_string(),
        })
        .collect()
}

// ── Risk rules — Group C: Droit du travail ──────────────────────────────────

fn rule_risk_non_concurrence_sans_contrepartie(text: &str) -> Vec<TypedFact> {
    use regex::Regex;
    static RE_MATCH: once_cell::sync::Lazy<Regex> = once_cell::sync::Lazy::new(|| {
        Regex::new(r"(?i)non[- ]concurrence").expect("valid non_concurrence regex")
    });
    static RE_MITIGANT: once_cell::sync::Lazy<Regex> = once_cell::sync::Lazy::new(|| {
        Regex::new(r"(?i)contrepartie\s+financi[èe]re").expect("valid mitigant regex")
    });
    let mut out = Vec::new();
    for m in RE_MATCH.find_iter(text) {
        let window_end = (m.end() + 300).min(text.len());
        let window = &text[m.start()..window_end];
        if !RE_MITIGANT.is_match(window) {
            out.push(TypedFact::Risk {
                category: "non_concurrence_sans_contrepartie".into(),
                severity: "high".into(),
                text_pseudo: m.as_str().to_string(),
            });
        }
    }
    out
}

fn rule_risk_periode_essai_excessive(text: &str) -> Vec<TypedFact> {
    use regex::Regex;
    static RE: once_cell::sync::Lazy<Regex> = once_cell::sync::Lazy::new(|| {
        Regex::new(r"(?i)p[ée]riode\s+d'essai\s+(?:est\s+)?(?:de\s+)?(\d+)\s*mois")
            .expect("valid periode_essai regex")
    });
    RE.captures_iter(text)
        .filter_map(|cap| {
            let months: u32 = cap[1].parse().ok()?;
            if months > 4 {
                Some(TypedFact::Risk {
                    category: "periode_essai_excessive".into(),
                    severity: "high".into(),
                    text_pseudo: cap[0].to_string(),
                })
            } else {
                None
            }
        })
        .collect()
}

fn rule_risk_mobilite_illimitee(text: &str) -> Vec<TypedFact> {
    use regex::Regex;
    static RE_MATCH: once_cell::sync::Lazy<Regex> = once_cell::sync::Lazy::new(|| {
        Regex::new(r"(?i)clause\s+de\s+mobilit[ée]").expect("valid mobilite regex")
    });
    static RE_MITIGANT: once_cell::sync::Lazy<Regex> = once_cell::sync::Lazy::new(|| {
        Regex::new(r"(?i)p[ée]rim[èe]tre|zone\s+g[ée]ographique\s+d[ée]finie|rayon\s+de")
            .expect("valid mobilite mitigant regex")
    });
    let mut out = Vec::new();
    for m in RE_MATCH.find_iter(text) {
        let window_end = (m.end() + 200).min(text.len());
        let window = &text[m.start()..window_end];
        if !RE_MITIGANT.is_match(window) {
            out.push(TypedFact::Risk {
                category: "mobilite_illimitee".into(),
                severity: "medium".into(),
                text_pseudo: m.as_str().to_string(),
            });
        }
    }
    out
}

fn rule_risk_dedit_formation(text: &str) -> Vec<TypedFact> {
    use regex::Regex;
    static RE: once_cell::sync::Lazy<Regex> = once_cell::sync::Lazy::new(|| {
        Regex::new(r"(?i)d[ée]dit[- ]formation|rembours(?:ement|er)\s+(?:les\s+|des\s+)?frais\s+de\s+formation(?:\s+en\s+cas\s+de\s+(?:d[ée]mission|d[ée]part))?")
            .expect("valid dedit_formation regex")
    });
    RE.find_iter(text)
        .map(|m| TypedFact::Risk {
            category: "dedit_formation".into(),
            severity: "medium".into(),
            text_pseudo: m.as_str().to_string(),
        })
        .collect()
}

fn rule_risk_forfait_jours_sans_suivi(text: &str) -> Vec<TypedFact> {
    use regex::Regex;
    static RE_MATCH: once_cell::sync::Lazy<Regex> = once_cell::sync::Lazy::new(|| {
        Regex::new(r"(?i)forfait\s+(?:en\s+)?jours").expect("valid forfait_jours regex")
    });
    static RE_MITIGANT: once_cell::sync::Lazy<Regex> = once_cell::sync::Lazy::new(|| {
        Regex::new(
            r"(?i)suivi\s+de\s+la\s+charge|entretien\s+annuel|droit\s+[àa]\s+la\s+d[ée]connexion",
        )
        .expect("valid forfait_jours mitigant regex")
    });
    let mut out = Vec::new();
    for m in RE_MATCH.find_iter(text) {
        let window_end = (m.end() + 300).min(text.len());
        let window = &text[m.start()..window_end];
        if !RE_MITIGANT.is_match(window) {
            out.push(TypedFact::Risk {
                category: "forfait_jours_sans_suivi".into(),
                severity: "medium".into(),
                text_pseudo: m.as_str().to_string(),
            });
        }
    }
    out
}

// ── Risk rules — Group D: Baux ──────────────────────────────────────────────

fn rule_risk_solidarite_cessionnaire(text: &str) -> Vec<TypedFact> {
    use regex::Regex;
    static RE: once_cell::sync::Lazy<Regex> = once_cell::sync::Lazy::new(|| {
        Regex::new(r"(?i)solidarit[ée]\s+(?:du\s+)?c[ée]dant|solidaire(?:ment)?\s+(?:responsable\s+)?(?:du|des)\s+(?:obligations|loyers)\s+(?:du\s+)?cessionnaire")
            .expect("valid solidarite_cessionnaire regex")
    });
    RE.find_iter(text)
        .map(|m| TypedFact::Risk {
            category: "solidarite_cessionnaire".into(),
            severity: "medium".into(),
            text_pseudo: m.as_str().to_string(),
        })
        .collect()
}

fn rule_risk_charges_locatives_illimitees(text: &str) -> Vec<TypedFact> {
    use regex::Regex;
    static RE: once_cell::sync::Lazy<Regex> = once_cell::sync::Lazy::new(|| {
        Regex::new(r"(?i)charges?\s+(?:r[ée]cup[ée]rables?\s+)?(?:sans\s+(?:limite|plafond)|int[ée]gralit[ée]\s+des\s+charges?\s+(?:de\s+)?copropri[ée]t[ée])")
            .expect("valid charges_locatives regex")
    });
    RE.find_iter(text)
        .map(|m| TypedFact::Risk {
            category: "charges_locatives_illimitees".into(),
            severity: "medium".into(),
            text_pseudo: m.as_str().to_string(),
        })
        .collect()
}

fn rule_risk_bail_derogatoire_excessif(text: &str) -> Vec<TypedFact> {
    use regex::Regex;
    static RE: once_cell::sync::Lazy<Regex> = once_cell::sync::Lazy::new(|| {
        Regex::new(r"(?i)bail\s+d[ée]rogatoire\s+(?:de\s+)?(\d+)\s*(mois|ans)")
            .expect("valid bail_derogatoire regex")
    });
    RE.captures_iter(text)
        .filter_map(|cap| {
            let value: u32 = cap[1].parse().ok()?;
            let unit = cap[2].to_lowercase();
            let months = if unit == "ans" { value * 12 } else { value };
            if months > 36 {
                Some(TypedFact::Risk {
                    category: "bail_derogatoire_excessif".into(),
                    severity: "high".into(),
                    text_pseudo: cap[0].to_string(),
                })
            } else {
                None
            }
        })
        .collect()
}

// ── Risk rules — Group E: RGPD ──────────────────────────────────────────────

fn rule_risk_transfert_hors_ue(text: &str) -> Vec<TypedFact> {
    use regex::Regex;
    static RE_MATCH: once_cell::sync::Lazy<Regex> = once_cell::sync::Lazy::new(|| {
        Regex::new(r"(?i)transfert\s+(?:de\s+donn[ée]es?\s+)?(?:hors\s+(?:de\s+)?(?:l')?(?:UE|Union\s+europ[ée]enne|EEE)|vers\s+(?:un\s+)?pays\s+tiers)")
            .expect("valid transfert_hors_ue regex")
    });
    static RE_MITIGANT: once_cell::sync::Lazy<Regex> = once_cell::sync::Lazy::new(|| {
        Regex::new(r"(?i)clauses?\s+contractuelles?\s+types?|CCT|d[ée]cision\s+d'ad[ée]quation|BCR")
            .expect("valid transfert mitigant regex")
    });
    let mut out = Vec::new();
    for m in RE_MATCH.find_iter(text) {
        let window_end = (m.end() + 300).min(text.len());
        let window = &text[m.start()..window_end];
        if !RE_MITIGANT.is_match(window) {
            out.push(TypedFact::Risk {
                category: "transfert_hors_ue".into(),
                severity: "high".into(),
                text_pseudo: m.as_str().to_string(),
            });
        }
    }
    out
}

fn rule_risk_sous_traitance_sans_art28(text: &str) -> Vec<TypedFact> {
    use regex::Regex;
    static RE_MATCH: once_cell::sync::Lazy<Regex> = once_cell::sync::Lazy::new(|| {
        Regex::new(r"(?i)sous[- ]trait(?:ant|ance)\s+(?:de\s+)?(?:donn[ée]es?|traitement)")
            .expect("valid sous_traitance regex")
    });
    static RE_MITIGANT: once_cell::sync::Lazy<Regex> = once_cell::sync::Lazy::new(|| {
        Regex::new(r"(?i)art(?:icle)?\s*\.?\s*28|clauses?\s+(?:de\s+)?sous[- ]traitance|mesures?\s+(?:techniques?\s+et\s+)?organisationnelles?")
            .expect("valid art28 mitigant regex")
    });
    let mut out = Vec::new();
    for m in RE_MATCH.find_iter(text) {
        let window_end = (m.end() + 300).min(text.len());
        let window = &text[m.start()..window_end];
        if !RE_MITIGANT.is_match(window) {
            out.push(TypedFact::Risk {
                category: "sous_traitance_sans_art28".into(),
                severity: "high".into(),
                text_pseudo: m.as_str().to_string(),
            });
        }
    }
    out
}

fn rule_risk_conservation_illimitee(text: &str) -> Vec<TypedFact> {
    use regex::Regex;
    static RE: once_cell::sync::Lazy<Regex> = once_cell::sync::Lazy::new(|| {
        Regex::new(r"(?i)conserv[ée](?:e)?s?\s+(?:sans\s+(?:limite|dur[ée]e\s+d[ée]termin[ée]e)|ind[ée]finiment|de\s+mani[èe]re\s+illimit[ée]e)")
            .expect("valid conservation_illimitee regex")
    });
    RE.find_iter(text)
        .map(|m| TypedFact::Risk {
            category: "conservation_illimitee".into(),
            severity: "high".into(),
            text_pseudo: m.as_str().to_string(),
        })
        .collect()
}

// ── Risk rules — Group F: Propriété intellectuelle ──────────────────────────

fn rule_risk_cession_pi_totale(text: &str) -> Vec<TypedFact> {
    use regex::Regex;
    static RE_MATCH: once_cell::sync::Lazy<Regex> = once_cell::sync::Lazy::new(|| {
        Regex::new(r"(?i)c[èe]de?\s+(?:l'(?:ensemble|int[ée]gralit[ée]|totalit[ée])\s+de\s+ses?\s+)?droits?\s+(?:de\s+)?propri[ée]t[ée]\s+intellectuelle")
            .expect("valid cession_pi regex")
    });
    static RE_MITIGANT: once_cell::sync::Lazy<Regex> = once_cell::sync::Lazy::new(|| {
        Regex::new(r"(?i)contrepartie|r[ée]mun[ée]ration|prix\s+de\s+cession")
            .expect("valid cession_pi mitigant regex")
    });
    let mut out = Vec::new();
    for m in RE_MATCH.find_iter(text) {
        let window_end = (m.end() + 300).min(text.len());
        let window = &text[m.start()..window_end];
        if !RE_MITIGANT.is_match(window) {
            out.push(TypedFact::Risk {
                category: "cession_pi_totale".into(),
                severity: "high".into(),
                text_pseudo: m.as_str().to_string(),
            });
        }
    }
    out
}

fn rule_risk_cession_oeuvres_futures(text: &str) -> Vec<TypedFact> {
    use regex::Regex;
    static RE: once_cell::sync::Lazy<Regex> = once_cell::sync::Lazy::new(|| {
        Regex::new(r"(?i)c[èe]de?\s+(?:par\s+avance\s+)?(?:les?\s+)?(?:droits?\s+sur\s+)?(?:l'ensemble\s+(?:de\s+)?ses?\s+)?[œo]euvres?\s+futures?|cession\s+(?:globale\s+)?(?:de\s+)?(?:l'ensemble\s+(?:de\s+)?ses?\s+)?[œo]euvres?\s+(?:[àa]\s+venir|futures?)")
            .expect("valid cession_oeuvres_futures regex")
    });
    RE.find_iter(text)
        .map(|m| TypedFact::Risk {
            category: "cession_oeuvres_futures".into(),
            severity: "high".into(),
            text_pseudo: m.as_str().to_string(),
        })
        .collect()
}

// ── GLiNER risk deduplication ───────────────────────────────────────────────

/// Merge GLiNER `risk_indicator` entities with regex-detected risks.
///
/// Deduplicates by span overlap (±20 chars). When a GLiNER candidate
/// overlaps a regex-detected Risk, the regex version wins (it has a
/// specific category and severity). GLiNER-only candidates get
/// `category = "clause_a_risque"` with severity derived from confidence.
fn merge_gliner_risks(
    regex_risks: &[TypedFact],
    entities: &[LegalEntity],
    text: &str,
) -> Vec<TypedFact> {
    let mut out = Vec::new();
    let risk_entities: Vec<_> = entities
        .iter()
        .filter(|e| e.label == "risk_indicator" && e.confidence >= 0.55)
        .collect();

    for entity in &risk_entities {
        let overlaps_regex = regex_risks.iter().any(|fact| {
            if let TypedFact::Risk { text_pseudo, .. } = fact {
                if let Some(regex_pos) = text.find(text_pseudo.as_str()) {
                    let regex_end = regex_pos + text_pseudo.len();
                    let ent_start = entity.byte_start as usize;
                    let ent_end = entity.byte_end as usize;
                    // Overlap check with ±20 char tolerance
                    ent_start <= regex_end + 20 && ent_end + 20 >= regex_pos
                } else {
                    false
                }
            } else {
                false
            }
        });

        if !overlaps_regex {
            let severity = if entity.confidence >= 0.75 {
                "medium"
            } else {
                "low"
            };
            out.push(TypedFact::Risk {
                category: "clause_a_risque".into(),
                severity: severity.into(),
                text_pseudo: entity.text.clone(),
            });
        }
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

    // ── Risk rule tests ─────────────────────────────────────────────────────

    #[test]
    fn risk_clause_penale_detected() {
        let fwd = fwd(&[]);
        let facts = apply_all(
            Uuid::nil(),
            "La clause pénale prévoit une indemnité de 5000 euros.",
            &[],
            &fwd,
        );
        assert!(facts
            .iter()
            .any(|f| matches!(f, TypedFact::Risk { category, severity, .. }
            if category == "clause_penale" && severity == "medium")));
    }

    #[test]
    fn risk_responsabilite_illimitee_detected() {
        let fwd = fwd(&[]);
        let facts = apply_all(
            Uuid::nil(),
            "Le prestataire est exonéré de toute responsabilité.",
            &[],
            &fwd,
        );
        assert!(facts
            .iter()
            .any(|f| matches!(f, TypedFact::Risk { category, severity, .. }
            if category == "responsabilite_illimitee" && severity == "high")));
    }

    #[test]
    fn risk_desequilibre_significatif_detected() {
        let fwd = fwd(&[]);
        let facts = apply_all(
            Uuid::nil(),
            "Cette clause crée un déséquilibre significatif entre les parties.",
            &[],
            &fwd,
        );
        assert!(facts
            .iter()
            .any(|f| matches!(f, TypedFact::Risk { category, severity, .. }
            if category == "desequilibre_significatif" && severity == "high")));
    }

    #[test]
    fn risk_tacite_reconduction_detected() {
        let fwd = fwd(&[]);
        let facts = apply_all(
            Uuid::nil(),
            "Le contrat est reconduit tacitement pour une durée identique.",
            &[],
            &fwd,
        );
        assert!(facts
            .iter()
            .any(|f| matches!(f, TypedFact::Risk { category, severity, .. }
            if category == "tacite_reconduction" && severity == "medium")));
    }

    #[test]
    fn risk_clause_resolutoire_detected() {
        let fwd = fwd(&[]);
        let facts = apply_all(
            Uuid::nil(),
            "Résiliation de plein droit sans mise en demeure préalable.",
            &[],
            &fwd,
        );
        assert!(facts
            .iter()
            .any(|f| matches!(f, TypedFact::Risk { category, severity, .. }
            if category == "clause_resolutoire" && severity == "high")));
    }

    #[test]
    fn risk_renonciation_recours_detected() {
        let fwd = fwd(&[]);
        let facts = apply_all(
            Uuid::nil(),
            "Le client renonce à tout recours contre le fournisseur.",
            &[],
            &fwd,
        );
        assert!(facts
            .iter()
            .any(|f| matches!(f, TypedFact::Risk { category, severity, .. }
            if category == "renonciation_recours" && severity == "high")));
    }

    #[test]
    fn risk_indexation_interdite_detected() {
        let fwd = fwd(&[]);
        let facts = apply_all(Uuid::nil(), "Le loyer est indexé sur le SMIC.", &[], &fwd);
        assert!(facts
            .iter()
            .any(|f| matches!(f, TypedFact::Risk { category, severity, .. }
            if category == "indexation_interdite" && severity == "high")));
    }

    #[test]
    fn risk_clause_leonine_detected() {
        let fwd = fwd(&[]);
        let facts = apply_all(
            Uuid::nil(),
            "L'associé est exonéré de toute contribution aux pertes.",
            &[],
            &fwd,
        );
        assert!(facts
            .iter()
            .any(|f| matches!(f, TypedFact::Risk { category, severity, .. }
            if category == "clause_leonine" && severity == "high")));
    }

    #[test]
    fn no_risk_on_benign_text() {
        let fwd = fwd(&[]);
        let facts = apply_all(
            Uuid::nil(),
            "Les parties conviennent du prix suivant.",
            &[],
            &fwd,
        );
        assert!(!facts.iter().any(|f| matches!(f, TypedFact::Risk { .. })));
    }

    #[test]
    fn risk_delai_paiement_excessif_90_jours() {
        let fwd = fwd(&[]);
        let facts = apply_all(
            Uuid::nil(),
            "Le délai de paiement est de 90 jours.",
            &[],
            &fwd,
        );
        assert!(facts
            .iter()
            .any(|f| matches!(f, TypedFact::Risk { category, severity, .. }
            if category == "delai_paiement_excessif" && severity == "high")));
    }

    #[test]
    fn risk_delai_paiement_30_jours_ok() {
        let fwd = fwd(&[]);
        let facts = apply_all(
            Uuid::nil(),
            "Le délai de paiement est de 30 jours.",
            &[],
            &fwd,
        );
        assert!(!facts.iter().any(|f| matches!(f, TypedFact::Risk { category, .. } if category == "delai_paiement_excessif")));
    }

    #[test]
    fn risk_rupture_brutale_detected() {
        let fwd = fwd(&[]);
        let facts = apply_all(
            Uuid::nil(),
            "Le contrat peut être résilié sans préavis.",
            &[],
            &fwd,
        );
        assert!(facts
            .iter()
            .any(|f| matches!(f, TypedFact::Risk { category, severity, .. }
            if category == "rupture_brutale" && severity == "high")));
    }

    #[test]
    fn risk_exclusivite_sans_duree_detected() {
        let fwd = fwd(&[]);
        let facts = apply_all(
            Uuid::nil(),
            "L'exclusivité est accordée sans limite de durée.",
            &[],
            &fwd,
        );
        assert!(facts
            .iter()
            .any(|f| matches!(f, TypedFact::Risk { category, severity, .. }
            if category == "exclusivite_sans_duree" && severity == "high")));
    }

    #[test]
    fn risk_non_sollicitation_detected() {
        let fwd = fwd(&[]);
        let facts = apply_all(
            Uuid::nil(),
            "Clause de non-sollicitation du personnel pendant 2 ans.",
            &[],
            &fwd,
        );
        assert!(facts
            .iter()
            .any(|f| matches!(f, TypedFact::Risk { category, severity, .. }
            if category == "non_sollicitation" && severity == "medium")));
    }

    #[test]
    fn risk_non_concurrence_sans_contrepartie() {
        let fwd = fwd(&[]);
        let facts = apply_all(
            Uuid::nil(),
            "Clause de non-concurrence applicable pendant 2 ans.",
            &[],
            &fwd,
        );
        assert!(facts
            .iter()
            .any(|f| matches!(f, TypedFact::Risk { category, severity, .. }
            if category == "non_concurrence_sans_contrepartie" && severity == "high")));
    }

    #[test]
    fn risk_non_concurrence_with_contrepartie_ok() {
        let fwd = fwd(&[]);
        let facts = apply_all(
            Uuid::nil(),
            "Clause de non-concurrence avec contrepartie financière de 50% du salaire.",
            &[],
            &fwd,
        );
        assert!(!facts
            .iter()
            .any(|f| matches!(f, TypedFact::Risk { category, .. }
            if category == "non_concurrence_sans_contrepartie")));
    }

    #[test]
    fn risk_periode_essai_excessive_6_mois() {
        let fwd = fwd(&[]);
        let facts = apply_all(Uuid::nil(), "La période d'essai est de 6 mois.", &[], &fwd);
        assert!(facts
            .iter()
            .any(|f| matches!(f, TypedFact::Risk { category, severity, .. }
            if category == "periode_essai_excessive" && severity == "high")));
    }

    #[test]
    fn risk_periode_essai_3_mois_ok() {
        let fwd = fwd(&[]);
        let facts = apply_all(Uuid::nil(), "La période d'essai est de 3 mois.", &[], &fwd);
        assert!(!facts
            .iter()
            .any(|f| matches!(f, TypedFact::Risk { category, .. }
            if category == "periode_essai_excessive")));
    }

    #[test]
    fn risk_mobilite_illimitee() {
        let fwd = fwd(&[]);
        let facts = apply_all(
            Uuid::nil(),
            "Le salarié accepte une clause de mobilité sur tout le territoire.",
            &[],
            &fwd,
        );
        assert!(facts
            .iter()
            .any(|f| matches!(f, TypedFact::Risk { category, .. }
            if category == "mobilite_illimitee")));
    }

    #[test]
    fn risk_dedit_formation() {
        let fwd = fwd(&[]);
        let facts = apply_all(
            Uuid::nil(),
            "En cas de démission, le salarié devra rembourser les frais de formation.",
            &[],
            &fwd,
        );
        assert!(facts
            .iter()
            .any(|f| matches!(f, TypedFact::Risk { category, .. }
            if category == "dedit_formation")));
    }

    #[test]
    fn risk_forfait_jours_sans_suivi() {
        let fwd = fwd(&[]);
        let facts = apply_all(
            Uuid::nil(),
            "Le cadre est soumis à un forfait en jours de 218 jours par an.",
            &[],
            &fwd,
        );
        assert!(facts
            .iter()
            .any(|f| matches!(f, TypedFact::Risk { category, .. }
            if category == "forfait_jours_sans_suivi")));
    }

    #[test]
    fn risk_solidarite_cessionnaire() {
        let fwd = fwd(&[]);
        let facts = apply_all(
            Uuid::nil(),
            "Le cédant reste solidairement responsable des loyers du cessionnaire.",
            &[],
            &fwd,
        );
        assert!(facts
            .iter()
            .any(|f| matches!(f, TypedFact::Risk { category, .. }
            if category == "solidarite_cessionnaire")));
    }

    #[test]
    fn risk_bail_derogatoire_excessif_48_mois() {
        let fwd = fwd(&[]);
        let facts = apply_all(Uuid::nil(), "Bail dérogatoire de 48 mois.", &[], &fwd);
        assert!(facts
            .iter()
            .any(|f| matches!(f, TypedFact::Risk { category, severity, .. }
            if category == "bail_derogatoire_excessif" && severity == "high")));
    }

    #[test]
    fn risk_conservation_illimitee() {
        let fwd = fwd(&[]);
        let facts = apply_all(
            Uuid::nil(),
            "Les données sont conservées sans limite de durée.",
            &[],
            &fwd,
        );
        assert!(facts
            .iter()
            .any(|f| matches!(f, TypedFact::Risk { category, severity, .. }
            if category == "conservation_illimitee" && severity == "high")));
    }

    #[test]
    fn risk_charges_locatives_illimitees() {
        let fwd = fwd(&[]);
        let facts = apply_all(
            Uuid::nil(),
            "Les charges récupérables sans plafond sont dues par le preneur.",
            &[],
            &fwd,
        );
        assert!(facts
            .iter()
            .any(|f| matches!(f, TypedFact::Risk { category, severity, .. }
            if category == "charges_locatives_illimitees" && severity == "medium")));
    }

    #[test]
    fn risk_transfert_hors_ue_without_guarantees() {
        let fwd = fwd(&[]);
        let facts = apply_all(
            Uuid::nil(),
            "Le contrat prévoit un transfert de données hors UE.",
            &[],
            &fwd,
        );
        assert!(facts
            .iter()
            .any(|f| matches!(f, TypedFact::Risk { category, severity, .. }
            if category == "transfert_hors_ue" && severity == "high")));
    }

    #[test]
    fn risk_transfert_hors_ue_with_cct_ok() {
        let fwd = fwd(&[]);
        let facts = apply_all(
            Uuid::nil(),
            "Le contrat prévoit un transfert de données hors UE encadré par des clauses contractuelles types.",
            &[],
            &fwd,
        );
        assert!(!facts
            .iter()
            .any(|f| matches!(f, TypedFact::Risk { category, .. }
            if category == "transfert_hors_ue")));
    }

    #[test]
    fn risk_sous_traitance_sans_art28() {
        let fwd = fwd(&[]);
        let facts = apply_all(
            Uuid::nil(),
            "La sous-traitance de données est autorisée librement.",
            &[],
            &fwd,
        );
        assert!(facts
            .iter()
            .any(|f| matches!(f, TypedFact::Risk { category, severity, .. }
            if category == "sous_traitance_sans_art28" && severity == "high")));
    }

    #[test]
    fn risk_sous_traitance_with_article_28_ok() {
        let fwd = fwd(&[]);
        let facts = apply_all(
            Uuid::nil(),
            "La sous-traitance de données respecte les obligations de l'article 28.",
            &[],
            &fwd,
        );
        assert!(!facts
            .iter()
            .any(|f| matches!(f, TypedFact::Risk { category, .. }
            if category == "sous_traitance_sans_art28")));
    }

    #[test]
    fn risk_cession_pi_totale_without_price() {
        let fwd = fwd(&[]);
        let facts = apply_all(
            Uuid::nil(),
            "Le prestataire cède l'intégralité de ses droits de propriété intellectuelle.",
            &[],
            &fwd,
        );
        assert!(facts
            .iter()
            .any(|f| matches!(f, TypedFact::Risk { category, severity, .. }
            if category == "cession_pi_totale" && severity == "high")));
    }

    #[test]
    fn risk_cession_pi_totale_with_contrepartie_ok() {
        let fwd = fwd(&[]);
        let facts = apply_all(
            Uuid::nil(),
            "Le prestataire cède l'intégralité de ses droits de propriété intellectuelle en contrepartie du prix de cession.",
            &[],
            &fwd,
        );
        assert!(!facts
            .iter()
            .any(|f| matches!(f, TypedFact::Risk { category, .. }
            if category == "cession_pi_totale")));
    }

    #[test]
    fn risk_cession_oeuvres_futures() {
        let fwd = fwd(&[]);
        let facts = apply_all(
            Uuid::nil(),
            "L'auteur cède par avance les oeuvres futures.",
            &[],
            &fwd,
        );
        assert!(facts
            .iter()
            .any(|f| matches!(f, TypedFact::Risk { category, severity, .. }
            if category == "cession_oeuvres_futures" && severity == "high")));
    }

    #[test]
    fn gliner_risk_indicator_becomes_low_risk_fact() {
        let fwd = fwd(&[]);
        let entity = crate::legal::types::LegalEntity {
            label: "risk_indicator".into(),
            text: "clause potentiellement abusive".into(),
            byte_start: 10,
            byte_end: 40,
            confidence: 0.60,
        };
        let facts = apply_all(
            Uuid::nil(),
            "Texte anodin avec clause potentiellement abusive ici.",
            &[entity],
            &fwd,
        );
        assert!(facts
            .iter()
            .any(|f| matches!(f, TypedFact::Risk { category, severity, .. }
            if category == "clause_a_risque" && severity == "low")));
    }

    #[test]
    fn gliner_risk_indicator_medium_when_high_confidence() {
        let fwd = fwd(&[]);
        let entity = crate::legal::types::LegalEntity {
            label: "risk_indicator".into(),
            text: "risque contractuel majeur".into(),
            byte_start: 0,
            byte_end: 25,
            confidence: 0.80,
        };
        let facts = apply_all(Uuid::nil(), "risque contractuel majeur", &[entity], &fwd);
        assert!(facts
            .iter()
            .any(|f| matches!(f, TypedFact::Risk { category, severity, .. }
            if category == "clause_a_risque" && severity == "medium")));
    }

    #[test]
    fn gliner_risk_indicator_below_threshold_ignored() {
        let fwd = fwd(&[]);
        let entity = crate::legal::types::LegalEntity {
            label: "risk_indicator".into(),
            text: "mention neutre".into(),
            byte_start: 0,
            byte_end: 14,
            confidence: 0.50,
        };
        let facts = apply_all(Uuid::nil(), "mention neutre", &[entity], &fwd);
        assert!(!facts.iter().any(|f| matches!(f, TypedFact::Risk { .. })));
    }

    #[test]
    fn gliner_risk_deduped_with_regex_rule() {
        let fwd = fwd(&[]);
        let entity = crate::legal::types::LegalEntity {
            label: "risk_indicator".into(),
            text: "clause pénale".into(),
            byte_start: 4,
            byte_end: 17,
            confidence: 0.90,
        };
        let facts = apply_all(
            Uuid::nil(),
            "La clause pénale prévoit 5000 euros.",
            &[entity],
            &fwd,
        );
        let risk_facts: Vec<_> = facts
            .iter()
            .filter(|f| matches!(f, TypedFact::Risk { .. }))
            .collect();
        assert_eq!(risk_facts.len(), 1);
        assert!(
            matches!(risk_facts[0], TypedFact::Risk { category, .. } if category == "clause_penale")
        );
    }
}
