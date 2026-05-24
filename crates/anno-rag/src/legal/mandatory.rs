//! Per-doctype mandatory-clause checklists for French law.
//!
//! Each evaluator scans the document text for required clause keywords using
//! case-insensitive substring matching and returns a [`MandatoryCheck`] per
//! requirement. The aggregate status is rolled up to `"complete"`,
//! `"partial"`, or `"missing"`.
//!
//! # Supported doctypes
//! | `doc_type` key | French law reference |
//! |---|---|
//! | `b2b_contract` | Code de commerce, LME 2008 (art. L441-10) |
//! | `b2c_contract` | Code de la consommation |
//! | `employment` | Code du travail |
//! | `lease_commercial` | Code de commerce art. L145 |
//! | `lease_residential` | Loi Alur / loi 89-462 |
//! | `rgpd` | RGPD + loi Informatique et Libertés |

use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ── Core types ─────────────────────────────────────────────────────────────

/// Result of a single mandatory-clause check.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MandatoryCheck {
    /// Stable requirement key (e.g. `"penalites_de_retard"`).
    pub requirement: String,
    /// Detection status: `"present"`, `"partial"`, or `"missing"`.
    pub status: String,
    /// Human-readable French description of the requirement.
    pub description: String,
    /// French law reference (article or directive).
    pub legal_ref: String,
}

impl MandatoryCheck {
    /// Convert into a [`crate::legal::kg::NodeWrite::MandatoryClauseCheck`]
    /// with a freshly generated id.
    #[must_use]
    pub fn into_node(self) -> crate::legal::kg::NodeWrite {
        crate::legal::kg::NodeWrite::MandatoryClauseCheck {
            check_id: Uuid::new_v4(),
            requirement: self.requirement,
            status: self.status,
        }
    }
}

// ── Internal helpers ────────────────────────────────────────────────────────

/// A single requirement descriptor used by the evaluators.
struct Requirement {
    key: &'static str,
    description: &'static str,
    legal_ref: &'static str,
    /// Keyword patterns — a requirement is "present" when ANY pattern matches
    /// case-insensitively. "partial" can be expressed via a two-element slice
    /// where only the second tier is optional.
    patterns: &'static [&'static str],
}

fn evaluate(text: &str, reqs: &[Requirement]) -> Vec<MandatoryCheck> {
    let lower = text.to_lowercase();
    reqs.iter()
        .map(|r| {
            let matched = r.patterns.iter().any(|p| lower.contains(p));
            MandatoryCheck {
                requirement: r.key.to_string(),
                description: r.description.to_string(),
                legal_ref: r.legal_ref.to_string(),
                status: if matched {
                    "present".to_string()
                } else {
                    "missing".to_string()
                },
            }
        })
        .collect()
}

// ── Per-doctype evaluators ──────────────────────────────────────────────────

/// Evaluate a B2B commercial contract (Code de commerce, LME 2008).
#[must_use]
pub fn evaluate_b2b_contract(text: &str) -> Vec<MandatoryCheck> {
    static REQS: &[Requirement] = &[
        Requirement {
            key: "penalites_de_retard",
            description: "Clause de pénalités de retard de paiement",
            legal_ref: "C.com.:L441-10",
            patterns: &[
                "pénalités de retard",
                "penalites de retard",
                "intérêts de retard",
                "interets de retard",
            ],
        },
        Requirement {
            key: "modalites_paiement",
            description: "Délai et modalités de règlement",
            legal_ref: "C.com.:L441-10",
            patterns: &[
                "modalités de paiement",
                "modalites de paiement",
                "délai de paiement",
                "delai de paiement",
                "conditions de règlement",
            ],
        },
        Requirement {
            key: "prix_ht_ttc",
            description: "Prix hors taxes et toutes taxes comprises",
            legal_ref: "C.com.:L441-1",
            patterns: &["prix ht", "prix ttc", "hors taxes", "toutes taxes"],
        },
        Requirement {
            key: "conditions_resiliation",
            description: "Conditions et préavis de résiliation",
            legal_ref: "C.civ.:1195",
            patterns: &[
                "résiliation",
                "resiliation",
                "résolution",
                "resolution",
                "préavis",
                "preavis",
            ],
        },
        Requirement {
            key: "clause_attribution_juridiction",
            description: "Clause attributive de juridiction ou de compétence",
            legal_ref: "CPC:42",
            patterns: &[
                "attribution de juridiction",
                "compétence exclusive",
                "tribunal compétent",
                "tribunal de commerce",
            ],
        },
        Requirement {
            key: "indemnite_recouvrement",
            description: "Indemnité forfaitaire de recouvrement (40 €)",
            legal_ref: "C.com.:L441-10 al.3",
            patterns: &[
                "indemnité forfaitaire",
                "indemnite forfaitaire",
                "40 €",
                "40 euros",
                "frais de recouvrement",
            ],
        },
    ];
    evaluate(text, REQS)
}

