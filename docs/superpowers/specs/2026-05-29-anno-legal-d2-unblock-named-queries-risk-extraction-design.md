# Design — Débloquer les outils D2 (`extract_contract`, `risk_review`, `timeline`)

**Date:** 2026-05-29
**Statut:** Design approuvé, prêt pour planification
**Crate principal:** `anno-rag` (`crates/anno-rag/src/legal/`)

## Contexte et vrai diagnostic

Une batterie de tests du MCP `anno-rag` a conclu que trois outils D2 —
`legal_extract_contract`, `legal_risk_review`, `legal_timeline` — échouaient
parce que « le backend SQLite ne supporte pas les traversées Cypher » et
qu'il faudrait migrer vers KùzuDB ou Neo4j.

**Ce diagnostic est faux.** La lecture du code montre :

1. `SqliteLegalGraphStore::cypher()`
   ([`kg.rs`](../../../crates/anno-rag/src/legal/kg.rs)) n'est **pas** un moteur
   Cypher : c'est un routeur de requêtes nommées. Il reconnaît cinq clés
   (`party_dossier`, `obligations_owed_by`, `citation_chain`,
   `procedural_timeline`, `appeal_chain`), chacune implémentée en SQL pur — y
   compris une traversée récursive (`appeal_chain`) via `WITH RECURSIVE`. Le
   tool D1 `legal_graph_query` fonctionne précisément grâce à ce routeur.
   SQLite réalise donc déjà les traversées de graphe nécessaires.

2. Les trois outils D2 passent des **chaînes Cypher brutes** à `cypher()`
   ([`extract.rs`](../../../crates/anno-rag/src/legal/extract.rs)). Aucune n'est
   l'une des cinq clés nommées, donc elles tombent dans le bras `_ =>` qui
   renvoie « raw Cypher execution is not supported ». C'est un **trou
   d'implémentation**, pas une limite de SQLite.

3. Les requêtes D2 ciblent un type d'edge `SOURCES` (Chunk→Obligation,
   Chunk→Risk). Or l'enricher d'ingestion
   ([`enricher.rs`](../../../crates/anno-rag/src/legal/enricher.rs)) n'écrit
   jamais `SOURCES` : il écrit `MENTIONS`, `BOUND_BY`, `PARTY_TO`, `REFERENCES`,
   `HEARS`. Les conventions d'edges divergent entre écriture et lecture. Même
   avec un vrai moteur Cypher, ces requêtes renverraient du vide.

4. L'enricher ne produit **aucun fait de risque**. `TypedFact`
   ([`rules.rs`](../../../crates/anno-rag/src/legal/rules.rs)) n'a que
   `PartyRole`, `Obligation`, `Reference`, `CourtRouting`, `Event`. Aucun nœud
   `Risk` n'est jamais créé. `risk_review` n'a donc pas de source de données —
   aucun changement de backend ne le débloquerait.

### Conséquence

- `timeline` et `extract_contract` se débloquent par du pur câblage SQL +
  alignement d'edges, **sans nouvelle dépendance**.
- `risk_review` exige d'abord une **étape d'extraction de risques** en amont.
- KùzuDB / Neo4j / un traducteur Cypher→SQL n'apporteraient rien ici et
  imposeraient une dépendance native lourde sur un build Windows/CI déjà
  contraint. Rejetés (YAGNI).

## Objectif

Faire fonctionner les trois outils D2 en restant **embedded, SQLite, model-free
par défaut**, sans changer le backend graphe ni le contrat du trait
`LegalKnowledgeGraph`.

## Principe directeur

On étend le **vocabulaire de requêtes nommées typées** du trait
`LegalKnowledgeGraph` au lieu d'interpréter du Cypher. Les call-sites D2 cessent
d'envoyer du Cypher brut et appellent des méthodes typées. Le bras
`cypher(_) => "not supported"` reste comme **garde-fou** : tout futur appel
Cypher par erreur échoue explicitement plutôt que de faire croire à un moteur
Cypher.

---

