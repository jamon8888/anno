# Hacienda

[![crates.io](https://img.shields.io/crates/v/anno.svg)](https://crates.io/crates/anno)
[![Documentation](https://docs.rs/anno/badge.svg)](https://docs.rs/anno)
[![CI](https://github.com/arclabs561/anno/actions/workflows/ci.yml/badge.svg)](https://github.com/arclabs561/anno/actions/workflows/ci.yml)

> **Hacienda** est le nom de ce projet. Les crates publiées conservent leurs noms historiques (`anno`, `anno-cli`, `anno-rag`, `anno-privacy-gateway`) pour la rétro-compatibilité ; « Hacienda » désigne le workspace dans son ensemble.

**Hacienda est une boîte à outils Rust pour transformer du texte brut — y compris des documents juridiques français — en données structurées exploitables par un LLM, tout en gardant les informations personnelles hors du cloud.**

Le workspace empile quatre couches :

1. **`anno`** — bibliothèque NER / coreference / PII / extraction de relations.
2. **`anno-cli`** — interface en ligne de commande pour exécuter ces extractions.
3. **`anno-rag`** — pipeline d'anonymisation + RAG local (LanceDB), avec serveur MCP exposant retrieval et mémoire à long terme à Claude.
4. **`anno-privacy-gateway`** — passerelle compatible API Anthropic qui filtre les PII avant qu'un prompt ne sorte vers un fournisseur LLM distant.

Tout fonctionne **localement par défaut** : poids de modèles en cache, vault chiffré AES-256-GCM sur disque, base vectorielle locale. Le seul cas où des données sortent de la machine est lorsque l'utilisateur l'autorise explicitement — et dans ce cas elles sont pseudonymisées au préalable.

Double licence MIT ou Apache-2.0. MSRV : 1.88.

---

## 1. Ce que fait le projet, fonctionnellement

### 1.1 Extraction d'entités (NER) — couche `anno`

Étant donné un texte, `anno::extract` retourne la liste des entités détectées avec leur type, leur position en offsets caractères, et un score de confiance.

```rust
let entities = anno::extract("Sophie Wilson designed the ARM processor.")?;
// Sophie Wilson [PER] (0,13) 0.60
// ARM           [ORG] (27,30) 0.55
# Ok::<(), anno::Error>(())
```

`StackedNER::default()` choisit dynamiquement le meilleur backend disponible à l'exécution :

- **BERT ONNX** et **NuNER** (chargés indépendamment si la feature `onnx` est active et que les poids sont en cache),
- puis **GLiNER** (zero-shot, types personnalisés) si aucun des précédents ne se charge,
- enfin **patterns + heuristiques** pures Rust comme filet de sécurité hors-ligne.

`ANNO_NO_DOWNLOADS=1` interdit les téléchargements HuggingFace ; les modèles déjà en cache ou exportés localement continuent de fonctionner. Aucun appel réseau silencieux.

Backends additionnels disponibles via features : **W2NER**, **TPLinker** (relations), **GLiRel** (relations zero-shot), **CRF** et **HMM** (statistiques), **LLM** (extraction par modèle distant via OpenRouter / Anthropic / Groq / Gemini / Ollama). Liste complète : [docs/BACKENDS.md](docs/BACKENDS.md).

### 1.2 Coreference et préprocessing RAG

Trois résolveurs : `SimpleCorefResolver` (règles, 9 sieves), `FCoref` (neuronal, 78.5 F1 sur CoNLL-2012), `MentionRankingCoref`.

`rag::resolve_for_rag()` et `rag::preprocess()` réécrivent les pronoms après chunking pour que **chaque chunk soit autonome** (« Elle a fondé X. » → « Marie Curie a fondé X. »). Indispensable pour qu'un système RAG retrouve les bons passages — un chunk avec un pronom non résolu est un chunk perdu pour la recherche sémantique.

### 1.3 PII et pseudonymisation

`anno::pii` classe les entités NER (personnes, lieux, organisations) **et** matche des motifs structurés : SSN, IBAN, IBAN-FR, NIR (sécurité sociale française), SIRET, cartes bancaires, e-mails, téléphones. Deux modes :

- `scan_and_redact` — remplace par `[PERSON_1]`, `[ID_NUMBER_1]`, etc. (perte d'information, mais irréversible).
- **Pseudonymisation via vault** (couche `anno-rag`) — remplace par des tokens stables (`PERSON_42`) tout en gardant le mapping cleartext ↔ token dans un fichier chiffré AES-256-GCM. La rehydratation est ensuite réversible côté machine locale uniquement.

### 1.4 RAG local sur documents juridiques français — couche `anno-rag`

Pipeline complet pour cabinet d'avocats ou DPO :

```
Dossier de documents
  → extraction texte (kreuzberg, OCR Tesseract optionnel pour PDF scannés)
  → détection PII (regex FR + anno NER) sur noms / NIR / SIRET / IBAN-FR / téléphones
  → pseudonymisation via Vault AES-256-GCM (cloakpipe-core)
  → embedding (BGE-multilingual-e5-small)
  → indexation LanceDB
  → sortie : outputs/*.anon.md (pseudonymisée) + index vectoriel
```

Au query time : la requête est pseudonymisée avec le **même mapping**, embeddée, et recherchée dans LanceDB ; les top-K chunks renvoyés sont eux aussi pseudonymisés. Le PII en clair ne quitte jamais `~/.anno-rag/vault.enc`.

Index vectoriel construit automatiquement dès que la table de chunks dépasse 1000 lignes (seuil configurable). Détails techniques : [crates/anno-rag/README.md](crates/anno-rag/README.md).

### 1.5 Mémoire à long terme — `anno-rag` MCP

Au-dessus du moteur RAG, `anno-rag` expose une mémoire structurée persistante destinée à un assistant LLM. Quatre catégories typées :

| Kind | Usage |
|---|---|
| `Fact` | Affirmation factuelle stable (« le cabinet a 12 avocats ») |
| `Preference` | Préférence utilisateur / session (« l'utilisateur préfère les tableaux à la prose ») |
| `Reference` | Pointeur vers une entité canonique citée par d'autres mémoires |
| `Context` | Contexte transitoire (dossier courant, tâche en cours) |

Chaque mémoire est **pseudonymisée à l'écriture** (les PII de son corps sont remplacés par des tokens vault), puis indexée avec recherche **hybride vecteur + plein texte**. Les IDs sont des UUID v7, triables lexicographiquement par temps de création. Forget = suppression logique avec cascade sur les tokens vault qui ne sont plus référencés (SLO d'effacement physique sous 24 h, conforme à l'esprit de l'Art. 17 RGPD).

### 1.6 Intégration Claude Desktop / Cowork via MCP

`anno-rag mcp` lance un serveur **Model Context Protocol** sur stdio. Claude Desktop, Cowork ou n'importe quel client MCP s'y branche en ajoutant une entrée à son fichier de configuration :

```json
{
  "anno-rag": {
    "command": "/absolute/path/to/anno-rag",
    "args": ["mcp"],
    "env": {
      "ANNO_RAG_VAULT_PASSPHRASE": "your-passphrase-here"
    }
  }
}
```

Outils MCP exposés :

| Outil | Description |
|---|---|
| `search(query, top_k)` | Recherche le corpus indexé. La requête est pseudonymisée → embeddée → top-K LanceDB. Renvoie des chunks **pseudonymisés** : c'est exactement ce que Claude verra. |
| `rehydrate(text)` | Remplace les tokens `PERSON_*`, `EMAIL_*`, `NIR_*`… par les originaux depuis le vault local. Seule la machine de l'utilisateur a les deux faces du mapping. |
| `detect(text)` | Scan PII à blanc : liste des entités avec catégorie, source, confiance, offsets. Aucune substitution. Pratique pour l'aperçu UI. |
| `vault_stats()` | Statistiques du vault : nombre total de mappings et comptes par catégorie. |
| `memory_save(text, kind, session?)` | Persiste une mémoire après tokenization PII. Renvoie l'id et le texte effectivement stocké. |
| `memory_recall(query, top_k)` | Recall hybride (vecteur + FTS). Renvoie le plaintext **rehydraté** pour le tenant appelant. |
| `memory_forget(id? \| query?)` | Oubli par id ou par requête. Cascade sur les tokens vault orphelins. |
| `memory_list(session?, kind?, cursor?)` | Listing paginé par session ou catégorie. |

Le flux typique avec Claude Desktop :

```
Utilisateur  →  Claude  →  search("résiliation Acme")
                            ↓
              anno-rag pseudonymise la query,
              embedde, fetch LanceDB
                            ↓
              chunks pseudonymisés (PERSON_42, NIR_7…)
                            ↓
              Claude raisonne UNIQUEMENT sur les tokens
                            ↓
Utilisateur  ←  Claude  ←  rehydrate("…PERSON_42…")
              (sortie finale avec noms réels,
               jamais vue par le modèle distant)
```

Conséquence : on peut utiliser un modèle hébergé (Claude API) sur des documents soumis au secret professionnel sans que les noms, NIR ou IBAN ne quittent la machine en clair.

### 1.7 Privacy Gateway — `anno-privacy-gateway`

Passerelle HTTP **compatible API Anthropic** qui s'intercale entre n'importe quel client (Cowork, app maison) et un fournisseur LLM. Elle :

- inspecte le prompt sortant,
- pseudonymise les PII via le même vault que `anno-rag`,
- relaie la requête vers le fournisseur,
- rehydrate la réponse avant de la rendre au client.

Permet d'imposer le tokenization à des outils qui ne savent pas parler MCP.

---

## 2. Démarrage rapide

### 2.1 Bibliothèque NER seule

```toml
[dependencies]
anno = "0.10"
```

```rust
use anno::prelude::*;

let entities = anno::extract("Sophie Wilson designed the ARM processor.")?;
let people: Vec<_> = entities.of_type(&EntityType::Person).collect();
let confident: Vec<_> = entities.above_confidence(0.8).collect();
# Ok::<(), Error>(())
```

Contrôle direct des backends :

```rust
use anno::{Model, StackedNER};

let m = StackedNER::default();
let ents = m.extract_entities("Sophie Wilson designed the ARM processor.", None)?;
# Ok::<(), anno::Error>(())
```

Zero-shot avec GLiNER :

```rust
use anno::GLiNEROnnx;

let m = GLiNEROnnx::new("onnx-community/gliner_small-v2.1")?;
let ents = m.extract("Aspirin treats headaches.", &["drug", "symptom"], 0.5)?;
# Ok::<(), anno::Error>(())
```

Backend externe via closure :

```rust
use anno::{AnyModel, Entity, EntityType, Language, Model, Result};

let model = AnyModel::new(
    "my-ner", "REST API wrapper",
    vec![EntityType::Person, EntityType::Organization],
    |_text, _lang| -> Result<Vec<Entity>> { Ok(vec![]) },
);
# Ok::<(), anno::Error>(())
```

### 2.2 RAG local complet

```sh
cargo build --release -p anno-rag

# Pré-chauffe le cache de modèles (~600 MiB : embedder + NER)
cargo run --release --example warmup_model -p anno-rag

# Ingestion d'un dossier
./target/release/anno-rag ingest ~/cabinet/dossier-acme --recursive

# Recherche CLI
./target/release/anno-rag search "résiliation pour cause"

# Serveur MCP (à brancher à Claude Desktop / Cowork)
./target/release/anno-rag mcp
```

### 2.3 CLI

```sh
cargo install anno-cli --features onnx

anno extract --text "Lynn Conway worked at IBM and Xerox PARC in California."
anno extract --model gliner --extract-types "DRUG,SYMPTOM" \
  --text "Aspirin can treat headaches and reduce fever."
anno debug --coref -t "Sophie Wilson designed the ARM. She revolutionized mobile computing."
```

Sortie JSON avec `--format json`. Batch avec `anno batch`. Export graphe (N-Triples, JSON-LD, CSV) avec `anno export --features graph`.

---

## 3. Aspects techniques

### 3.1 Feature flags (crate `anno`)

- `onnx` (par défaut) — runtime ONNX, backends BERT / NuNER / GLiNER / FCoref.
- `candle` — backends pure-Rust (pas de runtime C++).
- `metal` / `cuda` — accélération GPU (active `candle`).
- `llm` — extraction par LLM (OpenRouter, Anthropic, Groq, Gemini, Ollama).
- `discourse` — centering theory, anaphores abstraites, dialog acts.
- `analysis` — métriques de coreference et encodeurs de clusters.
- `schema` — JSON Schema pour les types de sortie.
- `production` — instrumentation `tracing`.

### 3.2 Garanties d'offsets

Tous les spans sont en **offsets caractères**, pas en bytes. Détails et invariants : [docs/CONTRACT.md](docs/CONTRACT.md).

### 3.3 Vault et clé de chiffrement (anno-rag)

- Clé dérivée par **Argon2id** depuis `ANNO_RAG_VAULT_PASSPHRASE`, ou tirée aléatoirement (32 bytes hex) et stockée dans le keyring OS.
- Cleartext jamais écrit hors de `~/.anno-rag/vault.enc`.
- Les `outputs/*.anon.md` et l'index LanceDB ne contiennent que des tokens.

### 3.4 Périmètre

Anno fait de l'**inférence**. Les pipelines d'entraînement sont hors scope : utilisez les frameworks upstream et exportez en ONNX.

### 3.5 Troubleshooting

- **Erreurs de link ONNX** : compilez avec `default-features = false` ou positionnez `ORT_DYLIB_PATH`.
- **Téléchargements** : `ANNO_NO_DOWNLOADS=1` pour rester sur le cache (utile derrière un firewall).
- **Features manquantes** : la plupart des backends sont gated derrière `onnx` ou `candle`.
- **Dépendance build sur Linux/WSL** (anno-rag) : `apt install libprotobuf-dev` puis build avec `PROTOC_INCLUDE=/usr/include` (lance-encoding réclame `google/protobuf/empty.proto`).
- **OCR PDF scanné** (anno-rag) : Tesseract n'est pas bundlé. `sudo apt install tesseract-ocr tesseract-ocr-fra` puis `--enable-ocr`.

---

## 4. Exemples

Tous dans `crates/anno/examples/`. `cargo run --example <name>`.

| Exemple | Feature | Démontre |
|---------|---------|----------|
| `quickstart` | — | Extraction en une ligne, filtrage `EntitySliceExt` |
| `pii_redact` | — | Détection noms / SSN / e-mails, redaction ou pseudonymisation |
| `zero_shot` | `onnx` | Types personnalisés via GLiNER |
| `relations` | — | Extraction de paires d'entités avec TPLinker |
| `gliner_multitask` | `onnx` | NER + classification via TaskSchema |
| `coref` | `analysis` | Chaînes coreference (« Marie Curie » → « Curie ») |
| `export_formats` | — | brat standoff, CoNLL BIO, JSONL, graph CSV |
| `rag_preprocess` | — | Chunking + réécriture pronominale pour chunks RAG autonomes |
| `batch` | — | Extraction parallèle multi-documents |

---

## 5. Références

[1] Grishman & Sundheim, *COLING* 1996.
[2] Tjong Kim Sang & De Meulder, *CoNLL* 2003.
[3] Otmazgin et al., *AACL* 2022 (F-COREF).
[4] Jurafsky & Martin, *SLP3* 2024.
[5] Zaratiana et al., *NAACL* 2024 (GLiNER).
[6] Bogdanov et al., 2024 (NuNER).
[7] Li et al., *AAAI* 2022 (W2NER).
[8] Devlin et al., *NAACL* 2019 (BERT).
[9] Lafferty et al., *ICML* 2001 (CRF).
[10] Wang et al., *COLING* 2020 (TPLinker).
[11] Stepanov & Shtopko, 2024 (GLiNER multi-task).
[12] Rabiner, *Proc. IEEE* 1989 (HMM).

Liste complète : [docs/REFERENCES.md](docs/REFERENCES.md). Citable via [CITATION.cff](CITATION.cff).

## License

Double licence MIT ou Apache-2.0.
Le `cloakpipe-core` vendu est Apache-2.0 (upstream `rohansx/cloakpipe`).
Kreuzberg est sous Elastic License 2.0 — compatible avec un usage on-prem ; pour une distribution SaaS, vérifier les termes.
