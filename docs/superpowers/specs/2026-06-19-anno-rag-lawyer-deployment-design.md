# Anno-RAG — Déploiement Avocat "Zéro Config"

**Date:** 2026-06-19  
**Statut:** Approuvé  
**Objectif:** Distribuer anno-rag-mcp à des avocats non-techniques avec une expérience d'installation en un clic, sans configuration manuelle.

---

## Contexte et problèmes actuels

| Problème | Impact |
|----------|--------|
| `ANNO_MODELS_DIR` doit être configuré manuellement | Blocant pour non-technique |
| `ANNO_RAG_VAULT_PASSPHRASE` exposé dans claude_desktop_config.json | Risque sécurité + confusion |
| Modèles téléchargés manuellement (~2.6 GB, Solon 2.1 GB + NER 500 MB) | Étape invisible, écho silencieux |
| `gliner2-candle-cpu` par défaut → 60-120s par inférence → timeout MCP | `detect` non fonctionnel sur CPU |
| Le `.msi` ne configure pas claude_desktop_config.json | L'avocat doit éditer du JSON |

---

## Architecture cible

### Modèles

| Rôle | Modèle actuel | Modèle cible | Format | Taille |
|------|---------------|--------------|--------|--------|
| NER / PII | SemplificaAI/gliner2-multi-v1-onnx (fp16) | **fastino/gliner2-privacy-filter-PII-multi** | ONNX FP16 (converti Plan 3) | ~150 MB |
| NER / Juridique | _(identique au PII, généraliste)_ | **SemplificaAI/gliner2-multi-v1-onnx** | ONNX FP16 (existant) | ~250 MB |
| Embedding | OrdalieTech/Solon-embeddings-large-0.1 | **BAAI/bge-m3 dense-only int8** | ONNX INT8 | ~145 MB |
| **Total download** | ~2.6 GB | **~545 MB** | | **−79 %** |

**Architecture dual-model NER :**
- `gliner2-privacy-filter-PII-multi` : spécialisé 42 types PII, 7 langues, F1 SOTA — utilisé par `detect` et le pipeline privacy
- `gliner2-multi-v1-onnx` : généraliste — utilisé par `legal_ingest`, `legal_extract_contract`, `legal_extract_case_file`
- Les deux sont label-conditioned (le schéma est un input) — aucun changement d'API MCP

**Pourquoi bge-m3 int8 dense-only :**
- MTEB SOTA multilingual 2026, excellent français juridique, 8192 tokens max
- Dense-only suffisant pour LanceDB (pas de ColBERT ni sparse dans anno-rag)
- INT8 ONNX : 2x plus rapide que fp16, perte de qualité négligeable pour retrieval
- `AlpEge/bge-m3-onnx-int8` déjà disponible sur HF — pas de conversion nécessaire
- dim=1024 (identique à Solon) → aucune migration d'index LanceDB existant

**Pourquoi FP16 et pas INT8 pour GLiNER2 :**
- INT8 dynamique casse le scoring des spans GLiNER2 (issue connue) → listes vides à threshold standard
- FP16 = sweet spot : 2x plus petit que fp32, aucune dégradation de précision

**Pourquoi ONNX (pas Candle) :**
- ONNX = <1s/inférence sur CPU ; Candle CPU = 60-120s → timeout MCP systématique
- Pour avocats sans GPU, ONNX est le seul backend viable

### Backend par défaut

```toml
# crates/anno-rag-bin/Cargo.toml
[features]
default = []  # ONNX par défaut, Candle opt-in explicite via --features gliner2-candle-cpu
```

### Fix narrow panic (Candle, pour les utilisateurs GPU)

Dans `pipeline.rs`, truncater à `max_position_embeddings` (512) avant l'appel encoder :

```rust
let max_seq = model.encoder.config.max_position_embeddings as usize;
let seq_len = record.input_ids.len().min(max_seq);
```

Ce fix est inclus même si Candle n'est plus le défaut, pour que les utilisateurs GPU ne paniquent pas.

---

## Déploiement zero-config

### 1. Chemin modèles standardisé (cross-platform)