## Lot 1 — Câblage SQL + alignement des edges

### 1a. `timeline` (effort quasi nul)

`legal::extract::timeline()`
([`extract.rs:254`](../../../crates/anno-rag/src/legal/extract.rs)) envoie
aujourd'hui un Cypher brut sur l'edge `MENTIONS` Chunk→Event. La méthode nommée
`procedural_timeline(dossier_id)` existe déjà, utilise le bon edge et trie
chronologiquement.

**Changement :** remplacer l'appel `kg.cypher("MATCH ...")` par
`kg.procedural_timeline(dossier_id)` et mapper ses lignes vers
`ProceduralTimeline`.

### 1b. `extract_contract` (effort faible)

Ajouter deux requêtes nommées au backend SQLite (`SqliteLegalGraphStore`),
exposées comme méthodes par défaut du trait `LegalKnowledgeGraph` (avec
implémentation SQL dans `SqliteLegalGraphStore`, et override no-op/vide dans
`InMemoryKG`) :

- **`contract_parties(doc_id)`** : `Document <-[PARTY_TO]- Party`.
  Colonnes : `value` (canonical_name), `role`.
- **`contract_obligations(doc_id)`** :
  `Document -[HAS_CHUNK]-> Chunk -[MENTIONS]-> Obligation`.
  Colonnes : `kind`, `text` (text_pseudo), `cid` (chunk_id).

**Alignement d'edge :** la lecture utilise `MENTIONS` (ce que l'enricher écrit),
pas `SOURCES`. Toute référence à `SOURCES` est supprimée du code de lecture.

**Réécriture :** `extract_contract()` consomme `kg.contract_parties(doc_id)` et
`kg.contract_obligations(doc_id)` au lieu des deux `kg.cypher("MATCH ...")`.

### Conventions d'edges (référence)

| Relation | Edge écrit par l'enricher | Source |
|----------|---------------------------|--------|
| Party → Document | `PARTY_TO` | `enricher.rs` |
| Chunk → Obligation | `MENTIONS` | `enricher.rs` |
| Party → Obligation | `BOUND_BY` | `enricher.rs` |
| Document → Article | `REFERENCES` | `enricher.rs` |
| Document → Court | `HEARS` | `enricher.rs` |
| Chunk → Event | `MENTIONS` | `enricher.rs` |
| Document → Chunk | `HAS_CHUNK` | pipeline |
| **Chunk → Risk** | **`MENTIONS`** (nouveau, Lot 2) | ce design |

`SOURCES` n'existe pas et ne sera pas introduit.

---

## Lot 2 — Extraction de risques (pour `risk_review`)

Détection **hybride** : GLiNER (Layer-1) pour le rappel sémantique + règles
déterministes (Layer-2) pour la classification structurée. C'est le seul lot qui
introduit une vraie logique nouvelle.

### État réel de la couche GLiNER (vérifié dans le code)

La moitié GLiNER de l'hybride est **déjà câblée et active à l'ingestion** :

- `default_legal_labels()`
  ([`types.rs:281`](../../../crates/anno-rag/src/legal/types.rs)) contient déjà
  les labels `risk_indicator` ("Legal risk indicator"), `sanction`,
  `clause_type`, `obligation`.
- `default_thresholds()`
  ([`types.rs:367`](../../../crates/anno-rag/src/legal/types.rs)) leur donne des
  seuils : `risk_indicator = 0.55`, `sanction = 0.65`, `clause_type = 0.60`.
- `LegalEntity` porte `label`, `text`, `byte_start`, `byte_end`, `confidence`
  ([`types.rs:20`](../../../crates/anno-rag/src/legal/types.rs)).
- L'enricher produit donc déjà des entités `risk_indicator` à chaque ingestion,
  **mais les jette** : `apply_all` ignore `entities`
  ([`rules.rs:73`](../../../crates/anno-rag/src/legal/rules.rs)) et le champ
  `LegalChunkEnrichment::risk_flags` est codé en dur à `Vec::new()`
  ([`enricher.rs:325`](../../../crates/anno-rag/src/legal/enricher.rs)).

