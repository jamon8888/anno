# Hacienda

[![crates.io](https://img.shields.io/crates/v/anno.svg)](https://crates.io/crates/anno)
[![Documentation](https://docs.rs/anno/badge.svg)](https://docs.rs/anno)
[![CI](https://github.com/jamon8888/anno/actions/workflows/ci.yml/badge.svg)](https://github.com/jamon8888/anno/actions/workflows/ci.yml)

> **Hacienda** est le nom de ce projet. Les crates conservent leurs noms historiques (`anno`, `anno-cli`, `anno-rag`, `anno-privacy-gateway`, `anno-rag-tabular`) pour la rétro-compatibilité ; « Hacienda » désigne le workspace dans son ensemble.

**Hacienda est une boîte à outils Rust pour transformer du texte brut — y compris des documents juridiques français — en données structurées exploitables par un LLM, tout en gardant les informations personnelles hors du cloud.**

Le workspace empile maintenant cinq couches principales :

1. **`anno`** — bibliothèque NER / coreference / PII / extraction de relations.
2. **`anno-cli`** — interface en ligne de commande pour exécuter ces extractions.
3. **`anno-rag`** — pipeline d'anonymisation + RAG local (LanceDB), avec serveur MCP exposant retrieval et mémoire à long terme à Claude.
4. **`anno-privacy-gateway`** — passerelle compatible API Anthropic qui filtre les PII avant qu'un prompt ne sorte vers un fournisseur LLM distant.
5. **`anno-rag-tabular`** — extraction tabulaire schema-driven pour revues juridiques, avec citations par cellule, vérification extractive et stockage LanceDB.

Tout fonctionne **localement par défaut** : poids de modèles en cache, vault chiffré AES-256-GCM sur disque, base vectorielle locale. Le seul cas où des données sortent de la machine est lorsque l'utilisateur l'autorise explicitement — et dans ce cas elles sont pseudonymisées au préalable.

Double licence MIT ou Apache-2.0. MSRV : 1.88.

---

## Product Documentation

- [Full documentation home](docs/README.md)
- [Install Hacienda](docs/getting-started/installation.md)
- [Claude Desktop/Cowork setup](docs/getting-started/claude-desktop-cowork.md)
- [Product concepts](docs/product/concepts.md)
- [MCP tools](docs/developers/mcp-tools.md)
- [Privacy model](docs/security-compliance/privacy-model.md)
- [Release management](docs/admins/release-management.md)

---

## État actuel

Le tronc local contient les travaux récents suivants :

- **Release binaire v0.11.0-rc.11** : archives Windows x64 et macOS Apple Silicon publiées sur GitHub Releases, avec `anno-rag`, `anno-privacy-gateway`, exemples Claude Desktop et checksums SHA-256.
- **anno-memory v0.2** dans `anno-rag` : mémoire bi-temporelle, références d'entités, rappel avec expansion graphe, invalidation de mémoires et audit MCP.
- **RGPD core + gateway v0.4** : routes de sujet `find` / `forget` / `export`, audit JSONL chaîné, bearer auth, pseudonymisation en streaming SSE et arrêt gracieux du gateway comme du serveur MCP.
- **Évaluation anonymisation v0.7** : corpus légal français annoté, regex e-mail, regex honorifiques FR pour personnes, baselines PII et logs d'audit du détecteur sans texte en clair.
- **`anno-rag-tabular` phase 1+2** : moteur d'extraction tabulaire, schémas TOML, batching de colonnes, client LLM abstrait, vérificateurs d'offsets/support, exports CSV/XLSX/Markdown, CLI `anno-rag review` et surface MCP de revue.

Les documents de conformité associés vivent dans `docs/superpowers/specs/`, `docs/runbooks/` et `docs/adrs/`.

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

#### GLiNER2 (fastino) + LoRA adapters — backend Candle multi-domaine

Pour les usages où l'on doit **changer de domaine à chaud** (juridique → médical → financier) sur un même modèle de base, Hacienda inclut le backend `gliner2_fastino_candle` (feature `gliner2-fastino-candle`). Caractéristiques :

- **Encoder DeBERTa-v2/v3** via `candle_transformers::models::debertav2::DebertaV2Model` (attention disentangled), 7 têtes Candle natives (`token_gather`, `span_rep`, `schema_gather`, `count_pred`, `count_lstm`, `scorer`, `classifier`) — pas de runtime ONNX, pure-Rust, accélération `metal` / `cuda` au choix.
- **Adapters LoRA au format PEFT**, fusionnés **à l'instant du load** :
  `W_merged = W_base + (alpha / r) · (lora_B @ lora_A)`
  La fusion est faite **une fois** sur les poids cibles à `load_adapter()`. Au forward, il n'y a **plus aucun coût LoRA** : on inference sur des poids fusionnés en place. Le compromis exact : swap d'adapter modéré (toutes les quelques minutes ou heures), pas une commutation par requête.
- **API** : `from_pretrained(model_id)` / `from_local(dir)`, puis `load_adapter(name, dir)`, `unload_adapter()` (réhydrate les poids de base depuis `base_model_dir`), `active_adapter()`. Vérifie automatiquement que l'adapter a été entraîné sur le même modèle de base que celui chargé — refus loud sinon.
- **Surface publique identique** au backend ONNX `gliner2_fastino` : `Model` + `ZeroShotNER`, plus `extract_with_label_descriptions`, `extract_with_label_thresholds`, `extract_structure`, `classify`. On bascule ONNX ↔ Candle par simple alias de type.
- **Pourquoi ça compte** : en pratique, exporter un modèle ONNX fusionné par domaine coûte ~6 Go de disque par variante. Avec ce backend, on stocke une base + N petits adapters LoRA (typiquement quelques Mo chacun) et on bascule à chaud sans re-export.

Détails d'implémentation : `crates/anno/src/backends/gliner2_fastino_candle/`.

### 1.2 Coreference et préprocessing RAG

Trois résolveurs : `SimpleCorefResolver` (règles, 9 sieves), `FCoref` (neuronal, 78.5 F1 sur CoNLL-2012), `MentionRankingCoref`.

`rag::resolve_for_rag()` et `rag::preprocess()` réécrivent les pronoms après chunking pour que **chaque chunk soit autonome** (« Elle a fondé X. » → « Marie Curie a fondé X. »). Indispensable pour qu'un système RAG retrouve les bons passages — un chunk avec un pronom non résolu est un chunk perdu pour la recherche sémantique.

### 1.3 PII et pseudonymisation

`anno::pii` classe les entités NER (personnes, lieux, organisations) **et** matche des motifs structurés : SSN, IBAN, IBAN-FR, NIR (sécurité sociale française), SIRET, cartes bancaires, e-mails, téléphones. Deux modes :

- `scan_and_redact` — remplace par `[PERSON_1]`, `[ID_NUMBER_1]`, etc. (perte d'information, mais irréversible).
- **Pseudonymisation via vault** (couche `anno-rag`) — remplace par des tokens stables (`PERSON_42`) tout en gardant le mapping texte clair ↔ token dans un fichier chiffré AES-256-GCM. La rehydratation est ensuite réversible côté machine locale uniquement.

### 1.4 RAG local sur documents juridiques français — couche `anno-rag`

Pipeline complet pour cabinet d'avocats ou DPO :

```
Dossier de documents
  → extraction texte (kreuzberg, OCR Tesseract optionnel pour PDF scannés)
  → détection PII (regex FR + anno NER) sur noms / NIR / SIRET / IBAN-FR / téléphones
  → pseudonymisation via Vault AES-256-GCM (cloakpipe-core)
  → embedding (intfloat/multilingual-e5-small)
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

La confidentialité des mémoires dépend de `ANNO_RAG_MEMORY_NER_MODE` : le mode asynchrone par défaut peut persister le texte brut avant enrichissement NER en arrière-plan, tandis que le mode synchrone utilise le chemin de tokenization inline quand il est disponible. Validez ce mode avant de stocker des mémoires sensibles. Les IDs sont des UUID v7, triables lexicographiquement par temps de création. Forget supprime les mémoires ciblées et cascade les tokens vault devenus orphelins quand le store le permet.

La v0.2 ajoute une couche mémoire temporelle et graphe sans base de graphe externe :

- `valid_from` / `valid_to` permettent les requêtes `as_of` et l'invalidation de faits obsolètes.
- `entity_refs` relie les mémoires par tokens vault et entités non-PII canonisées (`ent:TAG:value`).
- `memory_recall(..., graph_expand=true)` ajoute les voisins directs des meilleurs hits.
- `memory_graph_recall(entity, max_hops, per_hop_limit, as_of)` parcourt jusqu'à 2 hops par défaut.
- Les `Preference` et `Reference` concurrentes peuvent auto-invalider une ancienne ligne quand elles partagent une entité et dépassent le seuil de similarité configuré ; `Fact` et `Context` restent append-only.

### 1.6 Intégration Claude Desktop / Cowork via MCP

`anno-rag mcp` lance un serveur **Model Context Protocol** sur stdio. Claude Desktop, Cowork ou n'importe quel client MCP s'y branche en ajoutant une entrée à son fichier de configuration :

```json
{
  "anno-rag": {
    "command": "/absolute/path/to/anno-rag",
    "args": ["mcp"],
    "env": {}
  }
}
```

Par défaut, omettez `ANNO_RAG_VAULT_PASSPHRASE` pour utiliser le keyring OS. Les utilisateurs avancés peuvent ajouter localement `ANNO_RAG_VAULT_PASSPHRASE` avec un secret fort et unique ; JSON ne prend pas en charge les commentaires, gardez donc cette note hors du fichier de configuration.

Outils MCP exposés :

| Outil | Description |
|---|---|
| `search(query, top_k)` | Recherche le corpus indexé. La requête est pseudonymisée → embeddée → top-K LanceDB. Renvoie des chunks **pseudonymisés** : c'est exactement ce que Claude verra. |
| `rehydrate(text)` | Remplace les tokens `PERSON_*`, `EMAIL_*`, `NIR_*`… par les originaux depuis le vault local. Seule la machine de l'utilisateur a les deux faces du mapping. |
| `detect(text)` | Scan PII à blanc : liste des entités avec catégorie, source, confiance, offsets. Aucune substitution. Pratique pour l'aperçu UI. |
| `vault_stats()` | Statistiques du vault : nombre total de mappings et comptes par catégorie. |
| `memory_save(text, kind, session?)` | Persiste une mémoire ; la tokenization dépend de `ANNO_RAG_MEMORY_NER_MODE`. Renvoie l'id et le texte effectivement stocké. |
| `memory_recall(query, top_k, as_of?, graph_expand?)` | Recall hybride (vecteur + FTS), éventuellement point-in-time et augmenté par voisinage d'entités. Renvoie le plaintext **rehydraté** pour le tenant appelant. |
| `memory_graph_recall(entity, max_hops?, per_hop_limit?, as_of?)` | Rappel graphe sur `entity_refs`, avec nœuds, arêtes et mémoires connectées. |
| `memory_invalidate(id, at?)` | Pose `valid_to` sur une mémoire ; l'appel est idempotent. |
| `memory_forget(id? \| query?)` | Oubli par id ou par requête. Cascade sur les tokens vault orphelins. |
| `memory_list(session?, kind?, cursor?)` | Listing paginé par session ou catégorie. |
| `anno_init_vault(passphrase)` | Initialise le keyring du vault avec une passphrase fournie, sans l'écrire dans les logs. |
| `anno_health()` | Version moteur, cible de build, outils disponibles et état du vault. |
| `download_models()` | Télécharge les modèles attendus dans `~/.anno-rag/models` pour l'usage offline. |
| `legal_*` | Ingestion/recherche juridique, requêtes graphe, citations rehydratables, extraction de contrats/dossiers, timeline, revue de risque, audits de clauses, prescription et validation humaine. |
| `review_*` | Création de revues tabulaires, ajout de lignes, extraction/refinement de cellules, lock/unlock, override humain, export CSV/Markdown/XLSX et lecture de grille. |

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

Elle couvre aussi le streaming SSE : les deltas sont bufferisés et rescannés pour éviter qu'un token PII découpé entre deux chunks ressorte en clair. Les routes de données personnelles exposent `POST /v1/subjects/find`, `POST /v1/subjects/forget` et `GET /v1/subjects/{subject_ref}/export?format=json|csv`. Quand `ANNO_GATEWAY_BEARER_TOKEN` est configuré, les routes protégées exigent un bearer token en comparaison constant-time ; `/health` reste public. L'audit persistant peut écrire un registre JSONL chaîné par SHA-256 avec signature HMAC quotidienne.

Permet d'imposer le tokenization à des outils qui ne savent pas parler MCP.

### 1.8 Extraction tabulaire juridique — `anno-rag-tabular`

`anno-rag-tabular` ajoute un mode de revue en tableau pour contrats, NDA, emploi, immobilier et propriété intellectuelle. Le principe : un template TOML décrit les colonnes attendues, leurs types et conditions ; l'extracteur regroupe les colonnes par budget, appelle un `LlmClient`, parse les cellules, puis vérifie que chaque valeur est soutenue par les passages cités.

Phase 1 livrée :

- `schema` — définitions de colonnes, types de cellule, conditions et génération JSON Schema.
- `extract` — batching, parsing de cellules et orchestration par ligne.
- `verify` — vérification d'offsets, round-trip de citations et scoring de support.
- `storage` — tables LanceDB séparées pour reviews, colonnes, lignes et cellules.
- `fanout` — exécution concurrente par revue via `run_review`.
- `export` — sorties CSV, XLSX et Markdown.
- `anno-rag review` — CLI `list`, `create`, `add-rows`, `extract`, `export`.
- MCP `review_*` — création/lecture de revue, ajout de lignes, extraction en arrière-plan, refinement cellule, overrides humains, locks et exports.

La phase suivante reste explicitement à faire : UI ag-grid / application workbench finalisée et intégration complète avec les fournisseurs LLM locaux par abonnement.

---

## 2. Démarrage rapide

### 2.0 Installation Windows / macOS

Le workspace déclare MSRV 1.88, mais le dépôt épingle actuellement Rust **1.95** dans `rust-toolchain.toml` pour éviter un ICE rustc rencontré sur les diagnostics du backend `gliner2_fastino`. `rustup` utilisera automatiquement cette version depuis la racine du repo.

Pour une installation depuis les binaires GitHub Releases, voir aussi [docs/release/README-release.md](docs/release/README-release.md). Cette section décrit l'installation depuis le repo source ; les releases fournissent les mêmes binaires déjà compilés.

GPU sidecar builds are documented in [docs/release/accelerated-gpu-builds.md](docs/release/accelerated-gpu-builds.md). The default release remains CPU-first; use the Metal or CUDA archives only on matching hardware.

#### Windows 11

Prérequis :

1. Installer **Visual Studio Build Tools 2022** avec le workload **Desktop development with C++**.
2. Installer **Rustup** :

```powershell
winget install --id Rustlang.Rustup
rustup toolchain install 1.95
rustup default 1.95
```

3. Installer **Git** et **protoc** :

```powershell
winget install --id Git.Git
winget install --id protobuf.protoc
```

4. Optionnel, pour OCR des PDF scannés :

```powershell
winget install --id UB-Mannheim.TesseractOCR
```

Puis, depuis le dossier du repo :

```powershell
cargo build --release -p anno-rag-bin -p anno-privacy-gateway

# Pré-chauffe le cache HuggingFace : embedder + modèle NER
cargo run --release --example warmup_model -p anno-rag
```

Binaires produits :

```powershell
.\target\release\anno-rag.exe ingest C:\chemin\vers\dossier --recursive
.\target\release\anno-rag.exe mcp
.\target\release\anno-privacy-gateway.exe
```

Pour les binaires de release, Tesseract doit être dans le `PATH` pour l'OCR. Un `tesseract_path` personnalisé nécessite le support source/config et ne fait pas partie de l'installation release. Pour rester strictement hors-ligne après préchauffe, définir `ANNO_NO_DOWNLOADS=1`.

#### macOS

Prérequis :

```sh
xcode-select --install

# Homebrew si absent : https://brew.sh/
brew install git protobuf

curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup toolchain install 1.95
rustup default 1.95
```

Optionnel, pour OCR des PDF scannés :

```sh
brew install tesseract tesseract-lang
```

Puis, depuis le dossier du repo :

```sh
cargo build --release -p anno-rag-bin -p anno-privacy-gateway

# Pré-chauffe le cache HuggingFace : embedder + modèle NER
cargo run --release --example warmup_model -p anno-rag
```

Binaires produits :

```sh
./target/release/anno-rag ingest ~/cabinet/dossier-acme --recursive
./target/release/anno-rag mcp
./target/release/anno-privacy-gateway
```

Sur Apple Silicon, les features `metal` / `gliner2-fastino-candle-metal` servent aux backends Candle accélérés. Les backends ONNX/CoreML sont macOS-only côté link lorsqu'ils sont explicitement activés.

#### Claude Desktop

##### Installation release candidate via Claude Code

Pour tester une release candidate GitHub sans compiler, ouvrez Claude Code puis collez ce prompt :

```text
Install Hacienda anno-rag v0.11.0-rc.11 into Claude Desktop/Cowork from https://github.com/jamon8888/anno/releases/tag/v0.11.0-rc.11.
Download the asset for this machine (Windows x64: hacienda-v0.11.0-rc.11-x86_64-pc-windows-msvc.zip; macOS Apple Silicon: hacienda-v0.11.0-rc.11-aarch64-apple-darwin.tar.gz) plus SHA256SUMS.txt, verify the checksum, extract it to a stable local folder, and update Claude Desktop's claude_desktop_config.json so mcpServers.anno-rag runs the extracted anno-rag binary with args ["mcp"]. If models are not already installed, run anno-rag download-models once and set ANNO_MODELS_DIR to the path it prints. Do not add ANNO_RAG_VAULT_PASSPHRASE unless I provide one. After editing the config, tell me to fully restart Claude Desktop/Cowork and verify anno-rag appears under Connectors.
```

Remplacez le tag si vous installez une RC plus récente. Pour les builds depuis source, utilisez la configuration manuelle ci-dessous.

Claude Desktop se branche à Hacienda via le serveur MCP stdio `anno-rag mcp`. Il faut d'abord construire `anno-rag`, puis déclarer le binaire dans `claude_desktop_config.json`.

Chemins de config Claude Desktop :

- Windows : `%APPDATA%\Claude\claude_desktop_config.json`
- macOS : `~/Library/Application Support/Claude/claude_desktop_config.json`

Exemple Windows :

```json
{
  "mcpServers": {
    "anno-rag": {
      "command": "C:\\Users\\NMarchitecte\\anno\\target\\release\\anno-rag.exe",
      "args": ["mcp"],
      "env": {
        "ANNO_NO_DOWNLOADS": "1"
      }
    }
  }
}
```

Exemple macOS :

```json
{
  "mcpServers": {
    "anno-rag": {
      "command": "/Users/you/anno/target/release/anno-rag",
      "args": ["mcp"],
      "env": {
        "ANNO_NO_DOWNLOADS": "1"
      }
    }
  }
}
```

Points importants :

- Utiliser un chemin **absolu** vers le binaire.
- Sur Windows, les `\` doivent être doublés dans le JSON.
- Redémarrer complètement Claude Desktop après modification.
- Vérifier dans Claude Desktop via `+` → **Connectors**, ou dans les réglages développeur, que `anno-rag` est connecté.
- La première utilisation doit idéalement se faire après `cargo run --release --example warmup_model -p anno-rag`; sinon le serveur peut télécharger les modèles au premier appel, sauf si `ANNO_NO_DOWNLOADS=1`.
- Le vault reste local. Ne versionnez pas une vraie passphrase ; mettez-la dans la config locale Claude Desktop ou laissez `anno-rag` utiliser le keyring OS.

Le `anno-privacy-gateway` sert aux clients HTTP compatibles Anthropic. Pour Claude Desktop + MCP, la voie normale est `anno-rag mcp`.

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
cargo build --release -p anno-rag-bin

# Télécharge les modèles end-user (~970 MiB : embedder + NER)
./target/release/anno-rag download-models

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
- Les mappings clairs ↔ tokens restent dans le vault local.
- Les `outputs/*.anon.md` et les chunks/vecteurs RAG LanceDB doivent contenir des contenus pseudonymisés.
- LanceDB peut aussi stocker de l'état produit persistant (mémoires, schémas de revue, cellules, locks, corrections, exports). Cet état n'est pas un simple cache et peut contenir du texte clair selon les workflows et `ANNO_RAG_MEMORY_NER_MODE`.

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
Le `cloakpipe-core` vendored est Apache-2.0 (upstream `rohansx/cloakpipe`).
Kreuzberg est sous Elastic License 2.0 — compatible avec un usage on-prem ; pour une distribution SaaS, vérifier les termes.
