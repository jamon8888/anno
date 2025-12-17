//! Parity tests for backend registration.
//!
//! Goal: keep `BackendFactory::available_backends()` and `BackendFactory::create()`
//! in sync so we don't drift (e.g., listing a backend but failing with Unknown backend).

use anno::eval::backend_factory::BackendFactory;

#[test]
fn available_backends_are_creatable_or_explicitly_unavailable() {
    // We accept FeatureNotAvailable/Retrieval/etc. here — those are expected when
    // models/keys aren't present. What we *don't* accept is InvalidInput("Unknown backend").
    for name in BackendFactory::available_backends() {
        match BackendFactory::create(name) {
            Ok(_) => {}
            Err(e) => {
                let msg = format!("{e}");
                assert!(
                    !msg.to_lowercase().contains("unknown backend"),
                    "BackendFactory listed '{name}' but create() returned unknown: {msg}"
                );
            }
        }
    }
}