**Conséquence :** il n'y a rien à « activer » côté modèle. Le travail Lot 2 est
de **consommer** des entités déjà produites. Le patron existe déjà :
[`enricher.rs:291`](../../../crates/anno-rag/src/legal/enricher.rs) filtre
`entity.label == "clause_type"` pour peupler `clause_types`. On reproduit ce
patron pour `risk_indicator`.

### 2a. Nouveau fait typé

Ajouter à `TypedFact` ([`rules.rs`](../../../crates/anno-rag/src/legal/rules.rs)) :

```rust
Risk {
    /// Catégorie de risque, ex. "clause_penale", "non_concurrence".
    category: String,
    /// Sévérité : "high" | "medium" | "low".
    severity: String,
    /// Texte pseudonymisé du segment à risque.
    text_pseudo: String,
}
```

### 2b. Détection hybride dans `apply_all`

`apply_all` reçoit déjà `entities: &[LegalEntity]` mais les ignore
(`let _ = (chunk_id, entities)` à
[`rules.rs:73`](../../../crates/anno-rag/src/legal/rules.rs)). On change sa
signature/usage pour exploiter ce point d'ancrage :

1. **Layer-1 (GLiNER, rappel — déjà produit)** : les entités `risk_indicator`
   (et optionnellement `sanction`) deviennent des candidats `Risk`. Leur span
   (`byte_start`/`byte_end`) et `confidence` sont déjà disponibles. Aucun
   changement de modèle ni de label requis.
2. **Layer-2 (règles déterministes, structure)** : un jeu de fonctions
   `rule_risk_*` (regex, model-free) qui (a) détecte des patterns de risque
   connus avec `category` + `severity` codés et (b) classe les candidats GLiNER
   qui chevauchent un pattern connu.
3. **Fusion / déduplication** : dédupe par `(category, span chevauchant)` pour
   éviter qu'un même risque détecté par les deux couches produise deux faits.
   En cas de conflit de sévérité, garder la plus élevée.

Un candidat GLiNER `risk_indicator` qui ne chevauche aucune règle reçoit une
`category` générique (`"clause_a_risque"`) et une `severity` dérivée de la
confiance (`>= 0.75 → medium`, sinon `low`). Inversement, une règle qui matche
sans entité GLiNER produit quand même son `Risk` (precision sans rappel modèle).

**Effet de bord utile :** au passage, peupler le champ existant
`LegalChunkEnrichment::risk_flags`
([`enricher.rs:325`](../../../crates/anno-rag/src/legal/enricher.rs)) à partir
des mêmes candidats, sur le modèle de `clause_types`
([`enricher.rs:291`](../../../crates/anno-rag/src/legal/enricher.rs)).

### Catalogue complet de règles (droit français)

Les règles sont **complémentaires** à `mandatory.rs` (qui vérifie la
*présence* de clauses obligatoires). Ici on détecte des clauses **présentes
mais dangereuses**, ou des conditions qui rendent une clause risquée.

Chaque règle est une fonction `rule_risk_<category>` dans `rules.rs`, suit le
patron `once_cell::Lazy<Regex>` existant, et produit un `TypedFact::Risk`.
Chaque règle a son test unitaire.

#### A. Droit commun des contrats (Code civil post-réforme 2016)

