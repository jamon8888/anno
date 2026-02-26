# anno-metrics

[![crates.io](https://img.shields.io/crates/v/anno-metrics.svg)](https://crates.io/crates/anno-metrics)
[![Documentation](https://docs.rs/anno-metrics/badge.svg)](https://docs.rs/anno-metrics)
[![CI](https://github.com/arclabs561/anno/actions/workflows/ci.yml/badge.svg)](https://github.com/arclabs561/anno/actions/workflows/ci.yml)

Shared evaluation and analysis primitives for [anno](https://crates.io/crates/anno-lib): coreference scoring metrics and cluster encoders.

Depends only on `anno-core` and `serde`, so it can be used by both `anno` (library) and `anno-eval` (evaluation harness) without creating dependency cycles.

## Install

```toml
[dependencies]
anno-metrics = "0.3"
```

## Coreference metrics

Implements the standard coreference scoring metrics used in CoNLL-2011/2012 and CRAC shared tasks:

| Metric | Unit of evaluation | Reference |
|--------|--------------------|-----------|
| MUC | Links between mentions within an entity | Vilain et al., 1995 |
| B3 | Per-mention precision/recall | Bagga & Baldwin, 1998 |
| CEAF-e | Entity-level optimal alignment | Luo, 2005 |
| CEAF-m | Mention-level optimal alignment | Luo, 2005 |
| LEA | Link-based, entity-importance weighted | Moosavi & Strube, 2016 |
| BLANC | Rand-index over mention pairs | Recasens & Hovy, 2010 |
| CoNLL | Average of MUC, B3, CEAF-e F1 | Pradhan et al., 2012 |

## Cluster encoder

Primitives for representing within-context clusters and scoring merges across contexts (document windows or separate documents), used in cross-document coreference pipelines.

## License

Dual-licensed under MIT or Apache-2.0.