/// Evaluate a B2C consumer contract (Code de la consommation).
#[must_use]
pub fn evaluate_b2c_contract(text: &str) -> Vec<MandatoryCheck> {
    static REQS: &[Requirement] = &[
        Requirement {
            key: "droit_retractation",
            description: "Droit de rétractation de 14 jours",
            legal_ref: "C.conso.:L221-18",
            patterns: &[
                "droit de rétractation",
                "droit de retractation",
                "14 jours",
                "délai de rétractation",
            ],
        },
        Requirement {
            key: "information_prix",
            description: "Information précontractuelle sur le prix",
            legal_ref: "C.conso.:L111-1",
            patterns: &[
                "prix total",
                "coût total",
                "cout total",
                "prix toutes taxes",
                "frais de livraison",
            ],
        },
        Requirement {
            key: "garantie_legale_conformite",
            description: "Garantie légale de conformité",
            legal_ref: "C.conso.:L217-4",
            patterns: &[
                "garantie légale de conformité",
                "garantie legale de conformite",
                "garantie de conformité",
            ],
        },
        Requirement {
            key: "garantie_vices_caches",
            description: "Garantie légale contre les vices cachés",
            legal_ref: "C.civ.:1641",
            patterns: &[
                "vices cachés",
                "vices caches",
                "garantie des vices",
                "art. 1641",
            ],
        },
        Requirement {
            key: "voies_recours",
            description: "Voies de recours et médiation",
            legal_ref: "C.conso.:L612-1",
            patterns: &[
                "médiation",
                "mediation",
                "médiateur",
                "mediateur",
                "voie de recours",
            ],
        },
        Requirement {
            key: "coordonnees_professionnel",
            description: "Coordonnées du professionnel",
            legal_ref: "C.conso.:L111-1",
            patterns: &[
                "siège social",
                "siege social",
                "numéro siret",
                "numero siret",
                "rcs",
                "immatriculée",
            ],
        },
    ];
    evaluate(text, REQS)
}

/// Evaluate an employment contract (Code du travail).
#[must_use]
pub fn evaluate_employment(text: &str) -> Vec<MandatoryCheck> {
    static REQS: &[Requirement] = &[
        Requirement {
            key: "qualification_poste",
            description: "Qualification et description du poste",
            legal_ref: "C.trav.:L1221-1",
            patterns: &[
                "qualification",
                "intitulé du poste",
                "intitule du poste",
                "fonction",
                "mission",
            ],
        },
        Requirement {
            key: "remuneration",
            description: "Rémunération et avantages",
            legal_ref: "C.trav.:L3221-3",
            patterns: &[
                "rémunération",
                "remuneration",
                "salaire",
                "traitement mensuel",
            ],
        },
        Requirement {
            key: "duree_travail",
            description: "Durée et organisation du temps de travail",
            legal_ref: "C.trav.:L3121-1",
            patterns: &[
                "durée du travail",
                "duree du travail",
                "heures de travail",
                "temps de travail",
                "35 heures",
                "39 heures",
            ],
        },
        Requirement {
            key: "lieu_travail",
            description: "Lieu d'exécution du contrat",
            legal_ref: "C.trav.:L1221-1",
            patterns: &["lieu de travail", "lieu d'exécution", "siège de l'entreprise"],
        },
        Requirement {
            key: "periode_essai",
            description: "Période d'essai et conditions de renouvellement",
            legal_ref: "C.trav.:L1221-19",
            patterns: &[
                "période d'essai",
                "periode d'essai",
                "essai",
                "période probatoire",
            ],
        },
        Requirement {
            key: "convention_collective",
            description: "Référence à la convention collective applicable",
            legal_ref: "C.trav.:L2261-1",
            patterns: &[
                "convention collective",
                "accord de branche",
                "convention applicable",
            ],
        },
    ];
    evaluate(text, REQS)
}

