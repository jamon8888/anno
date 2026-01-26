//! Guardrail: ensure synthetic datasets retain minimum per-domain coverage.
//!
//! This prevents silent regressions where examples disappear from a domain.

use std::collections::HashMap;

use anno::eval::dataset::synthetic;
use anno::eval::dataset::Domain;

#[test]
fn synthetic_domain_minimum_counts() {
    // Minimums chosen at/below current counts to avoid flakiness but catch regressions.
    let mut minima: HashMap<Domain, usize> = HashMap::new();
    minima.insert(Domain::News, 5);
    minima.insert(Domain::SocialMedia, 5);
    minima.insert(Domain::Biomedical, 5);
    minima.insert(Domain::Financial, 5);
    minima.insert(Domain::Legal, 5);
    minima.insert(Domain::Scientific, 5);
    minima.insert(Domain::Entertainment, 5);
    minima.insert(Domain::Sports, 3);
    minima.insert(Domain::Politics, 3);
    minima.insert(Domain::Technical, 5);
    minima.insert(Domain::Conversational, 3);
    minima.insert(Domain::Historical, 3);
    minima.insert(Domain::Ecommerce, 3);
    minima.insert(Domain::Academic, 3);
    minima.insert(Domain::Travel, 3);
    minima.insert(Domain::Weather, 3);
    minima.insert(Domain::Food, 3);
    minima.insert(Domain::RealEstate, 3);
    minima.insert(Domain::Cybersecurity, 3);
    minima.insert(Domain::Multilingual, 5);
    minima.insert(Domain::Email, 2);

    for (domain, min_expected) in minima {
        let count = synthetic::by_domain(domain).len();
        assert!(
            count >= min_expected,
            "Domain {:?} has {} examples (< {}). Add examples or adjust minima.",
            domain,
            count,
            min_expected
        );
    }
}