| # | `category` | Pattern regex (FR, `(?i)`) | `severity` | Fondement | Pourquoi c'est un risque |
|---|-----------|--------------------------|-----------|-----------|--------------------------|
| A1 | `clause_penale` | `clause p[ée]nale\|p[ée]nalit[ée] forfaitaire\|indemnit[ée] forfaitaire de\b.*\brésiliation` | medium | Art. 1231-5 C.civ | Réductible par le juge ; montant imprévisible à l'exécution |
| A2 | `responsabilite_illimitee` | `responsabilit[ée]\s+(?:illimit[ée]e\|sans\s+(?:limite\|plafond))\|exclusion\s+(?:totale\s+)?de\s+(?:toute\s+)?responsabilit[ée]\|ne\s+pourra\s+[êe]tre\s+tenu\s+(?:d'aucune\s+)?responsabilit[ée]` | high | Art. 1231-3 C.civ | Exclusion nulle pour dol/faute lourde ; exposition illimitée côté créancier |
| A3 | `desequilibre_significatif` | `d[ée]s[ée]quilibre\s+significatif\|avantage\s+(?:excessif\|manifestement\s+disproportionn[ée])` | high | Art. 1171 C.civ | Clause réputée non écrite entre adhérents |
| A4 | `tacite_reconduction` | `tacite(?:ment)?\s+reconduit\|renouvellement\s+tacite\|reconduction\s+tacite\|reconduit\s+(?:automatiquement\|de\s+plein\s+droit)` | medium | Art. 1215 C.civ | Engagement perpétuel de fait si préavis de sortie absent ou trop court |
| A5 | `clause_resolutoire` | `r[ée]solu(?:tion)?\s+de\s+plein\s+droit\|clause\s+r[ée]solutoire\|r[ée]siliation\s+(?:automatique\|imm[ée]diate\|de\s+plein\s+droit)\s+sans\s+(?:pr[ée]avis\|mise\s+en\s+demeure)` | high | Art. 1225 C.civ | Résiliation brutale sans mise en demeure préalable (exigée par l'art. 1225) |
| A6 | `renonciation_recours` | `renonce\s+(?:irr[ée]vocablement\s+)?[àa]\s+(?:tout\s+)?recours\|renonciation\s+[àa]\s+(?:tout\s+)?recours` | high | Art. 6 C.civ (OP) | Potentiellement contraire à l'ordre public (droit fondamental d'accès au juge) |
| A7 | `indexation_interdite` | `index[ée](?:e)?\s+sur\s+(?:le\s+)?(?:smic\|smig\|salaire\s+minimum\|niveau\s+g[ée]n[ée]ral\s+des\s+(?:prix\|salaires))` | high | Art. L112-2 CMF | Indexation sur le SMIC/niveau général des prix formellement interdite |
| A8 | `clause_leonine` | `clause\s+l[ée]onine\|exon[ée]r[ée](?:e)?\s+de\s+toute\s+(?:perte\|contribution\s+aux\s+pertes)\|attribut(?:ion)?\s+(?:de\s+)?(?:la\s+)?totalit[ée]\s+des\s+(?:b[ée]n[ée]fices\|profits)` | high | Art. 1844-1 C.civ | Nulle dans les sociétés ; indicateur de déséquilibre dans les contrats |

#### B. Droit commercial (Code de commerce)

| # | `category` | Pattern regex | `severity` | Fondement | Pourquoi c'est un risque |
|---|-----------|--------------|-----------|-----------|--------------------------|
| B1 | `delai_paiement_excessif` | `d[ée]lai\s+de\s+(?:paiement\|r[èe]glement)\s+(?:de\s+)?\d+\s*jours` (post-match : vérifier >60) | high | Art. L441-10 C.com | Plafond légal 60j net / 45j fin de mois ; sanction administrative 2M€ |
| B2 | `rupture_brutale` | `r[ée]sili(?:ation\|er)\s+(?:sans\s+(?:pr[ée]avis\|motif)\|[àa]\s+(?:tout\s+)?moment\s+sans\s+(?:pr[ée]avis\|indemnit[ée]))\|rupture\s+(?:brutale\|sans\s+pr[ée]avis)` | high | Art. L442-1 II C.com | Engage la responsabilité pour rupture brutale de relation commerciale établie |
| B3 | `exclusivite_sans_duree` | `exclusivit[ée]\s+(?:(?:sans\s+(?:limite\|dur[ée]e))\|(?:pour\s+une\s+dur[ée]e\s+ind[ée]termin[ée]e))\|exclusivit[ée].*\bperp[ée]tu` | high | Jurisprudence C.com | Engagement perpétuel réductible ; risque d'annulation |
| B4 | `non_sollicitation` | `non[- ]sollicitation\|interdiction\s+de\s+sollicit(?:er\|ation)\s+(?:du\s+)?personnel\|d[ée]bauchage` | medium | Jurisprudence | Limite la liberté d'embauche ; potentiellement abusive sans durée/contrepartie |

