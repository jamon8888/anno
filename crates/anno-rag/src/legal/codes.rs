//! French code alias table and reference parser.

use crate::legal::types::ArticleRef;
use regex::Regex;

/// Map common French code aliases to canonical code identifiers.
const CODE_ALIASES: &[(&str, &str)] = &[
    ("c. civ.", "code_civil"),
    ("cciv", "code_civil"),
    ("code civil", "code_civil"),
    ("c. com.", "code_commerce"),
    ("ccom", "code_commerce"),
    ("code de commerce", "code_commerce"),
    ("c. trav.", "code_travail"),
    ("ctrav", "code_travail"),
    ("code du travail", "code_travail"),
    ("c. cons.", "code_consommation"),
    ("ccons", "code_consommation"),
    ("code de la consommation", "code_consommation"),
    ("cgi", "cgi"),
    ("code général des impôts", "cgi"),
    ("code general des impots", "cgi"),
    ("csp", "code_sante_publique"),
    ("code de la santé publique", "code_sante_publique"),
    ("csss", "code_securite_sociale"),
    ("code de la sécurité sociale", "code_securite_sociale"),
    ("cgct", "cgct"),
    ("code général des collectivités territoriales", "cgct"),
    ("cpc", "code_procedure_civile"),
    ("code de procédure civile", "code_procedure_civile"),
    ("cppen", "code_procedure_penale"),
    ("code de procédure pénale", "code_procedure_penale"),
    ("cpen", "code_penal"),
    ("code pénal", "code_penal"),
    ("cenv", "code_environnement"),
    ("code de l'environnement", "code_environnement"),
    ("cmf", "code_monetaire_financier"),
    ("code monétaire et financier", "code_monetaire_financier"),
];

/// Resolve a code alias case-insensitively.
#[must_use]
pub fn resolve_code(alias: &str) -> Option<&'static str> {
    let lower = alias.trim().to_lowercase();
    CODE_ALIASES
        .iter()
        .find(|(candidate, _)| *candidate == lower)
        .map(|(_, canonical)| *canonical)
}

/// Parse all article-with-code references from a span.
#[must_use]
pub fn parse_all(text: &str) -> Vec<ArticleRef> {
    static RE: once_cell::sync::Lazy<Regex> = once_cell::sync::Lazy::new(|| {
        Regex::new(
            r"(?ix)
            (?:art(?:icle|\.)?\s+)
            ([LRDA]?\s?\d+(?:[\-.]\d+)*)
            \s+
            (?:du\s+)?
            (
                c\.?\s?civ\.?|cciv|code\s+civil|
                c\.?\s?com\.?|ccom|code\s+de\s+commerce|
                c\.?\s?trav\.?|ctrav|code\s+du\s+travail|
                c\.?\s?cons\.?|ccons|code\s+de\s+la\s+consommation|
                cgi|code\s+g[ée]n[ée]ral\s+des\s+imp[ôo]ts|
                csp|code\s+de\s+la\s+sant[ée]\s+publique|
                csss|code\s+de\s+la\s+s[ée]curit[ée]\s+sociale|
                cgct|code\s+g[ée]n[ée]ral\s+des\s+collectivit[ée]s\s+territoriales|
                cpc|code\s+de\s+proc[ée]dure\s+civile|
                cppen|code\s+de\s+proc[ée]dure\s+p[ée]nale|
                cpen|code\s+p[ée]nal|
                cenv|code\s+de\s+l'environnement|
                cmf|code\s+mon[ée]taire\s+et\s+financier
            )
        ",
        )
        .expect("valid legal reference regex")
    });

    RE.captures_iter(text)
        .filter_map(|captures| {
            let article_num = captures.get(1)?.as_str().replace(' ', "");
            let code = resolve_code(captures.get(2)?.as_str())?.to_string();
            Some(ArticleRef { code, article_num })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_code_handles_common_aliases() {
        assert_eq!(resolve_code("c. civ."), Some("code_civil"));
        assert_eq!(resolve_code("Code de commerce"), Some("code_commerce"));
        assert_eq!(resolve_code("inconnu"), None);
    }

    #[test]
    fn parse_all_returns_multiple_refs() {
        let refs = parse_all("art. 1240 c. civ. et article L210-2 du Code de commerce");
        assert_eq!(refs.len(), 2);
        assert_eq!(
            refs[0],
            ArticleRef {
                code: "code_civil".into(),
                article_num: "1240".into()
            }
        );
        assert_eq!(
            refs[1],
            ArticleRef {
                code: "code_commerce".into(),
                article_num: "L210-2".into()
            }
        );
    }
}
