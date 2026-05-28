//! Prompt preflight guard — blocks fallback LLM calls when the user
//! prompt contains obvious clear-text PII patterns.
//!
//! The guard is deliberately conservative (regex-based, not ML-based)
//! so it is fast and cannot be fooled by paraphrasing into allowing
//! a clear email address through. It is not a complete PII detector;
//! its role is to catch the common cases so that pseudonymised chunks
//! (PERSON_1, ORG_1, …) can safely proceed to the fallback LLM, while
//! raw personal data does not.

use regex::Regex;
use std::sync::OnceLock;

/// Returns `true` when the prompt is safe to send to a fallback LLM —
/// i.e. it contains none of the obvious PII patterns checked here.
pub fn fallback_prompt_is_safe(prompt: &str) -> bool {
    obvious_email_absent(prompt)
        && obvious_phone_absent(prompt)
        && obvious_iban_absent(prompt)
        && obvious_siren_absent(prompt)
}

fn obvious_email_absent(text: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"(?i)[a-z0-9._%+\-]+@[a-z0-9.\-]+\.[a-z]{2,}").expect("email regex")
    });
    !re.is_match(text)
}

fn obvious_phone_absent(text: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"(?x)(?:\+33|0)\s*[1-9](?:[\s.\-]?\d{2}){4}").expect("phone regex")
    });
    !re.is_match(text)
}

fn obvious_iban_absent(text: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re =
        RE.get_or_init(|| Regex::new(r"(?i)\bFR\d{2}(?:[\s]?[0-9A-Z]){23}\b").expect("iban regex"));
    !re.is_match(text)
}

fn obvious_siren_absent(text: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"\b\d{3}\s?\d{3}\s?\d{3}\b").expect("siren regex"));
    !re.is_match(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_pseudonymized_prompt() {
        assert!(fallback_prompt_is_safe("ORG_1 signe avec PERSON_1."));
    }

    #[test]
    fn rejects_clear_email() {
        assert!(!fallback_prompt_is_safe(
            "Contact: marie.dupont@example.com"
        ));
    }

    #[test]
    fn rejects_clear_french_phone() {
        assert!(!fallback_prompt_is_safe("Tel: 06 12 34 56 78"));
    }

    #[test]
    fn rejects_french_iban() {
        assert!(!fallback_prompt_is_safe(
            "IBAN: FR76 3000 6000 0112 3456 7890 189"
        ));
    }

    #[test]
    fn accepts_legal_boilerplate_without_pii() {
        let text =
            "Le bail commercial est soumis aux articles L145-1 et suivants du Code de commerce.";
        assert!(fallback_prompt_is_safe(text));
    }
}