#### C. Droit du travail (Code du travail)

| # | `category` | Pattern regex | `severity` | Fondement | Pourquoi c'est un risque |
|---|-----------|--------------|-----------|-----------|--------------------------|
| C1 | `non_concurrence_sans_contrepartie` | `non[- ]concurrence(?!.*contrepartie\s+financi[èe]re)` (look-ahead context ~200 chars) | high | Cass. soc. 10/07/2002 | Nulle sans contrepartie financière depuis 2002 (source de contentieux n°1) |
| C2 | `periode_essai_excessive` | `p[ée]riode\s+d'essai\s+(?:de\s+)?\d+\s*mois` (post-match : >4 mois cadre, >3 non-cadre) | high | Art. L1221-19 C.trav | Plafonds légaux impératifs ; clause nulle si dépassée |
| C3 | `mobilite_illimitee` | `clause\s+de\s+mobilit[ée](?!.*(?:p[ée]rim[èe]tre\|zone\s+g[ée]ographique\s+d[ée]finie\|rayon\s+de))` | medium | Cass. soc. 14/10/2008 | Nulle si zone géographique non définie précisément |
| C4 | `dedit_formation` | `d[ée]dit[- ]formation\|remboursement\s+(?:des\s+)?frais\s+de\s+formation\s+en\s+cas\s+de\s+(?:d[ée]mission\|d[ée]part)` | medium | Jurisprudence constante | Disproportionné si montant > coût réel ou durée > 3-5 ans |
| C5 | `forfait_jours_sans_suivi` | `forfait\s+(?:en\s+)?jours(?!.*(?:suivi\s+de\s+la\s+charge\|entretien\s+annuel\|droit\s+[àa]\s+la\s+d[ée]connexion))` | medium | Art. L3121-64 C.trav | Nul si l'accord collectif ne prévoit pas le suivi de charge (Cass. soc. 29/06/2011) |

#### D. Baux (commercial et habitation)

| # | `category` | Pattern regex | `severity` | Fondement | Pourquoi c'est un risque |
|---|-----------|--------------|-----------|-----------|--------------------------|
| D1 | `solidarite_cessionnaire` | `solidarit[ée]\s+(?:du\s+)?c[ée]dant\|solidaire(?:ment)?\s+(?:responsable\s+)?(?:du\|des)\s+(?:obligations\|loyers)\s+(?:du\s+)?cessionnaire` | medium | Art. L145-16 C.com | Risque financier perpétuel pour le cédant après cession du bail |
| D2 | `charges_locatives_illimitees` | `charges?\s+(?:r[ée]cup[ée]rables?\s+)?(?:sans\s+(?:limite\|plafond)\|int[ée]gralit[ée]\s+des\s+charges?\s+(?:de\s+)?copropri[ée]t[ée])` | medium | Art. L145-40-2 C.com | Inventaire obligatoire des charges ; risque de nullité et de remboursement |
| D3 | `bail_derogatoire_excessif` | `bail\s+d[ée]rogatoire\s+(?:de\s+)?\d+\s*(?:mois\|ans)` (post-match : >3 ans) | high | Art. L145-5 C.com | Plafond 3 ans ; au-delà requalification automatique en bail commercial 3-6-9 |

#### E. Protection des données (RGPD)

