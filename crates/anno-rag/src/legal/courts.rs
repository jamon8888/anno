//! French court alias table.

use crate::legal::types::{CourtLevel, CourtRef};

/// One canonical court and the aliases used to resolve it.
pub struct CourtAlias {
    /// Text aliases that may appear in documents.
    pub aliases: &'static [&'static str],
    /// Stable court id.
    pub id: &'static str,
    /// Canonical display name.
    pub name: &'static str,
    /// Court level.
    pub level: CourtLevel,
}

const COURTS: &[CourtAlias] = &[
    CourtAlias {
        aliases: &[
            "Tribunal de commerce de Paris",
            "T. com. Paris",
            "Trib. com. Paris",
            "T.com. Paris",
        ],
        id: "trib_com_paris",
        name: "Tribunal de commerce de Paris",
        level: CourtLevel::Tribunal,
    },
    CourtAlias {
        aliases: &[
            "Tribunal judiciaire de Paris",
            "TGI Paris",
            "Trib. jud. Paris",
        ],
        id: "trib_jud_paris",
        name: "Tribunal judiciaire de Paris",
        level: CourtLevel::Tribunal,
    },
    CourtAlias {
        aliases: &["Cour d'appel de Paris", "CA Paris"],
        id: "ca_paris",
        name: "Cour d'appel de Paris",
        level: CourtLevel::CourAppel,
    },
    CourtAlias {
        aliases: &["Cour de cassation", "Cass."],
        id: "cour_cassation",
        name: "Cour de cassation",
        level: CourtLevel::CourCassation,
    },
    CourtAlias {
        aliases: &["Conseil d'État", "Conseil d'Etat", "CE"],
        id: "conseil_etat",
        name: "Conseil d'État",
        level: CourtLevel::ConseilEtat,
    },
    CourtAlias {
        aliases: &["Conseil de prud'hommes de Paris", "CPH Paris"],
        id: "cph_paris",
        name: "Conseil de prud'hommes de Paris",
        level: CourtLevel::Tribunal,
    },
];

/// Resolve a court alias contained in `text`.
#[must_use]
pub fn resolve(text: &str) -> Option<CourtRef> {
    let lower = text.to_lowercase();
    for court in COURTS {
        if court
            .aliases
            .iter()
            .any(|alias| lower.contains(&alias.to_lowercase()))
        {
            return Some(CourtRef {
                id: court.id.to_string(),
                name: court.name.to_string(),
                level: court.level,
                jurisdiction: None,
            });
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_handles_common_courts() {
        assert_eq!(
            resolve("Tribunal de commerce de Paris").unwrap().id,
            "trib_com_paris"
        );
        assert_eq!(resolve("Cour de cassation").unwrap().id, "cour_cassation");
        assert!(resolve("Some random text").is_none());
    }
}