| Plateforme | Chemin par défaut |
|------------|-------------------|
| Windows | `%APPDATA%\anno-rag\models` |
| macOS | `~/Library/Application Support/anno-rag/models` |
| Linux | `~/.local/share/anno-rag/models` |

- Résolution via `dirs::data_dir()` (crate `dirs`) — pas de code conditionnel par plateforme
- `ANNO_MODELS_DIR` reste supporté comme override avancé, mais n'est plus requis

### 2. Vault sans passphrase manuelle (cross-platform)

| Plateforme | Backend keyring | Comportement |
|------------|-----------------|--------------|
| Windows | DPAPI (`keyring` crate → Windows Credential Manager) | Lié à la session Windows |
| macOS | Keychain (`keyring` crate → macOS Keychain) | Lié au compte utilisateur |
| Linux | Secret Service / libsecret | Lié au trousseau GNOME/KDE |

- Aucun `ANNO_RAG_VAULT_PASSPHRASE` dans claude_desktop_config.json
- `ANNO_RAG_VAULT_PASSPHRASE` reste supporté pour les power users et les déploiements serveur
- La crate `keyring` (déjà dans le workspace) abstrait les trois plateformes

### 3. Auto-download au premier lancement

- Au démarrage MCP, si les modèles sont absents : téléchargement automatique en arrière-plan
- `status` expose la progression : `warmup_phase: "downloading"`, `download_progress_pct: 42`
- Pas de blocage : le MCP répond aux appels non-modèles (vault_stats, memory) pendant le download
- `detect`, `search`, `index` retournent `{"error": "models_loading", "progress_pct": 42}` pendant le download

### 4. Installeur Tauri — assistant de setup (cross-platform)

Un nouveau crate `anno-rag-setup` (Tauri + webview) produit les installeurs natifs :
- Windows : `.msi` via WiX bundle Tauri
- macOS : `.dmg` via Tauri macOS target

L'assisteur s'ouvre automatiquement à l'issue de l'installation et effectue **sans intervention utilisateur** :

1. **Patch `claude_desktop_config.json`** — détecte le chemin selon la plateforme, injecte l'entrée `anno-rag` sans écraser les autres serveurs MCP, écriture atomique
2. **Téléchargement des modèles** — barre de progression (~818 MB), téléchargés dans le chemin standardisé `dirs::data_dir()`
3. **Initialisation du vault** — clé générée et stockée dans le keyring système (DPAPI / Keychain)
4. **Confirmation finale** — "Anno-RAG est prêt. Redémarrez Claude Desktop."

```json
{
  "mcpServers": {
    "anno-rag": {
      "command": "/Applications/anno-rag.app/Contents/MacOS/anno-rag",
      "args": ["mcp"]
    }
  }
}
```

Aucune `env` nécessaire. L'avocat ne voit jamais un terminal.

Anno-rag reste un **binaire MCP pur** — Tauri est uniquement utilisé pour le setup, pas pour le runtime.

---

## CI — Matrice de builds

### Matrice cible

| Target | Runner CI | Features | Artifact produit |
|--------|-----------|----------|-----------------|
| `x86_64-pc-windows-msvc` | `windows-latest` | `default=[]` (ONNX CPU) | `.msi` Tauri + `.zip` binaire |
| `x86_64-pc-windows-msvc` | `self-hosted` CUDA | `gpu-cuda` | `.msi` CUDA + `.zip` binaire |
| `aarch64-apple-darwin` | `macos-14` | `default=[]` (ONNX CPU) | `.dmg` Tauri + `.tar.gz` binaire |
| `aarch64-apple-darwin` | `macos-14` | `gpu-metal` | `.dmg` Metal + `.tar.gz` binaire |
| `x86_64-apple-darwin` | `macos-13` | `default=[]` (ONNX CPU) | `.dmg` Tauri + `.tar.gz` binaire |

### Nommage des artifacts

