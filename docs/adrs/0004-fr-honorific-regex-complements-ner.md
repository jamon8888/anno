# ADR-004 — French honorific regex complements the NER tier instead of retraining the model

**Status:** Accepted (v0.7) · **Date:** 2026-05-15 · **Deciders:** anno team

## Context

The v0.7 anonymisation eval surfaced a precise NER weakness: GLiNER2-Fastino under-detects French names introduced by an honorific (Monsieur / Madame / Mlle / M. / Maître / Me). On the 35-doc French legal corpus, 14 of 59 Person truths were missed in the bare-NER run, putting Person recall at 0.7627 — well below the cabinet's protective-redaction need.

Three remediation paths:

1. **Retrain or LoRA-fine-tune the NER model** on French legal text with honorifics. Highest quality, but adds GPU time, model-cache drift, and a 1–2-week project.
2. **Swap the NER backend** (e.g. a French-specific model). Trades the existing model's strengths for unknown ones.
3. **Add a regex tier that complements the NER** — same architectural pattern already used for NIR / SIRET / IBAN / Phone / Email.

The detector architecture is already two-tier (regex + NER). The regex tier is fast, deterministic, and shippable today. The honorific pattern is small and well-bounded.

## Decision

**Add a French-honorific regex to `FrPatterns::person_fr_honorific`** that captures the *name* (2+ capitalised words allowing accents, hyphens, apostrophes) following one of the recognised honorifics. The honorific itself stays in cleartext; only the name is tokenised. Lowercase function words (`le`, `la`) between honorific and noun block role titles like *Monsieur le Président* from being misclassified.

## Consequences

- Person recall jumped from 0.7627 to 1.0000 on the corpus, with precision holding at 1.0000 (the 2+ capitalised word constraint kept false positives at zero).
- The fix is shippable in v0.7; the model-quality work is deferred to a separate roadmap item that needs neither v0.4 GDPR nor v0.1 memory to be useful.
- The regex tier becomes the canonical place to add cabinet-specific name patterns (e.g. specific titles found in particular case types). NER stays the safety net for names that don't fit a known pattern.
- The pattern is FR-only. Extending to other languages requires per-locale work (DE *Herr*/*Frau*, IT *Sig.*/*Sig.ra*, etc.) — tracked but not on the v0.4 critical path.
- The plan's Person/Org/Loc baseline was 0.0 placeholders until this regex landed and the eval ran — see commit `d43d8327` for the full numbers.

## Reference

`crates/anno-rag/src/detect.rs::FrPatterns::person_fr_honorific`, `crates/anno-rag/tests/fixtures/pii_baseline.toml`, commit `d43d8327`.
