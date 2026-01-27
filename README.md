# anno

Information extraction: named entity recognition and coreference.

Dual-licensed under MIT or Apache-2.0.

```sh
anno extract --text "Marie Curie was born in Warsaw."
```

```text
[Marie Curie] PERSON
[Warsaw] LOCATION
```

More: `docs/QUICKSTART.md`.
Docs index: `docs/README.md`.
Contract: `docs/CONTRACT.md`.

Install the CLI:

```sh
cargo install anno --features cli
```