```
anno-rag-{version}-{target}-{variant}.{ext}

Exemples :
anno-rag-0.14.0-x86_64-pc-windows-msvc-cpu.msi
anno-rag-0.14.0-aarch64-apple-darwin-metal.dmg
anno-rag-0.14.0-aarch64-apple-darwin-cpu.dmg
anno-rag-0.14.0-x86_64-apple-darwin-cpu.dmg
```

### Workflow GitHub Actions

Un workflow unifié `release-all.yml` remplace `release-binaries.yml` et `release-accelerated.yml` :

```yaml
strategy:
  matrix:
    include:
      - os: windows-latest
        target: x86_64-pc-windows-msvc
        features: ""           # ONNX CPU
        variant: cpu
      - os: [self-hosted, windows, cuda]
        target: x86_64-pc-windows-msvc
        features: gpu-cuda
        variant: cuda
      - os: macos-14
        target: aarch64-apple-darwin
        features: ""           # ONNX CPU
        variant: cpu
      - os: macos-14
        target: aarch64-apple-darwin
        features: gpu-metal
        variant: metal
      - os: macos-13
        target: x86_64-apple-darwin
        features: ""           # ONNX CPU Intel
        variant: cpu
```

Chaque job :
1. `cargo build --release -p anno-rag-bin --features {features}`
2. Smoke test : `anno-rag --help` + `anno-rag diagnose-gpu` si GPU
3. `tauri build` → produit `.msi` (Windows) ou `.dmg` (macOS)
4. Upload artifact nommé avec `{target}-{variant}`

## Périmètre de cette itération

**Inclus — runtime anno-rag (Plan 1) :**
- `default = []` dans `anno-rag-bin/Cargo.toml` (ONNX par défaut)
- Fix narrow panic dans `pipeline.rs` (Candle, pour utilisateurs GPU)
- `default_embed_model()` → `BAAI/bge-m3` int8 dense-only ONNX (dim=1024)
- Config dual-model NER : `ner_pii_model` (PII) + `ner_legal_model` (juridique)
- Download des 3 modèles : bge-m3-int8 + gliner2-PII-fp16 + gliner2-multi-fp16
- Chemin modèles cross-platform via `dirs::data_dir()`, sans `ANNO_MODELS_DIR`
- Vault keyring cross-platform (DPAPI / Keychain / Secret Service), sans passphrase manuelle
- Auto-download avec progression dans `status` (`download_progress_pct`)

**Inclus — conversion ONNX (Plan 3) :**
- Script de conversion `fastino/gliner2-privacy-filter-PII-multi` → ONNX FP16
- Via `gliner2-onnx 0.1.1` (Python, exécuté une seule fois en CI)
- Artifact hébergé sur HF ou GitHub Releases pour que download_models puisse le récupérer

**Inclus — installeur Tauri :**
- Crate `anno-rag-setup` (Tauri) : assistant de setup cross-platform
- Patch automatique `claude_desktop_config.json`, download modèles, init vault keyring
- Artifacts : `.msi` Windows + `.dmg` macOS (arm64 + x86_64)

**Inclus — CI :**
- Workflow `release-all.yml` unifié remplaçant `release-binaries.yml` + `release-accelerated.yml`
- Matrice : 5 jobs (Windows CPU, Windows CUDA, macOS arm64 CPU, macOS arm64 Metal, macOS x86_64 CPU)
- Tauri build intégré dans chaque job pour produire les installeurs natifs

**Hors périmètre (itération suivante) :**
- `anno-rag uninstall`
- Linux packaging (.deb / .AppImage)
- GPU CUDA macOS (non supporté par CUDA)

---

## Critères de succès

1. `cargo build --profile dev-fast -p anno-rag-bin` sans `--features gliner2-candle-cpu` → binaire ONNX
2. `detect` répond en <2s sur CPU Windows (froid après premier warmup)
3. Premier lancement sans `ANNO_MODELS_DIR` → modèles dans le chemin plateforme (`%APPDATA%`, `~/Library/Application Support`, `~/.local/share`)
4. Premier lancement sans `ANNO_RAG_VAULT_PASSPHRASE` → vault disponible via keyring système (DPAPI / Keychain / Secret Service)
5. `status` montre `download_progress_pct` pendant le premier téléchargement
