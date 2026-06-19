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

| Rôle | Modèle actuel | Modèle cible | Taille |
|------|---------------|--------------|--------|
| NER / détection PII | SemplificaAI/gliner2-multi-v1-onnx (fp16) | **identique** | ~250 MB |
| Embedding | OrdalieTech/Solon-embeddings-large-0.1 | **nomic-ai/nomic-embed-text-v1.5** | ~274 MB |
| **Total download** | ~2.6 GB | **~524 MB** | −80% |

**Pourquoi nomic-embed-text-v1.5 :**
- MTEB score comparable ou supérieur à Solon sur texte multilingue
- 274 MB vs 2136 MB — facteur 8x plus petit
- Licence Apache 2.0, disponible HF Hub
- Supporte les instructions de requête (`search_query:`, `search_document:`) pour meilleure précision RAG

**Pourquoi garder ONNX (pas Candle/fastino) :**
- `fastino/gliner2-multi-v1` et `SemplificaAI/gliner2-multi-v1-onnx` sont les mêmes poids
- ONNX = <1s/inférence sur CPU Windows ; Candle CPU = 60-120s → timeout MCP systématique
- Pour avocat sans GPU, ONNX est le seul backend viable

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

### 4. Commande `anno-rag install` (cross-platform)

Le binaire expose une sous-commande `install` qui patch `claude_desktop_config.json` :

```
anno-rag install
```

Comportement :
1. Détecte le chemin Claude Desktop selon la plateforme :
   - Windows : `%APPDATA%\Claude\claude_desktop_config.json`
   - macOS : `~/Library/Application Support/Claude/claude_desktop_config.json`
   - Linux : `~/.config/Claude/claude_desktop_config.json`
2. Lit le JSON existant (ou crée `{}` si absent)
3. Injecte ou met à jour l'entrée `anno-rag` — sans écraser les autres serveurs MCP
4. Écrit le fichier atomiquement (write + rename)
5. Avertit si Claude Desktop est en cours d'exécution (demande de redémarrage)

```json
{
  "mcpServers": {
    "anno-rag": {
      "command": "/usr/local/bin/anno-rag",
      "args": ["mcp"]
    }
  }
}
```

Aucune `env` nécessaire — vault via keyring système, modèles via chemin standardisé.

Le `.msi` (Windows) et le `.pkg` (macOS) exécutent `anno-rag install` en post-install automatiquement.

---

## Périmètre de cette itération

**Inclus :**
- Changer `default_embed_model()` vers `nomic-ai/nomic-embed-text-v1.5`
- Changer `default = []` dans `anno-rag-bin/Cargo.toml` (ONNX par défaut)
- Fix narrow panic dans `pipeline.rs` (Candle, pour utilisateurs GPU)
- Chemin modèles standardisé cross-platform via `dirs::data_dir()`, sans `ANNO_MODELS_DIR`
- Vault via keyring système cross-platform (DPAPI / Keychain / Secret Service), sans passphrase
- Auto-download avec progression dans `status` (`download_progress_pct`)
- Commande `anno-rag install` : patch `claude_desktop_config.json` cross-platform
- `.msi` (Windows) et `.pkg` (macOS) exécutent `anno-rag install` en post-install

**Hors périmètre (itération suivante) :**
- GUI de progression pendant le téléchargement
- Commande `anno-rag uninstall` (retrait de claude_desktop_config.json)

---

## Critères de succès

1. `cargo build --profile dev-fast -p anno-rag-bin` sans `--features gliner2-candle-cpu` → binaire ONNX
2. `detect` répond en <2s sur CPU Windows (froid après premier warmup)
3. Premier lancement sans `ANNO_MODELS_DIR` → modèles dans le chemin plateforme (`%APPDATA%`, `~/Library/Application Support`, `~/.local/share`)
4. Premier lancement sans `ANNO_RAG_VAULT_PASSPHRASE` → vault disponible via keyring système (DPAPI / Keychain / Secret Service)
5. `status` montre `download_progress_pct` pendant le premier téléchargement