| # | `category` | Pattern regex | `severity` | Fondement | Pourquoi c'est un risque |
|---|-----------|--------------|-----------|-----------|--------------------------|
| E1 | `transfert_hors_ue` | `transfert\s+(?:de\s+donn[ée]es?\s+)?(?:hors\s+(?:de\s+)?(?:l')?(?:UE\|Union\s+europ[ée]enne\|EEE)\|vers\s+(?:un\s+)?pays\s+tiers)(?!.*(?:clauses?\s+contractuelles?\s+types?\|CCT\|d[ée]cision\s+d'ad[ée]quation\|BCR))` | high | Art. 44-49 RGPD | Sans garantie appropriée : amende jusqu'à 4% CA mondial |
| E2 | `sous_traitance_sans_art28` | `sous[- ]trait(?:ant\|ance)\s+(?:de\s+)?(?:donn[ée]es?\|traitement)(?!.*(?:art(?:icle)?\s*\.?\s*28\|clauses?\s+(?:de\s+)?sous[- ]traitance\|mesures?\s+(?:techniques?\s+et\s+)?organisationnelles?))` | high | Art. 28 RGPD | Contrat obligatoire sous peine de co-responsabilité |
| E3 | `conservation_illimitee` | `conserv[ée](?:e)?s?\s+(?:sans\s+(?:limite\|dur[ée]e\s+d[ée]termin[ée]e)\|ind[ée]finiment\|de\s+mani[èe]re\s+illimit[ée]e)` | high | Art. 5(1)(e) RGPD | Principe de limitation de conservation ; amende CNIL |

#### F. Propriété intellectuelle

| # | `category` | Pattern regex | `severity` | Fondement | Pourquoi c'est un risque |
|---|-----------|--------------|-----------|-----------|--------------------------|
| F1 | `cession_pi_totale` | `c[èe]de?\s+(?:l'(?:ensemble\|int[ée]gralit[ée]\|totalit[ée])\s+de\s+ses?\s+)?droits?\s+(?:de\s+)?propri[ée]t[ée]\s+intellectuelle(?!.*(?:contrepartie\|r[ée]mun[ée]ration\|prix\s+de\s+cession))` | high | Art. L131-3 CPI | Cession sans contrepartie = nullité ; domaine d'exploitation non délimité = nullité |
| F2 | `cession_oeuvres_futures` | `c[èe]de?\s+(?:par\s+avance\s+)?(?:les?\s+)?(?:droits?\s+sur\s+)?(?:l'ensemble\s+(?:de\s+)?ses?\s+)?[œo]euvres?\s+futures?\|cession\s+(?:globale\s+)?(?:de\s+)?(?:l'ensemble\s+(?:de\s+)?ses?\s+)?[œo]euvres?\s+(?:[àa]\s+venir\|futures?)` | high | Art. L131-1 CPI | Cession globale des œuvres futures est nulle |

#### Règles à post-traitement numérique (B1, C2, D3)

Trois règles nécessitent une vérification numérique après le match regex
(le nombre capturé est comparé à un seuil légal) :

```
B1: délai de paiement — extraire le nombre de jours, émettre Risk si >60
C2: période d'essai — extraire le nombre de mois, émettre Risk si >4 (cadre) ou >3 (non-cadre)
D3: bail dérogatoire — extraire la durée, émettre Risk si >36 mois
```

Ces trois fonctions suivent le même patron : regex capture `(\d+)`, parse en
`u32`, compare au seuil. Le seuil est la severity : `high` si dépassé, pas de
Risk émis sinon.

#### Règles à look-ahead contextuel (C1, E1, E2)

Trois règles utilisent un **look-ahead textuel** : elles détectent un pattern de
risque **sauf si** un contexte atténuant apparaît dans un rayon de ~200-300
caractères après le match. En pratique : deux regex — une positive
(`clause_match`), une négative (`mitigant_match`). Risk émis si la positive
matche et la négative ne matche pas dans la fenêtre.

#### Articulation avec `mandatory.rs`

| `mandatory.rs` | `rule_risk_*` |
|----------------|---------------|
| Vérifie qu'une clause **obligatoire est présente** | Détecte qu'une clause **présente est dangereuse** |
| Binaire : present / missing | Gradué : category + severity |
| Indépendant du doc-type en rules.rs | Universel (tout type de document) |
| Résultat = nœud `MandatoryClauseCheck` | Résultat = nœud `Risk` |

