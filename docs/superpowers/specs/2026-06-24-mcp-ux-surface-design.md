# Spec C — UX du surface MCP anno-rag (design validé)

**Date** : 2026-06-24
**Statut** : ✅ **Design validé** — prêt pour plan d'implémentation.
**Portée** : les 6 items du backlog UX (U2, U3, U4, U5, U7, U8) + le quick-win U6 + les 4 items auparavant différés en §8 désormais promus (D1 simplification `search`, D2 noms canoniques, D3 ETA warmup, U1 résolution de handle dans la recherche), conçus comme **unités indépendantes derrière une seule convention d'enveloppe de réponse**.
**Source** : revue UX du surface MCP (~50 outils) du point de vue d'un avocat pilotant anno via Claude Desktop. Backlog : `2026-06-24-mcp-ux-surface-backlog.md`.
**Pré-requis** : Spec A (recherche corpus, handles document, `corpus_documents`) et Spec B (confiance NER) mergées (PR #81).

---

## § 0. Spine — convention d'enveloppe de réponse

On **codifie la forme déjà livrée** pour `corpus_required` ([lib.rs:833](../../../crates/anno-rag-mcp/src/lib.rs#L833)) en une convention documentée, appliquée à la main dans chaque outil. **Ce n'est pas un type wrapper** — on ne combat pas les macros rmcp `#[tool_router]`.

Toute réponse structurée d'outil porte un champ `status` de haut niveau (chaîne, ensemble fermé), plus deux champs en prose et une charge utile spécifique au `status` :

- `status` — stable pour la machine, ensemble fermé (table ci-dessous)
- `message` — ligne humaine : *ce qui s'est passé* (français, comme aujourd'hui)
- `hint` — *quoi faire ensuite* (le levier de routage de l'agent)
- + champs de charge utile selon le `status`

| `status` | signification | charge utile |
|----------|---------------|--------------|
| `ok` | succès, données présentes | les données |
| `empty` | succès, rien ne correspond réellement | — |
| `not_enriched` | document résolu mais KG juridique vide pour lui | — |
| `unknown_document` | doc_id/handle ne résout pas | — |
| `corpus_required` | désambiguïsation existante (inchangée) | `available[]` |
| `setup_required` | vault/modèles non configurés (cold start) | `next_step` |
| `not_ready` | modèles encore en warmup | `warmup{phase, elapsed_ms, eta_seconds, eta_human}` |
| `degraded` | service partiel — sous-système en erreur non-fatale **ou** dégradation volontaire par design (ex. fallback lexical) | `failing_component` (ex. `"semantic_ranking"`, `"kg_unavailable"`) |

**Règle de cohérence** : `status` toujours présent ; `message` = prose humaine ; `hint` = action suivante ; le reste est charge utile. L'enveloppe `corpus_required` actuelle (`status`/`message`/`available`/`hint`) est déjà conforme — aucun churn dessus.

---

## § 1. U2 — résultats vides silencieux (🔴) → trois statuts honnêtes

**Problème** : `legal_extract_contract` → `rows: []`, `legal_risk_review` → `findings: []`, `legal_timeline` → vide, sans explication, quand le KG n'est pas peuplé. « Rien trouvé » est indistinguable de « outil non branché ».

**Design** : chaque outil légal D2/D3 distingue trois cas avant de renvoyer un vide :

1. `unknown_document` — le handle/UUID ne résout pas → *« Document introuvable. Vérifiez le doc_id. »*
2. `not_enriched` — le doc existe dans le corpus mais le KG juridique n'a aucun nœud pour lui → *« Document non enrichi. Réindexez via index(profile=legal). »*
3. `empty` — enrichi, mais réellement aucun contrat/risque/événement → *« Aucun risque identifié dans ce document. »*

**Outils concernés** : `legal_extract_contract`, `legal_extract_case_file`, `legal_timeline`, `legal_risk_review`, `legal_mandatory_clause_audit`, et tout D2/D3 renvoyant des collections.

**Dépendance** : une vérification « le KG a-t-il des nœuds pour le doc X ? » bon marché par document (voir §7, risque 1).

---

## § 2. U6 — fuite du format Debug Rust (quick win) → labels propres

**Problème** : dans [`detect`](../../../crates/anno-rag-mcp/src/lib.rs#L2394), `format!("{:?}", e.category)` et `{:?}` sur `e.source` produisent `"Custom(\"IBAN_FR\")"` et `"Pattern"` au lieu de labels propres.

**Design** : une fonction d'aide de sérialisation de label → `"IBAN_FR"`, `"pattern"`. Testée unitairement, isolée. Indépendante du spine mais même esprit (sortie parseable).

> Note : U6 a déjà été partiellement traité dans la PR #81 (commit `21ac4f08`, `fix(detect-mcp): emit clean category label for Custom entities`). **Vérifier en plan** ce qui reste (notamment `e.source`) ; ne refaire que le manquant.

---

## § 3. U3 — description de `search` illisible (🔴) → exemples, pas matrice

**Problème** : [`search`](../../../crates/anno-rag-mcp/src/lib.rs#L2213) décrit la matrice `mode × scope` (auto/fast/semantic × all/knowledge/legal) en prose dense. Parsing peu fiable.

**Design** : **on garde la sémantique `mode × scope` inchangée** (une refonte sémantique est un changement séparé et plus risqué — YAGNI ici). On réécrit la description en : une règle de décision en une ligne + 3–4 exemples concrets travaillés (`search(mode=auto)` pour la plupart des cas ; `scope=legal` quand… ; `mode=fast` quand…). Aucun changement de comportement, aucun risque de migration.

---

## § 4. U4 — outils dépréciés toujours exposés (🟠) → masqués, pas retirés

**Problème** : ~10 outils dépréciés (`legacy_search`, `knowledge_search`, `ingest`, `reindex`, `legal_ingest`, `legal_search`, + variantes `forget`/`sources`/`status`) restent sélectionnables par l'agent → dilue la précision de routage.

**Design** :
- Marquer les outils dépréciés avec un marqueur de dépréciation (liste const ou attribut).
- Les filtrer hors de `list_tools` par défaut ; les exposer avec `ANNO_EXPOSE_DEPRECATED=1`.
- **Les handlers restent appelables** pour ne pas casser les configs existantes ; un appel à un outil déprécié journalise un avertissement de dépréciation.
- **Politique** : conserver les handlers 2 versions mineures de grâce.

**Mécanisme** : surcharge de `list_tools` dans le `ServerHandler` rmcp (voir §7, risque 2).

---

## § 5. U5 — taxonomie status/forget/rehydrate confuse (🟠) → descriptions croisées

**Problème** : 5 outils « status/health », 3 « forget », 2 « rehydrate ». L'agent doit deviner lequel.

**Design** : **pas de renommage** (un renommage casse les configs d'agent et la mémoire musculaire). Chaque outil chevauchant reçoit dans sa description (a) une phrase de périmètre précise et (b) un pointeur vers ses voisins. Exemple :

> `forget` → *« Efface une entrée mémoire conversationnelle. Pour un document indexé, voir knowledge_forget ; pour le vault, voir vault_admin. »*

Familles à croiser :
- **status/health** : `anno_health`, `status`, `corpus_health`, `knowledge_status`, `privacy_status`
- **forget** : `forget`, `memory_forget`, `knowledge_forget`
- **rehydrate** : `rehydrate`, `legal_rehydrate_citation`

Namespacing logique différé à une version majeure future.

---

## § 6. U7 + U8 — guidage du cycle de vie (🟡)

### U7 — cold start = 7 étapes orchestrées

**Design** : `anno_health` devient une machine à états qui renvoie `status: setup_required` + un unique `next_step` selon l'état :
- vault non initialisé → `next_step: "anno_init_vault"`
- modèles absents → `next_step: "download_models"`
- prêt → `status: ok`, `next_step: null`

Un seul champ sur l'outil d'entrée existant — **pas de nouvel outil « guide »** (moins d'outils = meilleur routage). La logique vit dans le handler de `anno_health`.

### U8 — chargement modèle (10–15 min) sans progression

**Design**, deux parties :
- **(a) Warmup proactif** : démarrer le warmup en arrière-plan au boot du serveur (non bloquant) pour que l'horloge des 10–15 min démarre immédiatement. *Changement de comportement* (aujourd'hui lazy, `serve_stdio_lazy_warmup_phase_starts_idle`) — voir §7, risque 3.
- **(b) ETA** : tant que `warmup_phase != Ready`, tout outil nécessitant les modèles renvoie `status: not_ready` avec `warmup{phase, elapsed_ms, eta_seconds, eta_human}`. ETA grossière basée sur la phase, pas une fausse précision en secondes :
  - `Downloading` → *« ~10–15 min (premier lancement) »*
  - `Loading` → *« ~1–2 min »*

L'état `warmup_phase` (Idle/Downloading/Loading/Ready/Failed) existe déjà et est exposé dans `status` ([lib.rs:1102](../../../crates/anno-rag-mcp/src/lib.rs#L1102)) ; U8 le **propage** aux autres outils.

---

## § 9. D1 — simplification sémantique de `search` (non-breaking)

**Constat** : les paramètres `mode`/`scope` sont déjà optionnels et `auto` auto-sélectionne déjà selon le scope ([lib.rs:2258](../../../crates/anno-rag-mcp/src/lib.rs#L2258)). Le vrai foot-gun n'est pas le défaut — c'est que **certaines combinaisons renvoient une erreur** (`mode=fast` + `scope=legal` → erreur).

**Design** : rendre la matrice `mode × scope` **totale — aucune combinaison ne renvoie d'erreur**. Les paires aujourd'hui invalides dégradent proprement au lieu d'échouer :
- `mode=fast` + `scope=legal` → passe lexicale rapide sur le scope légal, avec `status: degraded` + `failing_component: "semantic_ranking"` + `hint` (« recherche rapide ; relancez en mode=semantic pour le ranking légal complet »).

Aucun changement de signature de paramètre → non-breaking. Combiné à la réécriture par exemples de §3, l'agent ne peut plus composer un appel cassé. C'est la « simplification sémantique » de U3 sans rupture : chaque appel est valide.

---

## § 10. U1 — résolution de handle dans les résultats de recherche

**Constat** : Spec A a livré les handles lisibles (`corpus/relative_path`), mais les chunks LanceDB ne portent que `source_path`/`folder_path` absolus ([store.rs:110-111](../../../crates/anno-rag/src/store.rs#L110)) ; la table de handles (`corpus_documents`) vit ailleurs. A9-A10/A13 (schéma chunk `relative_path`) ont été différés. **Objectif** : permettre à l'agent de piper un hit de recherche directement dans `legal_extract_contract` sans recopier d'UUID.

**Design (recommandé : résolution au moment de la requête)** : à la construction des résultats, joindre `chunk.doc_id → corpus_documents.relative_path` et bâtir le handle `corpus_alias/relative_path`. **Pas de migration de schéma LanceDB ni de réindexation** (alternative écartée : ajouter une colonne `relative_path` aux chunks). Dégradation : si aucune ligne `corpus_documents` ne correspond, renvoyer l'UUID nu comme aujourd'hui.

**Dépendance** : `doc_id` doit être enregistré dans `corpus_documents` — vrai pour l'ingest légal après PR #81 ; le plan vérifie le chemin knowledge (voir §7, risque 4).

---

## § 11. D2 — noms canoniques + alias dépréciés (non-breaking)

**Constat** : les trois `forget` sont **réellement distincts** (`forget`=source indexée [lib.rs:2362], `memory_forget`=mémoire [2698], `knowledge_forget`=dossier knowledge) ; idem `status`/`rehydrate` vs leurs voisins. Le problème est le **nom nu non descriptif**, pas une redondance.

**Design** : donner aux outils nus ambigus un **nom canonique auto-descriptif**, et enregistrer **l'ancien nom comme alias déprécié routé par la machinerie de §4 (U4)** : le handler reste, l'alias disparaît de `list_tools`, un appel journalise un avertissement.

| Nom nu actuel | Nom canonique | Mécanisme |
|---------------|---------------|-----------|
| `forget` | `forget_source` | alias `forget` déprécié (caché via U4) |
| `rehydrate` | `detokenize` | alias `rehydrate` déprécié |
| `status` | `service_status` | alias `status` déprécié |

Non-breaking (les anciens noms restent appelables) et **réutilise §4** plutôt qu'un second mécanisme. Complète §5 (descriptions croisées) : §5 clarifie, §11 renomme proprement.

---

## § 12. D3 — ETA de warmup précise

**Design** : persister `download_ms` et `load_ms` à chaque transition `Ready` (petit JSON sous le répertoire des modèles — les durées ne sont pas sensibles). Au warmup suivant, `eta_seconds = max(0, durée_précédente_de_la_phase − elapsed_dans_la_phase)`. Premier lancement (aucun historique) → repli sur les chaînes par phase de §6/U8(b).

L'enveloppe `warmup` gagne `eta_seconds` (numérique, nullable) à côté de `eta_human`. Affine §6/U8 sans le remplacer.

---

## § 7. Risques d'implémentation à lever en planification

1. **U2** : le KG juridique doit répondre « des nœuds pour le doc X ? » à bas coût — vérifier que le store graphe le supporte ; sinon, U2 retombe sur une vérification de présence corpus plus grossière (`corpus_documents`).
2. **U4** : confirmer que rmcp permet de filtrer la sortie de `list_tools` (probablement via une surcharge `ServerHandler::list_tools`) sans casser le dispatch `#[tool_router]`. **§11 (D2) en dépend** (alias dépréciés cachés via le même mécanisme).
3. **U8(a)** : le warmup proactif consomme des ressources avidement au boot — confirmer que c'est acceptable pour le modèle de lancement stdio de Claude Desktop.
4. **U1 (§10)** : confirmer que `doc_id` des chunks est enregistré dans `corpus_documents` pour **tous** les chemins d'ingest (légal OK post-PR #81 ; vérifier knowledge) ; sinon la résolution de handle retombe sur l'UUID nu.

---

## § 8. Hors périmètre (vraiment différé)

Toutes les déferrals antérieures ont été promues en périmètre (§9–§12). Ne reste hors périmètre que :

- Renommage **majeur sans alias** (suppression définitive des anciens noms nus) — version majeure future, après la période de grâce de §11.
- Refonte de la sémantique `mode × scope` en un paramètre `intent` unique — au-delà de la totalisation non-breaking de §9.

---

## Ordre d'atterrissage suggéré (1 PR par unité)

1. **§0 + §2 (U6)** — convention documentée + quick-win labels (fondation, faible risque).
2. **§1 (U2)** — trois statuts honnêtes sur les outils légaux (plus haute sévérité).
3. **§10 (U1)** — résolution de handle dans les résultats de recherche (débloque le pipage hit → outil légal).
4. **§6 (U7+U8)** + **§12 (D3)** — guidage cycle de vie + ETA précise.
5. **§4 (U4)** + **§11 (D2)** — hygiène du registre + noms canoniques (D2 dépend de U4).
6. **§5 (U5)** — descriptions croisées.
7. **§3 (U3)** + **§9 (D1)** — réécriture description `search` + totalisation de la matrice.
