# Spec A — Sémantique de recherche corpus (orientée cabinet)

**Date** : 2026-06-24
**Statut** : Design validé (sections 1–5 approuvées en brainstorm)
**Périmètre** : `anno-corpus-core`, `anno-corpus-store`, `anno-rag-mcp` (`corpus.rs`, `lib.rs`), `anno-rag-bin` (`main.rs`)
**Hors périmètre** : confiance/précision NER PII → spec B distincte.

---

## 1. Problème

L'application indexe des dossiers clients locaux. La recherche doit pouvoir cibler **un seul dossier** ou **tous les dossiers** selon la demande. Trois défauts du comportement actuel bloquent cet usage :

1. **CLI `ingest` n'enregistre aucun corpus.** [`main.rs:298`](../../../crates/anno-rag-bin/src/main.rs) appelle `pipeline.ingest_folder()` qui écrit des vecteurs dans `index.lance` mais ne crée pas d'entrée dans le registre `corpora`. Résultat : `corpus_count() == 0` alors que des documents existent réellement.
2. **`resolve_effective()` échoue sur `count == 0`.** [`corpus.rs:160`](../../../crates/anno-rag-mcp/src/corpus.rs) renvoie `CorpusGuardError::NoCorpus` → message « index a folder before using this tool », même quand l'index contient des documents (régression vs. l'ancien `legal_search` qui cherchait sans corpus).
3. **Référence corpus par UUID uniquement.** [`resolve_effective()`](../../../crates/anno-rag-mcp/src/corpus.rs#L135) parse `corpus_id` en `CorpusId` (UUID). Aucun moyen lisible de dire « dossier 2026-0042 ». Le `label_pseudo` du registre est pseudonymisé, donc inutilisable comme clé de recherche en clair.

## 2. Contrainte directrice : déontologie

Le critère dominant est la pratique d'un avocat :

- **Cloisonnement (secret professionnel, art. 66-5)** : une recherche sur le dossier MARTIN ne doit jamais ramener par accident des données du dossier DUPONT.
- **Contrôle de conflits** : l'avocat doit parfois chercher *à travers tous les dossiers* (« ai-je déjà eu cette partie adverse comme client ? »). Légitime, mais **explicite**.

→ **Cloisonné par défaut, cross-corpus disponible mais explicite.**

## 3. Modèle retenu : 1 dossier client = 1 corpus

Le grain est déjà natif dans `anno-corpus-store` : un corpus = une racine de dossier (`register_root(path, profiles)`), les documents sont scopés par `(corpus_id, relative_path)` via `scoped_doc_uuid()`. On s'appuie dessus sans nouvelle abstraction.

- **Sous-dossiers** (contrats / correspondance / pièces) = **filtre sur `relative_path`**, pas un nouveau type de corpus.
- **Pas** de corpus composite multi-dossiers (sur-ingénierie, YAGNI).

## 4. Conception détaillée

### 4.1 Unification de l'ingestion (résout pb. 1)

Le CLI `ingest` enregistre une racine corpus, exactement comme MCP `legal_ingest`/`index`.

- Avant l'appel `ingest_folder`, [`main.rs`](../../../crates/anno-rag-bin/src/main.rs) ouvre le `CorpusService` et appelle `register_index_root(folder, profile)` (idempotent — `register_root` est déjà stable sur `normalized_root UNIQUE`).
- **Profil par défaut du CLI : `all`** (knowledge + legal). Indexe largement par défaut, y compris l'enrichissement KG légal — donc les outils `legal_extract`/`legal_risk_review` fonctionnent même sur des docs ingérés en CLI (atténue U2 à la source). Surclassable par `--profile legal|general|all`.
- Conséquence : après cette unification, `corpus_count() == 0` signifie **réellement** « rien d'indexé ».

### 4.2 Alias métier (résout pb. 3)

Ajout d'une colonne `alias TEXT` (nullable, `UNIQUE` quand non-NULL — via index partiel `WHERE alias IS NOT NULL`) à la table `corpora` ([`migrations.rs:12`](../../../crates/anno-corpus-store/src/migrations.rs)).

- **Optionnel + auto-alias (décision « propre »)** : l'alias est une **référence de dossier non sensible** fournie à l'ingestion (`--alias 2026-0042` au CLI, champ `alias` côté MCP). **S'il n'est pas fourni, le système génère un auto-alias court et non sensible** `corpus-NN` (NN = rang d'enregistrement). Résultat : **aucun handle n'expose jamais un UUID brut**, et l'avocat n'est pas forcé de saisir un alias à chaque ingestion. Il peut toujours imposer sa référence métier via `--alias`.
- L'auto-alias est stable : `register_root` étant idempotent sur `normalized_root`, ré-ingérer le même dossier conserve le même corpus donc le même alias.
- L'alias fourni par l'utilisateur l'emporte toujours sur l'auto-alias.
- `resolve_effective()` accepte désormais un identifiant **soit** UUID **soit** alias (fourni ou auto) : on tente d'abord le parse UUID, sinon lookup `alias`.
- Migration : colonne ajoutée via une migration idempotente (`ALTER TABLE corpora ADD COLUMN alias`). Les corpus existants reçoivent un auto-alias `corpus-NN` au prochain démarrage (back-fill) et restent adressables par UUID.

### 4.3 Table de résolution (résout pb. 2)

`resolve_effective(corpus_ref: Option<&str>, allow_cross_corpus: bool)` :

| `corpus_ref` | `count` | `allow_cross` | Résultat |
|---|---|---|---|
| `Some` (UUID ou alias, existe) | — | — | `Single(id)` |
| `Some` (inconnu) | — | — | `Err(UnknownCorpus)` |
| `None` | 0 | `false` | `Err(NoCorpus)` — vrai zéro, message « indexez un dossier » |
| `None` | 0 | `true` | `CrossCorpus` (résultat vide, pas une erreur) |
| `None` | 1 | — | `Single` auto (pas d'ambiguïté) |
| `None` | N>1 | `false` | `Err(CorpusRequired)` — **réponse de désambiguïsation** (voir 4.4) |
| `None` | N>1 | `true` | `CrossCorpus` |

Précédence : un `corpus_ref` explicite l'emporte sur tout ; sinon `allow_cross_corpus: true` court-circuite le comptage (jamais d'erreur quand l'utilisateur a demandé explicitement « tous »). Seul `count == 0` **sans** `allow_cross` reste une erreur dure — et ce cas ne se produit plus quand des documents existent, grâce à 4.1.

### 4.4 Réponse de désambiguïsation (N>1 sans précision)

Au lieu d'une erreur opaque, `CorpusRequired` est rendue à l'utilisateur comme une **réponse structurée exploitable** par les outils MCP de recherche (`search`, `legal_search`) :

```json
{
  "status": "corpus_required",
  "message": "Plusieurs dossiers indexés. Précisez un dossier ou demandez une recherche transversale.",
  "available": [
    { "corpus_id": "…", "alias": "2026-0042", "label": "corpus_7", "health": "fresh" },
    { "corpus_id": "…", "alias": "2025-0117", "label": "corpus_3", "health": "stale" }
  ],
  "hint": "Relancez avec corpus_id/alias, ou allow_cross_corpus: true pour un contrôle de conflits."
}
```

Les labels restent pseudonymes ; l'alias métier (non sensible) est la clé que l'utilisateur reconnaît.

### 4.5 Filtre sous-dossier (`path_prefix`)

Paramètre optionnel `path_prefix` sur `search` / `legal_search`.

- Applique un filtre `relative_path` au **niveau du chunk dans le vector store** (les chunks portent `relative_path` ; cf. résultats de recherche pseudonymisés).
- **Note d'implémentation** : la table `corpus_documents` ne stocke qu'un `relative_path_hash` ([`migrations.rs:37`](../../../crates/anno-corpus-store/src/migrations.rs)) — non utilisable pour un préfixe. Le filtre opère donc sur les métadonnées de chunk côté `Store`, pas sur le registre corpus.
- Sémantique : `path_prefix: "contrats"` → ne retient que les chunks dont `relative_path` commence par `contrats/`.

### 4.6 Provenance en cross-corpus

En mode `CrossCorpus`, chaque hit expose son `corpus_id` (+ `alias` si présent) dans la réponse de recherche. C'est ce qui rend un contrôle de conflits exploitable : regrouper les hits par corpus montre « cette partie apparaît dans 2026-0042 et 2025-0117 ».

- Côté `Pipeline`, les chemins `legal_search_scoped*` connaissent déjà la liste `doc_ids` par corpus ; il faut propager l'appartenance corpus jusqu'au `SearchHit` (champ `corpus_id: Option<String>`).
- En mode `Single`, le champ est renseigné mais redondant (un seul corpus).

### 4.7 Handles document lisibles (intègre revue UX U1)

**Problème UX** : tous les outils aval prennent des UUID — [`legal_extract_contract(doc_id)`](../../../crates/anno-rag-mcp/src/lib.rs#L3091), `legal_risk_review(scope_id)`, `legal_timeline(dossier_id)`, `corpus_get/corpus_health(corpus_id)`. Le workflow réel oblige l'avocat à copier-coller un UUID (`a9ea6215-c656-…`) d'une recherche vers l'outil suivant. C'est l'irritant n°1.

**Solution : un handle lisible, dérivé de l'alias corpus + chemin relatif.**

- **Forme** : `<alias>/<relative_path>` — ex. `2026-0042/contrats/contrat-prestation.txt`. L'alias étant non sensible (réf. de dossier) et le chemin étant local au cabinet, le handle est privacy-safe et reconnaissable.
- **Résolution bidirectionnelle** — une fonction `resolve_doc_ref(&str) -> DocumentInstanceId` :
  1. si l'entrée parse comme UUID → l'utilise directement (rétrocompatibilité totale) ;
  2. sinon, split au **premier `/`** : segment de tête = alias corpus, reste = `relative_path` ; lookup corpus par alias (§4.2), puis recompose le doc id déterministe via `scoped_doc_uuid(corpus_id, relative_path, content_id)` ou recherche directe sur `corpus_documents`.
- **Exposition** : chaque `SearchHit` et chaque ligne de `sources()`/`corpus_list()` renvoie **les deux** champs — `doc_id` (UUID, stable) **et** `handle` (lisible). L'agent et l'humain copient le lisible ; les intégrations existantes continuent d'utiliser l'UUID.
- **Couverture** : les paramètres `doc_id` / `scope_id` / `dossier_id` des outils légaux acceptent désormais « UUID **ou** handle ». Aucun changement de signature côté schéma (toujours une `String`), seul le résolveur en amont change.
- **Dossiers** : un dossier (case file) réutilise la même logique — son handle est l'alias du corpus le contenant + un suffixe de dossier si plusieurs dossiers coexistent dans un corpus.

**Limite assumée** : si un `relative_path` contient un nom de client dans le nom de fichier (ex. `MARTIN_contrat.pdf`), ce nom apparaît dans le handle. C'est un fichier local au cabinet, pas une fuite réseau ; documenté comme responsabilité utilisateur (nommer ses fichiers par référence de dossier s'il veut un handle 100 % anonyme). Le handle n'est jamais persisté en clair dans un index — il est calculé à la volée à partir de l'alias + chemin.

## 5. Frontières des composants

| Composant | Responsabilité | Change |
|---|---|---|
| `anno-corpus-store` | Registre + schéma | Migration `alias` ; lookup par alias ; `list_corpora` expose `alias` |
| `anno-corpus-core` | `EffectiveCorpus`, `CorpusGuardError` | Inchangé (réutilise `Single`/`CrossCorpus`/`CorpusRequired`) |
| `anno-rag-mcp/corpus.rs` | `resolve_effective` + `resolve_doc_ref` | Accepte UUID **ou** alias ; conserve la table 4.3 ; résout les handles document (§4.7) |
| `anno-rag-mcp/lib.rs` | Handlers `search`/`legal_search` + outils légaux | Désambiguïsation ; `path_prefix` ; provenance `corpus_id` + `handle` dans les hits ; `doc_id`/`scope_id`/`dossier_id` acceptent UUID ou handle |
| `anno-rag-bin/main.rs` | CLI `ingest` | Enregistre un corpus (`--profile`, `--alias`) avant `ingest_folder` |
| `anno-rag` `Store`/`Pipeline` | Recherche scoping | Filtre `path_prefix` sur chunks ; propage `corpus_id` + `handle` au `SearchHit` |

## 6. Gestion d'erreurs

- `NoCorpus` (count 0, vrai zéro) → message actionnable « indexez un dossier d'abord ».
- `UnknownCorpus(ref)` → distingue UUID malformé vs. alias introuvable dans le message.
- `CorpusRequired` → réponse structurée 4.4, **jamais** une erreur opaque.
- Migration `alias` : idempotente, n'altère pas les corpus existants (alias NULL).

## 7. Tests

- **Unitaires `anno-corpus-store`** : migration alias idempotente ; lookup par alias ; rejet alias dupliqué ; auto-alias `corpus-NN` généré quand absent ; alias fourni l'emporte sur l'auto ; back-fill des corpus pré-migration (auto-alias attribué, toujours adressable par UUID) ; stabilité de l'auto-alias à la ré-ingestion du même dossier.
- **Unitaires `corpus.rs`** : table 4.3 exhaustive (chaque ligne = un test), y compris alias inconnu et UUID malformé.
- **Intégration MCP** : `search` sans corpus avec N>1 → réponse `corpus_required` listant les alias ; `allow_cross_corpus:true` → hits avec `corpus_id` distincts ; `path_prefix` restreint bien aux sous-dossiers.
- **Handles document (§4.7)** : `resolve_doc_ref` accepte un UUID (passthrough) et un handle `alias/relative_path` (résolution vers le même UUID) ; `legal_extract_contract` donne le même résultat appelé par UUID ou par handle ; un handle dont l'alias est inconnu → erreur lisible ; chaque `SearchHit` expose `doc_id` **et** `handle`.
- **CLI** : `ingest` d'un dossier → `corpus_count()` passe à 1 ; ré-ingestion idempotente (pas de doublon).
- **Non-régression privacy** : aucun nom client en clair dans le registre ; les labels restent pseudonymes ; l'alias fourni est traité comme non sensible (responsabilité utilisateur, documentée).

## 8. Décisions hors périmètre (YAGNI)

- Pas de corpus composite multi-dossiers.
- Pas de type corpus hiérarchique pour les sous-dossiers (filtre `path_prefix` suffit).
- Pas d'état « dernier corpus utilisé » implicite (rejeté : ambigu pour la déontologie).
- Mémoire transversale et confiance NER → spec B.