Aucun doublon : `mandatory.rs` dit « il manque les pénalités de retard »,
`rule_risk_*` dit « la clause pénale présente est à 50% du montant (high) ».

Catalogue extensible ; chaque règle a son test unitaire.

### 2c. Câblage enricher

Dans `enricher.rs` (la fonction qui mappe `TypedFact` → nodes/edges,
[`enricher.rs:349`](../../../crates/anno-rag/src/legal/enricher.rs)), gérer
`TypedFact::Risk` :

- créer un `NodeWrite::Risk { risk_id, severity, category, text_pseudo }`
  (variante déjà définie dans `kg.rs`) avec
  `risk_id = uuid_v5(chunk_id::category)` ;
- créer un edge `Chunk -[MENTIONS]-> Risk`.

### 2d. Requête nommée + réécriture

- Ajouter **`risk_findings(scope_id, is_dossier)`** au backend SQLite, exposée
  via le trait :
  - `is_dossier = false` : `Document {doc_id} -[HAS_CHUNK]-> Chunk -[MENTIONS]-> Risk`
  - `is_dossier = true` : même chose filtré par `dossier_id`
  - Colonnes : `rid`, `severity`, `category`, `text` ; tri `severity DESC`.
- Réécrire `risk_review()` pour appeler `kg.risk_findings(scope_id, is_dossier)`
  au lieu du Cypher brut sur `SOURCES`.

---

## Découpage des unités

| Unité | Rôle | Dépend de |
|-------|------|-----------|
| `rules.rs::TypedFact::Risk` + `rule_risk_*` | Produire faits de risque | GLiNER labels, normalize |
| `enricher.rs` mapping `Risk` | Faits → nodes/edges | `TypedFact::Risk`, `NodeWrite::Risk` |
| `kg.rs` requêtes nommées (`contract_parties`, `contract_obligations`, `risk_findings`) | Traversées SQL | schéma `legal_nodes`/`legal_edges` |
| `extract.rs` réécriture des 3 workflows | Orchestration outils D2 | méthodes nommées du trait |

Chaque unité est testable isolément.

## Stratégie de test

- **Unitaires `rules.rs`** : chaque `rule_risk_*` sur un texte pseudonymisé
  connu → vérifie `category` + `severity`. Test de déduplication GLiNER↔règle.
- **Unitaires `kg.rs`** : insérer nodes/edges à la main, vérifier que
  `contract_parties`, `contract_obligations`, `risk_findings` renvoient les
  lignes attendues avec le bon edge `MENTIONS`.
- **Intégration `tests/legal_graph_v0.rs`** (harnais existant) : ingérer un
  contrat synthétique → `extract_contract`, `timeline`, `risk_review`
  renvoient des lignes non vides.
- **Non-régression** : un appel `cypher("MATCH ...")` inconnu renvoie toujours
  l'erreur « not supported ».

## Hors périmètre (YAGNI)

- KùzuDB, Neo4j, ou tout backend graphe externe.
- Traducteur Cypher→SQL générique.
- Classification de risque par modèle ML dédié (au-delà des labels GLiNER
  réutilisés).
- Suppression de la méthode `cypher()` du trait (conservée comme garde-fou).

## Risques et points d'attention

- **Couverture des règles de risque** : le catalogue regex est volontairement
  limité ; la couche GLiNER compense le rappel mais peut produire des faux
  positifs `low`. À calibrer via les tests d'intégration.
- **Calibration des candidats GLiNER** : `risk_indicator` (seuil 0.55) peut être
  bruité. Si le taux de faux positifs `low` est trop élevé en test
  d'intégration, relever le seuil dans `default_thresholds` ou n'émettre un
  `Risk` GLiNER-only qu'au-dessus d'une confiance plancher. Pas besoin de
  toucher au modèle : labels et seuils existent déjà.
- **Pseudonymisation** : `text_pseudo` des risques doit passer par la même
  pseudonymisation que les autres faits (cohérence vault).