/// Evaluate a commercial lease (Code de commerce art. L145).
#[must_use]
pub fn evaluate_lease_commercial(text: &str) -> Vec<MandatoryCheck> {
    static REQS: &[Requirement] = &[
        Requirement {
            key: "duree_bail",
            description: "Durée du bail (minimum 9 ans)",
            legal_ref: "C.com.:L145-4",
            patterns: &[
                "durée du bail",
                "duree du bail",
                "bail de neuf ans",
                "bail de 9 ans",
                "période triennale",
            ],
        },
        Requirement {
            key: "loyer_indexation",
            description: "Montant du loyer et clause d'indexation",
            legal_ref: "C.com.:L145-34",
            patterns: &[
                "loyer",
                "indexation",
                "indice des loyers",
                "ilc",
                "révision du loyer",
            ],
        },
        Requirement {
            key: "destination_locaux",
            description: "Destination contractuelle des locaux",
            legal_ref: "C.com.:L145-1",
            patterns: &[
                "destination",
                "usage",
                "activité autorisée",
                "activite autorisee",
            ],
        },
        Requirement {
            key: "charges_repartition",
            description: "Répartition des charges entre bailleur et preneur",
            legal_ref: "C.com.:L145-40-2",
            patterns: &[
                "charges",
                "répartition des charges",
                "repartition des charges",
                "état des lieux",
            ],
        },
        Requirement {
            key: "clause_resolutoire",
            description: "Clause résolutoire en cas de défaut de paiement",
            legal_ref: "C.com.:L145-41",
            patterns: &[
                "clause résolutoire",
                "clause resolutoire",
                "résolution de plein droit",
                "commandement de payer",
            ],
        },
        Requirement {
            key: "droit_renouvellement",
            description: "Droit au renouvellement du bail",
            legal_ref: "C.com.:L145-8",
            patterns: &[
                "renouvellement",
                "droit au renouvellement",
                "congé",
                "conge",
            ],
        },
    ];
    evaluate(text, REQS)
}

/// Evaluate a residential lease (loi 89-462 / loi Alur).
#[must_use]
pub fn evaluate_lease_residential(text: &str) -> Vec<MandatoryCheck> {
    static REQS: &[Requirement] = &[
        Requirement {
            key: "description_logement",
            description: "Description du logement (surface, équipements)",
            legal_ref: "L.89-462:3",
            patterns: &[
                "surface habitable",
                "superficie",
                "description du logement",
                "désignation du bien",
            ],
        },
        Requirement {
            key: "montant_loyer",
            description: "Montant du loyer et charges",
            legal_ref: "L.89-462:3",
            patterns: &["montant du loyer", "loyer mensuel", "loyer de base", "charges"],
        },
        Requirement {
            key: "montant_depot_garantie",
            description: "Montant du dépôt de garantie",
            legal_ref: "L.89-462:22",
            patterns: &[
                "dépôt de garantie",
                "depot de garantie",
                "caution",
                "garantie locative",
            ],
        },
        Requirement {
            key: "duree_bail",
            description: "Durée du bail (3 ans non meublé, 1 an meublé)",
            legal_ref: "L.89-462:10",
            patterns: &[
                "durée du bail",
                "duree du bail",
                "durée de la location",
                "bail d'un an",
                "bail de trois ans",
            ],
        },
        Requirement {
            key: "conditions_conge",
            description: "Conditions et préavis de congé",
            legal_ref: "L.89-462:15",
            patterns: &[
                "congé",
                "conge",
                "préavis",
                "preavis",
                "résiliation du bail",
            ],
        },
        Requirement {
            key: "dpe",
            description: "Diagnostic de performance énergétique (DPE)",
            legal_ref: "L.89-462:3-3",
            patterns: &[
                "dpe",
                "performance énergétique",
                "performance energetique",
                "diagnostic énergétique",
            ],
        },
    ];
    evaluate(text, REQS)
}

