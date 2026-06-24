# Spec C (backlog) — UX du surface MCP anno-rag

**Date** : 2026-06-24
**Statut** : ⚠️ **BACKLOG — pas un design validé.** Constats de revue UX à brainstormer après les specs A (recherche corpus) et B (confiance NER). Aucune solution n'est figée ici ; ce document capture les problèmes et des pistes, pour ne rien perdre.
**Source** : revue UX du surface MCP (≈50 outils) du point de vue d'un avocat pilotant anno via Claude Desktop.

---

## Items rattachés ailleurs (pour mémoire, ne pas retraiter ici)

- **U1 — handles document lisibles** → **intégré à la Spec A §4.7**. (UUID copiés à la main entre outils.)
- **U6 — fuite du format Debug Rust dans `detect`** → **quick-win isolé**, traitable hors spec (voir §Quick wins).

---

## Items du périmètre Spec C (à brainstormer)

### U2 — Résultats vides silencieux 🔴
**Problème** : [`legal_extract_contract`](../../../crates/anno-rag-mcp/src/legal/extract.rs#L111) → `rows: []`, `legal_risk_review` → `findings: []`, `legal_timeline` → vide, **sans explication**, quand le KG n'est pas peuplé (gap CLI-ingest vs `legal_ingest`/`index profile=legal`). « Rien trouvé » est indistinguable de « outil non branché ».
**Piste** : enveloppe de réponse avec `status` + `reason` actionnable, ex. `{"rows": [], "status": "no_kg_data", "reason": "Document non enrichi. Réindexez via index(profile=legal)."}`. À généraliser à tous les outils légaux D2/D3.
**À décider en brainstorm** : enveloppe commune à tous les outils ? Ou champ `status` par outil ? Comment distinguer « vide légitime » de « non enrichi » de façon fiable (présence du doc dans le KG vs absence) ?

### U3 — Description de `search` illisible 🔴
**Problème** : [`search`](../../../crates/anno-rag-mcp/src/lib.rs#L2213) décrit la matrice `mode × scope` (auto/fast/semantic × all/knowledge/legal) en prose dense. Parsing peu fiable (agent comme humain).
**Piste** : documenter par exemples plutôt que par matrice ; éventuellement réduire la surface de `mode` (auto + explicit-only). 
**À décider** : peut-on simplifier la *sémantique* (pas juste la doc) sans casser les usages ? Faut-il scinder `search` ?

### U4 — ~10 outils dépréciés toujours exposés 🟠
**Problème** : `legacy_search`, `knowledge_search`, `ingest`, `reindex`, `legal_ingest`, `legal_search`, + variantes `forget`/`sources`/`status` ([lib.rs:2200, 2939, 2953, 3806, 3831, 3843, 3866, 3882, 3901](../../../crates/anno-rag-mcp/src/lib.rs)) restent sélectionnables par l'agent → dilue la précision de routage.
**Piste** : masquer derrière un flag (`ANNO_EXPOSE_DEPRECATED=1`) tout en gardant les handlers fonctionnels ; ou retirer du registre `list_tools` mais garder l'appel direct.
**À décider** : politique de dépréciation (combien de versions de grâce ?), risque de casser des configs existantes.

### U5 — Taxonomie status/forget/rehydrate confuse 🟠
**Problème** : 5 outils « status/health » (`anno_health`, `status`, `corpus_health`, `knowledge_status`, `privacy_status`), 3 « forget » (`forget`, `memory_forget`, `knowledge_forget`), 2 « rehydrate » (`rehydrate`, `legal_rehydrate_citation`). L'agent doit deviner lequel.
**Piste** : croiser les descriptions (chacune pointe vers ses voisines et précise son périmètre) ; à terme, namespacing logique.
**À décider** : renommage (coûteux, casse) vs. clarification des descriptions seulement.

### U7 — Cold start = 7 étapes orchestrées 🟡
**Problème** : `anno_health` → `status` → init vault → `status` → `download_models` → `index` → `search`. Sans le skill onboarding, l'agent improvise l'ordre.
**Piste** : enrichir `anno_health` avec un champ `next_step` explicite selon l'état (vault non init → `anno_init_vault` ; modèles absents → `download_models` ; etc.).
**À décider** : machine à états du setup ; où vit la logique (un seul outil « guide » vs. champ sur `anno_health`).

### U8 — Chargement modèle (10–15 min) sans progression 🟡
**Problème** : `legal_ingest` async expose `job_id` + [`job_status`](../../../crates/anno-rag-mcp/src/lib.rs#L3818) (bon pattern), mais le warmup modèle est invisible : 15 min de silence dans Claude Desktop. `status` expose `warmup_phase` mais rien n'annonce le délai *à l'avance*.
**Piste** : appliquer le pattern `job_status` au warmup ; ou faire renvoyer aux outils, tant que `warmup_phase != ready`, un message d'attente explicite avec ETA.
**À décider** : warmup proactif au démarrage vs. lazy ; comment exposer une ETA fiable.

---

## Quick wins (hors brainstorm, corrigeables isolément)

- **U6** : dans [`detect`](../../../crates/anno-rag-mcp/src/lib.rs#L2394), remplacer `format!("{:?}", e.category)` / `format!("{:?}", e.source)` par une sérialisation de label propre (`"IBAN_FR"`, `"pattern"`) au lieu du Debug Rust (`"Custom(\"IBAN_FR\")"`). 1 fonction, testable seule.

---

## Ordonnancement proposé

1. Spec A (en cours) — inclut U1.
2. Spec B — confiance NER PII.
3. **Brainstorm Spec C** à partir de ce backlog (U2–U5, U7–U8).
4. U6 : quick-win, plantable à tout moment (idéalement groupé avec un autre passage sur `detect.rs`).
