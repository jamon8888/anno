# Spec B — Confiance & précision de la détection NER PII

**Date** : 2026-06-24
**Statut** : Design validé (composants B1–B5 approuvés en brainstorm)
**Périmètre** : `anno-rag` (`detect.rs`, `config.rs`, harness d'éval), fixtures de test
**Hors périmètre** : sémantique de recherche corpus → Spec A.

---

## 1. Problème

La détection PII alimente la pseudonymisation par le vault. Trois faiblesses observées en test :

1. **Sous-confiance sur texte court** : des noms/organisations passent sous le seuil sur des phrases isolées (queries, textes de test), alors qu'ils sont correctement détectés dans des chunks de documents au contexte riche.
2. **Deux chemins de détection incohérents** : `detect()` (query/MCP) utilise `gdpr_described()` avec plancher 0.25 puis filtre par seuil ([detect.rs:672](../../../crates/anno-rag/src/detect.rs)) ; `detect_for_ingest()` utilise `extract_with_label_thresholds()` (seuils par label dans la passe modèle, [detect.rs:743](../../../crates/anno-rag/src/detect.rs)). Deux stratégies d'appel pour le même modèle.
3. **Aucune mesure** : [`eval.rs`](../../../crates/anno-rag/src/eval.rs) évalue la qualité de **recherche** (recall@k, nDCG), pas la précision/rappel de la **détection PII**. Tout réglage de seuil est aujourd'hui à l'aveugle.

Diagnostic confirmé : l'architecture utilise déjà **deux modèles séparés** — `pii_ner` = `gliner2-privacy-filter-PII-multi` (PII, appelé par `detect`/ingest) et `ner` = `gliner2-multi-v1` (entités générales/légales). Le modèle PII spécialisé est donc bien en place ; le problème est dans **comment on l'interroge** (schéma de 24 labels en une passe) et l'**absence de mesure**, pas dans le choix du modèle.

## 2. Contrainte directrice : rappel d'abord, mené par la mesure

Asymétrie des coûts pour un cabinet :

| Échec | Conséquence | Gravité |
|-------|-------------|---------|
| **Rappel** (PII ratée) | Nom/IBAN/donnée santé en clair dans l'embedding, un export ou un résultat cross-corpus → secret pro (art. 66-5) + RGPD | 🔴 |
| **Précision** (sur-masquage) | `PERSON_5` à la place d'un mot non-PII ; recherche dégradée | 🟡 sûr |

→ **Priorité rappel, différenciée par catégorie.** Mais « rappel d'abord » ≠ « baisser tous les seuils » : le levier principal est la **largeur du schéma**, pas le seuil. Les seuils ne baissent que là où l'éval (B1) prouve un déficit de rappel.

## 3. Conception détaillée

### B1. Harness d'éval PII (fondation — à implémenter en premier)

Sans vérité terrain, aucun réglage n'est rationnel. On construit l'instrument avant de régler.

- **Fixtures synthétiques** : extraits de texte juridique FR avec spans PII annotés `(catégorie, byte_start, byte_end)`. **Zéro vraie PII** — noms/IBAN/etc. fictifs. Deux sous-ensembles :
  - `short/` : phrases isolées (simulent les queries) ;
  - `long/` : paragraphes au contexte riche (simulent les chunks).
- **Format** : un fichier par cas, ex. `tests/fixtures/pii_eval/short/person_01.json` :
  ```json
  { "text": "Jean-Pierre Moreau, avocat chez Cabinet Legrand.",
    "spans": [ { "category": "person", "start": 0, "end": 18 } ] }
  ```
- **Métriques par catégorie** : précision / rappel / F1, par appariement span (chevauchement ≥ 1 caractère sur la bonne catégorie). Macro-moyenne + détail par catégorie.
- **Gate de non-régression** : test `pii_eval_meets_floors` qui exécute le harness et assert des **planchers par catégorie** (valeurs initiales fixées au premier run mesuré, puis cliquetées vers le haut) :
  - Art. 9 (santé, biométrie, génétique, orientation, politique, religion, syndical) : **rappel ≥ 0.90** ;
  - identité de base (person, address, location) : rappel ≥ baseline mesurée ;
  - identifiants structurés (NIR, IBAN, email, phone) : rappel ≥ 0.98 (couche regex).
- **Emplacement** : fixtures sous `crates/anno-rag/tests/fixtures/pii_eval/`, runner dans un module `detect_eval` (à côté de `detect.rs`) réutilisable par les tests.

### B2. Réconcilier query et ingest sur une seule stratégie

`detect()` (query) et `detect_for_ingest()` doivent utiliser la **même** stratégie d'appel modèle : `extract_with_label_thresholds()` avec des seuils par label. Bénéfices :

- Comportement query/ingest identique (plus de divergence plancher-puis-filtre vs seuils-en-passe) ;
- Une seule source de configuration de labels (B3) ;
- `detect_inner()` est réécrit pour déléguer au même cœur que `detect_for_ingest()`, en ne gardant que la couche PII (le legal split reste propre à l'ingest).

### B3. Fix-schéma : passes focalisées (levier rappel principal)

Remplacer la passe unique de 24 labels par des **groupes de labels focalisés**, exécutés en passes séparées puis fusionnés. Configuration structurée :

- **Groupe `identity`** : person, address, date_of_birth, age, nationality, profession, organization, location.
- **Groupe `art9`** : racial_ethnic_origin, political_opinion, religious_belief, trade_union_membership, health_data, genetic_data, biometric_data, sexual_orientation, criminal_record.
- **Groupe `identifiers_model`** : national_id, tax_id, bank_account, ip_address, username, device_id — **secondaire**, car la couche regex (`detect_patterns`) couvre déjà NIR/IBAN/email/phone à confiance 1.0.

Règles :
- Chaque passe envoie un **schéma étroit** (≤ 8 labels) → confiance par-span plus haute → rappel ↑ **sans** baisser les seuils.
- Fusion : union des spans, puis `dedup_overlaps` existant (priorité Pattern > NER, span le plus long).
- **Validation obligatoire par B1** : on mesure rappel/précision/latence *avant/après* le narrowing. Si 2 groupes suffisent (identity + art9), on s'arrête à 2 (YAGNI sur le 3ᵉ). Le nombre de passes est une **conséquence mesurée**, pas un choix a priori.
- **Budget latence** : le coût = N passes modèle par texte. Mesuré sur les fixtures `long/`. Si le surcoût ingest est inacceptable, repli documenté : groupe unique pour l'ingest (contexte riche, déjà bon) + passes focalisées réservées au chemin query (texte court, là où le narrowing aide le plus).

### B4. Double périmètre de masquage

Deux profils sélectionnables, portés par un enum threadé dans la config détecteur :

- **`RgpdStrict`** (défaut) : périmètre RGPD actuel — `organization` n'est retenu que lié à une personne physique (comportement actuel).
- **`CabinetConfidential`** : élargit le secret pro — masque **toutes** les organisations et parties (seuil `organization` abaissé et description élargie « toute organisation, cabinet, société ou partie nommée »).

Mécanisme :
- Enum `MaskingScope { RgpdStrict, CabinetConfidential }` dans `config.rs`, défaut configurable + override par appel MCP (`detect`, recherche, ingestion).
- Le profil sélectionne le set de labels/descriptions/seuils du groupe `identity` (les autres groupes sont identiques entre profils).
- Le vault pseudonymise selon le profil actif ; un même corpus peut être ingéré sous un profil donné (cohérence intra-corpus recommandée, documentée).

### B5. Chemin query — réduction du risque de fuite

Pour une query courte contenant une entité **jamais indexée**, le nom peut partir en clair. Parades :

- **Vault-lookup-first** : les tokens déjà connus du vault sont pseudonymisés indépendamment du score NER (déjà partiellement en place via `pseudonymize`). On garantit que ce chemin couvre la query.
- Le narrowing B3 remonte la confiance sur texte court (mesuré via fixtures `short/`).
- **Aucune sur-ingénierie** : pas de second modèle ni de calibration probabiliste ; le gain vient du schéma + du lookup.

## 4. Ordre d'implémentation (imposé par la méthode)

1. **B1** d'abord — sans mesure, le reste est aveugle. Capturer les baselines au premier run.
2. **B2** — unifier les chemins (refactor sans changement de comportement, couvert par B1).
3. **B3** — narrowing, validé/réglé par B1 (avant/après).
4. **B4** — double périmètre, chaque profil mesuré par B1.
5. Seuils ajustés **uniquement** là où B1 montre un déficit, après B3.

## 5. Gestion d'erreurs & invariants

- **Cleartext-free audit** préservé : `emit_detect_audit` ne logue que compteurs/durées/model_id, jamais le texte (AI Act Art. 12/72). Les passes multiples émettent un seul événement agrégé.
- **Offsets char→byte** : la fusion multi-passes réutilise `anno_entities_to_detected` (lookup char→byte par texte) — pas de panic sur accents/€.
- **Déterminisme** : à schéma + seuils fixes, la sortie est stable (tests reproductibles).

## 6. Tests

- **B1 self-test** : `recall_at`/`precision_at` corrects sur cas jouets ; appariement par chevauchement ; macro-moyenne.
- **Gate** : `pii_eval_meets_floors` rouge si une catégorie passe sous son plancher.
- **B2** : `detect()` et `detect_for_ingest()` renvoient le **même** ensemble PII sur un texte donné (parité query/ingest).
- **B3** : rappel mesuré ≥ baseline mono-passe sur fixtures `short/` et `long/` ; latence par passe enregistrée.
- **B4** : `CabinetConfidential` masque une organisation que `RgpdStrict` laisse passer (ex. « Cabinet Legrand ») ; `RgpdStrict` conserve le comportement actuel (non-régression).
- **B5** : une query courte avec un token déjà au vault est pseudonymisée même si le NER ne le redétecte pas.

## 7. Décisions hors périmètre (YAGNI)

- Pas de fine-tuning / ré-entraînement de modèle.
- Pas de nouveau modèle PII (`gliner2-privacy-filter` actuel conservé).
- Pas de calibration probabiliste avancée (Platt/temperature) — seuils par catégorie + groupes de passes suffisent jusqu'à preuve du contraire par B1.
- Pas de 3ᵉ groupe de passe tant que B1 ne montre pas qu'il apporte du rappel.