/// Evaluate a RGPD data-processing agreement or privacy policy.
#[must_use]
pub fn evaluate_rgpd(text: &str) -> Vec<MandatoryCheck> {
    static REQS: &[Requirement] = &[
        Requirement {
            key: "finalites_traitement",
            description: "Finalités du traitement des données personnelles",
            legal_ref: "RGPD:art.13",
            patterns: &[
                "finalités",
                "finalites",
                "objectifs du traitement",
                "but du traitement",
            ],
        },
        Requirement {
            key: "base_legale",
            description: "Base légale du traitement",
            legal_ref: "RGPD:art.6",
            patterns: &[
                "base légale",
                "base legale",
                "fondement juridique",
                "consentement",
                "intérêt légitime",
                "interet legitime",
            ],
        },
        Requirement {
            key: "droits_personnes",
            description: "Droits des personnes concernées",
            legal_ref: "RGPD:art.15-22",
            patterns: &[
                "droit d'accès",
                "droit d'acces",
                "droit de rectification",
                "droit à l'effacement",
                "droit d'opposition",
                "droits des personnes",
            ],
        },
        Requirement {
            key: "conservation_donnees",
            description: "Durée de conservation des données",
            legal_ref: "RGPD:art.13(2)(a)",
            patterns: &[
                "durée de conservation",
                "duree de conservation",
                "conservation des données",
                "archivage",
            ],
        },
        Requirement {
            key: "destinataires_donnees",
            description: "Destinataires ou catégories de destinataires",
            legal_ref: "RGPD:art.13(1)(e)",
            patterns: &[
                "destinataires",
                "tiers autorisés",
                "tiers autorises",
                "sous-traitants",
                "transfert de données",
            ],
        },
        Requirement {
            key: "dpo_contact",
            description: "Coordonnées du délégué à la protection des données (DPO)",
            legal_ref: "RGPD:art.37",
            patterns: &[
                "délégué à la protection",
                "delegue a la protection",
                "dpo",
                "data protection officer",
                "responsable de traitement",
            ],
        },
    ];
    evaluate(text, REQS)
}

// ── Dispatcher ────────────────────────────────────────────────────────────────

/// Dispatch to the correct per-doctype evaluator based on `doc_type`.
///
/// Returns an empty `Vec` for unknown doc types (not an error).
#[must_use]
pub fn evaluate_doc(doc_type: &str, text: &str) -> Vec<MandatoryCheck> {
    match doc_type {
        "b2b_contract" | "contrat_commercial" => evaluate_b2b_contract(text),
        "b2c_contract" | "contrat_consommation" => evaluate_b2c_contract(text),
        "employment" | "contrat_travail" => evaluate_employment(text),
        "lease_commercial" | "bail_commercial" => evaluate_lease_commercial(text),
        "lease_residential" | "bail_residentiel" | "bail_habitation" => {
            evaluate_lease_residential(text)
        }
        "rgpd" | "privacy_policy" | "politique_confidentialite" => evaluate_rgpd(text),
        _ => Vec::new(),
    }
}

/// Roll up a slice of checks into a single aggregate status.
///
/// - `"complete"` — all requirements are `"present"`
/// - `"partial"` — at least one `"present"` but not all
/// - `"missing"` — no requirements are `"present"` (or list is empty)
#[must_use]
pub fn aggregate_status(checks: &[MandatoryCheck]) -> String {
    if checks.is_empty() {
        return "missing".to_string();
    }
    let present = checks.iter().filter(|c| c.status == "present").count();
    if present == checks.len() {
        "complete".to_string()
    } else if present > 0 {
        "partial".to_string()
    } else {
        "missing".to_string()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn b2b_contract_missing_penalites_de_retard_is_missing() {
        let text =
            "Article 1 - Parties. Article 2 - Prix HT et TTC. Article 3 - Modalités de paiement.";
        let result = evaluate_b2b_contract(text);
        let pen = result
            .iter()
            .find(|r| r.requirement == "penalites_de_retard")
            .unwrap();
        assert_eq!(pen.status, "missing");
    }

    #[test]
    fn b2b_contract_present_penalites_de_retard() {
        let text = "Article 4 - Pénalités de retard au taux légal majoré.";
        let result = evaluate_b2b_contract(text);
        let pen = result
            .iter()
            .find(|r| r.requirement == "penalites_de_retard")
            .unwrap();
        assert_eq!(pen.status, "present");
    }

    #[test]
    fn aggregate_status_complete_when_all_present() {
        let checks = vec![
            MandatoryCheck {
                requirement: "a".into(),
                status: "present".into(),
                description: String::new(),
                legal_ref: String::new(),
            },
            MandatoryCheck {
                requirement: "b".into(),
                status: "present".into(),
                description: String::new(),
                legal_ref: String::new(),
            },
        ];
        assert_eq!(aggregate_status(&checks), "complete");
    }

    #[test]
    fn aggregate_status_partial_when_some_present() {
        let checks = vec![
            MandatoryCheck {
                requirement: "a".into(),
                status: "present".into(),
                description: String::new(),
                legal_ref: String::new(),
            },
            MandatoryCheck {
                requirement: "b".into(),
                status: "missing".into(),
                description: String::new(),
                legal_ref: String::new(),
            },
        ];
        assert_eq!(aggregate_status(&checks), "partial");
    }

    #[test]
    fn aggregate_status_missing_when_none_present() {
        let checks = vec![MandatoryCheck {
            requirement: "a".into(),
            status: "missing".into(),
            description: String::new(),
            legal_ref: String::new(),
        }];
        assert_eq!(aggregate_status(&checks), "missing");
    }

    #[test]
    fn evaluate_doc_dispatches_employment_correctly() {
        let text = "Rémunération mensuelle brute de 3 000 €. Durée du travail: 35 heures.";
        let checks = evaluate_doc("employment", text);
        assert!(!checks.is_empty());
        let rem = checks.iter().find(|c| c.requirement == "remuneration").unwrap();
        assert_eq!(rem.status, "present");
    }

    #[test]
    fn evaluate_doc_returns_empty_for_unknown_type() {
        let checks = evaluate_doc("unknown_doc_type", "some text");
        assert!(checks.is_empty());
    }

    #[test]
    fn into_node_produces_mandatory_clause_check_variant() {
        let check = MandatoryCheck {
            requirement: "penalites_de_retard".into(),
            status: "present".into(),
            description: "desc".into(),
            legal_ref: "ref".into(),
        };
        let node = check.into_node();
        assert!(
            matches!(node, crate::legal::kg::NodeWrite::MandatoryClauseCheck { .. }),
            "expected MandatoryClauseCheck variant"
        );
    }

    #[test]
    fn rgpd_full_policy_is_complete() {
        let text = "\
            Finalités du traitement: amélioration du service. \
            Base légale: consentement. \
            Droits des personnes: droit d'accès, droit de rectification, \
            droit à l'effacement, droit d'opposition. \
            Durée de conservation: 3 ans. \
            Destinataires: sous-traitants techniques. \
            DPO: dpo@example.com.";
        let checks = evaluate_rgpd(text);
        assert_eq!(aggregate_status(&checks), "complete");
    }
}
