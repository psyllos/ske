# SKE — Software Knowledge Engine
## Spécification d'implémentation pour un agent de développement

**Version du document :** 1.4 — 15 octobre 2026 (relecture finale corrective de la v1.3, avant transmission à l'agent d'implémentation)
**Statut :** cahier des charges exécutable ; à mettre en œuvre par incréments
**Langue :** français pour les explications, anglais pour les identifiants de code/API
**Licence du futur logiciel :** à décider ; ne pas choisir à la place du commanditaire.

---

# Journal des corrections v1.3 → v1.4 (normatif)

**Objectif :** corriger les défauts vérifiables restants de la v1.3 qui auraient bloqué ou induit en erreur l'agent d'implémentation, **sans changer la vision du produit ni le contrat Protobuf** (`cir.proto`/`adapter.proto` inchangés ; `cir_minor` reste `3`). Les corrections sont intégrées dans les sections concernées. Aucune valeur attendue existante n'est assouplie ; les seules valeurs attendues modifiées sont celles de V14-03, V14-04, V14-05, V14-06, V14-07 et V14-12 ; les ajouts de V14-16 au §15.1.5 complètent les tableaux sans modifier de ligne existante.

| ID | Défaut constaté en v1.3 | Correction normative v1.4 | Sections |
|---|---|---|---|
| V14-01 | L'ancrage source→IR de §9.5 était inapplicable aux fixtures : `p(I) = I.Cond.Pos()` vaut la position de **déclaration** pour un `ssa.Parameter` (cas de `if !enabled`, `if a && b`…), et le span d'un `SWITCH` (le tag) ne contient pas les tests `case` | SSA construit avec `ssa.GlobalDebug` ; ancrage par `ssa.DebugRef` puis position d'instruction ; **zones d'ancrage** par kind de décision ; repli `APPROX` défini | §9.3, §9.4, §9.5 |
| V14-02 | §9.5-2 (« condition constante ⇒ `elided` ») contredisait §9.2 (`go/ssa` conserve le `ssa.If` et ses deux arêtes) | Une condition constante n'est jamais `elided` | §9.2, §9.5 |
| V14-03 | `check.cir.pb` : liens `sd1/o*` vers `b2/i1/e*`, arêtes inexistantes (`BRANCH` est `b2/i0`) | `b2/i0/e1` et `b2/i0/e0` (conforme à AC-0.1-26) | §15.2.1 |
| V14-04 | `ske inspect` : `analysis_defaults` (`max_solver_queries`, `solver_rlimit`, clés manquantes) en désaccord avec §6.7 | Les 12 clés et défauts de §6.7, à l'identique | §11.5 |
| V14-05 | Un run portant une preuve acceptée pouvait encore passer en `CANCELLED`/`FAILED`, ce qui rendait CHK-15 en erreur | Trigger `trg_proven_run_status_final` (**21 triggers**) + règle de statut définitif après l'étape (2) de §6.6 | §6.7, §8.2, §8.3, §15.4, §16, AC-0.1-13, AC-0.1-41, AC-0.5-13 |
| V14-06 | AC-0.1-06 exigeait ≥ 32 tests `negative_` alors que 34 sont requis en v0.1 | Seuil ≥ 34 | §5.5, AC-0.1-06 |
| V14-07 | Code Go généré : `adapters/go/gen/` (§9.7) contre `adapters/go/internal/proto/` (§15.4, AC-0.1-05) | Chemin unique `adapters/go/gen/` | §15.4, AC-0.1-05 |
| V14-08 | Codes de diagnostic utilisés mais absents du registre (`ADAPTER_EFFECT_CLAIMS_TRUSTED`, `SYNTHETIC_FUNCTION`, `LOOP_ABSTRACTED`) ; aucun code pour une unité en échec | Registre complété, dont `UNIT_FAILED` | §5.4, §5.5, §5.6, §9.2 |
| V14-09 | `ske-abstract` absent des règles de dépendances (un `ske-symbolic` v0.4 qui l'utilise aurait violé `check-deps`) ; nature de la colonne « Peut dépendre de » ambiguë | Ligne ajoutée ; colonne = liste blanche exhaustive des crates du workspace | §2.2 |
| V14-10 | `ske ingest cir --repository` sans `root_uri` défini (collision `UNIQUE(root_uri)` dans AC-0.1-33) ; collisions de noms de dépôts ; `--snapshot` sans défaut alors que des AC l'omettent | Règles d'identification des dépôts ; `--snapshot` vaut `latest` par défaut | §11.1, §11.2 |
| V14-11 | Fixtures négatives ambiguës ou non isolées (`E_TERMINATOR_NOT_LAST`, `E_SSA_MULTI_DEF`, `E_EDGE_SHAPE`, `E_TYPE_MISMATCH`, `E_SOURCE_ACTIVATION_INVALID`, `E_DUPLICATE_ID`) ; valeur SSA jamais définie et types de `RETURN` non couverts par §4.8 | Mutations précisées ; invariants complétés | §4.8, §5.3, §5.4, §15.2.3 |
| V14-12 | Piège de troncature des comptes de décalage plus larges que la valeur | 27ᵉ vecteur obligatoire et règle d'encodage | §4.9, §6.4, AC-0.1-08 |
| V14-13 | Texte SMT-LIB non garanti avec `ScriptedSolver` (bloquait T-VERIFIER-REJECT) ; sémantique du cache de requêtes vis-à-vis du budget | Texte toujours produit par l'encodeur ; succès de cache sans appel ni incrément | §6.4 |
| V14-14 | `analysis_runs.config_fingerprint`, `assumptions_fingerprint` et « JSON canonique » non définis ; hypothèses de run non répertoriées | Formules, JSON canonique (RFC 8785), registre des hypothèses | §3.3, §6.5, §8.1 |
| V14-15 | §9.4 (valeur fonction : « cibles inconnues ») contredisait §9.6 (CHA) ; `world_closed` des appels statiques non spécifié alors qu'AC-0.4-01 l'exige | Alignement sur §9.6 ; `world_closed=true` pour un appel statique | §9.4, §9.6 |
| V14-16 | Divers : `SourceFileDef` partagés entre unités (`UNIQUE(snapshot_id, path)`) ; ordinal des issues source ; numérotation SSA des fixtures ; types `fn()→void` ; noms qualifiés `partial.*`/`tags.*` ; fonctions sans décision manquantes au §15.1.5 ; `exit_kind` des sites implicites ; records après `EndUnit` ; formulation « Un seul `PROVEN_UNREACHABLE` » ; `AllocSite` → `AllocInstance` ; fixtures Java sans `package` ; `runs mark-failed` ; budget mémoire absent du pseudo-code ; emplacement de `consent.json` | Précisions ponctuelles | §5.3, §5.4, §6.5, §6.6, §6.7, §6.9, §9.2, §14, §15, v0.6 |

**Priorité :** toutes ces corrections s'appliquent dès le démarrage (v0.1/v0.2). La règle « ne jamais falsifier une fixture » reste entière.

# Journal des corrections v1.2 → v1.3 (normatif)

**Objectif :** intégrer les anomalies vérifiables de `SKE_v1.2_revue.md` sans changer la vision du produit. Cette version remplace intégralement v1.2 ; les exigences corrigées sont intégrées **dans les sections concernées**, non laissées comme simples errata. Les anomalies de la revue conservent leurs identifiants `B`, `M`, `m` pour traçabilité.

| Ensemble | Corrections et précisions | Références de la revue |
|---|---|---|
| Contrat de base et ingestion | Protobuf compilable, booléens conservateurs, paramètres et exceptions persistés, transaction/savepoints, ordre des validations, exemples et fixtures | B1, B2, B8–B11, B13, B14, M2, M3, M9–M11, M15, M20, M21 |
| Analyse et exactitude | Boucles bornées, provenance de la soundness, mémoire aliasée, vérificateur indépendant, forks anormaux, fonctions synthétiques, conditions constantes | B3–B7, B12, M4–M8, M14 |
| Preuves SQLite | Insertion conditionnée par run fermé, gel ciblé, métriques finalisées avant preuve, résultats prouvés immuables, relectures de solveur non persistées | M1 |
| Roadmap et tests | Fixtures autonomes dès v0.1, génération déterministe, corrections Go/Rust/Java, critères et commandes exécutables, numéros et exemples | B15–B18, M12–M13, M16–M19, M22, m1–m22 |

**Convention de livraison :** une vérification syntaxique des contrats exécutables prouve uniquement leur validité de surface ; elle ne démontre pas la correction globale de l'analyse statique. L'agent NE DOIT PAS déclarer un jalon `PASS` avant exécution de ses tests dans le dépôt créé.

# Journal des corrections v1.1 → v1.2 (normatif)

La révision v1.2 **remplaçait** la v1.1. Ses corrections demeurent en vigueur dans cette v1.3 et sont intégrées **dans les contrats et les critères d’acceptation**, et non présentées comme de simples recommandations. Toutes les règles préexistantes non explicitement modifiées restent applicables.

| ID | Sujet | Décision normative | Sections |
|---|---|---|---|
| V12-01 | Court-circuit source→IR (`&&`, `||`, expressions mixtes) | Cas d’activation ordonnés, preuve de complétude des mappings, interdiction de déduire l’inaccessibilité source à partir du seul classement d’arêtes individuelles | §4.5, §5.1, §6.6, §8, §9.5, §15, §19 |
| V12-02 | Allocations dans les boucles | Séparer site d’allocation et instance d’exécution ; mises à jour fortes uniquement si unicité démontrée ; repli conservateur en cas de fusion | §6.9, §15, §19 |
| V12-03 | Niveaux de garantie | `PROVEN_UNREACHABLE` reste une conclusion **dans le modèle**, jamais une certification de programme ; trois niveaux de vérification explicites | §3.2, §6.6, §8 |
| V12-04 | Preuves SQL mutables | Preuves immuables ; gel des données qui les supportent après acceptation ; contrôles et tests négatifs associés | §8, §15, §19 |
| V12-05 | Gel précoce de CIR | Contrat candidat après v0.2 ; gel après tests d’extension Go/Rust/Java en v0.6 | §2.3, §10, §19 |
| V12-06 | Exceptions et effets | Plusieurs voies exceptionnelles possibles ; filtres et priorité ; pureté séparée de transparence référentielle ; sorties implicites inventoriées | §4.3, §4.7, §5.1, §6.3, §9, §10 |
| V12-07 | Tests multilangages | Comparer les comportements et la traçabilité, non des graphes identiques ni des statuts d’issues supprimées | §15, §19 |
| V12-08 | Modèle fonctionnel à plusieurs niveaux | Fonction → scénario → capacité ; lien de provenance, extension interservice prudente, nouvelle migration v0.5 | §7, §8.4, §19 |

**Priorité avant démarrage :** V12-01, V12-03 et V12-04 doivent être inclus dans v0.1/v0.2 ; V12-02 avant v0.3 ; V12-05 avant tout gel public du protocole ; V12-06/08 sont préparés par la CIR puis livrés progressivement. **Ne jamais falsifier une fixture pour satisfaire le nouveau document.**

---

# Journal des corrections v1.0 → v1.1

Ce tableau liste les défauts de la v1.0 corrigés dans ce document. Il sert de référence de non-régression : aucun de ces défauts ne doit réapparaître.

| # | Défaut v1.0 | Correction v1.1 | Voir |
|---|---|---|---|
| C1 | Le pseudo-code d'exploration enregistrait `PROVEN_UNREACHABLE` dès qu'**un** état était `UNSAT`, ce qui contredisait la règle écrite juste après. | Algorithme réécrit : on enregistre des *arrivées* par issue ; la classification finale se fait après l'exploration, avec la règle de « fermeture globale » et un vérificateur de preuve indépendant. | §6.5, §6.6 |
| C2 | Le proto et le DDL ne se correspondaient pas pour les liens source→IR (granularité décision vs issue, pas de messages pour les issues source, pas de dérivation des décisions IR). | `SourceOutcomeDef` ajouté ; `SourceIrLinkDef` relie issue source ↔ arête ; règle de dérivation des `ir_decisions/ir_outcomes` écrite ; colonnes `inverted`, `quality` ajoutées. | §5.1, §5.4, §8 |
| C3 | Noms d'opcodes incohérents (`CMP_GT`, `COMPARE`, `BINARY`). | Registre normatif unique par opcode ; `CONST` n'est plus une instruction (constantes = `ValueDef`) ; `CAST` supprimé. | §4.3 |
| C4 | Catégories de soundness du §3.1 différentes du `CHECK` SQL. | Une seule énumération : `EXACT, OVERAPPROX, OPAQUE, UNSUPPORTED`. | §3.1, §8 |
| C5 | Un seul `query_id` par résultat ; aucun composant « vérificateur de preuve ». | Tables `outcome_arrivals` et `proof_records`, triggers SQL, procédure de vérification indépendante. | §6.6, §8 |
| C6 | Statuts de connaissance mélangés entre §3.2 et le DDL. | Deux axes séparés : `origin` et `validation_status`. | §3.4, §8 |
| C7 | Exemple JSON `branches list` incohérent avec `Check`. | Exemples recalculés sur `Decide` et `Check`. | §11, §15 |
| C8 | `Check` déclaré « exact » alors que `ErrUnderage` est une variable globale. | Hypothèse obligatoire `SEQUENTIAL_EXECUTION` ; l'exactitude est toujours « sous les hypothèses du run ». | §3.3, §7.2 |
| C9 | `CALL` « applique un résumé » en v0.3 alors que les résumés arrivent en v0.4. | v0.3 : tout `CALL` est havocké, sauf si transparence référentielle prouvée (précision apportée en v1.2). | §6.3 |
| C10 | Contrat CIR figé sans retour d'un adaptateur réel ; payloads insuffisants (`Extract`, `FieldAddr`, `RunDefers`…). | Opcodes et payloads ajoutés ; CIR v1 déclaré initialement **provisoire jusqu'à la fin de la v0.2** (décision historique **remplacée** par V12-05 : gel en v0.6, après validation Go/Rust/Java). | §4.3, §5 |
| C11 | Garde des arêtes `SWITCH`/`default` et exceptionnelles non définie. | `edge_kind` énuméré, `case_value_ids`, règle de garde du `default`. | §4.7 |
| C12 | Références croisées entre unités et clés `local_key` ambiguës. | Tous les identifiants d'un flux sont **locaux au flux** ; le cœur préfixe par `unit_key` ; callees non extraits = **stubs externes**. | §4.6, §5.3 |
| C13 | `semantic_profile` unique par snapshot (bloque le multilangage). | Profil et empreinte de build portés par l'**unité de compilation**. | §8 |
| C14 | Tables manquantes (`phi_inputs`, lien décision→fonction, lien cas→issues, retours multiples). | Ajoutées dans la migration 0001. | §8 |
| C15 | Réutilisation d'analyses entre snapshots non conçue. | Copie-sur-réutilisation avec `reused_from_summary_id` (v0.5) ; aucune réutilisation avant. | §13 |
| C16 | Formats `budget.toml`, `build-profile.json`, contexte d'analyse, `adapter.proto` non définis. | Spécifiés. | §5.2, §11.4 |
| C17 | Déterminisme impossible avec des timeouts en temps réel. | Limite de ressources solveur (`rlimit`) + graine fixe ; timeout mur = filet de sécurité non déterministe, déclaré comme tel. | §6.4 |
| C18 | `go/packages` pouvait compiler / télécharger un toolchain. | Environnement forcé (`GOTOOLCHAIN=local`, `GOPROXY=off`, …), pas d'export data. | §9.1 |
| C19 | `utf8_string` inadapté aux chaînes Go ; incohérence `benchmarks/` ; contre-test sans effet. | `bytes string_bytes` ; chemin unique ; contre-test réécrit avec mémoire. | §5.1, §2, §15 |
| C20 | Critères d'acceptation vagues. | Chaque jalon a une table `AC-x.y-nn` : critère, commande, résultat attendu ; fixtures entièrement définies. | §15, §19 |

---

# 0. Mode d'emploi pour l'agent d'implémentation

## 0.1. Rôle et mission

Tu es l'agent de développement responsable de la conception exécutable, du code, des tests et de la documentation de **SKE (Software Knowledge Engine)**. Ce document est la source de vérité du projet (version 1.4).

**Mission :** construire un moteur de *reverse engineering fonctionnel* de logiciels existants à partir de leur code source, en commençant par Go, puis Rust et Java, avec un design extensible. Tu dois produire un logiciel **réellement exécutable** : pas de proposition d'architecture seule, pas d'interfaces vides, pas de pseudo-code laissé en place.

**Objectifs, par ordre de priorité :**

1. **Priorité A — Reverse engineering fonctionnel :** inventaire exhaustif **des branches identifiables dans le périmètre déclaré**, analyse des conditions et effets, reconstruction traçable des comportements et des règles implémentées.
2. **Priorité B — Knowledge backend pour agents IA :** interrogation structurée, contextualisation à budget de tokens, puis API/MCP. Ne jamais sacrifier A pour B.
3. **Aucun LLM** pour les analyses structurelles, symboliques et la première extraction de comportements. Le LLM est un enrichissement facultatif ultérieur (v0.7).
4. **SQLite** est le seul stockage de référence ; aucun serveur de base de données en v0.x.
5. Ne **jamais** affirmer connaître « tous les chemins d'exécution concrets ».

## 0.2. Vocabulaire normatif

Les mots **DOIT**, **NE DOIT PAS**, **DEVRAIT**, **PEUT** ont leur sens RFC 2119.
- **DOIT / NE DOIT PAS** : obligation testable. Une violation est un défaut.
- **DEVRAIT** : défaut recommandé ; s'en écarter exige un ADR.
- **PEUT** : libre.

## 0.3. Hiérarchie des sources en cas de doute

1. Les **tables de critères d'acceptation** (`AC-x.y-nn`, §19) et les **fixtures** (§15) priment sur tout le reste.
2. Ensuite les sections marquées **normatives**.
3. Ensuite le reste du texte (exemples, rationales).

**Si deux passages se contredisent, ou si une instruction est ambiguë :**
1. **Ne devine pas en silence.** Choisis l'interprétation la plus *conservatrice* (celle qui affirme le moins de choses prouvées).
2. Écris la question et ton choix dans `docs/open-questions.md` (date, section, choix, justification).
3. Continue. Ne bloque que si aucun choix conservateur n'existe ; dans ce cas, écris un rapport `BLOCKED` (§0.6) et arrête-toi.

## 0.4. Règles d'exécution

- Implémente **dans l'ordre** les jalons de la section 19. Ne commence pas un jalon avant que tous les critères `AC-x.y-nn` du jalon précédent soient `PASS` ou explicitement `WAIVED` par un ADR approuvé par le commanditaire.
- À chaque jalon, livre : **code**, **migration** (si le schéma change), **tests**, **exemples**, **documentation**, **commandes de validation** et **rapport des limites** (format §20).
- **Ne coche un critère qu'après avoir exécuté sa commande de vérification** et obtenu le résultat attendu. Consigne la commande exacte et la sortie (résumée) dans `docs/validation/v0.X.md`.
- Si un besoin dépasse la version courante, implémente une **frontière d'extension** sans faire croire que la fonction est prise en charge : toute opération non modélisée produit un diagnostic et une surapproximation prudente.
- **NE DOIT PAS** transformer `UNKNOWN` ou `TIMEOUT` en `UNREACHABLE` ; **NE DOIT PAS** transformer `SAT` d'un modèle abstrait en « chemin réel prouvé ».
- Les adaptateurs évoluent indépendamment du noyau. Aucun type interne Go, `rustc` ou SootUp **NE DOIT** apparaître dans le modèle public CIR.
- Une analyse est liée à : un instantané du code, une configuration de build, un toolchain, des hypothèses, des versions de modèles externes.
- **Politique de sécurité :** par défaut, ne pas exécuter le code étudié, ses tests, scripts de génération ni des commandes de build arbitraires (voir §9.1 et §14).
- **Priorité de livraison :** une v0.3 réellement démontrable en Go vaut mieux qu'une pseudo-v0.7 multilangage avec des stubs.
- **Interdits explicites** (cause fréquente d'échec) :
  - écrire un test qui compare la sortie du programme à un fichier produit *par ce même programme* sans vérification indépendante (« test tautologique ») ; les valeurs attendues des fixtures sont **données dans ce document** (§15) ;
  - laisser `todo!()`, `unimplemented!()`, `panic!("not implemented")` sur un chemin atteint par un critère d'acceptation ;
  - désactiver un test (`#[ignore]`, `t.Skip`) pour faire passer un critère, sauf ceux marqués « non bloquant » ;
  - modifier une valeur attendue de ce document pour que le test passe : en cas de désaccord, applique §0.3.

## 0.5. Hypothèses par défaut (à documenter dans `docs/architecture.md`)

Monorepo ; développement local macOS/Linux ; Rust stable (édition 2021) pour le cœur ; toolchain Go installé pour l'adaptateur Go ; SQLite local ; sans interface graphique. **Les versions précises des dépendances sont vérifiées et figées à l'implémentation** (`Cargo.lock`, `go.sum` committés ; liste dans `docs/dependencies.md`). Ne jamais inventer un numéro de version : exécuter `cargo add` / `go get` et recopier le résultat.

## 0.6. Rapport `BLOCKED`

Si tu ne peux pas satisfaire un critère, écris `docs/validation/BLOCKED-<AC>.md` contenant : le critère, la commande exécutée, la sortie complète, les hypothèses testées, ce qui manque. **Ne contourne pas** le critère. Ne déclare pas le jalon terminé.

## 0.7. Gabarit de commande de validation

Toutes les commandes sont exécutées depuis la racine du dépôt `ske/`. Une commande « réussit » si son code de sortie vaut `0` (sauf mention contraire) **et** que la sortie contient ce qui est attendu. Les commandes utilisent `--db "$DB"` où `DB=$(mktemp -d)/k.sqlite`.

---

# 1. Périmètre et non-objectifs

## 1.1. Fonctionnalités attendues

| Fonctionnalité | Exigence cible | Disponibilité initiale |
|---|---|---|
| Inventaire sources, packages, modules, symboles | Déclarations + provenance | v0.1 (CIR), v0.2 (Go réel) |
| Inventaire des décisions **source** | Toutes les constructions prises en charge et leurs issues | v0.2 |
| Inventaire des décisions **IR** | Branches multi-issues du CFG (≥ 2 arêtes sortantes) | v0.1 (fixtures) / v0.2 |
| CFG intra-fonction | Blocs/arêtes typées | v0.1/v0.2 |
| Graphe d'appels | Cibles résolues + inconnues, sans monde fermé | v0.2 partiel ; v0.4 approfondi |
| Propagation symbolique | Valeurs, contraintes, mémoire M0/M1 | v0.3 |
| Vérification SMT | SAT / UNSAT / UNKNOWN / TIMEOUT, budget, hypothèses | v0.3 |
| Boucles/récursivité | Exploration bornée + diagnostics ; puis résumés | v0.3 / v0.4 |
| Résumés interprocéduraux | Préconditions, sorties, mutations, effets | v0.4 |
| Modèle de comportements | Conditions, actions, résultats, preuves | v0.5 |
| Connaissance fonctionnelle | Règles déduites, traçabilité | v0.5 |
| Adaptateurs Rust / Java | Traduction vers CIR, rapports de couverture | v0.6 |
| API/MCP et LLM | Queries et enrichissement contrôlé | v0.7 |
| Analyse incrémentale | Réutilisation/invalidation | Base v0.1 ; copie-sur-réutilisation v0.5 |

## 1.2. Non-objectifs initiaux

- Ne pas énumérer toutes les combinaisons de décisions ni toutes les traces concrètes.
- Ne pas prouver la correction du programme ni l'intention métier de ses auteurs.
- Ne pas fournir de preuve de faisabilité exacte avec réflexivité, code natif/`unsafe`, appels externes, concurrence, mémoire non modélisée.
- Ne pas inférer qu'un appel `save()` signifie une écriture en base sans modèle sémantique ou analyse du corps.
- Ne pas analyser le code généré absent du périmètre si sa génération exige d'exécuter des outils non approuvés.
- Ne pas proposer de couverture dynamique des tests sans instrumentation réelle.

## 1.3. Deux niveaux d'exhaustivité

**Inventaire structurel :** lister chaque issue de branche reconnue, même non atteignable, et signaler les constructions non prises en charge. L'exhaustivité se rapporte à **un ensemble de fichiers, une configuration de compilation et des catégories sémantiques déclarées**.

**Exploration comportementale :** pour chaque branche, qualifier l'atteignabilité dans un **contexte** défini (§11.4). Plusieurs contextes, hypothèses ou budgets donnent des résultats différents, stockés dans des **runs** différents. Une branche `UNANALYZED` n'a jamais disparu : elle reste dans l'inventaire.

## 1.4. Glossaire

- **Source decision :** construction du code source qui sélectionne le flot de contrôle (`if`, `switch`, `select`, opérande de `&&`/`||`, condition de boucle…).
- **Source outcome :** issue d'une source decision (`TRUE`, `FALSE`, `CASE:n`, `DEFAULT`…).
- **IR decision :** terminateur du CFG ayant **au moins 2 arêtes sortantes** (`BRANCH`, `SWITCH`, `INVOKE`).
- **IR outcome :** une arête sortante d'une IR decision.
- **Exit :** terminateur sans arête (`RETURN`, `THROW`, `PANIC`, `UNREACHABLE`). Un exit n'est **pas** une decision.
- **CFG :** graphe de flot de contrôle intra-fonction.
- **CIR :** Common Intermediate Representation, typée, indépendante des langages.
- **State :** environnement symbolique, mémoire, contraintes, contexte d'appel, effets, position.
- **Summary :** résumé de fonction en cas conditionnels et effets, valable sous ses hypothèses.
- **Arrival :** un état symbolique atteignant l'arête d'une IR outcome, avec le verdict du solveur sur « chemin + garde ».
- **Evidence :** référence de provenance (construction CIR/source, arrivée, requête SMT…), pas nécessairement une preuve mathématique.
- **Sound overapproximation :** peut inclure des comportements inexistants, **ne doit exclure aucun** comportement réel du périmètre modélisé.
- **Stub externe :** `FunctionDef` avec `is_external=true` et sans bloc, représentant un callee non extrait.

---

# 2. Architecture générale et découpage du dépôt

```text
source Go ─► adaptateur Go ──┐
source Rust ► adaptateur Rust ├─► flux CIR (protobuf) ─► validate/normalize
source Java ► adaptateur Java ┘                    │
                                                 ├─► inventaires source et IR
                                                 ├─► CFG / call graph
                                                 └─► SQLite (structure)
                                                          │
                                                   symbolic engine
                                                          │
                                               SMT (Z3) / domaines abstraits
                                                          │
                                         arrivées, preuves, résumés
                                                          │
                                                 behavior engine
                                                          │
                                            SQLite (résultats, règles)
                                                          │
                                                CLI / API / MCP
                                                          │
                                              LLM facultatif
```

## 2.1. Arborescence **normative**

Tout chemin cité dans ce document **DOIT** exister à cet emplacement.

```text
ske/
├── Cargo.toml                       # workspace Rust
├── Cargo.lock
├── rust-toolchain.toml              # canal stable épinglé
├── README.md
├── .github/workflows/ci.yml
├── docs/
│   ├── architecture.md
│   ├── cir-semantics.md             # registre normatif des opcodes (§4.3) + vecteurs de test
│   ├── soundness-and-limits.md
│   ├── adapter-contract.md
│   ├── sql-schema.md
│   ├── dependencies.md
│   ├── implementation-status.md
│   ├── open-questions.md
│   ├── validation/                  # v0.1.md, v0.2.md, … + BLOCKED-*.md
│   └── adr/                         # ADR-001.md … (§18)
├── schemas/
│   ├── cir/v1/cir.proto
│   └── adapter/v1/adapter.proto
├── crates/
│   ├── ske-common/                  # IDs, erreurs (E_*), diagnostics, hashing
│   ├── ske-cir/                     # IR typée, BitVec, registre d'opcodes, vérificateur, gen_fixtures (example)
│   ├── ske-storage/                 # SQLite, migrations, transactions, DAO, requêtes de contrôle
│   ├── ske-ingest/                  # lecture du flux, validation, import
│   ├── ske-graph/                   # CFG/call graph/traversée/SCC/dominance
│   ├── ske-symbolic/                # interpréteur, états, DAG d'expressions, exploration
│   ├── ske-verify/                  # vérification indépendante, persistance et rejeu des preuves
│   ├── ske-solver/                  # trait Solver, backend Z3, ScriptedSolver (tests)
│   ├── ske-abstract/                # intervalles (v0.4)
│   ├── ske-summary/                 # résumés (v0.4)
│   ├── ske-behavior/                # comportements/règles (v0.5)
│   ├── ske-query/                   # services applicatifs / DTO
│   ├── ske-cli/                     # binaire `ske`
│   ├── ske-api/                     # (v0.7)
│   └── ske-mcp/                     # (v0.7)
├── adapters/
│   ├── go/                          # module Go indépendant (go.mod propre)
│   ├── rust/                        # (v0.6)
│   └── java/                        # (v0.6)
├── migrations/
│   └── 0001_initial.sql
├── examples/
│   ├── go-branch-lab/               # module Go `example` (§15.1)
│   ├── rust-branch-lab/             # (v0.6)
│   └── java-branch-lab/             # (v0.6)
├── tests/
│   ├── fixtures/cir/                # *.cir.pb générés (§15.2)
│   ├── golden/                      # fichiers JSON attendus, écrits à la main (§15.5)
│   ├── differential/                # module Go séparé : go.mod avec `replace example => ../../examples/go-branch-lab` (§16, AC-0.3-21)
│   ├── integration/
│   └── benchmarks/README.md
└── scripts/
    ├── verify.sh
    ├── gen-proto.sh
    └── check-deps.sh
```

## 2.2. Règles de dépendances entre crates (**normatif**, vérifié par `scripts/check-deps.sh`)

| Crate | Peut dépendre de (workspace) | NE DOIT PAS dépendre de |
|---|---|---|
| `ske-common` | — | tout autre crate |
| `ske-cir` | `ske-common` | `rusqlite`, `z3`, `ske-storage`, `ske-symbolic`, `ske-solver` |
| `ske-graph` | `ske-common`, `ske-cir` | `rusqlite`, `z3` |
| `ske-storage` | `ske-common`, `ske-cir` | `ske-cli`, `ske-symbolic`, `ske-solver` |
| `ske-ingest` | `ske-common`, `ske-cir`, `ske-storage`, `ske-graph` | `ske-cli`, `z3` |
| `ske-solver` | `ske-common`, `ske-cir` | `ske-storage`, tout accès fichier source |
| `ske-abstract` | `ske-common`, `ske-cir`, `ske-graph` | `ske-storage`, `ske-symbolic`, `ske-solver`, `z3` |
| `ske-symbolic` | `ske-common`, `ske-cir`, `ske-graph`, `ske-solver`, `ske-abstract` (v0.4+) | `ske-storage`, tout adaptateur |
| `ske-summary`, `ske-behavior` | `ske-common`, `ske-cir`, `ske-graph`, `ske-symbolic`, `ske-solver`, `ske-abstract` | `ske-storage` |
| `ske-verify` | `ske-common`, `ske-cir`, `ske-solver`, `ske-storage` | `ske-symbolic` et son module de classification, tout adaptateur |
| `ske-query` | tous sauf `ske-cli`, `ske-api`, `ske-mcp` | — |
| `ske-cli`, `ske-api`, `ske-mcp` | `ske-query` (+ `ske-common`) | accès direct à `rusqlite` |

La colonne « Peut dépendre de » est une **liste blanche exhaustive** des crates du workspace : toute dépendance directe vers un autre crate du workspace est une violation. Les crates externes ne sont contraints que par la colonne « NE DOIT PAS ». Seules les dépendances **normales** sont contrôlées (`kind` nul dans `cargo metadata`) ; les `dev-dependencies` et `build-dependencies` sont libres (ex. `rusqlite` dans les tests d'intégration de `ske-cli` qui vérifient les `COUNT(*)` de §15.2.2).

`scripts/check-deps.sh` **DOIT** utiliser `cargo metadata --format-version 1`, calculer les dépendances directes de chaque crate du workspace et sortir en code `1` avec le message `FORBIDDEN_DEP <crate> -> <dep>` pour toute violation de ce tableau. Les adaptateurs ne dépendent que des schémas publics (`schemas/`) et de leurs outils de langage.

## 2.3. IPC adaptateur ↔ cœur (**normatif**)

- Les adaptateurs sont des **processus enfants** lancés après consentement explicite et validation du toolchain.
- `stdin` : requêtes `AdapterRequest` (protobuf). `stdout` : messages `AdapterResponse` (protobuf). `stderr` : logs structurés (une ligne JSON par événement). **Aucun octet parasite sur `stdout`.**
- **Encadrement (framing)** : chaque message = **4 octets de longueur en big-endian non signé** (`u32`) suivis de la sérialisation protobuf. Longueur maximale par défaut : 16 MiB (`--max-message-bytes`). Longueur 0 = message vide valide. Un flux se termine par la fermeture de `stdout` (EOF) **après** le message `EndUnit` ou `ListUnitsDone`.
- Limites configurables : taille de message, temps total, nombre de messages, mémoire (si l'OS le permet). Dépassement ⇒ le cœur tue le processus et enregistre `E_ADAPTER_LIMIT`.
- Les fichiers de fixtures `*.cir.pb` utilisent **le même encadrement** : une suite de `Record` encadrés, sans en-tête de fichier.
- Protocole versionné (`cir_major=1`) ; le cœur **refuse** un `cir_major` inconnu (`E_CIR_MAJOR_UNSUPPORTED`) et **enregistre un diagnostic** pour tout champ/opcode/kind inconnu (jamais d'ignorance silencieuse).

**Décision de version :** CIR major `1`, minor évolutif. **CIR v1 est PROVISOIRE jusqu’à la clôture de la v0.6** (validation Go/Rust/Java) : jusque-là, des changements incompatibles du proto sont autorisés **avec** ADR, version mineure, migration, et mise à jour des fixtures. À la clôture de la v0.2, tagger `cir-v1-candidate` ; la CIR v1 **reste candidate jusqu’à la validation multilangage v0.6**, où seulement alors `cir-v1.0-frozen` est créé. Avant ce tag, toute rupture nécessite ADR, mise à jour du `.proto`, fixtures et migration/documentation. Après gel, seules les extensions compatibles sont permises.

---

# 3. Modèle de sémantique et statuts

## 3.1. Principe fondamental

Le moteur ne travaille pas avec « des entiers mathématiques génériques » mais avec les types et sémantiques **réels du langage et de la configuration** : entiers de largeur définie, flottants IEEE-754, conversions, panics/exceptions, objets, tableaux, effets de bord, dispatch.

**Profils de sémantique** (portés par chaque **unité de compilation**, §8) : `go-<version>-<GOOS>-<GOARCH>`, `rust-<toolchain>-<target>-<profiles>`, `java-<target>-<classpath>-<framework-profile>`. Le hash du profil (sha256 hex du texte du profil) est stocké avec l'unité. Pour les fixtures CIR codées à la main, le profil est exactement `go-fixture-linux-amd64`.

Chaque opération analysée est attribuée à **une et une seule** catégorie de soundness (énumération unique, utilisée partout, y compris en SQL) :

1. `EXACT` — traduction sémantique prise en charge, sans approximation.
2. `OVERAPPROX` — tous les comportements réels sont couverts, plus éventuellement des comportements supplémentaires.
3. `OPAQUE` — comportement inconnu, non inspectable : valeur non déterministe, effets possibles prudents. (C'est une sous-forme d'`OVERAPPROX` côté moteur ; la catégorie reste distincte pour les diagnostics.)
4. `UNSUPPORTED` — construction détectée mais non analysée ; diagnostic obligatoire.

Règle de monotonie : la soundness d'un état ne peut que **se dégrader** dans l'ordre `EXACT < OVERAPPROX < OPAQUE < UNSUPPORTED`. Si les hypothèses permettent la perte de comportements réels, **aucune conclusion d'inaccessibilité ne peut être « prouvée »**.

## 3.2. Trois axes de statut — ne pas les mélanger

**Inventaire (`inventory_status`)** : `COMPLETE`, `PARTIAL`, `FAILED`. Calculé par fonction, puis par unité, puis par snapshot (règles §3.5).

**Exécution de l'analyse (`run_status`)** : `PENDING`, `RUNNING`, `COMPLETED`, `PARTIAL`, `FAILED`, `CANCELLED`.

**Résultat par issue IR (`reachability_status`)** :

- `UNANALYZED` — aucun résultat pour cette issue et ce contexte.
- `POSSIBLE` — le modèle surapproximatif admet cette issue ; **pas** de témoin concret vérifié.
- `WITNESSED` — exécution concrète observée ou témoin vérifié par exécution (**aucun producteur avant une version dynamique ; la valeur existe dans l'énumération mais NE DOIT PAS être produite en v0.x**).
- `PROVEN_UNREACHABLE` — impossibilité démontrée sous hypothèses documentées, avec `proof_record` accepté (§6.6).
- `UNKNOWN` — solveur, modèle ou dépendances insuffisants.
- `TIMEOUT` — budget dépassé (par requête ou global).
- `UNSUPPORTED` — opérations indispensables non modélisées.

**`SAT` n'implique pas `WITNESSED`.** `UNSAT` n'implique pas `PROVEN_UNREACHABLE` : voir §6.6.

**Niveau de garantie distinct du statut (`verification_level`, §8) :**
- `SOLVER_REPLAYED` : les requêtes SMT `UNSAT` ont été rejouées ; aucune garantie de couverture de l’exploration.
- `MODEL_UNREACHABLE` : les conditions de fermeture, l’encodage, les hypothèses et les éliminations vérifiées permettent une conclusion **dans le modèle CIR** ; c’est le niveau maximal que vise la v0.3. Il ne certifie pas la sémantique du programme source.
- `CERTIFIED_UNREACHABLE` : réservé à une future version dotée d’un certificat contrôlé par un vérificateur réellement indépendant. **Interdit en v0.x.**

`PROVEN_UNREACHABLE` signifie toujours : **inaccessible dans le modèle considéré, sous les hypothèses du run**, avec `verification_level = MODEL_UNREACHABLE` en v0.3. La seconde interrogation du même solveur ne suffit pas à une certification formelle indépendante. Les API **DOIVENT** exposer ce niveau de garantie et les hypothèses, jamais présenter ce résultat comme preuve absolue sur le logiciel réel.


## 3.3. Hypothèses d'analyse

Tout run enregistre ses hypothèses dans `analysis_assumptions`. En v0.x, **tout run** porte au minimum :

- `SEQUENTIAL_EXECUTION` : aucune modification concurrente de la mémoire entre deux instructions du chemin analysé (goroutines/threads non modélisés).
- `CLOSED_ENTRY_CONTEXT` : seuls les paramètres du point d'entrée et la mémoire accessible depuis eux sont symboliques ; l'état global initial est un symbole libre non contraint.

**Registre des hypothèses (`analysis_assumptions.kind`, v1.4)** : `SEQUENTIAL_EXECUTION`, `CLOSED_ENTRY_CONTEXT`, `CALLEE_PANIC_EXITS_NOT_EXPLORED` (§6.3), `ADAPTER_EFFECT_CLAIMS_TRUSTED` (§6.5), `ADAPTER_SOURCE_MAPPING_TRUSTED` (§4.5.1), `LOOP_INTERVAL_ABSTRACTION` (v0.4), `PARAM_RANGE` (une ligne par `[[param_range]]` du contexte §11.4, `payload` = JSON canonique `{"max":…,"min":…,"param":"…"}`). Toute autre valeur exige un ADR et la mise à jour de ce registre. Toutes les hypothèses d'un run sont persistées **avant** son passage en `COMPLETED` (§6.6).

**« EXACT » signifie toujours « exact sous les hypothèses du run ».** Un cas de comportement `EXACT` et une `reachability` sont donc toujours accompagnés de la liste d'hypothèses du run.

## 3.4. Statut de connaissance (règles fonctionnelles)

Deux axes **séparés** (deux colonnes SQL) :

- `origin` : `DETERMINISTIC`, `ABSTRACT_DERIVED`, `LLM_INFERRED`.
- `validation_status` : `UNVALIDATED`, `HUMAN_VALIDATED`, `REJECTED`, `STALE`.

Une règle `LLM_INFERRED` **NE DOIT PAS** être produite avant la v0.7 et ne remplace jamais une règle déterministe.

## 3.5. Règles d'agrégation de l'inventaire (**normatif**)

1. **Fonction** : `COMPLETE` ssi aucun diagnostic de cette fonction n'a `impact = 'INVENTORY'` avec `severity ∈ {WARNING, ERROR}` ; `FAILED` ssi l'adaptateur a émis `FUNCTION_TRANSLATION_FAILED` pour elle ; sinon `PARTIAL`. Un stub externe est `COMPLETE`. Une fonction non externe déclarée `FAILED` à cause de `FUNCTION_TRANSLATION_FAILED` peut avoir 0 bloc et aucune entrée ; ses décisions source déjà inventoriées restent persistées, et aucune analyse symbolique ne peut être déclarée complète pour elle.
2. **Unité** : `COMPLETE` ssi (a) `EndUnit.expected_source_decisions` = nombre réel de `SourceDecisionDef` importées, (b) `EndUnit.emitted_ir_edges` = nombre réel de `ControlEdgeDef`, (c) toutes les fonctions non externes sont `COMPLETE`, (d) aucun diagnostic d'unité avec `impact='INVENTORY'` et `severity ≥ WARNING`. `FAILED` si la validation d'import échoue ou si l'adaptateur déclare `FAILED`. Sinon `PARTIAL`.
3. **Le cœur recalcule.** Si l'adaptateur déclare `COMPLETE` mais que le cœur trouve un écart, le cœur stocke `PARTIAL` et ajoute le diagnostic `INVENTORY_STATUS_DOWNGRADED`. Le cœur **ne promeut jamais** un `PARTIAL` en `COMPLETE`.
4. **Snapshot** : `COMPLETE` ssi toutes ses unités sont `COMPLETE` (et il en a ≥ 1) ; `FAILED` ssi toutes ses unités sont `FAILED` ou s'il n'a aucune unité ; sinon `PARTIAL`.
5. **`run_status`** : voir §6.7.

---

# 4. CIR v1 — contrat

## 4.1. Hiérarchie

```text
Snapshot → CompilationUnit → Symbol → Function
                                    ├─ Parameters / Values (SSA, constantes, globales)
                                    ├─ BasicBlocks
                                    │    ├─ Instructions (PHI en tête de bloc)
                                    │    └─ exactement 1 Terminator (dernière instruction)
                                    ├─ ControlEdges
                                    └─ Diagnostics
```

Les valeurs sont SSA ; les effets mémoire passent par `LOAD`/`STORE` sur des objets abstraits. Chaque bloc se termine par **exactement un** terminateur ; chaque issue d'un terminateur correspond à une arête de contrôle ou à une sortie (`exit`).

## 4.2. Types (`TypeDef`)

Catégories (`kind`) et champs utilisés. Les champs non listés pour un `kind` **DOIVENT** être vides/0 (sinon `E_TYPE_MISMATCH`).

| kind | `bit_width` | `is_signed` | `param_type_ids` | `result_type_ids` | `component_names` | `nullable` | Remarques |
|---|---|---|---|---|---|---|---|
| `VOID` | — | — | — | — | — | — | |
| `BOOL` | — | — | — | — | — | — | jamais un entier |
| `BITVECTOR` | ≥ 1 | oui | — | — | — | — | entiers Go/Rust/Java ; `int` Go dépend de `GOARCH` |
| `IEEE_FLOAT` | 16/32/64/128 | — | — | — | — | — | non modélisé en SMT avant v0.4+ |
| `CHAR` / `STRING` | — | — | — | — | — | — | `canonical_descriptor` porte l'encodage |
| `POINTER` | — | — | `[pointee]` | — | — | oui | |
| `REFERENCE` | — | — | `[pointee]` | — | — | oui | |
| `ARRAY` | — | — | `[element]` | — | — | — | longueur en `extra` (u64 big-endian) si connue |
| `SLICE` | — | — | `[element]` | — | — | — | |
| `TUPLE` | — | — | éléments | — | — | — | retours multiples Go |
| `STRUCT` | — | — | types des champs | — | noms des champs | — | |
| `ENUM` | — | — | — | — | noms des variantes | — | |
| `INTERFACE` | — | — | — | — | noms des méthodes | — | `error` Go = INTERFACE `go:error` |
| `FUNCTION` | — | — | paramètres | résultats | — | — | |
| `OPAQUE` | — | — | — | — | — | — | `canonical_descriptor` obligatoire ; types non modélisés |

**Règles :** `int`/`uintptr` Go dépendent de `GOARCH` (jamais « toujours 64 bits ») ; un `BOOL` n'est pas un entier ; `nil`/`NULL` n'est pas une valeur zéro ; ne pas normaliser les `NaN` ; préserver division, débordement et conversions.

## 4.3. Registre normatif des opcodes (v1)

**Règles générales** (valables pour toutes les lignes) :
- Tout opcode **non présent** dans ce tableau est refusé : `E_UNKNOWN_OPCODE`. Ne **jamais** l'interpréter comme un no-op.
- « Opérandes » = `operand_value_ids` ordonnés. « T » = un type quelconque ; « BV » = `BITVECTOR` ; « T1=T2 » = types identiques (même `TypeDef.id`).
- « Erreurs » : comportements anormaux possibles. « SMT v0.3 » : `Oui` = encodé en SMT-LIB BitVec/Bool ; `Havoc` = résultat = nouveau symbole libre du bon type + diagnostic `OPAQUE_OP_EMITTED` (INFO) si l'adaptateur ne l'a pas déjà donné ; `Mém` = traité par le modèle mémoire §6.9 ; `Ctl` = géré par le contrôle.
- Chaque ligne **DOIT** avoir : une section `### OPCODE` dans `docs/cir-semantics.md`, ≥ 1 test unitaire positif et ≥ 1 test négatif d'arité/type. (Vérifié par AC-0.1-24.)

| Opcode | Opérandes | Résultat | Payload | Erreurs / effets | SMT v0.3 |
|---|---|---|---|---|---|
| `COPY` | 1 : T | T | — | — | Oui |
| `PHI` | **0** (entrées dans `phi_inputs`) | T | — | — | Oui (choix par prédécesseur) |
| `SELECT` | 3 : BOOL, T, T | T | — | — | Oui (`ite`) |
| `UNDEF` | 0 | T | — | valeur indéfinie = symbole libre | Havoc |
| `HAVOC` | 0 | T | — | valeur arbitraire | Havoc |
| `ADD` `SUB` `MUL` | 2 : BV, T1=T2 | T1 | `BinarySemantics` (obligatoire) | `overflow_mode` ∈ {`WRAP`, `CHECKED`, `SATURATE`, `UNDEFINED`}. Go : `WRAP` | Oui si `WRAP` ; sinon Havoc + diag `UNSUPPORTED_OVERFLOW_MODE` |
| `DIV` `REM` | 2 : BV, T1=T2 | T1 | `BinarySemantics` | **Div. par zéro ⇒ sortie `PANIC`** ; signé : arrondi vers zéro ; `INT_MIN / -1 = INT_MIN`, reste 0 (Go) | Oui (`bvsdiv/bvudiv/bvsrem/bvurem`) + successeur panic |
| `NEG` | 1 : BV | T | `BinarySemantics` | `WRAP` | Oui (`bvneg`) |
| `EQ` `NE` | 2 : T1=T2 (tout type sauf `IEEE_FLOAT`) | BOOL | — | — | Oui (BV/BOOL : `=` ; autres types : sorte non interprétée, §6.4) |
| `LT` `LE` `GT` `GE` | 2 : BV, T1=T2 | BOOL | — | signé/non signé selon `is_signed` du type | Oui (`bvslt/bvult`…) |
| `BIT_AND` `BIT_OR` `XOR` | 2 : BV, T1=T2 | T1 | — | — | Oui |
| `BIT_NOT` | 1 : BV | T | — | — | Oui (`bvnot`) |
| `SHL` `SHR` | 2 : BV (valeur), BV (compte, largeur quelconque) | type de la valeur | `BinarySemantics` (`signedness` du **compte**) | compte négatif (signé) ⇒ sortie `PANIC` ; compte ≥ largeur ⇒ `SHL`→0, `SHR` logique→0, `SHR` arithmétique (type signé)→remplissage du signe | Oui (`bvshl/bvlshr/bvashr`) + successeur panic |
| `LOGICAL_NOT` | 1 : BOOL | BOOL | — | — | Oui |
| `SIGN_EXTEND` `ZERO_EXTEND` | 1 : BV | BV plus large | — | — | Oui |
| `TRUNCATE` | 1 : BV | BV plus étroit | — | — | Oui (`extract`) |
| `BITCAST` | 1 : BV | BV même largeur | — | réinterprétation (ex. `int`↔`uint`) | Oui |
| `EXTRACT_FIELD` | 1 : STRUCT/TUPLE | type du champ | `FieldAccess` | — | Mém/agrégat : champ symbolique |
| `INSERT_FIELD` | 2 : STRUCT/TUPLE, T | même type | `FieldAccess` | — | idem |
| `INDEX` | 2 : ARRAY/SLICE/STRING, BV | élément | — | hors bornes ⇒ `PANIC` | Havoc (v0.3) + diag `IMPLICIT_PANIC_UNMODELED` |
| `LENGTH` | 1 : ARRAY/SLICE/STRING | BV | — | — | Havoc |
| `GET_TAG` | 1 : ENUM/INTERFACE | BV | — | — | Havoc |
| `ALLOC` | 0 | POINTER | `MemorySemantics` | alloue un objet abstrait | Mém |
| `LOAD` | 1 : POINTER/REFERENCE | pointee | `MemorySemantics` | pointeur nul ⇒ `PANIC` possible | Mém |
| `STORE` | 2 : POINTER/REFERENCE, pointee | aucun | `MemorySemantics` | idem | Mém |
| `FIELD_ADDR` | 1 : POINTER vers STRUCT | POINTER | `FieldAccess` | nul ⇒ `PANIC` possible | Mém |
| `INDEX_ADDR` | 2 : POINTER/SLICE, BV | POINTER | — | hors bornes ⇒ `PANIC` | Havoc |
| `ADDRESS_OF` | 1 : valeur | POINTER | `MemorySemantics` | rend la valeur adressable (escape) | Mém |
| `MEMORY_BARRIER` | 0 | aucun | — | — | Mém (no-op) |
| `CALL` | `[callee, args…]` ; `callee` = valeur `GLOBAL` (fonction), valeur de type FUNCTION, ou récepteur interface | 0 ou 1 (tuple si multi-retours) | `CallSemantics` (obligatoire) | `may_raise`, `may_mutate_memory` | Havoc sauf `is_pure_proven` |
| `NEW_OBJECT` | 0 | POINTER/REFERENCE | `TypeTest.target_type_id` | — | Mém |
| `TYPE_TEST` | 1 : INTERFACE | BOOL (ou TUPLE(T,BOOL) si `comma_ok`) | `TypeTest` | — | Havoc |
| `DOWNCAST` | 1 | type cible | `TypeTest` | échec ⇒ `PANIC` possible | Havoc |
| `MAKE_INTERFACE` | 1 | INTERFACE | `TypeTest.target_type_id` | — | Havoc (sorte non interprétée) |
| `MAKE_CLOSURE` | 1+ : fonction, captures | FUNCTION | — | — | Havoc |
| `DEFER_PUSH` | `[callee, args…]` | aucun | `CallSemantics` | empile un appel différé | Havoc + diag `DEFER_UNMODELED` |
| `RUN_DEFERS` | 0 | aucun | — | exécute les appels différés | Havoc + diag `DEFER_UNMODELED` |
| `RECOVER` | 0 | INTERFACE | — | — | Havoc + diag `RECOVER_UNMODELED` |
| `EMIT_EFFECT` | n | aucun | `semantic_extension_id` | effet observable modélisé | Havoc |
| `UNKNOWN_EFFECT` | n | aucun | `OpaqueSemantics` | effet inconnu | Havoc mémoire |
| `OPAQUE_OP` | n | 0 ou 1 | `OpaqueSemantics` (obligatoire) | selon `may_mutate_memory`, `may_exit_abnormally` | Havoc |
| `SPAWN` | `[callee, args…]` | aucun | `CallSemantics` | concurrence | Havoc + diag `CONCURRENCY_PARTIAL` |
| `JOIN` | n | aucun | — | — | Havoc + diag |
| `CHANNEL_SEND` | 2 | aucun | — | peut bloquer | Havoc + diag |
| `CHANNEL_RECV` | 1 | T (ou TUPLE(T,BOOL)) | — | peut bloquer | Havoc + diag |
| `LOCK` `UNLOCK` | 1 | aucun | — | — | Havoc + diag |
| **Terminateurs** | | | | | |
| `RETURN` | 0..n (valeurs retournées, dans l'ordre) | — | — | exit | Ctl |
| `JUMP` | 0 | — | — | 1 arête `UNCONDITIONAL` | Ctl |
| `BRANCH` | 1 : BOOL | — | — | 2 arêtes `BRANCH_TRUE`(ord 0), `BRANCH_FALSE`(ord 1) | Ctl |
| `SWITCH` | 1 : BV (tag) | — | — | ≥ 2 arêtes, §4.7 | Ctl |
| `INVOKE` | `[callee, args…]` | 0 ou 1 | `CallSemantics` | ≥2 arêtes : `NORMAL`(0), `EXCEPTION`(1..n) avec filtres/ordre ; v0.1 fixture simple à 2 | Havoc + Ctl |
| `THROW` | 1 | — | — | exit exceptionnel | Ctl |
| `PANIC` | 1 | — | — | exit anormal (Go `panic`) | Ctl |
| `UNREACHABLE` | 0 | — | — | exit : chemin interdit par le compilateur | Ctl |
| `RESUME` `SUSPEND` | — | — | — | **réservés** (v0.6) ⇒ `E_UNKNOWN_OPCODE` tant qu'absents du registre | — |

**Note sur `CONST` :** il n'existe **pas** d'instruction `CONST`. Une constante est un `ValueDef` de rôle `CONST` porteur d'un `LiteralValue` (§4.4). `CAST` n'existe pas non plus : utiliser `SIGN_EXTEND`/`ZERO_EXTEND`/`TRUNCATE`/`BITCAST` ; une conversion flottante est un `OPAQUE_OP`.

**Opcodes de comparaison et d'arithmétique, seuls noms valides** : `ADD SUB MUL DIV REM NEG EQ NE LT LE GT GE`. Les noms `CMP_*`, `COMPARE`, `BINARY` n'existent pas dans le registre (ils peuvent apparaître dans des tables de *correspondance* d'adaptateur, jamais dans un `InstructionDef.opcode`).

## 4.4. Valeurs et constantes

`ValueDef.role` ∈ {`PARAM`, `CONST`, `SSA`, `GLOBAL`, `MEMORY`}.
- `CONST` : porte `literal`. Les constantes sont dédupliquées **par fonction** (même type + mêmes bits ⇒ une seule `ValueDef`).
- `GLOBAL` : porte `symbol_id` (symbole global ou fonction). Le type est un `POINTER` vers le type de la variable (comme en SSA Go : la variable globale est une *adresse*) ; pour une fonction, un type `FUNCTION`.
- `SSA` : résultat d'**exactement une** instruction.
- `MEMORY` : réservé aux objets abstraits.

**Encodage `LiteralValue`.** `machine_bits_be` contient le motif binaire exact en **big-endian**, de longueur `⌈bit_width/8⌉` octets, bits de poids fort inutilisés à 0 ; une constante signée se lit en complément à deux sur `bit_width` bits. Exemples (type `int` 64 bits) : `0` → `00 00 00 00 00 00 00 00` ; `-1` → `ff ff ff ff ff ff ff ff` ; `-10` → `ff ff ff ff ff ff ff f6`. Pour `BOOL` : `bool_value`. Pour un pointeur/interface nul : `null_value = true`. Une chaîne : `string_bytes` (octets arbitraires, pas forcément UTF-8). Un littéral dont la longueur ne correspond pas au type ⇒ `E_LITERAL_WIDTH`.

## 4.5. Inventaires source et IR

Deux inventaires **distincts**, reliés par une table de correspondance plusieurs-à-plusieurs :

- `source_decisions` + `source_outcomes` : construction source, avec span et type (`kind`). Un outcome source peut être `elided` (supprimé par l'abaissement : ex. `if false {…}` ; voir §9.4) ; il reste dans l'inventaire.
- `ir_decisions` + `ir_outcomes` : **dérivés par le cœur** (jamais envoyés par l'adaptateur) selon la règle : *tout terminateur ayant ≥ 2 arêtes sortantes est une IR decision ; chaque arête est une IR outcome ; `ordinal` et `label` viennent de l'arête (`outcome_ordinal`, `outcome_label`) ; `kind` = opcode du terminateur (`BRANCH`, `SWITCH`, `INVOKE`)*.
- `source_ir_links` : relie un **source outcome** à une **arête** (donc à une IR outcome) avec `mapping_kind`, `quality`, `inverted`.

**Valeurs de `kind` de source decision** (ensemble ouvert, préfixer `X_` pour un kind propre à un langage) : `IF`, `LOGICAL_OPERAND` (opérande de `&&`/`||`), `SWITCH`, `TYPE_SWITCH`, `SELECT`, `LOOP_CONDITION`, `RANGE_ITERATION`, `MATCH`, `TERNARY`, `TRY_CATCH`, `ERROR_PROPAGATION`.

**Valeurs de `label` d'un source outcome :** `TRUE`, `FALSE`, `CASE:<n>` (n = 0..), `DEFAULT`, `ENTER_BODY`, `EXIT`, `SUCCESS`, `ERROR`, `PANIC`, `EXCEPTION`.

**`mapping_kind` (provenance uniquement, §4.5.1)** : `DIRECT` (1 outcome source ↔ 1 arête), `SHORT_CIRCUIT` (opérande de `&&`/`||`), `SWITCH_CASE` (cas d'un `switch` abaissé en chaîne de `BRANCH`), `LOOP_CONDITION`, `IMPLICIT`. **`quality`** : `EXACT`, `APPROX` (correspondance déduite par position), `HEURISTIC` (devinée ; interdit de conclure sur ce seul lien).

**`inverted`** : `1` si la polarité est inversée (ex. `if !enabled { … }` : l'outcome source `TRUE` correspond à l'arête `BRANCH_FALSE` de `enabled`).

Cas obligatoire : `a && b` préserve l'évaluation court-circuitée (voir §9.2 pour le comptage exact des décisions). Un `switch` peut avoir plus de deux issues ; `panic`, `defer`, `recover` sont **indiqués** (même non résolus).

### 4.5.1. Cas d’activation d’une issue source (v1.2 — normatif)

Les liens `source_ir_links` conservent la **provenance**, mais ne définissent PAS une formule logique complète. En particulier, **il est interdit** de calculer l’atteignabilité d’une issue source par un `MAX`/`MIN` des statuts de ses seules arêtes IR liées.

Une issue source non supprimée peut posséder une **formule d’activation DNF**, composée de plusieurs cas alternatifs (OU), chaque cas comportant une séquence ordonnée d’issues IR (ET, dans l’ordre d’exécution). Pour les sources dont l’abaissement est direct, un cas à un terme suffit. Pour `a || b` et `a && b` :

| Source | Issue | Cas d’activation (arêtes évaluées ; ordre significatif) |
|---|---|---|
| `if a || b` | TRUE | `[a.TRUE]` **OU** `[a.FALSE, b.TRUE]` |
| `if a || b` | FALSE | `[a.FALSE, b.FALSE]` |
| `if a && b` | TRUE | `[a.TRUE, b.TRUE]` |
| `if a && b` | FALSE | `[a.FALSE]` **OU** `[a.TRUE, b.FALSE]` |
| `if a || (b && c)` | TRUE | `[a.TRUE]` **OU** `[a.FALSE,b.TRUE,c.TRUE]` |
| `if a || (b && c)` | FALSE | `[a.FALSE,b.FALSE]` **OU** `[a.FALSE,b.TRUE,c.FALSE]` |

Un terme signifie « cette issue IR a été franchie **à cette occurrence de la décision source** » : ne pas considérer une ancienne traversée de boucle comme une occurrence courante. La provenance source doit permettre d’identifier les instances de décision dans l’historique ; si l’appariement est ambigu, `UNKNOWN`, jamais `PROVEN_UNREACHABLE`.

**Contrat :**
1. `SourceOutcomeActivationCaseDef` (§5.1) décrit une alternative avec `case_ordinal`, `ordered_control_edge_ids` et `quality` ; les lignes SQL correspondantes se trouvent dans `source_outcome_activation_cases` et `source_outcome_activation_terms` (§8).
2. `source_outcomes.activation_mapping_complete=1` signifie que l’adaptateur **atteste** avoir énuméré toutes les alternatives d’activation dans sa configuration ; le cœur vérifie les références, les ordres, les cas non vides et la cohérence des liens, mais **ne peut garantir indépendamment** l’exhaustivité sémantique de cette attestation. Toute conclusion d’inaccessibilité fondée sur l’attestation inclut `ADAPTER_SOURCE_MAPPING_TRUSTED` dans les hypothèses.
3. Un mapping `APPROX` ou `HEURISTIC`, une issue `elided`, un cas manquant ou un drapeau incomplet **ne peuvent pas** autoriser `PROVEN_UNREACHABLE` pour une issue source. Ils conduisent à `UNKNOWN` ou `UNANALYZED` (si `elided`), en conservant la provenance.
4. En v0.3, les cas sont évalués à partir des **historiques ordonnés des états symboliques** et de leurs contraintes conjointes. L'atteignabilité d’une issue source exige une arrivée satisfaisable sur **un cas complet**, et non la satisfaisabilité séparée de chacune de ses arêtes. L’inaccessibilité exige une exploration fermée, tous les cas impossibles dans le modèle, des mappings `EXACT`/complets et des preuves de modèle **reconstructibles à partir des requêtes et des arrivées du run**. En v0.3 les issues source sont dérivées à la lecture et n’ont pas leur propre `proof_id` persistant : si l’évidence vérifiable n’est pas disponible, répondre `UNKNOWN`. Si aucun certificat de cas n’est disponible, retourner `UNKNOWN` plutôt qu’inventer une preuve.
5. Les expressions booléennes mixtes, négations, `switch`, cas joints et évaluations court-circuitées DOIVENT préserver l’ordre de sélection et la non-évaluation des opérandes ignorés. Les fixtures couvrent les deux polarités de `&&` et `||`, ainsi qu’une expression mixte.

**Explosion combinatoire des formules d’activation :** pour des expressions très imbriquées, la DNF peut elle-même croître exponentiellement. En v0.2, plafonner la matérialisation des cas (`max_source_activation_cases`, défaut 4096 par issue). Dès que cette limite est atteinte, conserver **toutes les décisions et issues de l’inventaire**, mais marquer `activation_mapping_complete=false`, produire `SOURCE_ACTIVATION_INCOMPLETE` et ne jamais prouver l’inaccessibilité de l’issue source. Une version ultérieure pourra remplacer les cas DNF par un DAG de décision partagé sans réécrire le modèle de branches.

**Important :** la formule en cas ordonnés est une **représentation d’événements de contrôle**, pas une expression SMT universelle ni une affirmation que toute combinaison de termes est réalisable. La faisabilité reste décidée en présence des contraintes accumulées.

## 4.6. Identité et provenance (**normatif**)

**Tous les identifiants d'un flux (`id`, `*_id`) sont locaux au flux** : aucune référence n'est autorisée vers un autre flux (autre unité). Le cœur calcule la clé stockée `local_key = unit_key + "|" + id`. Conséquences :
- `unit_key` est **unique dans le snapshot** ; `local_key` est unique par snapshot.
- Un callee non extrait (autre unité, bibliothèque standard, dépendance) est déclaré par l'adaptateur comme **stub externe** (`FunctionDef.is_external = true`, symbole avec `qualified_name`, aucun bloc) **dans l'unité de l'appelant**. À la clôture du snapshot, le cœur lie chaque stub à la fonction réelle de même `(language, qualified_name)` d'une autre unité si elle existe (`functions.resolved_function_id`) ; sinon le stub reste externe.

**Formes d'identifiants imposées aux adaptateurs** (déterministes, pour que les golden tests soient stables) :

| Objet | Forme | Exemple |
|---|---|---|
| type | descripteur canonique | `go:int@amd64`, `go:bool`, `go:error` |
| symbole | `qualified_name` (+ `#<kind>` si collision) | `example.Decide` |
| fonction | = id du symbole | `example.Decide` |
| bloc | `<function_id>/b<ordinal>` | `example.Decide/b0` |
| instruction | `<block_id>/i<ordinal>` | `example.Decide/b0/i1` |
| valeur SSA/param | `<function_id>/v<n>` (n = ordre de définition, 0..) | `example.Decide/v0` |
| globale | `global:<qualified_name>` (locale au flux ; l'identité stockée inclut `unit_key`) | `global:example.Hook` |
| constante | `<function_id>/k<n>` | `example.Decide/k0` |
| arête | `<terminator_id>/e<ordinal>` | `example.Decide/b0/i1/e0` |
| source decision | `<function_id>/sd<n>` (n = rang après tri par **(ligne de début, colonne de début, fin décroissante, kind)** ; ainsi un `IF` englobant précède ses `LOGICAL_OPERAND`) | `example.Decide/sd0` |
| source outcome | `<decision_id>/o<ordinal>` | `example.Decide/sd0/o0` |

Un objet CIR porte (au niveau stockage) : `snapshot_id`, `unit_id`, `local_key`, spans éventuels, `adapter_version` et `semantic_profile_hash` (via l'unité). Un `structural_fingerprint` (sha256 hex de la sérialisation canonique de la fonction : ses blocs, instructions, opérandes et arêtes, **sans** les ids de flux ni les spans) sert à la correspondance/invalidation. Ne pas dériver l'unicité d'un hash du texte source.

## 4.7. Arêtes de contrôle et gardes (**normatif**)

`edge_kind` ∈ {`UNCONDITIONAL`, `BRANCH_TRUE`, `BRANCH_FALSE`, `SWITCH_CASE`, `SWITCH_DEFAULT`, `NORMAL`, `EXCEPTION`}. Toute autre valeur ⇒ `E_UNKNOWN_KIND`.

| Terminateur | Arêtes | `outcome_label` | Garde (calculée par le moteur, jamais stockée en SMT) |
|---|---|---|---|
| `JUMP` | 1 `UNCONDITIONAL` (ord 0) | `NEXT` | vrai |
| `BRANCH` | ord 0 `BRANCH_TRUE`, ord 1 `BRANCH_FALSE` | `TRUE`, `FALSE` | `guard_value_id` = opérande 0 ; la 2ᵉ arête a `guard_negated = true` |
| `SWITCH` | ord 0..n−2 `SWITCH_CASE` (chacune avec ≥ 1 `case_value_ids`, constantes du type du tag, **distinctes entre arêtes**), puis ord n−1 `SWITCH_DEFAULT` (exactement une, dernière) | `CASE:<ord>`, `DEFAULT` | `SWITCH_CASE` : `OR_i EQ(tag, c_i)` ; `SWITCH_DEFAULT` : `AND_{toutes arêtes, tous c} NE(tag, c)` ; pas de `guard_value_id` |
| `INVOKE` | ord 0 `NORMAL`, puis ord 1..n `EXCEPTION` (≥1) | `NORMAL`, `EXCEPTION` | `NORMAL` : issue normale ; exception : dépend de `may_raise`, du filtre de type et de la priorité des handlers ; si la sémantique des exceptions est inconnue, garder toutes les issues exceptionnelles possibles (surapproximation), jamais les prouver impossibles |
| `RETURN` `THROW` `PANIC` `UNREACHABLE` | 0 arête | — | — |

Le Go ne produit **pas** de terminateur `SWITCH` (le `switch` est abaissé en chaîne de `BRANCH` par `go/ssa`) ; `SWITCH` et `INVOKE` servent à Java/Rust/autres langages et **doivent** néanmoins être implémentés et testés dès la v0.1 avec une fixture CIR dédiée (`switch3.cir.pb`, §15.2).

**Exceptions multi-gestionnaires (v1.2).** Pour `INVOKE`, une arête exceptionnelle peut porter `exception_filter_type_id` et `exception_catch_all`. L’ordre des arêtes exceptionnelles représente l’ordre de sélection du runtime (notamment l’ordre des handlers Java). Les gardes sont mutuellement exclusives **seulement si** le dispatch d’exception et les sous-types sont connus ; sinon, conserver une surapproximation et `EXCEPTION_FLOW_PARTIAL`. `finally`, `defer` et les nettoyages Rust doivent être représentés comme chemins de contrôle/effets distincts ; aucune sortie implicitement omise n’autorise une conclusion `COMPLETE`. Test obligatoire en v0.6 avec plusieurs `catch`, `finally` et propagation non interceptée. En v0.3, une arête `EXCEPTION` de type inconnu n’est pas éliminée sur son filtre.

## 4.8. Invariants à vérifier à l'import (liste exhaustive, code d'erreur associé)

| Invariant | Erreur |
|---|---|
| Une `ValueDef` de rôle `SSA` est le résultat d'**exactement une** instruction de sa fonction : ≥ 2 définitions, ou `result_value_id` désignant une valeur non `SSA` ou d'une autre fonction ⇒ `E_SSA_MULTI_DEF` ; 0 définition ⇒ `E_SSA_DOMINANCE` (valeur non définie) | `E_SSA_MULTI_DEF`, `E_SSA_DOMINANCE` |
| Toute valeur utilisée est définie ; la définition **domine** l'usage (sauf entrées de `PHI`, où elle doit dominer la **fin du bloc prédécesseur**) ; `PARAM`, `CONST`, `GLOBAL` dominent tout | `E_SSA_DOMINANCE` |
| Chaque `PHI` a une entrée par prédécesseur du bloc, ni plus ni moins, sans doublon de prédécesseur ; les `PHI` sont groupés en tête de bloc ; types des entrées = type du résultat | `E_PHI_PRED_MISMATCH` |
| Un bloc a exactement un terminateur, c'est sa dernière instruction (`is_terminator=true` ⇔ opcode terminateur) | `E_NO_TERMINATOR`, `E_MULTI_TERMINATOR`, `E_TERMINATOR_NOT_LAST` |
| Arêtes : source = bloc du terminateur ; cible dans la **même fonction** ; nombre/types d'arêtes conformes à §4.7 | `E_EDGE_SOURCE_MISMATCH`, `E_EDGE_CROSS_FUNCTION`, `E_EDGE_SHAPE` |
| Arité et types des opérandes conformes au registre §4.3 ; pour `RETURN`, nombre et types des opérandes = `FunctionDef.return_type_ids` | `E_ARITY_MISMATCH`, `E_TYPE_MISMATCH` |
| Payload requis présent, du bon type de payload | `E_PAYLOAD_MISSING`, `E_PAYLOAD_KIND` |
| `entry_block_id` est un bloc de la fonction ; fonction non externe ⇒ ≥ 1 bloc, **sauf** si la fonction porte `FUNCTION_TRANSLATION_FAILED` (`inventory_status=FAILED`, 0 bloc, `entry_block_id` absent, exclue de l’analyse) ; externe ⇒ 0 bloc | `E_ENTRY_BLOCK` |
| Tous les blocs atteignables depuis l'entrée ne sont **pas** exigés (les blocs morts sont permis et inventoriés) | — |

L'algorithme de dominance **DOIT** être celui de Cooper-Harvey-Kennedy (itératif) ou Lengauer-Tarjan, implémenté dans `ske-graph` et testé sur : (a) diamant, (b) boucle simple, (c) boucle imbriquée, (d) graphe irréductible, (e) bloc inatteignable.

## 4.9. Arithmétique machine : vecteurs de test **obligatoires**

Le module `ske-cir::bitvec` implémente `BitVec { width: u32, bits: u128 }` (largeurs 1..=128) avec les opérations du registre. Les tests unitaires **DOIVENT** contenir au moins ces vecteurs (notation : `iN` signé, `uN` non signé, valeurs en décimal) :

| Opération | Entrées | Résultat attendu |
|---|---|---|
| `ADD` i8 | 127 + 1 | −128 |
| `ADD` u8 | 200 + 100 | 44 |
| `SUB` u8 | 0 − 1 | 255 |
| `MUL` i8 | 64 × 2 | −128 |
| `MUL` u8 | 16 × 16 | 0 |
| `NEG` i8 | −(−128) | −128 |
| `DIV` i8 | −128 / −1 | −128 |
| `REM` i8 | −128 % −1 | 0 |
| `DIV` i8 | −7 / 2 | −3 |
| `REM` i8 | −7 % 2 | −1 |
| `DIV` i8 | 7 / −2 | −3 |
| `REM` i8 | 7 % −2 | 1 |
| `DIV` i8 | 5 / 0 | **erreur `DivByZero`** (pas un résultat) |
| `SHL` i8 | 1 << 7 | −128 |
| `SHL` i8 | 1 << 8 | 0 |
| `SHR` i8 (arith.) | −128 >> 7 | −1 |
| `SHR` i8 (arith.) | −128 >> 100 | −1 |
| `SHR` u8 (logique) | 128 >> 7 | 1 |
| `SHR` u8 (logique) | 128 >> 8 | 0 |
| shift, compte i8 = −1 | tout `<<` | **erreur `NegativeShift`** |
| `SHL` u8, compte u16 | 1 << 256 | 0 (le compte est comparé à la largeur **avant** toute troncature : jamais `1 << (256 mod 2⁸)` = 1) |
| `LT` i8 | −1 < 1 | vrai |
| `LT` u8 | 255 < 1 | faux |
| `SIGN_EXTEND` i8→i16 | −1 | −1 (0xFFFF) |
| `ZERO_EXTEND` u8→u16 | 255 | 255 |
| `TRUNCATE` i16→i8 | 300 | 44 |
| `BITCAST` i8→u8 | −1 | 255 |

## 4.10. Exemple de normalisation (Go → CIR)

Source (`examples/go-branch-lab/adjusted.go`) :

```go
func Adjusted(x int) int {
	y := 0
	if x > 10 {
		y = 3
	}
	return x - y
}
```

CIR lisible (illustration ; la sérialisation de référence est le protobuf §5) :

```text
func example.Adjusted(x: go:int@amd64) -> go:int@amd64
  consts: k0 = 0, k10 = 10, k3 = 3
b0:
  v1 = GT x, k10                      ; bool
  BRANCH v1 -> b1 [TRUE], b2 [FALSE]
b1:
  JUMP -> b2 [NEXT]
b2:
  v2 = PHI (b0: k0), (b1: k3)
  v3 = SUB x, v2                      ; overflow_mode=WRAP
  RETURN v3
```

Invariant SSA : un résultat défini une fois ; chaque `PHI` a un prédécesseur par entrée ; les définitions dominent leurs usages (hors `PHI`). Les noms `k0`, `k10`, `k3` de cette illustration sont des alias de lecture ; les identifiants sérialisés suivent §4.6 et §15.2.1 (ici `example.Adjusted/k0`, `/k1`, `/k2`).

---

# 5. Protocole d'adaptateur v1 et import

## 5.1. `schemas/cir/v1/cir.proto` (**normatif** — à copier tel quel)

```proto
syntax = "proto3";
package ske.cir.v1;

// Position source. start_line/start_column : 1-based. end_column est EXCLUSIF
// (premier caractère après le span). Les colonnes comptent des OCTETS (une
// tabulation vaut 1).
message Span {
  string path = 1;                  // relatif à la racine du projet, '/' comme séparateur
  uint32 start_line = 2;
  uint32 start_column = 3;
  uint32 end_line = 4;
  uint32 end_column = 5;
}

message Header {
  uint32 cir_major = 1;             // doit valoir 1
  uint32 cir_minor = 2;             // valeur de référence = 3 (inchangée en v1.4)
  string adapter_name = 3;
  string adapter_version = 4;
  string language = 5;              // "go", "rust", "java"
  string toolchain_version = 6;
  string build_fingerprint = 7;     // sha256 hex de la configuration de build
  string semantic_profile = 8;      // ex. "go-1.22-linux-amd64"
  repeated string capability_flags = 9;
}

message UnitDef {
  string unit_id = 1;
  string unit_key = 2;              // unique dans le snapshot
  string language = 3;
}

message SourceFileDef {
  string path = 1;
  string language = 2;
  string content_sha256 = 3;        // sha256 hex minuscule du contenu exact
}

message LiteralValue {
  oneof representation {
    bool bool_value = 1;
    bytes machine_bits_be = 2;      // motif binaire big-endian, ceil(width/8) octets
    bytes string_bytes = 3;         // octets arbitraires (pas forcément UTF-8)
    bool null_value = 4;            // uniquement à true
  }
}

message PhiInput {
  string predecessor_block_id = 1;
  string value_id = 2;
}

message BinarySemantics {
  string operator = 1;              // nom d'opcode du registre (informatif)
  string overflow_mode = 2;         // WRAP | CHECKED | SATURATE | UNDEFINED
  string signedness = 3;            // SIGNED | UNSIGNED | N_A
}

message CallSemantics {
  string dispatch_kind = 1;         // STATIC | VIRTUAL | INTERFACE | TRAIT | REFLECTIVE | DYNAMIC | UNKNOWN
  optional bool may_raise = 2;
  optional bool may_mutate_memory = 3;
  bool is_pure_proven = 4;          // vrai seulement si PROUVÉ pur (jamais par défaut)
  bool world_closed = 5;            // vrai seulement si l'ensemble des cibles est prouvé fermé
  bool is_referentially_transparent_proven = 6; // mêmes arguments + même état immuable => même résultat, sans effets ni exceptions
}

message MemorySemantics {
  string access_kind = 1;           // LOAD | STORE | ALLOC | ADDRESS_OF
  string address_space = 2;
  string alias_class = 3;
}

message OpaqueSemantics {
  string source_construct = 1;
  string reason = 2;
  optional bool may_mutate_memory = 3;
  optional bool may_exit_abnormally = 4;
}

message FieldAccess {
  uint32 field_index = 1;
  string field_name = 2;
}

message TypeTest {
  string target_type_id = 1;
  bool comma_ok = 2;
}

message OperationPayload {
  oneof details {
    BinarySemantics binary = 1;
    CallSemantics call = 2;
    MemorySemantics memory = 3;
    OpaqueSemantics opaque = 4;
    FieldAccess field = 7;
    TypeTest type_test = 8;
  }
  repeated PhiInput phi_inputs = 5;
  string semantic_extension_id = 6;
}

message TypeDef {
  string id = 1;
  string kind = 2;                  // voir §4.2 ; inconnu => E_UNKNOWN_KIND
  string canonical_descriptor = 3;
  repeated string param_type_ids = 4;
  uint32 bit_width = 5;
  bool is_signed = 6;
  bytes extra = 7;
  repeated string component_names = 8;
  bool nullable = 9;
  repeated string result_type_ids = 10;
}

message SymbolDef {
  string id = 1;
  string owner_id = 2;              // symbole propriétaire (vide si racine)
  string name = 3;
  string qualified_name = 4;
  string kind = 5;                  // PACKAGE | FUNCTION | METHOD | GLOBAL | TYPE | CONSTANT | OTHER
  string type_id = 6;               // vide si sans type (PACKAGE)
  repeated Span spans = 7;          // spans[0] = span primaire
}

message ValueDef {
  string id = 1;
  string function_id = 2;           // vide pour une valeur GLOBAL
  string type_id = 3;
  string role = 4;                  // PARAM | CONST | SSA | GLOBAL | MEMORY
  LiteralValue literal = 5;         // seulement si role = CONST
  string symbol_id = 6;             // seulement si role = GLOBAL
  string name = 7;                  // nom source d'un PARAM (informatif ; sert au rendu des gardes)
  optional uint32 param_index = 8;  // PARAM seulement ; présence obligatoire, 0 autorisé
}

message FunctionDef {
  string id = 1;
  string symbol_id = 2;
  repeated string parameter_value_ids = 3;
  repeated string return_type_ids = 4;
  string entry_block_id = 5;        // vide ssi is_external
  bool is_external = 6;
}

message BlockDef {
  string id = 1;
  string function_id = 2;
  uint32 ordinal = 3;
}

message InstructionDef {
  string id = 1;
  string block_id = 2;
  uint32 ordinal = 3;
  string opcode = 4;
  string result_value_id = 5;
  repeated string operand_value_ids = 6;
  OperationPayload payload = 7;
  repeated Span spans = 8;          // spans[0] = span primaire
  bool is_terminator = 9;
}

message ControlEdgeDef {
  string id = 1;
  string source_block_id = 2;
  string target_block_id = 3;
  string terminator_instruction_id = 4;
  uint32 outcome_ordinal = 5;
  string outcome_label = 6;         // voir §4.7
  string guard_value_id = 7;        // BRANCH uniquement
  bool guard_negated = 8;
  string edge_kind = 9;             // voir §4.7
  repeated string case_value_ids = 10; // SWITCH_CASE uniquement
  string exception_filter_type_id = 11; // EXCEPTION : type catch (vide = inconnu ou catch-all avec drapeau)
  bool exception_catch_all = 12;    // EXCEPTION : garde finale couvrant le reste des exceptions
}

message SourceOutcomeDef {
  string id = 1;
  string label = 2;                 // voir §4.5
  bool elided = 3;                  // supprimé par l'abaissement (pas d'arête IR)
  bool activation_mapping_complete = 4; // attestation pour TOUS les cas d’activation
}

message SourceDecisionDef {
  string id = 1;
  string decision_kind = 2;
  Span span = 3;                    // span de la CONDITION (voir §9.2 par kind)
  string function_id = 4;           // vide si hors fonction
  string parent_decision_id = 5;    // pour LOGICAL_OPERAND : l'IF/boucle englobant
  repeated SourceOutcomeDef outcomes = 6;
}

message SourceIrLinkDef {
  string source_outcome_id = 1;
  string control_edge_id = 2;
  string mapping_kind = 3;          // DIRECT | SHORT_CIRCUIT | SWITCH_CASE | LOOP_CONDITION | IMPLICIT
  string quality = 4;               // EXACT | APPROX | HEURISTIC
  bool inverted = 5;
}

// Alternative d’activation d’une source outcome : OU entre les cas, ET ordonné entre les arêtes d’un cas.
// `ordered_control_edge_ids` sont des IDs du MÊME flux/unité.
message SourceOutcomeActivationCaseDef {
  string source_outcome_id = 1;
  uint32 case_ordinal = 2;
  repeated string ordered_control_edge_ids = 3;
  string quality = 4;                 // EXACT | APPROX | HEURISTIC
}

message ImplicitExitSiteDef {
  string id = 1;
  string function_id = 2;
  Span span = 3;
  string exit_kind = 4;
  string modeling_status = 5; // MODELED | UNMODELED | UNKNOWN
}

message CallTargetDef {
  string call_instruction_id = 1;
  string target_function_id = 2;    // fonction réelle OU stub externe du même flux
  string resolution_kind = 3;       // STATIC | CHA | RTA | VTA | MANUAL
}

message Diagnostic {
  string severity = 1;              // INFO | WARNING | ERROR
  string code = 2;                  // voir §5.6
  string message = 3;
  Span span = 4;
  string affected_function_id = 5;
  string impact = 6;                // INVENTORY | SYMBOLIC | EFFECTS | CALLGRAPH | TYPES | CONCURRENCY | OTHER
}

message EndUnit {
  string unit_id = 1;
  string inventory_status = 2;      // avis de l'adaptateur : COMPLETE | PARTIAL | FAILED (le cœur recalcule)
  uint64 expected_source_decisions = 3; // compté INDÉPENDAMMENT dans l'AST
  uint64 emitted_source_decisions = 4;  // nombre de SourceDecisionDef émis
  uint64 emitted_ir_edges = 5;          // nombre de ControlEdgeDef émis
}

message Record {
  oneof content {
    Header header = 1;
    TypeDef type_def = 2;
    SymbolDef symbol_def = 3;
    ValueDef value_def = 4;
    FunctionDef function_def = 5;
    BlockDef block_def = 6;
    InstructionDef instruction_def = 7;
    ControlEdgeDef control_edge_def = 8;
    SourceDecisionDef source_decision_def = 9;
    SourceIrLinkDef source_ir_link = 10;
    CallTargetDef call_target_def = 11;
    Diagnostic diagnostic = 12;
    EndUnit end_unit = 13;
    UnitDef unit_def = 14;
    SourceFileDef source_file_def = 15;
    SourceOutcomeActivationCaseDef source_outcome_activation_case = 16;
    ImplicitExitSiteDef implicit_exit_site = 17;
  }
}
```

## 5.2. `schemas/adapter/v1/adapter.proto` (**normatif**)

```proto
syntax = "proto3";
package ske.adapter.v1;
import "cir/v1/cir.proto";

message Limits {
  uint64 max_message_bytes = 1;     // défaut 16777216
  uint64 max_messages = 2;          // défaut 50000000
  uint64 timeout_ms = 3;            // défaut 600000
  uint64 max_memory_mb = 4;         // défaut 4096
}

message ListUnitsRequest {
  string project_root = 1;          // chemin absolu
  string build_profile_json = 2;    // contenu de build-profile.json (§11.4)
  Limits limits = 3;
}

message ExtractUnitRequest {
  string project_root = 1;
  string build_profile_json = 2;
  string unit_key = 3;
  Limits limits = 4;
}

message AdapterRequest {
  uint32 protocol_major = 1;        // 1
  oneof body {
    ListUnitsRequest list_units = 2;
    ExtractUnitRequest extract_unit = 3;
  }
}

message UnitDescriptor {
  string unit_key = 1;
  string language = 2;
  repeated string source_paths = 3; // fichiers sources de l'unité (relatifs)
}

message ListUnitsDone {}

message AdapterError {
  string code = 1;                  // ex. TOOLCHAIN_MISSING, BUILD_PROFILE_INVALID
  string message = 2;
}

message AdapterResponse {
  oneof body {
    UnitDescriptor unit = 1;        // réponse à list_units (0..n)
    ListUnitsDone list_done = 2;    // termine list_units
    ske.cir.v1.Record record = 3;   // réponse à extract_unit (Header en premier … EndUnit en dernier)
    AdapterError error = 4;         // en cas d'échec ; puis le processus sort avec le code 1
  }
}
```

**Séquence :** le cœur lance l'adaptateur, envoie **un** `AdapterRequest`, ferme `stdin`, lit les `AdapterResponse` jusqu'à EOF. Pour `list_units` : 0..n `unit` puis `list_done`. Pour `extract_unit` : des `record` (`Header` d'abord, `UnitDef` ensuite, `EndUnit` en dernier). L'adaptateur sort avec le code `0` en cas de succès. `AdapterError` ⇒ l'unité est `FAILED`.

Pour les fichiers `*.cir.pb` (fixtures) : suite de `Record` encadrés (pas d'`AdapterResponse`).

## 5.3. Conventions normatives du flux d'unité

1. Un flux = **une** unité de compilation : `Header`, `UnitDef`, puis des déclarations dans **n'importe quel ordre**, puis `EndUnit`. Tout autre ordre pour ces trois ⇒ `E_HEADER_MISSING` / `E_RECORD_ORDER` / `E_END_UNIT_MISSING`. Un record après `EndUnit` ⇒ `E_RECORD_ORDER`.
2. **Références en avant autorisées** : toutes les références sont résolues à `EndUnit`.
3. Tous les `id` de définitions sont non vides (`E_EMPTY_ID`) et **uniques dans leur espace** (un espace par type de `Record` : types, symboles, valeurs, fonctions, blocs, instructions, arêtes, décisions source, issues source, sites de sortie implicite) (`E_DUPLICATE_ID`, détecté par la map d'ids **avant** toute insertion SQL). Une **référence** obligatoire vide est une référence pendante (`E_DANGLING_REF`), sauf `FunctionDef.entry_block_id` (règle `E_ENTRY_BLOCK`, §4.8).
4. Aucune référence vers un id d'un autre flux. Callees non extraits ⇒ **stub externe** dans le même flux (§4.6).
5. Les chemins (`Span.path`, `SourceFileDef.path`) sont **relatifs, normalisés** : séparateur `/`, pas de segment `.` ou `..`, pas de `/` initial, pas de `\`, pas de segment vide, pas d'octet NUL ; sinon `E_PATH_INVALID`. Tout `Span.path` doit correspondre à un `SourceFileDef` du flux (`E_DANGLING_REF`).
6. Un span est valide si `start_line ≥ 1`, `start_column ≥ 1`, `end_line ≥ start_line`, et (`end_line > start_line` ou `end_column ≥ start_column`) ; sinon `E_SPAN_INVALID`.
7. `PHI` : `phi_inputs` est la **seule** source des entrées (`operand_value_ids` vide).
8. `BRANCH` : `guard_value_id` = valeur de l'opérande 0 ; `SWITCH` : arêtes `SWITCH_CASE` avec `case_value_ids`.
9. `CALL`, `INVOKE`, `DEFER_PUSH` et `SPAWN` produisent chacun un `call_site` distinct lorsque le callee est présent dans la CIR. Les cibles viennent de `CallTargetDef` (0..n). Dériver `resolution_status` ainsi, dans l'ordre : aucune cible + callee GLOBAL stub externe ⇒ `EXTERNAL` ; aucune cible autrement ⇒ `UNRESOLVED` ; ≥1 cible, toutes stubs ⇒ `EXTERNAL` ; exactement une cible réelle et `dispatch_kind=STATIC` ET `world_closed=true` ⇒ `RESOLVED_UNIQUE` ; tous les autres cas ⇒ `RESOLVED_POSSIBLE`. Un `world_closed=false` interdit toujours de conclure à l'unicité exhaustive ; une résolution statique connue conserve son candidat même si l'appel est ouvert.
10. **Booléens conservateurs Protobuf v3** : `CallSemantics.may_raise`, `CallSemantics.may_mutate_memory`, `OpaqueSemantics.may_mutate_memory` et `OpaqueSemantics.may_exit_abnormally` sont `optional bool`. **Absent ⇒ `true` pour l'analyse**, présent `false` ⇒ assertion explicite de l'adaptateur à vérifier selon la politique de confiance du run. Le cœur ne doit JAMAIS lire `false` par défaut de proto3 comme preuve d'absence d'effet. `is_pure_proven`, `is_referentially_transparent_proven` et `world_closed` restent à `false` si absents. Le DDL conserve `may_raise/may_mutate_memory = 1` par défaut ; l'ingestion écrit la valeur effective, pas celle du getter brut. Toute assertion explicite `false` d'effet doit produire le diagnostic `ADAPTER_EFFECT_CLAIMS_TRUSTED` (INFO), avec identité du modèle ou justification.
11. Les `ControlEdgeDef` ne contiennent **pas** de formule SMT.
12. Mineur CIR de référence : **3** (champ protobuf numéro **2**). Le cœur v0.x supporte `cir_minor <= 3` ; un mineur > 3 ⇒ diagnostic `CIR_MINOR_NEWER` (INFO), sans échec, avec marquage conservateur des champs et fonctionnalités inconnus.
13. `SourceOutcomeActivationCaseDef` : `source_outcome_id` déclaré, `case_ordinal` unique et contigu dès 0 pour une issue, `ordered_control_edge_ids` non vide, arêtes de **même unité**, qualité autorisée ; incohérence ⇒ `E_SOURCE_ACTIVATION_INVALID`. Un `activation_mapping_complete=true` avec cas manquant ou non `EXACT` ⇒ même erreur à l'import. En cas de mapping partiel, laisser `activation_mapping_complete=false` et diagnostic `SOURCE_ACTIVATION_INCOMPLETE` sans inventer un cas.
14. `ImplicitExitSiteDef` : référence une fonction locale, span local connu ou absent, `modeling_status` autorisé ; le site reste dans l'inventaire même si `UNMODELED`.
15. `CallSemantics.is_referentially_transparent_proven=true` implique `is_pure_proven=true` ; si l'adaptateur ne peut démontrer cette propriété, mettre les deux drapeaux à `false` par défaut (aucune déduction par nom de méthode).
16. Pour les fixtures CIR sans expression composée, chaque issue source non `elided` fournit un cas direct exact (§15.2). Un `SourceOutcomeDef.activation_mapping_complete=false` n'empêche pas l'inventaire structurel, mais bloque l'inaccessibilité **source**.
17. L'`ordinal` d'une issue source (`source_outcomes.ordinal`) est son **rang** (0..) dans `SourceDecisionDef.outcomes` ; les adaptateurs de référence et les fixtures lui donnent l'id `<decision_id>/o<ordinal>` (§4.6).
18. Un même `SourceFileDef.path` peut apparaître dans plusieurs unités d'un snapshot (ex. variante `[test]` Go) : la ligne `source_files` est **partagée** (`UNIQUE(snapshot_id, path)`, insertion `ON CONFLICT DO NOTHING`) ; un `content_sha256` différent pour un chemin déjà présent ⇒ `E_SOURCE_HASH_MISMATCH`.


**Test obligatoire de défauts Protobuf (v0.1)** : construire `CallSemantics{}` (aucune présence sur les champs `optional`), le décoder en Go puis Rust, et vérifier `effective_may_raise == true`, `effective_may_mutate_memory == true` ; construire `OpaqueSemantics{}` et vérifier les deux `effective_* == true`. Puis envoyer une valeur explicitement `false` et vérifier qu’elle reste discernable de l’absence après aller-retour. Un `bool` absent ne doit jamais conduire à une preuve d’inaccessibilité d’une issue exceptionnelle ou mémoire. Test `proto_optional_effects_conservative`.

## 5.4. Procédure d'import (**normative** — dans `ske-ingest`)

**Transaction :** `ske ingest cir` démarre **un seul** `BEGIN IMMEDIATE` global ; après ce `BEGIN`, exécute `PRAGMA defer_foreign_keys=ON`, puis place chaque unité dans un `SAVEPOINT unit_<ordinal>` (le nom est généré par le cœur, jamais construit depuis l’entrée). `RELEASE SAVEPOINT` au succès ; `ROLLBACK TO` puis `ROLLBACK` global à l'échec. **Aucun `BEGIN` imbriqué.** `ske ingest go` utilise `BEGIN IMMEDIATE` distinct pour chaque unité et repose `PRAGMA defer_foreign_keys=ON` **après chacun de ces `BEGIN`**. Les étapes ci-dessous sont identiques dans la transaction ou savepoint approprié :

1. Lire les messages encadrés (4 octets BE + protobuf). Longueur > `max_message_bytes` ⇒ `E_MSG_TOO_LARGE` pour **fichier** `.cir.pb`, mais `E_ADAPTER_LIMIT` pour **stdout d’un adaptateur enfant** (même raison physique, contrat CLI distinct) ; décodage invalide ou flux tronqué ⇒ `E_PROTO_DECODE`.
2. Premier record = `Header` ; `cir_major != 1` ⇒ `E_CIR_MAJOR_UNSUPPORTED`.
3. Pour chaque record : valider localement (id non vide, `kind` connu, chemins, spans, longueur des littéraux), **dédupliquer les spans identiques** (`INSERT ... ON CONFLICT DO NOTHING`, puis SELECT de l'id), et transformer les contraintes SQLite connues en `E_*` stables (§5.5), **attribuer un identifiant entier** (compteur initialisé à `MAX(id)+1` de la table, valable car la transaction est exclusive), conserver `HashMap<String, i64>` par espace d'ids, insérer la ligne avec cet id. Une référence à un id pas encore vu réserve un entier dans la map (insertion ultérieure du `Def`). Mémoire : seules les maps d'ids sont gardées, jamais l'unité entière.
4. À `EndUnit` :
   a. **Références** : tout id référencé doit avoir été déclaré, sinon `E_DANGLING_REF` (message : espace + id + record source).
   b. **Validation par fonction** (lecture depuis SQLite, une fonction à la fois) : suivre STRICTEMENT cette précédence pour décider le **premier** code d'erreur : `LOCAL_FIELDS` (ids, opcodes, types primitifs, chemins/spans) → `REFERENCES` (arêtes/liens/activations) → `BLOCK_TERMINATORS` → `EDGE_SHAPE` → `ARITY_TYPE_PAYLOAD` → `SSA_DOMINANCE` (définitions uniques `E_SSA_MULTI_DEF`, puis dominance) → `PHI` → `ACTIVATION` → `COUNTERS`. Si une contrainte SQL se déclenche pendant le streaming avant son contrôle logique, le code retourné DOIT être mappé conformément à §5.5, et les fixtures négatives DOIVENT isoler une seule anomalie afin de produire leur erreur cible.
   c. **Dérivation** : persister dans `functions.return_type_ids` la liste ordonnée de `FunctionDef.return_type_ids` ; pour chaque `parameter_value_ids[i]`, vérifier la présence de `ValueDef.param_index=i`, la bijection index↔id et la continuité 0..n−1 (sinon `E_PARAM_ORDER`). Créer `ir_decisions`/`ir_outcomes` pour tout terminateur à ≥ 2 arêtes (§4.5) ; résoudre chaque `SourceIrLinkDef` (arête → `ir_outcome_id`) et **chaque `SourceOutcomeActivationCaseDef`** (arêtes → `source_outcome_activation_cases` et `source_outcome_activation_terms`, conserver l’ordre) ; importer `ImplicitExitSiteDef` dans `implicit_exit_sites` ; créer `call_sites` et `call_targets` (statut §5.3-9) ; poser `functions.inventory_status`. Un cas d’activation incomplet n’est pas promu en exact par le cœur.
   d. **Compteurs** : comparer `EndUnit` aux nombres réels (§3.5) ⇒ `E_COUNTER_MISMATCH` si `emitted_*` ≠ réel (l'écart `expected_source_decisions` ≠ réel donne `PARTIAL`, pas une erreur).
   e. Calculer `inventory_status` de l'unité (§3.5) et enregistrer la ligne `compilation_units`.
   f. `RELEASE SAVEPOINT` (`ingest cir`) ou `COMMIT` (`ingest go`).
5. Erreur à n'importe quelle étape ⇒ `ROLLBACK` de l'unité, code d'erreur `E_*` retourné.

**Cycle de vie du snapshot :**
- `ske ingest cir` (un ou plusieurs `--input`, une unité par fichier) : tout dans **une seule** transaction globale et un SAVEPOINT par unité (création du dépôt si besoin + snapshot `OPEN` + toutes les unités + clôture). Échec ⇒ rollback total ⇒ **aucune ligne** dans aucune table (`SELECT COUNT(*) FROM snapshots` = valeur d'avant).
- **Unité échouée avant `Header`** (`ske ingest go` uniquement ; avec `ske ingest cir`, toute erreur annule la transaction globale) : insérer une ligne `compilation_units` avec `inventory_status='FAILED'`, `unit_key` dérivée de la découverte en amont (ou `failed:<ordinal>` si inconnue), `language='unknown'` si inconnu, `adapter_name='unknown'`, `adapter_version='unknown'`, `toolchain_version='unknown'`, `semantic_profile='unknown'`, `semantic_profile_hash=sha256_hex("")`, `build_fingerprint=sha256_hex("")`, `capability_flags='[]'`; diagnostic `UNIT_FAILED` (§5.6) expliquant l'absence du Header. Ces sentinelles sont interdites pour une unité `COMPLETE`/`PARTIAL`. Ne jamais inventer une version d'outil réelle.
- `ske ingest go` (n unités) : le snapshot `OPEN` est créé et validé d'abord ; chaque unité a sa transaction ; une unité en échec produit une ligne `compilation_units` avec `inventory_status='FAILED'` (transaction séparée) et ses diagnostics (au moins `UNIT_FAILED`, dont le message porte le code `E_*` ou `AdapterError.code` d'origine) ; à la fin, **clôture** : lier les stubs externes (§4.6), calculer `source_fingerprint` et `config_fingerprint`, statut agrégé (§3.5), `status='CLOSED'`, `closed_at`. Si **toutes** les unités échouent ⇒ `status='ABORTED'`, exit `3`.
- **Clôture** : `source_fingerprint = sha256_hex(lignes "path\tcontent_sha256\n", triées par path en ordre d'octets, dédupliquées par path)` ; `config_fingerprint = sha256_hex(lignes "unit_key\tbuild_fingerprint\tsemantic_profile_hash\tadapter_name@adapter_version\n", triées par unit_key)`.
- Un snapshot `CLOSED` **est immuable** : toute tentative d'y ajouter une unité, un symbole, une fonction… ⇒ `E_SNAPSHOT_CLOSED`. Seuls des `analysis_runs` et leurs tables filles peuvent y être rattachés.
- **Chaque `ske ingest` crée un nouveau snapshot** ; aucune déduplication en v0.x (deux ingestions identiques ⇒ deux snapshots distincts, mêmes fingerprints).
- **Vérification des sources :** si une racine est connue, le cœur recalcule le SHA-256 exact de chaque `SourceFileDef`. Sur échec `E_SOURCE_HASH_MISMATCH` : `ske ingest cir` ⇒ rollback GLOBAL et exit 3, aucun snapshot résiduel ; `ske ingest go` ⇒ rollback de l'unité, ligne d'unité `FAILED` + diagnostic dans une transaction distincte, poursuite des autres unités, exit 0 si au moins une réussit (4 si `--require-complete`) ; si toutes échouent ⇒ snapshot `ABORTED`, exit 3. Sans racine : `SOURCE_NOT_VERIFIED` (INFO).

## 5.5. Registre des codes d'erreur `E_*` (**normatif**)

Chaque code **d’import** a un test nommé `negative_<code en minuscules sans "E_">` (ex. `negative_ssa_dominance`) : en v0.1, **34** tests (33 fichiers `bad_*` + le scénario CLI `E_UNIT_DUPLICATE`) ; `E_ADAPTER_LIMIT` est couvert en v0.2 (AC-0.2-18). « Exit » = code de sortie du CLI.

| Code | Déclenchement | Exit |
|---|---|---|
| `E_PROTO_DECODE` | flux tronqué / protobuf invalide | 3 |
| `E_MSG_TOO_LARGE` | longueur annoncée > limite | 3 |
| `E_ADAPTER_LIMIT` | temps, nombre de messages ou mémoire dépassés | 3 |
| `E_HEADER_MISSING` | premier record ≠ `Header` | 3 |
| `E_RECORD_ORDER` | `UnitDef` pas en 2ᵉ position, `Header`/`UnitDef` répétés | 3 |
| `E_CIR_MAJOR_UNSUPPORTED` | `cir_major` ≠ 1 | 3 |
| `E_END_UNIT_MISSING` | flux terminé sans `EndUnit` | 3 |
| `E_DUPLICATE_ID` | id déjà utilisé dans son espace | 3 |
| `E_ORDINAL_DUPLICATE` | ordinal déjà utilisé dans un bloc ou une liste à unicité ordonnée (SQLite UNIQUE) | 3 |
| `E_PARAM_ORDER` | incohérence ou trou entre FunctionDef.parameter_value_ids et ValueDef.param_index | 3 |
| `E_EMPTY_ID` | id vide | 3 |
| `E_UNKNOWN_OPCODE` | opcode absent du registre | 3 |
| `E_UNKNOWN_KIND` | `kind`/`edge_kind`/`role` hors énumération | 3 |
| `E_DANGLING_REF` | référence à un id jamais déclaré | 3 |
| `E_ARITY_MISMATCH` | nombre d'opérandes ≠ registre | 3 |
| `E_TYPE_MISMATCH` | types incompatibles avec l'opcode / champs `TypeDef` interdits | 3 |
| `E_PAYLOAD_MISSING` | payload obligatoire absent | 3 |
| `E_PAYLOAD_KIND` | mauvais type de payload pour l'opcode | 3 |
| `E_NO_TERMINATOR` | bloc sans terminateur | 3 |
| `E_MULTI_TERMINATOR` | plusieurs terminateurs dans un bloc | 3 |
| `E_TERMINATOR_NOT_LAST` | terminateur pas en dernière position | 3 |
| `E_EDGE_SOURCE_MISMATCH` | arête dont la source ≠ bloc du terminateur | 3 |
| `E_EDGE_CROSS_FUNCTION` | arête vers un bloc d'une autre fonction | 3 |
| `E_EDGE_SHAPE` | nombre/kinds d'arêtes non conformes à §4.7 | 3 |
| `E_ENTRY_BLOCK` | `entry_block_id` invalide ou fonction externe avec blocs | 3 |
| `E_SSA_MULTI_DEF` | valeur SSA définie plusieurs fois | 3 |
| `E_SSA_DOMINANCE` | usage non dominé par sa définition | 3 |
| `E_PHI_PRED_MISMATCH` | entrées de PHI ≠ prédécesseurs | 3 |
| `E_PATH_INVALID` | chemin source non normalisé | 3 |
| `E_SPAN_INVALID` | span incohérent | 3 |
| `E_LITERAL_WIDTH` | longueur de littéral ≠ type | 3 |
| `E_COUNTER_MISMATCH` | `emitted_*` de `EndUnit` ≠ réel | 3 |
| `E_SOURCE_HASH_MISMATCH` | sha256 déclaré ≠ fichier réel | 3 |
| `E_UNIT_DUPLICATE` | `unit_key` déjà présent dans le snapshot | 3 |
| `E_SNAPSHOT_CLOSED` | écriture structurelle dans un snapshot clos | 5 |
| `E_MIGRATION_CHECKSUM` | checksum d'une migration appliquée modifié | 6 |
| `E_DB_INTEGRITY` | `ske db check` trouve des violations ERROR | 6 |
| `E_PROOF_REQUIRED` | `PROVEN_UNREACHABLE` sans `proof_record` accepté (trigger SQL) | 5 |
| `E_PROOF_IMMUTABLE` | tentative UPDATE/DELETE sur une preuve enregistrée (trigger SQL, test SQL nommé `proof_immutability_`) | 5 |
| `E_PROOF_SUPPORT_FROZEN` | mutation des données déterminantes d’un run après preuve acceptée (trigger SQL, test SQL nommé `proof_immutability_`) | 5 |
| `E_SOURCE_ACTIVATION_INVALID` | cas d’activation vide, références/arêtes/ordres invalides, ou `complete=true` sans alternatives EXACT | 3 |
| `E_HASH_COLLISION` | deux objets canoniques non identiques ont la même empreinte cryptographique (ne pas fusionner) | 5 |
| `E_WITNESSED_NOT_PRODUCIBLE` | v0.x interdit l’écriture d’un statut `WITNESSED` | 5 |
| `E_NOT_FOUND` | snapshot/fonction/run introuvable | 7 |
| `E_AMBIGUOUS` | plusieurs candidats (message liste les candidats) | 2 |
| `E_USAGE` | arguments/configuration invalides | 2 |

**Correspondance minimale des erreurs SQL** : `UNIQUE(basic_blocks.function_id, ordinal)`, `UNIQUE(instructions.block_id, ordinal)` et `UNIQUE(control_edges.terminator_instruction_id, ordinal)` ⇒ `E_ORDINAL_DUPLICATE` ; `idx_one_terminator_per_block` ⇒ `E_MULTI_TERMINATOR` ; violation de FK après validation de référence ⇒ `E_DANGLING_REF` ; violation de `CHECK` de forme d'une instruction ⇒ erreur typée de registre (ne jamais exposer une chaîne SQLite brute). Les `source_spans` identiques ne sont **pas** une erreur. `AdapterError.code` est conservé si reconnu dans le registre, sinon converti en `E_ADAPTER_LIMIT` (erreur d'exécution de l'adaptateur) avec code original dans le diagnostic `UNIT_FAILED` (ex. `TOOLCHAIN_MISSING`, `TOOLCHAIN_MISMATCH`, `BUILD_PROFILE_INVALID`) ; les codes de validation d'entrée donnent exit 3, les incohérences internes exit 5.

Un `LOOP_ABSTRACTED` est un **diagnostic** (INFO, impact SYMBOLIC, producteur `ske-abstract` v0.4), pas un code d'erreur. `UNSUPPORTED_TERMINATOR` est un diagnostic de défense interne, normalement inatteignable puisque les opcodes inconnus sont refusés à l'import ; aucune arrivée ne peut être enregistrée sur une issue inexistante.

## 5.6. Registre des diagnostics (**normatif**)

Un diagnostic est `(severity, code, impact)`. **Seuls** les diagnostics `impact = INVENTORY` avec `severity ∈ {WARNING, ERROR}` dégradent un statut d'inventaire (§3.5).

| Code | Sévérité | Impact | Émis par | Signification |
|---|---|---|---|---|
| `UNSUPPORTED_CONSTRUCT` | WARNING | INVENTORY | adaptateur | construction dont on ne sait même pas énumérer les issues |
| `FUNCTION_TRANSLATION_FAILED` | ERROR | INVENTORY | adaptateur | fonction non traduisible |
| `SOURCE_MISMATCH` | WARNING | INVENTORY | adaptateur/cœur | issue source sans lien IR et non `elided` |
| `INVENTORY_STATUS_DOWNGRADED` | WARNING | INVENTORY | cœur | adaptateur disait `COMPLETE`, le cœur a trouvé un écart |
| `RECOVER_UNMODELED` | WARNING | INVENTORY | adaptateur | `recover()` présent : l'ensemble des sorties peut être incomplet |
| `CGO_UNSUPPORTED` | WARNING | INVENTORY | adaptateur | fichier `cgo` exclu |
| `OUTCOME_ELIDED_BY_LOWERING` | INFO | INVENTORY | adaptateur | issue source supprimée par la compilation |
| `SOURCE_ACTIVATION_INCOMPLETE` | INFO | SYMBOLIC | adaptateur | les alternatives source→IR ne sont pas toutes cartographiées exactement ; interdit toute preuve d’inaccessibilité source |
| `EXCEPTION_FLOW_PARTIAL` | WARNING | INVENTORY | adaptateur/moteur | sélections typées/handler/finally non intégralement modélisés |
| `DEFER_UNMODELED` | WARNING | EFFECTS | adaptateur/moteur | `defer` présent, effets différés non modélisés |
| `IMPLICIT_PANIC_UNMODELED` | INFO | EFFECTS | adaptateur | 1 par fonction ayant au moins un site de panic implicite (déréférencement, indice, assertion de type, division, écriture dans une map nil, fermeture de canal) ; le message contient le nombre de sites |
| `EXTERNAL_CALL_OPAQUE` | INFO | EFFECTS | adaptateur | appel d'un callee externe sans modèle |
| `OPAQUE_OP_EMITTED` | INFO | SYMBOLIC | adaptateur/moteur | `OPAQUE_OP` ou opération havockée |
| `UNSUPPORTED_OVERFLOW_MODE` | WARNING | SYMBOLIC | moteur | `overflow_mode` ≠ `WRAP` |
| `CONCURRENCY_PARTIAL` | WARNING | CONCURRENCY | adaptateur/moteur | goroutines/canaux/select/verrous non modélisés |
| `CALL_TARGET_UNRESOLVED` | INFO | CALLGRAPH | adaptateur | aucun target pour l'appel |
| `CALL_TARGET_APPROX` | INFO | CALLGRAPH | adaptateur | cibles issues de CHA/RTA/VTA (monde non fermé) |
| `GENERIC_ORIGIN_OPAQUE` | INFO | TYPES | adaptateur | corps générique avec paramètres de type : opérations `OPAQUE_OP` |
| `BUILD_TAG_EXCLUDED_FILE` | INFO | OTHER | adaptateur | fichier exclu par contraintes de build |
| `SOURCE_NOT_VERIFIED` | INFO | OTHER | cœur | pas de racine de sources pour recalculer les hash |
| `CIR_MINOR_NEWER` | INFO | OTHER | cœur | mineur du flux supérieur au mineur supporté |
| `LOOP_BOUND_REACHED` | WARNING | SYMBOLIC | moteur | borne de boucle atteinte |
| `BUDGET_EXHAUSTED` | WARNING | SYMBOLIC | moteur | budget global dépassé (raison dans le message) |
| `SOLVER_UNKNOWN` | WARNING | SYMBOLIC | moteur | solveur `unknown` |
| `SOLVER_TIMEOUT` | WARNING | SYMBOLIC | moteur | limite de ressources/temps du solveur |
| `SOLVER_UNAVAILABLE` | ERROR | SYMBOLIC | moteur | solveur non disponible |
| `UNSUPPORTED_TERMINATOR` | WARNING | SYMBOLIC | moteur | terminateur non pris en charge : exploration non fermée |
| `UNIT_FAILED` | ERROR | INVENTORY | cœur | unité `FAILED` (Header absent, `AdapterError`, limite, validation, hash) ; le message porte le code `E_*` ou `AdapterError.code` d'origine |
| `ADAPTER_EFFECT_CLAIMS_TRUSTED` | INFO | SYMBOLIC | cœur | assertion explicite `false` d'un drapeau d'effet (§5.3-10) ; le message identifie le modèle/la justification |
| `SYNTHETIC_FUNCTION` | INFO | OTHER | adaptateur | fonction synthétique émise (`<package>.init`, wrapper…) ; message = catégorie (`ssa.Function.Synthetic` en Go) |
| `LOOP_ABSTRACTED` | INFO | SYMBOLIC | moteur (`ske-abstract`, v0.4) | boucle traitée en mode `intervals` |

---

# 6. Moteur symbolique — conception des algorithmes (v0.3)

## 6.1. État symbolique

```rust
struct SymbolicState {
    location: ProgramLocation,                 // (function_id, block_id, instruction_ordinal)
    env: PersistentMap<ValueId, ExprId>,       // valeur SSA -> expression
    memory: MemoryState,                       // §6.9 (identités d'allocation par activation, map, epoch, escaped)
    constraints: Vec<ExprId>,                  // contraintes de chemin, DANS L'ORDRE d'ajout
    soundness: Soundness,                      // EXACT < OVERAPPROX < OPAQUE < UNSUPPORTED (monotone)
    back_edge_counts: PersistentMap<EdgeId, u32>,
    taken_outcomes: Vec<IrOutcomeId>,          // issues IR empruntées (provenance, ordre du chemin)
    call_depth: u32,
}
```

Utiliser des collections persistantes (partage structurel, ex. `im`) ou des arènes + IDs. L'historique des issues empruntées est un DAG compact (liste chaînée immuable), pas une copie par bifurcation.

## 6.2. DAG d'expressions

Nœuds : `CONST(type, bits)`, `SYMBOL(type, name)`, `UNARY(op, e)`, `BINARY(op, a, b)`, `ITE(c, a, b)`, `FIELD(e, i)`, `UNINTERPRETED(name, args)`, `HAVOC(type, n)`. Chaque nœud est typé. **Interning** : une `HashMap<NodeKey, ExprId>` dont la clé est la **structure complète** `(kind, type_id, payload, operand ExprIds)` (jamais le hash seul : la comparaison structurelle gère les collisions).

**Simplifications autorisées (liste exhaustive en v0.3)** — aucune autre :
1. repli de constantes quand **tous** les opérandes sont `CONST`, avec la sémantique exacte de `ske-cir::bitvec` (§4.9) ; **sauf** `DIV`/`REM` par zéro et décalage négatif (jamais repliés) ;
2. `NOT(CONST b)`, `NOT(NOT x) → x` ;
3. `ITE(true,a,b) → a`, `ITE(false,a,b) → b`, `ITE(c,a,a) → a` ;
4. `AND`/`OR` avec une constante booléenne (`AND(true,x) → x`, `AND(false,x) → false`, `OR(true,x) → true`, `OR(false,x) → x`).

Pas de réordonnancement commutatif, pas d'algèbre. Noms de symboles : paramètre `param:<function_id>:<index>` ; état initial d'une globale `init:global:<qualified_name>@e<epoch>` ; contenu initial d'un `Loc` `init:<loc>@e<epoch>` ; havoc `havoc#<n>` (n = compteur du run, commençant à 0).

**Persistance** : seules les expressions référencées par un résultat persisté (gardes de `branch_cases`, `summary_cases`…) sont écrites dans `symbolic_expressions`, **enfants d'abord** (donc `operand_expression_id < expression_id` : c'est l'invariant d'acyclicité testé par CHK-18). `expression_hash = sha256_hex(sérialisation canonique : kind | type_local_key | payload | hashes des opérandes)`. Sur un `INSERT` en conflit `(snapshot_id, expression_hash)`, comparer `kind/type/payload/opérandes` : s'ils diffèrent ⇒ erreur interne `E_HASH_COLLISION` (exit 5).

## 6.3. Fonctions de transfert (v0.3)

Pour chaque opcode, le comportement est celui de la colonne « SMT v0.3 » du registre §4.3. Règles supplémentaires :

- **Valeurs** : `CONST` → `CONST` typé ; `GLOBAL` → pointeur vers `Loc::Global` ; `PARAM` → `SYMBOL` (contraint par le contexte, §11.4).
- **`PHI`** : prendre la valeur de l'entrée dont le prédécesseur est le **bloc d'où l'état vient réellement** (mémorisé dans l'état). Pas de fusion d'états en v0.3 (donc pas de `ITE` de merge).
- **`DIV`/`REM`** : deux successeurs, vérifiés par le solveur : (a) *panic* avec contrainte `diviseur == 0` → l'état devient une **sortie** `PANIC` (enregistrée, ce n'est pas une issue IR) ; (b) normal avec `diviseur != 0`. Idem `SHL`/`SHR` avec compte signé : panic si `compte < 0`.
- **`BRANCH`/`SWITCH`/`INVOKE`** : un successeur candidat par arête (§4.7 pour les gardes).
- **`CALL`** (**v0.3 : aucun résumé**) : si `is_referentially_transparent_proven` **et** absence prouvée d’effets/exception/lecture d’état mutable ⇒ résultat = `UNINTERPRETED(nom_du_callee, args)` ; sinon résultat = nouveau `HAVOC` du type de retour ; si `may_mutate_memory` ⇒ `havoc_all_memory` (§6.9) ; la soundness de l'état devient au moins `OPAQUE`. Les sorties `PANIC` propagées par le callee **ne sont pas explorées en v0.3** (elles quittent la fonction et ne peuvent atteindre aucune issue ultérieure ; elles ne comptent donc pas pour l'atteignabilité) ; l'hypothèse `CALLEE_PANIC_EXITS_NOT_EXPLORED` est ajoutée au run.
- **`RUN_DEFERS`, `DEFER_PUSH`, `RECOVER`, `OPAQUE_OP`, `UNKNOWN_EFFECT`, `SPAWN`…** : résultat `HAVOC`, `havoc_all_memory`, diagnostic correspondant (§5.6), soundness `OPAQUE`.
- **Terminateur non pris en charge** (opcode terminateur ni dans §4.3 ni géré) : l'état est abandonné, diagnostic `UNSUPPORTED_TERMINATOR`, l'exploration n'est plus « fermée ».
- **`RETURN`/`THROW`/`PANIC`/`UNREACHABLE`** : `record_exit(state)` (nécessaire aux résumés v0.4) puis fin du chemin. `UNREACHABLE` est enregistré comme sortie `UNREACHABLE_EXIT`.

## 6.4. Solveur SMT, encodage, déterminisme

```rust
pub trait Solver {
    fn check(&mut self, arena: &ExprArena, constraints: &[ExprId], budget: &SolverBudget) -> CheckResult;
    fn check_smtlib(&mut self, smtlib: &str, budget: &SolverBudget) -> CheckResult; // utilisé par le vérificateur de preuve
    fn name(&self) -> &str;  fn version(&self) -> String;
}
pub struct SolverBudget { pub rlimit: u64, pub wall_timeout_ms: u64, pub seed: u32 }
pub enum CheckResult { Sat { model: Option<Model> }, Unsat, Unknown { reason: String }, Timeout { kind: TimeoutKind } }
pub enum TimeoutKind { Rlimit, WallClock, Budget }
```

- **Backend initial : Z3.** `ske-solver` fournit : `Z3Solver` et **`ScriptedSolver`** (réponses prédéfinies dans l'ordre des appels, pour les tests déterministes de `UNKNOWN`/`TIMEOUT`).
- **Encodage** (module séparé du moteur ; aucun objet Z3 dans `ske-cir`) : `BOOL` → `Bool` ; `BITVECTOR(w)` → `(_ BitVec w)` ; tout autre type de valeur (interface, pointeur chargé, chaîne…) → **sorte non interprétée** `S_<type_id nettoyé>` ; la constante `null` de ce type est la constante `nil_<sorte>` ; seules `EQ`/`NE` portent sur ces sortes. Flottants, chaînes, tableaux : non encodés ⇒ `HAVOC` + soundness `OPAQUE`. Décalages : si la largeur du compte diffère de celle de la valeur, comparer le compte **non tronqué** à la largeur avant toute extraction/extension (vecteur §4.9 `1 << 256`).
- **Texte SMT-LIB** : pour **chaque** requête, l'encodeur du moteur (indépendant du backend) produit le texte SMT-LIB complet ; c'est ce texte qui est persisté (`solver_queries.smtlib`), empreint (`query_fingerprint`) et rejoué par le vérificateur, **y compris** lorsque le backend est `ScriptedSolver` (indispensable à T-VERIFIER-REJECT).
- **Déterminisme** (corrige un défaut de la v1.0) : la limite **primaire** est `rlimit` (compteur de ressources Z3, reproductible) ; `wall_timeout_ms` n'est qu'un filet de sécurité **non déterministe** : un `Timeout{WallClock}` est enregistré comme tel et signalé dans le rapport (`TIMEOUT` non reproductible). Paramètres Z3 posés à chaque requête : `smt.random_seed = seed`, `sat.random_seed = seed`, `rlimit`, `timeout`. **Une instance de solveur neuve par requête** en v0.3.
- **Mapping `unknown` Z3** : raison contenant `timeout`, `canceled` ou `resource` ⇒ `Timeout` ; sinon `Unknown`.
- **Empreinte de requête** : `query_fingerprint = sha256_hex(texte SMT-LIB de la requête + "|seed=" + seed + "|rlimit=" + rlimit)`. Un cache **par run** évite de redemander la même requête : un succès de cache n'appelle pas le solveur, n'incrémente pas `queries`, ne crée pas de ligne `solver_queries` et réutilise le `query_id` existant.
- **Persistance** : chaque requête effectivement posée au solveur ⇒ une ligne `solver_queries` (`smtlib` **obligatoirement non NULL** quand le résultat est `UNSAT`, car le vérificateur de preuve le rejoue).
- **Modèle (`SAT`)** : pour une issue `POSSIBLE`, stocker dans `branch_cases.witness_json` les valeurs des paramètres, ex. `{"x":"-1"}` (décimal signé selon le type), avec `validation_method = 'SMT_MODEL'`. **Ce témoin n'est pas vérifié par exécution** : il ne justifie jamais `WITNESSED`.
- Si le solveur est absent ou plante : diagnostic `SOLVER_UNAVAILABLE`, `run_status = FAILED`, résultats déjà persistés conservés.

## 6.5. Exploration guidée par les branches — **algorithme normatif**

```text
analyze(snapshot, entry_function, context, budget, solver):
    run := create_run(status=PENDING)             # config/context/budget fingerprints
    persist assumptions (SEQUENTIAL_EXECUTION, CLOSED_ENTRY_CONTEXT, CALLEE_PANIC_EXITS_NOT_EXPLORED, + contexte)
    run.status := RUNNING
    cuts := 0 ; closed := true ; states_created := 1 ; queries := 0 ; instr_count := 0
    worklist := VecDeque[ initial_state(entry_function, context) ]      # FIFO => déterministe
    seen := HashSet<StateKey>()

    while worklist non vide:
        if budget.wall_time_exceeded() or instr_count >= budget.max_instructions or estimated_memory_mb() > budget.max_memory_mb:
            cuts += len(worklist) ; closed := false ; diag BUDGET_EXHAUSTED ; break
        s := worklist.pop_front()
        k := state_key(s)                         # (location, env, memoire, constraints, back_edge_counts, call_depth) par ExprIds
        if k in seen: continue                    # doublon EXACT ⇒ abandon sûr
        seen.add(k)
        ins := fetch(s.location) ; instr_count += 1

        if ins n'est pas un terminateur:
            for t in transfer(ins, s):            # §6.3 ; éventuelles sorties implicites/panic
                # Tout fork implicite effectue la MÊME vérification SMT qu'une arête IR,
                # incrémente queries / states_created et respecte les MÊMES budgets.
                # Il n'invente pas d'ir_outcome : loguer la sortie implicite séparément.
                if t.requires_guard_check:
                    if queries >= budget.max_solver_queries:
                        r := Timeout{Budget}
                    else:
                        r := solver.check(s.constraints + [t.guard]); queries += 1
                        persist_solver_query(r)
                    if r == Unsat: continue
                    if r in {Unknown, Timeout}:
                        closed := false; diag SOLVER_UNKNOWN|SOLVER_TIMEOUT
                    if states_created >= budget.max_states:
                        cuts += 1; closed := false; diag BUDGET_EXHAUSTED; continue
                    states_created += 1
                worklist.push_back(t)
            continue

        match ins.opcode:
          RETURN|THROW|PANIC|UNREACHABLE: record_exit(s) ; continue
          JUMP: e := unique edge ; follow(s, e)                       # pas une décision
          BRANCH|SWITCH|INVOKE:
              for e in edges(ins) triées par outcome_ordinal:
                  if e.kind == EXCEPTION and not callsemantics.may_raise: continue     # arête non explorée
                  g := guard(e, s)                                    # §4.7
                  if queries >= budget.max_solver_queries:
                      r := Timeout{Budget} ; (pas d'appel solveur)
                  else: r := solver.check(s.constraints + [g]) ; queries += 1 ; persist solver_query
                  record_arrival(run, e.ir_outcome, s, r, s.soundness)     # table outcome_arrivals
                  match r:
                    Sat:                     follow(s ∧ g, e)
                    Unsat:                   (état éliminé : rien à suivre)
                    Unknown|Timeout:         closed := false ; diag SOLVER_UNKNOWN|SOLVER_TIMEOUT ; follow(s ∧ g, e)   # on continue : conserver g est sûr
          autre terminateur: record_arrival(... Unsupported) ; closed := false ; diag UNSUPPORTED_TERMINATOR

    follow(s', e):        # e.from_decision = vrai si e est une arête d'un BRANCH/SWITCH/INVOKE, faux pour un JUMP
        if e est une arête de retour (« retreating edge », §6.8):
            s'.back_edge_counts[e] += 1
            if s'.back_edge_counts[e] > budget.loop_unroll:
                cuts += 1 ; closed := false ; diag LOOP_BOUND_REACHED (une fois par fonction) ; return
        if e.from_decision:
            if states_created >= budget.max_states:
                cuts += 1 ; closed := false ; diag BUDGET_EXHAUSTED("max_states") ; return
            states_created += 1
            s'.taken_outcomes.push(e.ir_outcome)
        s'.location := (fonction, e.target, 0) ; worklist.push_back(s')
```

**Précision sur `states_created`** : il vaut 1 au départ (état initial) et n'augmente que pour un état poussé **après une décision**. Un `JUMP` déplace le même état et ne l'incrémente pas. Le test `max_states` ne s'applique donc qu'aux arêtes de décision ; le test de boucle s'applique à toute arête de retour.

**Empreintes de run (v1.4).** `budget_json` = JSON canonique (§8.1) du budget effectif ; `config_fingerprint = sha256_hex(snapshots.config_fingerprint + "|" + budget_json + "|" + context_fingerprint + "|" + engine_version + "|" + solver_name + "@" + solver_version)` ; `assumptions_fingerprint = sha256_hex(kinds d'hypothèses distincts, triés par octets, joints par "\n")`, recalculé quand une hypothèse est ajoutée pendant l'exploration et définitif avant l'étape (2) de §6.6.

**Affirmations d'adaptateur.** Une arête `EXCEPTION` ignorée parce que `may_raise = false`, ou un `CALL` traité comme référentiellement transparent parce que `is_referentially_transparent_proven = true`, reposent sur une **affirmation de l'adaptateur**, non vérifiée par le cœur : dès que le moteur s'appuie sur l'une d'elles, il ajoute l'hypothèse `ADAPTER_EFFECT_CLAIMS_TRUSTED` au run. (Les adaptateurs DOIVENT mettre `may_raise = true` et `may_mutate_memory = true` par défaut, §5.3-10.)

À la fin de la boucle : persister `exploration_closed = closed`, `cut_count = cuts`, métriques ; puis **classification** (§6.6) ; puis `run_status` (§6.7).

## 6.6. Classification des issues et preuve d'inaccessibilité — **normatif**

**Règle de fermeture globale (v0.3).** L'exploration est *fermée* ssi, à la fin : la `worklist` est vide, `cuts == 0`, aucune arrivée `Unknown`/`Timeout`/`Unsupported` n'a été enregistrée, et aucun budget n'a été dépassé. `PROVEN_UNREACHABLE` n'est permis pour une issue **que si** l'exploration est fermée **et** qu'aucune de ses arrivées n'est `Sat` (voir ci-dessous). (Raffinement par issue : v0.4+.)

Pour chaque IR outcome `o` de la fonction d'entrée (v0.4 : des fonctions explorées), calculer, **dans cet ordre de priorité**, à partir de `arrivals(o)` :

| Priorité | Condition | Statut | `soundness` | `explanation` |
|---|---|---|---|---|
| 1 | ∃ arrivée `Sat` | `POSSIBLE` | la **meilleure** (la plus petite) soundness parmi les arrivées `Sat` | `SAT_ARRIVAL` |
| 2 | ∃ arrivée `Unsupported` | `UNSUPPORTED` | `UNSUPPORTED` | `UNSUPPORTED_TERMINATOR` |
| 3 | ∃ arrivée `Timeout` | `TIMEOUT` | pire soundness des arrivées | `SOLVER_TIMEOUT` |
| 4 | ∃ arrivée `Unknown` | `UNKNOWN` | idem | `SOLVER_UNKNOWN` |
| 5 | arrivées toutes `Unsat` (≥ 0) et exploration **fermée** | `PROVEN_UNREACHABLE` **sous réserve** du vérificateur ci-dessous | pire soundness des arrivées (ou `EXACT` si aucune arrivée) | `ALL_ARRIVALS_UNSAT` ou `NO_ARRIVALS_CLOSED_EXPLORATION` |
| 6 | arrivées toutes `Unsat` (≥ 1) et exploration **non fermée** | `UNKNOWN` | pire soundness | `EXPLORATION_NOT_CLOSED` |
| 7 | aucune arrivée et exploration non fermée | `UNANALYZED` | `UNSUPPORTED` n'est **pas** utilisé : `OPAQUE` | `NOT_REACHED_BUDGET_OR_CUT` |

**Vérificateur de cohérence/rejeu indépendant de la classification — pas un vérificateur formel indépendant du modèle source** (fonction `verify_unreachability(run_id, outcome_id)` dans `ske-verify`, **sans réutiliser** le code de classification ; il relit la base) — toute étape qui échoue ⇒ verdict `REJECTED` et l'issue devient `UNKNOWN` avec `explanation = 'PROOF_REJECTED:<raison>'` :

1. `analysis_runs.exploration_closed = 1` et `cut_count = 0` (relus en base) ;
2. toutes les lignes `outcome_arrivals` de `(run, outcome)` ont `result = 'UNSAT'` ;
3. aucune de ces arrivées n'a une soundness `UNSUPPORTED` ; `model_soundness` du proof = la pire parmi elles (∈ {`EXACT`, `OVERAPPROX`, `OPAQUE`}) ;
4. **toutes** les requêtes `UNSAT` du **run** (table `solver_queries`, `smtlib` non NULL — pas seulement celles de l'issue examinée : une issue sans aucune arrivée n'est « inatteignable » que parce que des états en amont ont été éliminés, donc ces éliminations doivent toutes être valides) sont **rejouées** avec `check_smtlib` sur une **instance neuve** et une graine différente (`seed = 1`) et rendent `Unsat` ;
5. les hypothèses du run incluent au minimum `SEQUENTIAL_EXECUTION` et `CLOSED_ENTRY_CONTEXT`.

En cas de succès : insérer `proof_records(verdict='ACCEPTED', verification_level='MODEL_UNREACHABLE', residual_frontier=0, …)` **avant** d'insérer le résultat `PROVEN_UNREACHABLE` qui y fait référence (`proof_id`). Les triggers SQL (§8) rendent obligatoire une preuve acceptée **et immuable**, un `run` **fermé** (`status=COMPLETED`, `exploration_closed=1`, `cut_count=0`), et figent les données déterminantes de ce run après acceptation. **Ordre obligatoire** : (1) persister les résultats d'exploration et métriques finales, (2) passer le run en `COMPLETED` et `exploration_closed=1`, (3) lancer le vérificateur en lecture seule ; ses rejeux SMT ne sont PAS enregistrés dans `solver_queries`, (4) insérer la preuve acceptée, (5) insérer le résultat démontré. Chaque échec laisse `UNKNOWN` et un `proof_record` rejeté distinct. Le vérificateur ne réutilise ni le classifieur ni son état en mémoire. Il s’agit d’une preuve **dans le modèle et sous hypothèses** ; le rejeu du même solveur avec une graine différente ne certifie pas l’exhaustivité sémantique de la traduction source→CIR.

**Décision source ↔ IR (v1.2, impératif) :** une issue source `elided` reste `UNANALYZED` (`explanation="ELIDED"`). Pour les autres issues, les `source_ir_links` ne sont **que de la provenance**. La classification à la lecture est calculée à partir des **cas d’activation ordonnés** (§4.5.1), des états symboliques/contraintes conjointes, de la complétude attestée du mapping et des preuves du run. `POSSIBLE` exige au moins un cas entier satisfaisable ; `PROVEN_UNREACHABLE` exige tous les cas de l’issue impossibles sous exploration fermée et vérification de modèle, avec mapping exact. Si des cas ne sont pas prouvés ou le mapping n’est pas complet/exact : `UNKNOWN`, `TIMEOUT`, `UNSUPPORTED` ou `UNANALYZED` suivant le motif, **jamais** `PROVEN_UNREACHABLE`. Il est interdit de choisir un statut source par simple `MAX/MIN` des statuts des IR outcomes individuellement. Les résultats source sont dérivés à la lecture, pas stockés en v0.3.

**Format des témoins `witness_json` :** objet JSON clé = nom de paramètre, valeur = chaîne décimale pour `BITVECTOR` signé/non signé ; chaînes littérales `"true"`/`"false"` pour BOOL ; `"nil"` ou `"non-nil"` pour pointeur/interface sous abstraction. Pour les valeurs de sortes non interprétées, utiliser `{"kind":"UNINTERPRETED","token":"<id opaque>"}`. Un témoin de solveur ne constitue jamais un `WITNESSED` sans exécution concrète validée.

## 6.7. Statut d'exécution, budgets, annulation

- `run_status = COMPLETED` ssi l'exploration est fermée ; `PARTIAL` si au moins une coupure, budget épuisé, arrivée `Unknown`/`Timeout`/`Unsupported` ; `FAILED` sur erreur interne ou `SOLVER_UNAVAILABLE` ; `CANCELLED` sur SIGINT/SIGTERM (persister l'existant, puis sortir avec le code 130). **Statut définitif (v1.4)** : un run passé en `COMPLETED` à l'étape (2) de §6.6 ne change plus de statut (le trigger `trg_proven_run_status_final` l'impose dès qu'une preuve est acceptée). Un signal ou une erreur interne survenant **après** l'étape (2) arrête la vérification : les issues candidates non encore vérifiées reçoivent `UNKNOWN` (`explanation = 'VERIFICATION_CANCELLED'` ou `'VERIFICATION_ERROR'`, sans `proof_record`), le run reste `COMPLETED`, puis sortie 130 (signal) ou 5 (erreur interne).
- Un `ske` qui plante laisse un run `RUNNING` : `ske db check` le signale (CHK-19, WARNING) ; la commande `ske runs mark-failed --run N` le passe en `FAILED` (refusée avec `E_USAGE` si le run n'est pas `PENDING`/`RUNNING`). Aucun nettoyage automatique.
- **Budget par défaut** (`budget.toml` §11.4), consigné dans `analysis_runs.budget_json` :

| Clé | Défaut | Rôle |
|---|---|---|
| `max_wall_time_ms` | 60000 | temps global |
| `max_states` | 10000 | états créés (§6.5) |
| `max_instructions` | 1000000 | instructions exécutées |
| `max_call_depth` | 8 | v0.4 |
| `loop_unroll` | 3 | nombre max de traversées d’une même arête de retour |
| `loop_mode` | `"unroll"` | v0.4+ : `"unroll"` ou `"intervals"` ; en v0.3 seule `"unroll"` est autorisée |
| `max_recursion_unfold` | 2 | v0.4+ : profondeur maximale de dépliage récursif ; clé acceptée et stockée dès v0.3 |
| `max_solver_queries` | 5000 | requêtes au solveur |
| `solver_rlimit` | 5000000 | ressources Z3 par requête |
| `solver_wall_timeout_ms` | 10000 | filet non déterministe |
| `solver_seed` | 0 | graine |
| `max_memory_mb` | 1024 | estimation (états × taille estimée) ; dépassement = coupure |

## 6.8. Boucles et récursivité

**Arête de retour (« retreating edge »)** : parcours DFS depuis le bloc d'entrée, arêtes visitées par `(ordinal du bloc source, outcome_ordinal)` croissants ; une arête vers un bloc **actuellement sur la pile DFS** est une arête de retour (pour un CFG réductible, c'est exactement l'arête dont la cible domine la source).

v0.3 : chaque état compte, par arête de retour, ses traversées ; au-delà de `loop_unroll` ⇒ **coupure** (§6.5). Une boucle observée avec deux issues ne prouve rien sur toutes ses exécutions : l'exploration devient non fermée, donc `PARTIAL`.

v0.4+ : points fixes avec intervalles, `widening`/`narrowing`, résumés récursifs par SCC. Tout invariant utilisé comme preuve doit être validé dans un domaine sound pour le type ciblé ; en cas de doute, `UNKNOWN`.

## 6.9. Mémoire et aliasing (v0.3 : M0 + M1 minimal) — **normatif, corrigé v1.2**

**Objectif prioritaire :** conserver une surapproximation correcte ; **ne jamais** conclure qu’une écriture est impossible ou qu’une lecture a une valeur certaine si l’identité/aliasing est ambigu.

- **M0** : valeurs SSA et constantes uniquement.
- **M1** : les identités suivantes sont **distinctes** :
  - `AllocationSite(instruction_id)` : **site de code**, jamais utilisé seul comme identité d’objet concret.
  - `AllocInstance(instruction_id, activation_id)` : **allocation effectivement rencontrée dans un état**. Chaque réexécution du même site (boucle, récursion) produit un `activation_id` neuf dans la lignée d’exécution. Les deux allocations à une même instruction restent distinctes.
  - `AllocSummary(instruction_id, context_abstraction)` : objet récapitulatif après fusion ou perte de contexte ; représente **plusieurs** instances possibles.
  - `ParamPointee(function,param_index)`, `Global(symbol)`, `FieldOf(Loc,i)`, `Unknown` : autres abstractions de localisation.
- **Identité déterministe des activations :** chaque état conserve un compteur local de créations et un identifiant de lignée d’exécution ; le fork de branche préserve les objets **déjà alloués** et attribue des identités distinctes aux allocations futures. Le stockage d’IDs ne prouve pas la disjonction des alias lorsque des objets se sont échappés.
- `pts(v)` fournit un ensemble de localisations **et un drapeau de précision** `MUST_SINGLETON` / `MAY_SET` / `UNKNOWN`. `ALLOC` neuf non échappé → `MUST_SINGLETON(AllocInstance(...))`. Pointeurs provenant des paramètres et appels inconnus ne deviennent jamais `MUST_SINGLETON` seulement parce que l’ensemble abstrait contient une localisation.
- **Séparation :** deux instances d’allocation distinctes, fraîchement créées et non échappées, sont disjointes entre elles et de l’état initial. Deux paramètres pointeurs, ou un paramètre et une globale, peuvent s’aliaser. Les `AllocSummary` ne sont jamais supposés disjoints des instances qu’ils résument.
- `MemoryState` utilise `map: Loc -> ExprId`, `store_log: ordered writes`, `epoch`, `escaped`, `allocation_counter` et informations de précision. La lecture d’un `Loc` reconstruit les modifications possibles dans l’ordre (avec `ITE(alias(...), new, old)` lorsqu’il existe une possibilité d’alias). Les relations d’alias peuvent être surapproximées ; ne **jamais** poser une contrainte d’alias incohérente pour exclure un comportement réel.
- `LOAD p` : **pour `ParamPointee`, un premier `LOAD` sans STORE reçoit un symbole initial frais stable `init:<loc>@epoch` et conserve `EXACT` sous l'hypothèse `CLOSED_ENTRY_CONTEXT` (l'inconnu est la valeur, pas la sémantique).** Une lecture répétée via la même adresse SSA (ou adresse prouvée identique), sans STORE intermédiaire pouvant aliaser et sans `havoc`, doit retrouver la valeur du dernier STORE. Cette cohérence d'identité est garantie même lorsque la localisation abstraite n'est pas `MUST_SINGLETON` : la mise à jour du `store_log` peut rester faible pour les **autres** alias, mais la relecture de cette même référence doit respecter le STORE concret. Une `AllocInstance` fraîche non échappée ne peut aliaser le pointee d'un paramètre ; pour un résumé multi-instance d'allocations, ne pas appliquer cette exception. Lorsque `pts(p)={l}`, la lecture respecte le journal d'écritures antérieur ; lorsque la cible est inconnue, nouveau symbole et `OVERAPPROX` ou `OPAQUE` selon les effets.
- `STORE p,v` : **mise à jour forte uniquement si** `pts(p)=MUST_SINGLETON(l)` et que cette localisation abstraite représente **exactement un objet** dans l’état courant. Sinon mise à jour faible de **tous** les candidats possibles et des alias ; si cibles non bornables ou `Unknown`, `havoc_all_memory` avec diagnostic. Une écriture d’une référence pouvant échapper marque l’objet concerné `escaped`.
- `FIELD_ADDR` conserve l’identité du parent ; `LOAD`/`STORE` doivent intégrer les champs et alias du parent. Les opérations sur un agrégat partiellement modélisé dégradent la précision ; une approximation ne doit jamais exclure un alias réel.
- `havoc_all_memory` : `epoch += 1`, invalider les connaissances sur les valeurs mémoire, vider le cache et les logs dépendants de l’ancien epoch. `CALL` dont les effets mémoire ne sont pas prouvés absents ⇒ `havoc_all_memory` en v0.3.
- **Unicité temporelle :** deux `new(int)` successifs au même site (deux itérations) sont deux instances. La mémoire réutilisée à travers une boucle ne peut pas écraser la valeur du premier objet en utilisant le seul `instruction_id`.
- À toute **fusion**, **perte d’identité** ou **incertitude sur les écritures**, remplacer les instances par `AllocSummary` ou par `Unknown` conservateur ; **aucune conclusion d’inaccessibilité dépendant d’une égalité mémoire non démontrée**.
- Les effets concurrents partagés restent hors modèle v0.3, sous l’hypothèse explicite `SEQUENTIAL_EXECUTION` ; aucune conclusion sur le programme concurrent réel ne doit être présentée comme absolue.

**Tests normatifs (§15.3 et AC-0.3-10) :** `memory_two_loads_same_expr`, `memory_strong_update`, `memory_alias_param_param`, `memory_alloc_distinct`, `memory_havoc_epoch`, **`memory_same_site_two_iterations`** (les deux objets restent distincts), **`memory_summary_never_strong_update`** (la perte d’identité force une mise à jour faible / havoc), **`memory_alias_chain_soundness`** (aucune valeur exclue par une relation d’alias possible). Le laboratoire ajoute `AllocTwice` (§15.1).

## 6.10. Appels et dispatch

Un appel a : `callsite`, `dispatch_kind`, ensemble de cibles résolues, indicateur de monde fermé. **Un ensemble de cibles vide n'implique pas « aucun appel ».** Cible inconnue : valeur de retour non déterministe (`HAVOC`), mémoire potentiellement modifiée, exceptions possibles selon `may_raise`, diagnostic `CALL_TARGET_UNRESOLVED`. En v0.3, les cibles ne sont **pas** utilisées par le moteur (tout `CALL` est havocké) ; elles sont stockées pour l'inventaire du graphe d'appels et pour la v0.4.

## 6.11. Concurrence

v0.x : **inventaire structurel obligatoire** ; exploration concurrente exhaustive hors périmètre. `SPAWN`, `CHANNEL_*`, `LOCK`, … sont havockés avec le diagnostic `CONCURRENCY_PARTIAL`. L'hypothèse `SEQUENTIAL_EXECUTION` de chaque run rend ce choix explicite.

---

# 7. Résumés et reconstruction comportementale (v0.4 – v0.5)

## 7.1. Résumé d'une fonction (v0.4)

```text
FunctionSummary {
  function_id, context_fingerprint, summary_fingerprint, precision, complete,
  cases: [ SummaryCase {
      case_ordinal,
      guard: Vec<ExprId>,              # conjonction, dans l'ordre du chemin
      returns: Vec<ExprId>,            # UNE expression par valeur de retour (Go : plusieurs)
      exit_kind: RETURN | PANIC | THROW | UNREACHABLE_EXIT,
      memory_delta: Vec<(Loc, ExprId)>,# Loc dont le contenu final diffère du contenu initial
      effects: Vec<Effect>,
      referenced_outcomes: Vec<IrOutcomeId>,   # issues IR empruntées
      precision: EXACT | OVERAPPROX | PARTIAL | UNKNOWN } ],
  assumptions, unknowns
}
```

**Calcul** : exécuter l'exploration (§6.5) de la fonction avec tous ses paramètres symboliques ; chaque `record_exit` produit **un cas** (pas de fusion en v0.4). **Application à un site d'appel** : substituer paramètres → expressions des arguments (fonction de substitution sur le DAG, avec mémoïsation) dans `guard`, `returns`, `memory_delta` ; forker l'état par cas dont `contraintes ∧ garde'` est `Sat` (`Unknown` ⇒ garder le cas et dégrader la soundness) ; ajouter les effets ; lier les `returns` au résultat (tuple ⇒ `EXTRACT_FIELD` sur la liste). Cible inconnue ⇒ havoc (§6.10). **Cycles d'appels** : SCC (Tarjan) résumés bas-en-haut ; récursion : au plus 2 dépliages puis cas résiduel `precision = PARTIAL` et entrée dans `unknowns`. **Clé de cache** : `summary_fingerprint = sha256_hex(function.structural_fingerprint | semantic_profile_hash | context_fingerprint | summary_fingerprints des callees triés | empreintes des modèles externes | config_fingerprint du run)`.

## 7.2. Extraction des comportements (v0.5) — **algorithme normatif**

Entrée : fonction d'entrée `E`, run symbolique complet.
1. Obtenir les cas du résumé de `E` (callees déjà composés par application des résumés) dont `exit_kind ∈ {RETURN, PANIC, THROW}`.
2. Pour chaque cas, `referenced_outcomes` = issues IR empruntées ; stocker dans `behavior_case_outcomes` ; les issues **source** liées s'obtiennent par `source_ir_links`.
3. **Déduplication conservatrice** : fusionner deux cas ssi même `exit_kind`, `returns` structurellement égaux (mêmes `ExprId`), même `memory_delta`, mêmes effets ; garde fusionnée = disjonction ; `referenced_outcomes` = union. Sinon les garder séparés. **Ne jamais** supprimer une issue source de l'inventaire.
4. **Ordre** : par `(ligne, colonne)` du span primaire de l'instruction de sortie (`RETURN`/`PANIC`…) croissant ; à égalité : ordre **lexicographique des chemins d'issues IR** (suite ordonnée des `(ordinal de la décision dans la fonction, ordinal de l'issue)`, préfixe plus court d'abord), puis fingerprint du cas ; les cas sans span en dernier. Ce comparateur est indépendant de l'ordre BFS/DFS et doit être employé pour l'attribution des `case_ordinal`.
5. Les effets externes non modélisés restent `UNKNOWN_EFFECT` : le cas porte `precision = OVERAPPROX` au mieux.
6. Produire une **règle comportementale** par cas ; aucune interprétation métier.

**Rendu des gardes (`render_guard`, déterministe — règles R1 à R6)** :
- **R1** : la garde est la conjonction des contraintes **dans l'ordre du chemin** ; aucune simplification supplémentaire en v0.5.
- **R2** (négation poussée) : `NOT(a < b)` → `a >= b` ; `NOT(a <= b)` → `a > b` ; `NOT(a > b)` → `a <= b` ; `NOT(a >= b)` → `a < b` ; `NOT(a == b)` → `a != b` ; `NOT(a != b)` → `a == b`. Un booléen `b` (symbole ou paramètre) s'écrit `b == true` ; `NOT(b)` s'écrit `b == false`.
- **R3** : constantes entières en décimal selon la signature du type ; booléens `true`/`false` ; nul `nil`.
- **R4** : un paramètre s'écrit `ValueDef.name` (sinon `p<index>`) ; un chargement de globale s'écrit son nom court (`ErrUnderage`) ; sinon `<expr:N>`.
- **R5** : conjonction ` AND `, disjonction ` OR ` (mots en majuscules), parenthèses autour de chaque conjonction d'une disjonction de plusieurs atomes ; conjonction vide = `true`.
- **R6** : arithmétique infixe `+ - * / % << >> & | ^` avec parenthèses autour de tout `BINARY` imbriqué.

**Règle fonctionnelle (une par cas)** : `rule_key = "<qualified_name>@run<analysis_run_id>#case<ordinal>"` ; `description = "WHEN <garde> THEN <résultat>"` (résultat : `return <e1>, <e2>…` ou `panic`) ; `origin = DETERMINISTIC` (`ABSTRACT_DERIVED` si un domaine abstrait a servi) ; `validation_status = UNVALIDATED`. **Evidence** (`knowledge_evidence`, une ligne par ancre) : `(summary_case_id, 'DERIVED_FROM')`, un `(ir_outcome_id, 'GUARD_OUTCOME')` par issue empruntée, et `(source_span_id de la sortie, 'RESULT_SITE')`.

**Exemple de référence** (programme `Check`, voir §15.3 pour les spans) — la deuxième règle dépend de l'échec de la première : ne **jamais** la résumer en « si non enabled alors erreur ».

```yaml
function: example.Check
precision: EXACT                  # sous les hypothèses du run
assumptions: [SEQUENTIAL_EXECUTION, CLOSED_ENTRY_CONTEXT, CALLEE_PANIC_EXITS_NOT_EXPLORED]
cases:
  - ordinal: 0
    when: "age < 18"
    then: "return ErrUnderage"
    referenced_source_outcomes: ["example.Check/sd0/o0"]
  - ordinal: 1
    when: "age >= 18 AND enabled == false"
    then: "return ErrDisabled"
    referenced_source_outcomes: ["example.Check/sd0/o1", "example.Check/sd1/o0"]
  - ordinal: 2
    when: "age >= 18 AND enabled == true"
    then: "return nil"
    referenced_source_outcomes: ["example.Check/sd0/o1", "example.Check/sd1/o1"]
```

## 7.3. Effets externes

`ExternalEffect` = `kind`, `target`, `arguments` symboliques, `precondition`, issues normale/exceptionnelle, `evidence`, `semantic_model_id`, `confidence`. Bibliothèque de modèles versionnés (HTTP, SQL, brokers, frameworks) **seulement** si une sémantique explicite est justifiée ; sinon `UNKNOWN_EFFECT`.

## 7.4. LLM optionnel (v0.7)

Le LLM ne reçoit **que** des faits structurés sélectionnés (et, si nécessaire, des extraits source justifiants) ; il formule descriptions et hypothèses métier. Stocker séparément `origin='LLM_INFERRED'`, `prompt_version`, `llm_model`, `model_parameters`, références d'evidence, `validation_status`. Aucun texte LLM n'écrase une donnée déterministe. Le Context Builder applique un budget de tokens **après** sélection par liens structurels, pas par recherche vectorielle seule. **Jamais** de code source envoyé à un LLM distant sans consentement explicite et enregistré.

---

## 7.5. Hiérarchie fonctionnelle et scénarios interservices (v1.2 ; préparation v0.5)

Le **résumé de fonction** n’est pas l’unité métier terminale. Le modèle DOIT distinguer :
1. `FunctionBehavior` — relation entre entrées/sorties, prédicats, mutations et effets d’une fonction, sous hypothèses.
2. `FunctionalScenario` — chaîne causale conditionnée d’étapes (fonctions, événements, transactions, interactions externes), avec des nœuds potentiellement situés dans plusieurs services.
3. `BusinessCapability` — regroupement explicite de scénarios, décrit comme une **hypothèse d’abstraction fonctionnelle** tant qu’il n’a pas été validé humainement ; la dénomination métier ne découle pas mécaniquement du graphe.

Les étapes interservices sont reliées par des **contrats et événements** (appel HTTP, publication/consommation, persistance) ; une corrélation heuristique ne doit **jamais** être convertie en causalité prouvée. Chaque arête interservice enregistre `relation_kind`, `evidence`, `confidence_kind` (`CONFIRMED`, `INFERRED`, `UNKNOWN`) et les hypothèses de transport, d’échec et de temporalité.

La v0.5 introduit une migration `0002_capabilities.sql` (voir §8.4) avec des tables dédiées ; l’absence de corrélation externe produit une frontière `EXTERNAL_UNKNOWN`, **pas** un scénario complet. Les sorties CLI doivent autoriser la navigation scénario → cas symboliques → issue IR → source, et inversement. La v0.5 peut démontrer cette structure sur un exemple mono-service ; l’analyse interservice complète reste une extension future explicitement non couverte.

---

# 8. Persistance SQLite

## 8.1. Règles (**normatives**)

- Une base SQLite par workspace local ; plusieurs dépôts et snapshots possibles.
- **Chaque connexion** exécute `PRAGMA foreign_keys = ON` ; `PRAGMA journal_mode = WAL` à l'initialisation ; `PRAGMA busy_timeout` configurable (défaut 5000 ms). Un test vérifie que **toute** connexion issue de `Storage::open` rend `1` pour `PRAGMA foreign_keys`.
- **Runner de migrations** (dans `ske-storage`, sans bibliothèque externe) : (1) crée `schema_migrations` si absente (**cette table n'est pas dans `0001_initial.sql`**) ; (2) pour chaque fichier `migrations/NNNN_*.sql` non encore appliqué, dans l'ordre : `BEGIN IMMEDIATE`, exécuter le fichier, insérer `(version, description, checksum = sha256_hex du fichier)`, `COMMIT` ; (3) pour une migration déjà appliquée, comparer le checksum ; différent ⇒ `E_MIGRATION_CHECKSUM` (exit 6) ; (4) les fichiers sont **embarqués** (`include_str!`). Idempotent : relancer ne change rien. **Aucune migration destructive silencieuse** (pas de `DROP`/`DELETE` de données historiques sans ADR).
- Les migrations ne contiennent **ni** `BEGIN/COMMIT` **ni** `PRAGMA`.
- Transactions d'ingestion : voir §5.4.
- Un snapshot `CLOSED` est **immuable** : les écritures structurelles passent par un type `OpenSnapshotWriter` qui n'existe que pour un snapshot `OPEN` (le type `ClosedSnapshot` n'expose que de la lecture et l'ajout de runs d'analyse). Test : tenter d'obtenir un writer sur un snapshot clos ⇒ `E_SNAPSHOT_CLOSED`.
- Les `INTEGER PRIMARY KEY` ne sont pas des identifiants stables exposés entre reconstructions : le CLI/JSON expose les **ids de flux** (sans le préfixe `unit_key|`) et la `unit_key`.
- Intégrité **inter-tables et inter-snapshot** : par les contrôles SQL de §8.5 (`ske db check`) ; elle ne repose **pas** sur les seules clés étrangères (voir ADR-011).
- Un résultat `PROVEN_UNREACHABLE` est protégé par les triggers du DDL §8.2 (preuve acceptée et niveau `MODEL_UNREACHABLE`) ; l’évaluation de la preuve elle-même reste une responsabilité du vérificateur du moteur (§6.6).
- Les hashes sont calculés sur encodages canoniques. **JSON canonique** (empreintes de build, de contexte, de budget, payloads d'hypothèses) : JSON conforme à RFC 8785 (JCS) — UTF-8, clés triées, aucun espace ; les valeurs numériques de ces documents sont des entiers.

## 8.2. Migration `migrations/0001_initial.sql` (**normative — à copier tel quel**)

```sql
-- Pas de BEGIN/COMMIT/PRAGMA ici. schema_migrations est créée par le runner.

CREATE TABLE repositories (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    root_uri TEXT NOT NULL UNIQUE
);

CREATE TABLE snapshots (
    id INTEGER PRIMARY KEY,
    repository_id INTEGER NOT NULL REFERENCES repositories(id),
    revision TEXT,
    source_fingerprint TEXT,
    config_fingerprint TEXT,
    status TEXT NOT NULL DEFAULT 'OPEN' CHECK (status IN ('OPEN','CLOSED','ABORTED')),
    inventory_status TEXT NOT NULL DEFAULT 'PARTIAL'
        CHECK (inventory_status IN ('COMPLETE','PARTIAL','FAILED')),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    closed_at TEXT,
    CHECK (status <> 'CLOSED' OR (source_fingerprint IS NOT NULL
                                  AND config_fingerprint IS NOT NULL
                                  AND closed_at IS NOT NULL))
);

CREATE TABLE compilation_units (
    id INTEGER PRIMARY KEY,
    snapshot_id INTEGER NOT NULL REFERENCES snapshots(id),
    unit_key TEXT NOT NULL,
    language TEXT NOT NULL,
    adapter_name TEXT NOT NULL,
    adapter_version TEXT NOT NULL,
    toolchain_version TEXT NOT NULL,
    build_fingerprint TEXT NOT NULL,
    semantic_profile TEXT NOT NULL,
    semantic_profile_hash TEXT NOT NULL,
    capability_flags TEXT NOT NULL DEFAULT '[]',          -- tableau JSON
    inventory_status TEXT NOT NULL
        CHECK (inventory_status IN ('COMPLETE','PARTIAL','FAILED')),
    expected_source_decisions INTEGER NOT NULL DEFAULT 0 CHECK (expected_source_decisions >= 0),
    source_decision_count INTEGER NOT NULL DEFAULT 0 CHECK (source_decision_count >= 0),
    ir_decision_count INTEGER NOT NULL DEFAULT 0 CHECK (ir_decision_count >= 0),
    ir_edge_count INTEGER NOT NULL DEFAULT 0 CHECK (ir_edge_count >= 0),
    UNIQUE (snapshot_id, unit_key)
);

CREATE TABLE source_files (
    id INTEGER PRIMARY KEY,
    snapshot_id INTEGER NOT NULL REFERENCES snapshots(id),
    path TEXT NOT NULL,
    language TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    UNIQUE (snapshot_id, path)
);

CREATE TABLE source_spans (
    id INTEGER PRIMARY KEY,
    source_file_id INTEGER NOT NULL REFERENCES source_files(id),
    start_line INTEGER NOT NULL CHECK (start_line > 0),
    start_column INTEGER NOT NULL CHECK (start_column > 0),
    end_line INTEGER NOT NULL CHECK (end_line > 0),
    end_column INTEGER NOT NULL CHECK (end_column > 0),     -- EXCLUSIF
    CHECK (end_line > start_line OR (end_line = start_line AND end_column >= start_column)),
    UNIQUE (source_file_id, start_line, start_column, end_line, end_column)
);

CREATE TABLE cir_types (
    id INTEGER PRIMARY KEY,
    snapshot_id INTEGER NOT NULL REFERENCES snapshots(id),
    unit_id INTEGER NOT NULL REFERENCES compilation_units(id),
    local_key TEXT NOT NULL,                                 -- unit_key|id
    kind TEXT NOT NULL,
    descriptor TEXT NOT NULL,
    payload BLOB NOT NULL,                                   -- TypeDef protobuf sérialisé (autoritatif)
    UNIQUE (snapshot_id, local_key)
);

CREATE TABLE symbols (
    id INTEGER PRIMARY KEY,
    snapshot_id INTEGER NOT NULL REFERENCES snapshots(id),
    unit_id INTEGER NOT NULL REFERENCES compilation_units(id),
    local_key TEXT NOT NULL,
    owner_symbol_id INTEGER REFERENCES symbols(id),
    name TEXT NOT NULL,
    qualified_name TEXT NOT NULL,
    kind TEXT NOT NULL,
    type_id INTEGER REFERENCES cir_types(id),
    primary_span_id INTEGER REFERENCES source_spans(id),
    structural_fingerprint TEXT,
    UNIQUE (snapshot_id, local_key)
);

CREATE TABLE functions (
    id INTEGER PRIMARY KEY,
    symbol_id INTEGER NOT NULL UNIQUE REFERENCES symbols(id),
    entry_block_id INTEGER,                                  -- validé applicativement (E_ENTRY_BLOCK)
    return_type_ids TEXT NOT NULL DEFAULT '[]',               -- tableau ordonné d'identifiants CIR de types (vérifié à import)
    is_external INTEGER NOT NULL DEFAULT 0 CHECK (is_external IN (0,1)),
    resolved_function_id INTEGER REFERENCES functions(id),   -- stub externe lié à une fonction réelle
    inventory_status TEXT NOT NULL DEFAULT 'PARTIAL'
        CHECK (inventory_status IN ('COMPLETE','PARTIAL','FAILED')),
    CHECK (resolved_function_id IS NULL OR is_external = 1)
);

CREATE TABLE values_ir (
    id INTEGER PRIMARY KEY,
    snapshot_id INTEGER NOT NULL REFERENCES snapshots(id),
    unit_id INTEGER NOT NULL REFERENCES compilation_units(id),
    function_id INTEGER REFERENCES functions(id),            -- NULL pour une GLOBAL
    local_key TEXT NOT NULL,
    type_id INTEGER NOT NULL REFERENCES cir_types(id),
    value_role TEXT NOT NULL CHECK (value_role IN ('PARAM','CONST','SSA','GLOBAL','MEMORY')),
    name TEXT,
    symbol_id INTEGER REFERENCES symbols(id),                -- GLOBAL
    literal BLOB,                                            -- LiteralValue protobuf (CONST)
    param_index INTEGER CHECK (param_index IS NULL OR param_index >= 0),
    UNIQUE (snapshot_id, local_key),
    UNIQUE (function_id, param_index),
    CHECK ((value_role = 'PARAM') = (param_index IS NOT NULL)),
    CHECK ((value_role = 'CONST') = (literal IS NOT NULL)),
    CHECK ((value_role = 'GLOBAL') = (symbol_id IS NOT NULL))
);

CREATE TABLE basic_blocks (
    id INTEGER PRIMARY KEY,
    function_id INTEGER NOT NULL REFERENCES functions(id),
    local_key TEXT NOT NULL,
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    UNIQUE (function_id, local_key),
    UNIQUE (function_id, ordinal)
);

CREATE TABLE instructions (
    id INTEGER PRIMARY KEY,
    block_id INTEGER NOT NULL REFERENCES basic_blocks(id),
    local_key TEXT NOT NULL,
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    opcode TEXT NOT NULL,
    result_value_id INTEGER REFERENCES values_ir(id),
    is_terminator INTEGER NOT NULL DEFAULT 0 CHECK (is_terminator IN (0,1)),
    payload BLOB,                                            -- OperationPayload protobuf
    primary_span_id INTEGER REFERENCES source_spans(id),
    UNIQUE (block_id, local_key),
    UNIQUE (block_id, ordinal)
);
CREATE UNIQUE INDEX idx_one_terminator_per_block ON instructions(block_id) WHERE is_terminator = 1;

CREATE TABLE instruction_operands (
    instruction_id INTEGER NOT NULL REFERENCES instructions(id),
    operand_index INTEGER NOT NULL CHECK (operand_index >= 0),
    value_id INTEGER NOT NULL REFERENCES values_ir(id),
    PRIMARY KEY (instruction_id, operand_index)
);

CREATE TABLE phi_inputs (
    instruction_id INTEGER NOT NULL REFERENCES instructions(id),
    input_index INTEGER NOT NULL CHECK (input_index >= 0),
    predecessor_block_id INTEGER NOT NULL REFERENCES basic_blocks(id),
    value_id INTEGER NOT NULL REFERENCES values_ir(id),
    PRIMARY KEY (instruction_id, input_index),
    UNIQUE (instruction_id, predecessor_block_id)
);

CREATE TABLE control_edges (
    id INTEGER PRIMARY KEY,
    local_key TEXT NOT NULL,
    source_block_id INTEGER NOT NULL REFERENCES basic_blocks(id),
    target_block_id INTEGER NOT NULL REFERENCES basic_blocks(id),
    terminator_instruction_id INTEGER NOT NULL REFERENCES instructions(id),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    label TEXT NOT NULL,
    edge_kind TEXT NOT NULL CHECK (edge_kind IN
        ('UNCONDITIONAL','BRANCH_TRUE','BRANCH_FALSE','SWITCH_CASE','SWITCH_DEFAULT','NORMAL','EXCEPTION')),
    guard_value_id INTEGER REFERENCES values_ir(id),
    guard_negated INTEGER NOT NULL DEFAULT 0 CHECK (guard_negated IN (0,1)),
    exception_filter_type_id INTEGER REFERENCES cir_types(id),
    exception_catch_all INTEGER NOT NULL DEFAULT 0 CHECK (exception_catch_all IN (0,1)),
    UNIQUE (terminator_instruction_id, ordinal)
);

CREATE TABLE control_edge_case_values (
    edge_id INTEGER NOT NULL REFERENCES control_edges(id),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    value_id INTEGER NOT NULL REFERENCES values_ir(id),
    PRIMARY KEY (edge_id, ordinal)
);

-- Sites de sortie anormale implicite, inventoriés même sans arête CFG explicite.
CREATE TABLE implicit_exit_sites (
  id INTEGER PRIMARY KEY,
  snapshot_id INTEGER NOT NULL REFERENCES snapshots(id),
  unit_id INTEGER NOT NULL REFERENCES compilation_units(id),
  function_id INTEGER NOT NULL REFERENCES functions(id),
  source_span_id INTEGER REFERENCES source_spans(id),
  local_key TEXT NOT NULL,
  exit_kind TEXT NOT NULL,
  modeling_status TEXT NOT NULL CHECK(modeling_status IN ('MODELED','UNMODELED','UNKNOWN')),
  UNIQUE(snapshot_id, local_key)
);

CREATE TABLE source_decisions (
    id INTEGER PRIMARY KEY,
    snapshot_id INTEGER NOT NULL REFERENCES snapshots(id),
    unit_id INTEGER NOT NULL REFERENCES compilation_units(id),
    function_id INTEGER REFERENCES functions(id),            -- NULL hors fonction
    parent_decision_id INTEGER REFERENCES source_decisions(id),
    local_key TEXT NOT NULL,
    kind TEXT NOT NULL,
    source_span_id INTEGER NOT NULL REFERENCES source_spans(id),
    UNIQUE (snapshot_id, local_key)
);

CREATE TABLE source_outcomes (
    id INTEGER PRIMARY KEY,
    decision_id INTEGER NOT NULL REFERENCES source_decisions(id),
    local_key TEXT NOT NULL,
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    label TEXT NOT NULL,
    elided INTEGER NOT NULL DEFAULT 0 CHECK (elided IN (0,1)),
    activation_mapping_complete INTEGER NOT NULL DEFAULT 0 CHECK (activation_mapping_complete IN (0,1)),
    UNIQUE (decision_id, ordinal)
);

CREATE TABLE ir_decisions (
    id INTEGER PRIMARY KEY,
    terminator_instruction_id INTEGER NOT NULL UNIQUE REFERENCES instructions(id),
    kind TEXT NOT NULL CHECK (kind IN ('BRANCH','SWITCH','INVOKE'))
);

CREATE TABLE ir_outcomes (
    id INTEGER PRIMARY KEY,
    decision_id INTEGER NOT NULL REFERENCES ir_decisions(id),
    control_edge_id INTEGER NOT NULL UNIQUE REFERENCES control_edges(id),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    label TEXT NOT NULL,
    UNIQUE (decision_id, ordinal)
);

CREATE TABLE source_ir_links (
    source_outcome_id INTEGER NOT NULL REFERENCES source_outcomes(id),
    ir_outcome_id INTEGER NOT NULL REFERENCES ir_outcomes(id),
    mapping_kind TEXT NOT NULL CHECK (mapping_kind IN
        ('DIRECT','SHORT_CIRCUIT','SWITCH_CASE','LOOP_CONDITION','IMPLICIT')),
    quality TEXT NOT NULL CHECK (quality IN ('EXACT','APPROX','HEURISTIC')),
    inverted INTEGER NOT NULL DEFAULT 0 CHECK (inverted IN (0,1)),
    PRIMARY KEY (source_outcome_id, ir_outcome_id)
);

-- v1.2 : activation source ; les liens source_ir_links ne sont pas des formules de preuve.
CREATE TABLE source_outcome_activation_cases (
    id INTEGER PRIMARY KEY,
    source_outcome_id INTEGER NOT NULL REFERENCES source_outcomes(id),
    case_ordinal INTEGER NOT NULL CHECK (case_ordinal >= 0),
    quality TEXT NOT NULL CHECK (quality IN ('EXACT','APPROX','HEURISTIC')),
    UNIQUE (source_outcome_id, case_ordinal)
);

CREATE TABLE source_outcome_activation_terms (
    case_id INTEGER NOT NULL REFERENCES source_outcome_activation_cases(id),
    term_ordinal INTEGER NOT NULL CHECK (term_ordinal >= 0),
    ir_outcome_id INTEGER NOT NULL REFERENCES ir_outcomes(id),
    PRIMARY KEY (case_id, term_ordinal)
);
CREATE INDEX idx_source_activation_term_outcome
  ON source_outcome_activation_terms(ir_outcome_id);

CREATE TABLE call_sites (
    id INTEGER PRIMARY KEY,
    instruction_id INTEGER NOT NULL UNIQUE REFERENCES instructions(id),
    dispatch_kind TEXT NOT NULL CHECK (dispatch_kind IN
        ('STATIC','VIRTUAL','INTERFACE','TRAIT','REFLECTIVE','DYNAMIC','UNKNOWN')),
    resolution_status TEXT NOT NULL CHECK (resolution_status IN
        ('RESOLVED_UNIQUE','RESOLVED_POSSIBLE','UNRESOLVED','EXTERNAL')),
    world_closed INTEGER NOT NULL DEFAULT 0 CHECK (world_closed IN (0,1)),
    may_raise INTEGER NOT NULL DEFAULT 1 CHECK (may_raise IN (0,1)),
    may_mutate_memory INTEGER NOT NULL DEFAULT 1 CHECK (may_mutate_memory IN (0,1)),
    is_pure_proven INTEGER NOT NULL DEFAULT 0 CHECK (is_pure_proven IN (0,1)),
    is_referentially_transparent_proven INTEGER NOT NULL DEFAULT 0 CHECK (is_referentially_transparent_proven IN (0,1)),
    CHECK (is_referentially_transparent_proven = 0 OR is_pure_proven = 1)
);

CREATE TABLE call_targets (
    call_site_id INTEGER NOT NULL REFERENCES call_sites(id),
    target_function_id INTEGER NOT NULL REFERENCES functions(id),
    resolution_method TEXT NOT NULL,
    PRIMARY KEY (call_site_id, target_function_id)
);

CREATE TABLE diagnostics (
    id INTEGER PRIMARY KEY,
    snapshot_id INTEGER NOT NULL REFERENCES snapshots(id),
    unit_id INTEGER REFERENCES compilation_units(id),
    function_id INTEGER REFERENCES functions(id),
    source_span_id INTEGER REFERENCES source_spans(id),
    analysis_run_id INTEGER REFERENCES analysis_runs(id),    -- diagnostics du moteur ; NULL pour ceux de l'import
    severity TEXT NOT NULL CHECK (severity IN ('INFO','WARNING','ERROR')),
    code TEXT NOT NULL,
    impact TEXT NOT NULL CHECK (impact IN
        ('INVENTORY','SYMBOLIC','EFFECTS','CALLGRAPH','TYPES','CONCURRENCY','OTHER')),
    message TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- DAG symbolique canonique par snapshot
CREATE TABLE symbolic_expressions (
    id INTEGER PRIMARY KEY,
    snapshot_id INTEGER NOT NULL REFERENCES snapshots(id),
    expression_hash TEXT NOT NULL,
    kind TEXT NOT NULL,
    type_id INTEGER REFERENCES cir_types(id),
    payload BLOB,
    UNIQUE (snapshot_id, expression_hash)
);

CREATE TABLE expression_operands (
    expression_id INTEGER NOT NULL REFERENCES symbolic_expressions(id),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    operand_expression_id INTEGER NOT NULL REFERENCES symbolic_expressions(id),
    PRIMARY KEY (expression_id, ordinal)
);

CREATE TABLE analysis_runs (
    id INTEGER PRIMARY KEY,
    snapshot_id INTEGER NOT NULL REFERENCES snapshots(id),
    entry_function_id INTEGER REFERENCES functions(id),
    analysis_kind TEXT NOT NULL,                             -- 'BRANCH_ANALYSIS','SUMMARY','BEHAVIOR'
    engine_version TEXT NOT NULL,
    solver_name TEXT,
    solver_version TEXT,
    context_name TEXT NOT NULL DEFAULT 'default',
    context_json TEXT NOT NULL DEFAULT '{}',
    budget_json TEXT NOT NULL DEFAULT '{}',
    config_fingerprint TEXT NOT NULL,
    assumptions_fingerprint TEXT NOT NULL,
    status TEXT NOT NULL CHECK
        (status IN ('PENDING','RUNNING','COMPLETED','PARTIAL','FAILED','CANCELLED')),
    exploration_closed INTEGER NOT NULL DEFAULT 0 CHECK (exploration_closed IN (0,1)),
    cut_count INTEGER NOT NULL DEFAULT 0 CHECK (cut_count >= 0),
    metrics_json TEXT NOT NULL DEFAULT '{}',
    error_message TEXT,
    started_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    finished_at TEXT
);

CREATE TABLE analysis_assumptions (
    id INTEGER PRIMARY KEY,
    analysis_run_id INTEGER NOT NULL REFERENCES analysis_runs(id),
    kind TEXT NOT NULL,
    description TEXT NOT NULL,
    payload TEXT
);

CREATE TABLE solver_queries (
    id INTEGER PRIMARY KEY,
    analysis_run_id INTEGER NOT NULL REFERENCES analysis_runs(id),
    query_fingerprint TEXT NOT NULL,
    smtlib TEXT,
    result TEXT NOT NULL CHECK (result IN ('SAT','UNSAT','UNKNOWN','TIMEOUT','ERROR')),
    timeout_kind TEXT CHECK (timeout_kind IS NULL OR timeout_kind IN ('RLIMIT','WALL_CLOCK','BUDGET')),
    duration_ms INTEGER CHECK (duration_ms IS NULL OR duration_ms >= 0),
    reason TEXT,
    solver_seed INTEGER NOT NULL DEFAULT 0,
    solver_rlimit INTEGER NOT NULL DEFAULT 0,
    UNIQUE (analysis_run_id, query_fingerprint),
    CHECK (result <> 'UNSAT' OR smtlib IS NOT NULL)
);

CREATE TABLE outcome_arrivals (
    id INTEGER PRIMARY KEY,
    analysis_run_id INTEGER NOT NULL REFERENCES analysis_runs(id),
    ir_outcome_id INTEGER NOT NULL REFERENCES ir_outcomes(id),
    arrival_seq INTEGER NOT NULL CHECK (arrival_seq >= 0),
    query_id INTEGER REFERENCES solver_queries(id),           -- NULL si aucune requête (budget)
    result TEXT NOT NULL CHECK (result IN ('SAT','UNSAT','UNKNOWN','TIMEOUT','UNSUPPORTED')),
    soundness TEXT NOT NULL CHECK (soundness IN ('EXACT','OVERAPPROX','OPAQUE','UNSUPPORTED')),
    path_json TEXT NOT NULL DEFAULT '[]',                     -- ids d'ir_outcomes empruntées
    UNIQUE (analysis_run_id, ir_outcome_id, arrival_seq)
);

CREATE TABLE proof_records (
    id INTEGER PRIMARY KEY,
    analysis_run_id INTEGER NOT NULL REFERENCES analysis_runs(id),
    ir_outcome_id INTEGER NOT NULL REFERENCES ir_outcomes(id),
    verifier_name TEXT NOT NULL,
    verifier_version TEXT NOT NULL,
    verdict TEXT NOT NULL CHECK (verdict IN ('ACCEPTED','REJECTED')),
    verification_level TEXT NOT NULL DEFAULT 'SOLVER_REPLAYED' CHECK
      (verification_level IN ('SOLVER_REPLAYED','MODEL_UNREACHABLE','CERTIFIED_UNREACHABLE')),
    model_soundness TEXT NOT NULL CHECK (model_soundness IN ('EXACT','OVERAPPROX','OPAQUE','UNSUPPORTED')),
    eliminated_arrivals INTEGER NOT NULL CHECK (eliminated_arrivals >= 0),
    residual_frontier INTEGER NOT NULL CHECK (residual_frontier >= 0),
    reason TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CHECK (verdict <> 'ACCEPTED' OR verification_level = 'MODEL_UNREACHABLE') -- v0.x
);

CREATE TABLE branch_analysis_results (
    id INTEGER PRIMARY KEY,
    analysis_run_id INTEGER NOT NULL REFERENCES analysis_runs(id),
    ir_outcome_id INTEGER NOT NULL REFERENCES ir_outcomes(id),
    status TEXT NOT NULL CHECK (status IN
      ('UNANALYZED','POSSIBLE','WITNESSED','PROVEN_UNREACHABLE','UNKNOWN','TIMEOUT','UNSUPPORTED')),
    soundness TEXT NOT NULL CHECK (soundness IN ('EXACT','OVERAPPROX','OPAQUE','UNSUPPORTED')),
    explanation TEXT,
    proof_id INTEGER REFERENCES proof_records(id),
    UNIQUE (analysis_run_id, ir_outcome_id)
);

CREATE TABLE branch_cases (
    id INTEGER PRIMARY KEY,
    result_id INTEGER NOT NULL REFERENCES branch_analysis_results(id),
    guard_expression_id INTEGER REFERENCES symbolic_expressions(id),
    witness_json TEXT,
    validation_method TEXT,                                  -- 'SMT_MODEL' en v0.3
    note TEXT
);

CREATE TABLE function_summaries (
    id INTEGER PRIMARY KEY,
    function_id INTEGER NOT NULL REFERENCES functions(id),
    produced_by_run_id INTEGER NOT NULL REFERENCES analysis_runs(id),
    context_fingerprint TEXT NOT NULL,
    summary_fingerprint TEXT NOT NULL,
    precision TEXT NOT NULL CHECK (precision IN ('EXACT','OVERAPPROX','PARTIAL','UNKNOWN')),
    complete INTEGER NOT NULL CHECK (complete IN (0,1)),
    unknowns_json TEXT NOT NULL DEFAULT '[]',
    reused_from_summary_id INTEGER REFERENCES function_summaries(id),
    UNIQUE (function_id, context_fingerprint, summary_fingerprint)
);

CREATE TABLE summary_cases (
    id INTEGER PRIMARY KEY,
    summary_id INTEGER NOT NULL REFERENCES function_summaries(id),
    case_ordinal INTEGER NOT NULL CHECK (case_ordinal >= 0),
    guard_json TEXT NOT NULL,                                -- tableau d'ids symbolic_expressions (conjonction ordonnée)
    exit_kind TEXT NOT NULL CHECK (exit_kind IN ('RETURN','PANIC','THROW','UNREACHABLE_EXIT')),
    memory_delta_json TEXT NOT NULL DEFAULT '[]',
    precision TEXT NOT NULL CHECK (precision IN ('EXACT','OVERAPPROX','PARTIAL','UNKNOWN')),
    exit_span_id INTEGER REFERENCES source_spans(id),
    UNIQUE (summary_id, case_ordinal)
);

CREATE TABLE summary_case_returns (
    summary_case_id INTEGER NOT NULL REFERENCES summary_cases(id),
    return_ordinal INTEGER NOT NULL CHECK (return_ordinal >= 0),
    expression_id INTEGER NOT NULL REFERENCES symbolic_expressions(id),
    PRIMARY KEY (summary_case_id, return_ordinal)
);

CREATE TABLE summary_case_outcomes (
    summary_case_id INTEGER NOT NULL REFERENCES summary_cases(id),
    path_ordinal INTEGER NOT NULL CHECK (path_ordinal >= 0),
    ir_outcome_id INTEGER NOT NULL REFERENCES ir_outcomes(id),
    PRIMARY KEY (summary_case_id, path_ordinal)
);

CREATE TABLE observable_effects (
    id INTEGER PRIMARY KEY,
    summary_case_id INTEGER NOT NULL REFERENCES summary_cases(id),
    effect_ordinal INTEGER NOT NULL CHECK (effect_ordinal >= 0),
    effect_kind TEXT NOT NULL,
    target_descriptor TEXT,
    payload TEXT,
    semantic_model_fingerprint TEXT,
    precision TEXT NOT NULL,
    UNIQUE (summary_case_id, effect_ordinal)
);

CREATE TABLE behavior_scenarios (
    id INTEGER PRIMARY KEY,
    analysis_run_id INTEGER NOT NULL REFERENCES analysis_runs(id),
    entry_function_id INTEGER NOT NULL REFERENCES functions(id),
    local_key TEXT NOT NULL,
    name TEXT,
    status TEXT NOT NULL,
    precision TEXT NOT NULL,
    UNIQUE (analysis_run_id, local_key)
);

CREATE TABLE behavior_cases (
    id INTEGER PRIMARY KEY,
    behavior_id INTEGER NOT NULL REFERENCES behavior_scenarios(id),
    summary_case_id INTEGER REFERENCES summary_cases(id),
    case_ordinal INTEGER NOT NULL CHECK (case_ordinal >= 0),
    guard_text TEXT NOT NULL,                                -- rendu R1-R6
    result_text TEXT NOT NULL,
    effects_text TEXT,
    precision TEXT NOT NULL,
    UNIQUE (behavior_id, case_ordinal)
);

CREATE TABLE behavior_case_outcomes (
    behavior_case_id INTEGER NOT NULL REFERENCES behavior_cases(id),
    ir_outcome_id INTEGER NOT NULL REFERENCES ir_outcomes(id),
    PRIMARY KEY (behavior_case_id, ir_outcome_id)
);

CREATE TABLE functional_rules (
    id INTEGER PRIMARY KEY,
    snapshot_id INTEGER NOT NULL REFERENCES snapshots(id),
    behavior_case_id INTEGER REFERENCES behavior_cases(id),
    rule_key TEXT NOT NULL,
    short_name TEXT,
    description TEXT NOT NULL,
    origin TEXT NOT NULL CHECK (origin IN ('DETERMINISTIC','ABSTRACT_DERIVED','LLM_INFERRED')),
    validation_status TEXT NOT NULL DEFAULT 'UNVALIDATED' CHECK (validation_status IN
        ('UNVALIDATED','HUMAN_VALIDATED','REJECTED','STALE')),
    prompt_version TEXT,
    llm_model TEXT,
    model_parameters TEXT,
    UNIQUE (snapshot_id, rule_key),
    UNIQUE (behavior_case_id)
);

CREATE TABLE knowledge_evidence (
    id INTEGER PRIMARY KEY,
    functional_rule_id INTEGER NOT NULL REFERENCES functional_rules(id),
    source_span_id INTEGER REFERENCES source_spans(id),
    instruction_id INTEGER REFERENCES instructions(id),
    ir_outcome_id INTEGER REFERENCES ir_outcomes(id),
    summary_case_id INTEGER REFERENCES summary_cases(id),
    relation TEXT NOT NULL,
    CHECK ((source_span_id IS NOT NULL) + (instruction_id IS NOT NULL)
         + (ir_outcome_id IS NOT NULL) + (summary_case_id IS NOT NULL) = 1)
);

CREATE TABLE analysis_dependencies (
    id INTEGER PRIMARY KEY,
    snapshot_id INTEGER NOT NULL REFERENCES snapshots(id),
    dependent_kind TEXT NOT NULL,
    dependent_key TEXT NOT NULL,
    dependency_kind TEXT NOT NULL,
    dependency_key TEXT NOT NULL,
    UNIQUE (snapshot_id, dependent_kind, dependent_key, dependency_kind, dependency_key)
);

-- Index
CREATE INDEX idx_units_snapshot ON compilation_units(snapshot_id);
CREATE INDEX idx_files_snapshot ON source_files(snapshot_id);
CREATE INDEX idx_spans_file ON source_spans(source_file_id);
CREATE INDEX idx_symbols_name ON symbols(snapshot_id, qualified_name);
CREATE INDEX idx_symbols_unit ON symbols(unit_id);
CREATE INDEX idx_values_function ON values_ir(function_id);
CREATE INDEX idx_blocks_function ON basic_blocks(function_id);
CREATE INDEX idx_instr_block ON instructions(block_id, ordinal);
CREATE INDEX idx_operands_value ON instruction_operands(value_id);
CREATE INDEX idx_phi_pred ON phi_inputs(predecessor_block_id);
CREATE INDEX idx_edges_source ON control_edges(source_block_id);
CREATE INDEX idx_edges_target ON control_edges(target_block_id);
CREATE INDEX idx_source_decisions_snapshot ON source_decisions(snapshot_id);
CREATE INDEX idx_source_decisions_function ON source_decisions(function_id);
CREATE INDEX idx_source_outcomes_decision ON source_outcomes(decision_id);
CREATE INDEX idx_ir_outcomes_decision ON ir_outcomes(decision_id);
CREATE INDEX idx_source_ir_ir ON source_ir_links(ir_outcome_id);
CREATE INDEX idx_call_targets_function ON call_targets(target_function_id);
CREATE INDEX idx_diagnostics_snapshot ON diagnostics(snapshot_id, impact);
CREATE INDEX idx_diagnostics_run ON diagnostics(analysis_run_id);
CREATE INDEX idx_expression_operands_child ON expression_operands(operand_expression_id);
CREATE INDEX idx_analysis_run_snapshot ON analysis_runs(snapshot_id, analysis_kind);
CREATE INDEX idx_arrivals_outcome ON outcome_arrivals(analysis_run_id, ir_outcome_id);
CREATE INDEX idx_branch_results_run_status ON branch_analysis_results(analysis_run_id, status);
CREATE INDEX idx_summaries_function ON function_summaries(function_id);
CREATE INDEX idx_behaviors_entry ON behavior_scenarios(entry_function_id);
CREATE INDEX idx_rules_snapshot ON functional_rules(snapshot_id, validation_status);
CREATE INDEX idx_evidence_rule ON knowledge_evidence(functional_rule_id);
CREATE INDEX idx_dependencies_dependency
    ON analysis_dependencies(snapshot_id, dependency_kind, dependency_key);

-- La preuve ne peut être ACCEPTED avant fermeture globale du run.
CREATE TRIGGER trg_proof_accept_only_closed
BEFORE INSERT ON proof_records
WHEN NEW.verdict='ACCEPTED'
  AND NOT EXISTS (
      SELECT 1 FROM analysis_runs r
      WHERE r.id=NEW.analysis_run_id AND r.status='COMPLETED'
        AND r.exploration_closed=1 AND r.cut_count=0)
BEGIN SELECT RAISE(ABORT, 'E_PROOF_REQUIRED'); END;

-- V1.2 : preuve et faits à sa base sont APPEND-ONLY après acceptation.
CREATE TRIGGER trg_proof_immutable_update
BEFORE UPDATE ON proof_records
BEGIN SELECT RAISE(ABORT, 'E_PROOF_IMMUTABLE'); END;
CREATE TRIGGER trg_proof_immutable_delete
BEFORE DELETE ON proof_records
BEGIN SELECT RAISE(ABORT, 'E_PROOF_IMMUTABLE'); END;

-- Interdire la mutation des attributs de run déterminants (finished_at/metrics_json restent modifiables ;
-- status : voir trg_proven_run_status_final).
CREATE TRIGGER trg_proven_run_frozen_update
BEFORE UPDATE OF snapshot_id, entry_function_id, engine_version, solver_name, solver_version,
                 context_name, context_json, budget_json, config_fingerprint,
                 assumptions_fingerprint, exploration_closed, cut_count
ON analysis_runs
WHEN EXISTS (SELECT 1 FROM proof_records p WHERE p.analysis_run_id=OLD.id AND p.verdict='ACCEPTED')
BEGIN SELECT RAISE(ABORT, 'E_PROOF_SUPPORT_FROZEN'); END;
CREATE TRIGGER trg_proven_run_frozen_delete
BEFORE DELETE ON analysis_runs
WHEN EXISTS (SELECT 1 FROM proof_records p WHERE p.analysis_run_id=OLD.id AND p.verdict='ACCEPTED')
BEGIN SELECT RAISE(ABORT, 'E_PROOF_SUPPORT_FROZEN'); END;

-- v1.4 : le statut d'un run portant une preuve acceptée est définitif (COMPLETED).
CREATE TRIGGER trg_proven_run_status_final
BEFORE UPDATE OF status ON analysis_runs
WHEN NEW.status <> 'COMPLETED'
  AND EXISTS (SELECT 1 FROM proof_records p WHERE p.analysis_run_id=OLD.id AND p.verdict='ACCEPTED')
BEGIN SELECT RAISE(ABORT, 'E_PROOF_SUPPORT_FROZEN'); END;

-- Les arrivées, requêtes et hypothèses d'un run certifié au niveau modèle sont figées.
-- NEW pour les INSERT ; OLD pour UPDATE/DELETE.
CREATE TRIGGER trg_proof_support_analysis_assumptions_insert
BEFORE INSERT ON analysis_assumptions
WHEN EXISTS (SELECT 1 FROM proof_records p
             WHERE p.analysis_run_id=NEW.analysis_run_id AND p.verdict='ACCEPTED')
BEGIN SELECT RAISE(ABORT, 'E_PROOF_SUPPORT_FROZEN'); END;
CREATE TRIGGER trg_proof_support_analysis_assumptions_update
BEFORE UPDATE ON analysis_assumptions
WHEN EXISTS (SELECT 1 FROM proof_records p
             WHERE p.analysis_run_id=OLD.analysis_run_id AND p.verdict='ACCEPTED')
BEGIN SELECT RAISE(ABORT, 'E_PROOF_SUPPORT_FROZEN'); END;
CREATE TRIGGER trg_proof_support_analysis_assumptions_delete
BEFORE DELETE ON analysis_assumptions
WHEN EXISTS (SELECT 1 FROM proof_records p
             WHERE p.analysis_run_id=OLD.analysis_run_id AND p.verdict='ACCEPTED')
BEGIN SELECT RAISE(ABORT, 'E_PROOF_SUPPORT_FROZEN'); END;
CREATE TRIGGER trg_proof_support_solver_queries_insert
BEFORE INSERT ON solver_queries
WHEN EXISTS (SELECT 1 FROM proof_records p
             WHERE p.analysis_run_id=NEW.analysis_run_id AND p.verdict='ACCEPTED')
BEGIN SELECT RAISE(ABORT, 'E_PROOF_SUPPORT_FROZEN'); END;
CREATE TRIGGER trg_proof_support_solver_queries_update
BEFORE UPDATE ON solver_queries
WHEN EXISTS (SELECT 1 FROM proof_records p
             WHERE p.analysis_run_id=OLD.analysis_run_id AND p.verdict='ACCEPTED')
BEGIN SELECT RAISE(ABORT, 'E_PROOF_SUPPORT_FROZEN'); END;
CREATE TRIGGER trg_proof_support_solver_queries_delete
BEFORE DELETE ON solver_queries
WHEN EXISTS (SELECT 1 FROM proof_records p
             WHERE p.analysis_run_id=OLD.analysis_run_id AND p.verdict='ACCEPTED')
BEGIN SELECT RAISE(ABORT, 'E_PROOF_SUPPORT_FROZEN'); END;
CREATE TRIGGER trg_proof_support_outcome_arrivals_insert
BEFORE INSERT ON outcome_arrivals
WHEN EXISTS (SELECT 1 FROM proof_records p
             WHERE p.analysis_run_id=NEW.analysis_run_id AND p.verdict='ACCEPTED')
BEGIN SELECT RAISE(ABORT, 'E_PROOF_SUPPORT_FROZEN'); END;
CREATE TRIGGER trg_proof_support_outcome_arrivals_update
BEFORE UPDATE ON outcome_arrivals
WHEN EXISTS (SELECT 1 FROM proof_records p
             WHERE p.analysis_run_id=OLD.analysis_run_id AND p.verdict='ACCEPTED')
BEGIN SELECT RAISE(ABORT, 'E_PROOF_SUPPORT_FROZEN'); END;
CREATE TRIGGER trg_proof_support_outcome_arrivals_delete
BEFORE DELETE ON outcome_arrivals
WHEN EXISTS (SELECT 1 FROM proof_records p
             WHERE p.analysis_run_id=OLD.analysis_run_id AND p.verdict='ACCEPTED')
BEGIN SELECT RAISE(ABORT, 'E_PROOF_SUPPORT_FROZEN'); END;

-- Triggers : règles de soundness portées par la base (§8.3)
CREATE TRIGGER trg_proven_requires_proof_ins
BEFORE INSERT ON branch_analysis_results
WHEN NEW.status = 'PROVEN_UNREACHABLE'
BEGIN
    SELECT RAISE(ABORT, 'E_PROOF_REQUIRED')
    WHERE NOT EXISTS (
        SELECT 1 FROM proof_records p
        WHERE p.id = NEW.proof_id
          AND p.verdict = 'ACCEPTED'
          AND p.verification_level = 'MODEL_UNREACHABLE'
          AND p.residual_frontier = 0
          AND p.model_soundness IN ('EXACT','OVERAPPROX','OPAQUE')
          AND p.ir_outcome_id = NEW.ir_outcome_id
          AND p.analysis_run_id = NEW.analysis_run_id
          AND EXISTS (
              SELECT 1 FROM analysis_runs r
              WHERE r.id = NEW.analysis_run_id
                AND r.status = 'COMPLETED' AND r.exploration_closed = 1 AND r.cut_count = 0));
END;

CREATE TRIGGER trg_proven_requires_proof_upd
BEFORE UPDATE OF status, proof_id, ir_outcome_id, analysis_run_id ON branch_analysis_results
WHEN NEW.status = 'PROVEN_UNREACHABLE'
BEGIN
    SELECT RAISE(ABORT, 'E_PROOF_REQUIRED')
    WHERE NOT EXISTS (
        SELECT 1 FROM proof_records p
        WHERE p.id = NEW.proof_id
          AND p.verdict = 'ACCEPTED'
          AND p.verification_level = 'MODEL_UNREACHABLE'
          AND p.residual_frontier = 0
          AND p.model_soundness IN ('EXACT','OVERAPPROX','OPAQUE')
          AND p.ir_outcome_id = NEW.ir_outcome_id
          AND p.analysis_run_id = NEW.analysis_run_id
          AND EXISTS (
              SELECT 1 FROM analysis_runs r
              WHERE r.id = NEW.analysis_run_id
                AND r.status = 'COMPLETED' AND r.exploration_closed = 1 AND r.cut_count = 0));
END;

-- Un résultat démontré ne peut être supprimé/rétrogradé dans le même run ;
-- une correction doit créer un nouveau run avec provenance.
CREATE TRIGGER trg_proven_result_immutable_update
BEFORE UPDATE ON branch_analysis_results
WHEN OLD.status = 'PROVEN_UNREACHABLE'
BEGIN SELECT RAISE(ABORT, 'E_PROOF_SUPPORT_FROZEN'); END;
CREATE TRIGGER trg_proven_result_immutable_delete
BEFORE DELETE ON branch_analysis_results
WHEN OLD.status = 'PROVEN_UNREACHABLE'
BEGIN SELECT RAISE(ABORT, 'E_PROOF_SUPPORT_FROZEN'); END;

CREATE TRIGGER trg_no_witnessed_ins
BEFORE INSERT ON branch_analysis_results
WHEN NEW.status = 'WITNESSED'
BEGIN
    SELECT RAISE(ABORT, 'E_WITNESSED_NOT_PRODUCIBLE');
END;

CREATE TRIGGER trg_no_witnessed_upd
BEFORE UPDATE OF status ON branch_analysis_results
WHEN NEW.status = 'WITNESSED'
BEGIN
    SELECT RAISE(ABORT, 'E_WITNESSED_NOT_PRODUCIBLE');
END;
```

**Liste normative des tables** (test `schema_tables_match` : égalité d'ensemble, hors tables `sqlite_*`) — **47 tables** : `schema_migrations` (runner) + 46 de la migration 0001 :

`repositories, snapshots, compilation_units, source_files, source_spans, cir_types, symbols, functions, values_ir, basic_blocks, instructions, instruction_operands, phi_inputs, control_edges, control_edge_case_values, implicit_exit_sites, source_decisions, source_outcomes, ir_decisions, ir_outcomes, source_ir_links, source_outcome_activation_cases, source_outcome_activation_terms, call_sites, call_targets, diagnostics, symbolic_expressions, expression_operands, analysis_runs, analysis_assumptions, solver_queries, outcome_arrivals, proof_records, branch_analysis_results, branch_cases, function_summaries, summary_cases, summary_case_returns, summary_case_outcomes, observable_effects, behavior_scenarios, behavior_cases, behavior_case_outcomes, functional_rules, knowledge_evidence, analysis_dependencies`.

Nombre de triggers : **21** : les 4 triggers de la v1.1 (`trg_proven_requires_proof_ins`, `trg_proven_requires_proof_upd`, `trg_no_witnessed_ins`, `trg_no_witnessed_upd`) + 13 triggers v1.2 (preuve immuable, gel du run et des faits support) + 3 triggers v1.3 (résultat démontré immuable, acceptation seulement si run fermé) + 1 trigger v1.4 (`trg_proven_run_status_final` : statut définitif d'un run prouvé).

## 8.3. Justification des règles portées par la base

- `PROVEN_UNREACHABLE` ne peut pas être écrit sans un `proof_record` `ACCEPTED` enregistré **sur un run `COMPLETED`, exploration fermée et `cut_count=0`** portant sur **la même issue et le même run**, `residual_frontier = 0` et une soundness ≠ `UNSUPPORTED` (le vérificateur §6.6 est le seul producteur légitime de `proof_records`).
- `WITNESSED` est interdit en v0.x (aucun producteur dynamique). Une future migration dynamique **supprimera explicitement** ces deux triggers avec un ADR.
- Retours multiples (Go) : `summary_case_returns` porte **une ligne par valeur de retour** dès la v0.4 ; il n'y a pas de colonne « retour unique ».

**v1.3 — intégrité de preuve.** `proof_records` est immuable (UPDATE/DELETE refusés) ; après une preuve acceptée, `analysis_assumptions`, `solver_queries`, `outcome_arrivals` et les attributs déterminants de `analysis_runs` deviennent append-only/figés au niveau du run ; `metrics_json` et `finished_at` restent modifiables (non probants) ; `status` reste `COMPLETED` (trigger v1.4). **Les métriques finales sont néanmoins enregistrées avant l’insertion du premier `proof_record` accepté**. Les rejeux de vérification n’insèrent **aucune** nouvelle ligne dans `solver_queries` du run prouvé. `branch_analysis_results` en statut `PROVEN_UNREACHABLE` est immuable (UPDATE et DELETE interdits). L’ajout d’une nouvelle analyse crée un **nouveau run**. Les mutations sont interdites même lorsque le FK reste valide. Les triggers figurent dans la migration 0001 ; tester le scénario `ACCEPTED → UPDATE verdict='REJECTED'` ainsi que UPDATE/DELETE/INSERT des faits support. La commande `ske db check` vérifie également le niveau de garantie modèle des preuves.

## 8.4. Extensions prévues (migrations suivantes, sans casser l'historique)

**Contrat de migration v0.5 `migrations/0002_capabilities.sql` (normatif ; à implémenter en v0.5, sans modifier les golden de v0.1) :**

```sql
CREATE TABLE functional_capabilities (
  id INTEGER PRIMARY KEY,
  snapshot_id INTEGER NOT NULL REFERENCES snapshots(id),
  canonical_key TEXT NOT NULL,
  title TEXT,
  origin TEXT NOT NULL CHECK (origin IN ('DETERMINISTIC','ABSTRACT_DERIVED','LLM_INFERRED')),
  validation_status TEXT NOT NULL CHECK (validation_status IN ('UNVALIDATED','HUMAN_VALIDATED','REJECTED','STALE')),
  UNIQUE(snapshot_id, canonical_key)
);
CREATE TABLE capability_scenarios (
  capability_id INTEGER NOT NULL REFERENCES functional_capabilities(id),
  scenario_id INTEGER NOT NULL REFERENCES behavior_scenarios(id),
  PRIMARY KEY(capability_id, scenario_id)
);
CREATE TABLE scenario_links (
  id INTEGER PRIMARY KEY,
  from_scenario_id INTEGER NOT NULL REFERENCES behavior_scenarios(id),
  to_scenario_id INTEGER REFERENCES behavior_scenarios(id),
  relation_kind TEXT NOT NULL CHECK (relation_kind IN ('CALL','EVENT','HTTP','MESSAGE','PERSISTENCE','EXTERNAL_UNKNOWN')),
  confidence_kind TEXT NOT NULL CHECK (confidence_kind IN ('CONFIRMED','INFERRED','UNKNOWN')),
  evidence_json TEXT NOT NULL DEFAULT '[]',
  assumptions_json TEXT NOT NULL DEFAULT '[]',
  CHECK (to_scenario_id IS NOT NULL OR relation_kind = 'EXTERNAL_UNKNOWN')
);
CREATE INDEX idx_capability_scenarios_scenario ON capability_scenarios(scenario_id);
CREATE INDEX idx_scenario_links_from ON scenario_links(from_scenario_id);
```

`capability_scenarios` et `scenario_links` doivent être validés applicativement pour éviter les références de snapshots incompatibles. Il est interdit de remplir automatiquement `CONFIRMED` à partir du seul nom d’une méthode ou d’un topic.


Relations de mémoire et dépendances de données ; modèles externes versionnés ; configurations de build multiples par unité ; plusieurs spans par objet. Toute migration ultérieure est un **nouveau fichier** `NNNN_*.sql` ; ne **jamais** éditer `0001_initial.sql` après sa publication (le checksum le détecterait).

## 8.5. Contrôles d'intégrité `ske db check` (**normatifs**)

Chaque contrôle est un fichier `crates/ske-storage/sql/checks/chk_NN.sql` retournant **les lignes en violation** (0 ligne = OK). `ske db check` les exécute **tous** et sort en code `6` si au moins un contrôle de sévérité `ERROR` rend ≥ 1 ligne. `CHK-01` utilise `PRAGMA foreign_key_check`.

| ID | Sév. | Requête (SQL de référence) |
|---|---|---|
| CHK-01 | ERROR | `PRAGMA foreign_key_check;` |
| CHK-02 | ERROR | types/symboles/valeurs de snapshots différents : `SELECT s.id FROM symbols s JOIN cir_types t ON t.id=s.type_id WHERE t.snapshot_id<>s.snapshot_id UNION ALL SELECT v.id FROM values_ir v JOIN cir_types t ON t.id=v.type_id WHERE t.snapshot_id<>v.snapshot_id;` |
| CHK-03 | ERROR | valeur d'une fonction d'un autre snapshot : `SELECT v.id FROM values_ir v JOIN functions f ON f.id=v.function_id JOIN symbols s ON s.id=f.symbol_id WHERE s.snapshot_id<>v.snapshot_id;` |
| CHK-04 | ERROR | bloc sans terminateur : `SELECT b.id FROM basic_blocks b WHERE NOT EXISTS (SELECT 1 FROM instructions i WHERE i.block_id=b.id AND i.is_terminator=1);` |
| CHK-05 | ERROR | fonction non externe sans bloc ou sans entrée : `SELECT f.id FROM functions f WHERE f.is_external=0 AND f.inventory_status<>'FAILED' AND (f.entry_block_id IS NULL OR NOT EXISTS (SELECT 1 FROM basic_blocks b WHERE b.function_id=f.id));` |
| CHK-06 | ERROR | terminateur pas en dernier : `SELECT i.id FROM instructions i WHERE i.is_terminator=1 AND i.ordinal < (SELECT MAX(j.ordinal) FROM instructions j WHERE j.block_id=i.block_id);` |
| CHK-07 | ERROR | arête dont la source ≠ bloc du terminateur : `SELECT e.id FROM control_edges e JOIN instructions t ON t.id=e.terminator_instruction_id WHERE t.block_id<>e.source_block_id OR t.is_terminator=0;` |
| CHK-08 | ERROR | arête inter-fonctions : `SELECT e.id FROM control_edges e JOIN basic_blocks s ON s.id=e.source_block_id JOIN basic_blocks d ON d.id=e.target_block_id WHERE s.function_id<>d.function_id;` |
| CHK-09 | ERROR | entrée d'une autre fonction : `SELECT f.id FROM functions f WHERE f.entry_block_id IS NOT NULL AND NOT EXISTS (SELECT 1 FROM basic_blocks b WHERE b.id=f.entry_block_id AND b.function_id=f.id);` |
| CHK-10 | ERROR | issue IR dont l'arête n'appartient pas au terminateur de sa décision : `SELECT o.id FROM ir_outcomes o JOIN ir_decisions d ON d.id=o.decision_id JOIN control_edges e ON e.id=o.control_edge_id WHERE e.terminator_instruction_id<>d.terminator_instruction_id;` |
| CHK-11 | ERROR | PHI : (a) prédécesseur inexistant `SELECT p.instruction_id FROM phi_inputs p JOIN instructions i ON i.id=p.instruction_id WHERE NOT EXISTS (SELECT 1 FROM control_edges e WHERE e.source_block_id=p.predecessor_block_id AND e.target_block_id=i.block_id);` ; (b) prédécesseur manquant `SELECT i.id FROM instructions i WHERE i.opcode='PHI' AND EXISTS (SELECT 1 FROM control_edges e WHERE e.target_block_id=i.block_id AND NOT EXISTS (SELECT 1 FROM phi_inputs p WHERE p.instruction_id=i.id AND p.predecessor_block_id=e.source_block_id));` |
| CHK-12 | ERROR | terminateur à ≥ 2 arêtes sans `ir_decision` : `SELECT e.terminator_instruction_id FROM control_edges e GROUP BY e.terminator_instruction_id HAVING COUNT(*)>=2 AND e.terminator_instruction_id NOT IN (SELECT terminator_instruction_id FROM ir_decisions);` |
| CHK-13 | ERROR | compteurs d'unité : `SELECT u.id FROM compilation_units u WHERE u.source_decision_count <> (SELECT COUNT(*) FROM source_decisions d WHERE d.unit_id=u.id) OR u.ir_edge_count <> (SELECT COUNT(*) FROM control_edges e JOIN basic_blocks b ON b.id=e.source_block_id JOIN functions f ON f.id=b.function_id JOIN symbols s ON s.id=f.symbol_id WHERE s.unit_id=u.id) OR u.ir_decision_count <> (SELECT COUNT(*) FROM ir_decisions d JOIN instructions i ON i.id=d.terminator_instruction_id JOIN basic_blocks b ON b.id=i.block_id JOIN functions f ON f.id=b.function_id JOIN symbols s ON s.id=f.symbol_id WHERE s.unit_id=u.id);` |
| CHK-14 | ERROR | snapshot `COMPLETE` avec une unité non `COMPLETE` : `SELECT sn.id FROM snapshots sn WHERE sn.status='CLOSED' AND sn.inventory_status='COMPLETE' AND EXISTS (SELECT 1 FROM compilation_units u WHERE u.snapshot_id=sn.id AND u.inventory_status<>'COMPLETE');` |
| CHK-15 | ERROR | `PROVEN_UNREACHABLE` sans preuve : `SELECT r.id FROM branch_analysis_results r WHERE r.status='PROVEN_UNREACHABLE' AND NOT EXISTS (SELECT 1 FROM proof_records p WHERE p.id=r.proof_id AND p.verdict='ACCEPTED' AND p.verification_level='MODEL_UNREACHABLE' AND p.residual_frontier=0 AND p.ir_outcome_id=r.ir_outcome_id AND p.analysis_run_id=r.analysis_run_id AND EXISTS (SELECT 1 FROM analysis_runs a WHERE a.id=r.analysis_run_id AND a.status='COMPLETED' AND a.exploration_closed=1 AND a.cut_count=0));` |
| CHK-16 | ERROR | résultat d'un run rattaché à une issue d'un autre snapshot : `SELECT r.id FROM branch_analysis_results r JOIN analysis_runs a ON a.id=r.analysis_run_id JOIN ir_outcomes o ON o.id=r.ir_outcome_id JOIN ir_decisions d ON d.id=o.decision_id JOIN instructions i ON i.id=d.terminator_instruction_id JOIN basic_blocks b ON b.id=i.block_id JOIN functions f ON f.id=b.function_id JOIN symbols s ON s.id=f.symbol_id WHERE s.snapshot_id<>a.snapshot_id;` |
| CHK-17 | ERROR | span d'un autre snapshot : `SELECT s.id FROM symbols s JOIN source_spans sp ON sp.id=s.primary_span_id JOIN source_files sf ON sf.id=sp.source_file_id WHERE sf.snapshot_id<>s.snapshot_id UNION ALL SELECT d.id FROM source_decisions d JOIN source_spans sp ON sp.id=d.source_span_id JOIN source_files sf ON sf.id=sp.source_file_id WHERE sf.snapshot_id<>d.snapshot_id;` |
| CHK-18 | ERROR | DAG non acyclique (opérande ≥ parent) : `SELECT expression_id FROM expression_operands WHERE operand_expression_id >= expression_id;` |
| CHK-19 | WARNING | runs inachevés : `SELECT id FROM analysis_runs WHERE status IN ('PENDING','RUNNING');` |
| CHK-20 | ERROR | issue source d'une unité `COMPLETE` ni liée ni `elided` : `SELECT so.id FROM source_outcomes so JOIN source_decisions d ON d.id=so.decision_id JOIN compilation_units u ON u.id=d.unit_id WHERE u.inventory_status='COMPLETE' AND so.elided=0 AND NOT EXISTS (SELECT 1 FROM source_ir_links l WHERE l.source_outcome_id=so.id);` |
| CHK-21 | ERROR | arrivée d'un run dont l'issue appartient à un autre snapshot que le run : `SELECT a.id FROM outcome_arrivals a JOIN analysis_runs r ON r.id=a.analysis_run_id JOIN ir_outcomes o ON o.id=a.ir_outcome_id JOIN ir_decisions d ON d.id=o.decision_id JOIN instructions i ON i.id=d.terminator_instruction_id JOIN basic_blocks b ON b.id=i.block_id JOIN functions f ON f.id=b.function_id JOIN symbols s ON s.id=f.symbol_id WHERE s.snapshot_id<>r.snapshot_id;` |
| CHK-22 | ERROR | cas d’activation vide, case/terme inter-unité, `activation_mapping_complete=1` sans couverture complète de cas `EXACT`, ou issue `elided` avec cas ; SQL normatif ci-dessous. |


**SQL normatif `chk_22.sql`** (une ligne d'identification par violation ; les tests prennent en compte le nombre de lignes distinctes) :

```sql
SELECT 'EMPTY_CASE:' || c.id AS violation
FROM source_outcome_activation_cases c
WHERE NOT EXISTS (
    SELECT 1 FROM source_outcome_activation_terms t WHERE t.case_id=c.id
)
UNION ALL
SELECT 'ELIDED_WITH_CASE:' || so.id
FROM source_outcomes so
WHERE so.elided=1 AND EXISTS (
    SELECT 1 FROM source_outcome_activation_cases c WHERE c.source_outcome_id=so.id
)
UNION ALL
SELECT 'COMPLETE_NO_CASE:' || so.id
FROM source_outcomes so
WHERE so.elided=0 AND so.activation_mapping_complete=1
  AND NOT EXISTS (
    SELECT 1 FROM source_outcome_activation_cases c WHERE c.source_outcome_id=so.id
  )
UNION ALL
SELECT 'COMPLETE_NONEXACT:' || so.id
FROM source_outcomes so
WHERE so.elided=0 AND so.activation_mapping_complete=1
  AND EXISTS (
    SELECT 1 FROM source_outcome_activation_cases c
    WHERE c.source_outcome_id=so.id AND c.quality <> 'EXACT'
  )
UNION ALL
SELECT 'CROSS_UNIT:' || t.case_id || ':' || t.term_ordinal
FROM source_outcome_activation_terms t
JOIN source_outcome_activation_cases c ON c.id=t.case_id
JOIN source_outcomes so ON so.id=c.source_outcome_id
JOIN source_decisions sd ON sd.id=so.decision_id
JOIN ir_outcomes io ON io.id=t.ir_outcome_id
JOIN ir_decisions idec ON idec.id=io.decision_id
JOIN instructions ins ON ins.id=idec.terminator_instruction_id
JOIN basic_blocks bb ON bb.id=ins.block_id
JOIN functions f ON f.id=bb.function_id
JOIN symbols sym ON sym.id=f.symbol_id
WHERE sym.unit_id <> sd.unit_id OR sym.snapshot_id <> sd.snapshot_id;
```

Ce contrôle valide **l'intégrité déclarative**, pas la véracité d'un `activation_mapping_complete=true` signé par l'adaptateur. Toute preuve source exige en plus l'évaluation symbolique conjointe de ses alternatives exactes (§6.6).

**Sortie JSON** de `ske db check --format json` : `{"ok": true|false, "checks": [{"id":"CHK-01","severity":"ERROR","violations":0}, …]}` (22 entrées, dans l'ordre). Avec violations : le champ `"rows"` (max 20 premières valeurs) est ajouté à l'entrée concernée.

## 8.6. Exemple de requête

Lister les issues IR d'une fonction et leur statut pour un run donné, en faisant apparaître celles **non analysées** :

```sql
SELECT i.local_key AS terminator, io.id AS outcome_id, io.label,
       COALESCE(br.status, 'UNANALYZED') AS status, br.explanation
FROM functions f
JOIN basic_blocks bb ON bb.function_id = f.id
JOIN instructions i  ON i.block_id = bb.id AND i.is_terminator = 1
JOIN ir_decisions d  ON d.terminator_instruction_id = i.id
JOIN ir_outcomes io  ON io.decision_id = d.id
LEFT JOIN branch_analysis_results br
       ON br.ir_outcome_id = io.id AND br.analysis_run_id = :run_id
WHERE f.id = :function_id
ORDER BY i.id, io.ordinal;
```

`:run_id` **DOIT** provenir du même snapshot que la fonction ; la couche `ske-query` le vérifie avant d'exécuter (sinon `E_USAGE`, message « run d'un autre snapshot »).

---

# 9. Adaptateur Go — spécification d'implémentation (v0.2)

## 9.1. Outils, configuration et sécurité (**normatif**)

**Bibliothèques :** `golang.org/x/tools/go/packages` (chargement), `go/ast`/`go/token`/`go/types` (décisions source, types), `golang.org/x/tools/go/ssa` + `ssautil` (SSA/CFG), `golang.org/x/tools/go/callgraph` (`static`, `cha`) pour les cibles d'appel, `google.golang.org/protobuf` (flux). Versions figées dans `adapters/go/go.mod`/`go.sum` à l'implémentation (ne pas inventer de version).

**Aucune compilation, aucun réseau, aucun script du projet analysé.** L'adaptateur DOIT :
1. exécuter `go/packages` avec `Mode = NeedName | NeedFiles | NeedImports | NeedDeps | NeedTypes | NeedSyntax | NeedTypesInfo | NeedModule` — **sans** `NeedExportFile` ni `NeedCompiledGoFiles` (le typage se fait depuis les sources, `go list` ne compile rien) ;
2. fixer l'environnement du processus `go list` (et ignorer l'environnement hérité hors liste blanche `PATH`, `HOME`, `GOCACHE`, `GOMODCACHE`, `GOROOT`) : `GOENV=off`, `GOEXPERIMENT=` (valeur vide), `GOFLAGS=-mod=readonly`, `GOTOOLCHAIN=local`, `GOPROXY=off`, `GONOSUMDB=*`, `GOFLAGS` additionnel interdit, `CGO_ENABLED=0` (sauf `cgo=true` dans le profil, alors fichiers cgo exclus avec `CGO_UNSUPPORTED`), `GOOS`/`GOARCH` du profil, `GOWORK` du profil (ou `off`) ;
3. ne **jamais** exécuter `go generate`, `go build`, `go test`, ni de code du projet ;
4. refuser (`AdapterError{code:"TOOLCHAIN_MISSING"}`) si `go` n'est pas trouvé ; refuser si la version de `go` (`go version`) diffère de `go_version` du profil quand ce champ est renseigné ;
5. si une dépendance est absente du cache module (pas de réseau), produire une erreur d'unité `FAILED` avec le message `go list`, pas de téléchargement.

**Configuration capturée** (dans `build_fingerprint` = sha256 hex de la forme canonique JSON, clés triées) : `GOOS`, `GOARCH`, tags, version du compilateur, `go.mod`/`go.work` (contenu hashé), chemins de modules/packages, contraintes de build, taille de `int`, variante test/non-test. Un snapshot **n'agrège pas silencieusement** des configurations incompatibles (plusieurs configs ⇒ plusieurs `ske ingest`, donc plusieurs snapshots).

**Clés d'unité :** `go:<import_path>@<goos>/<goarch>` ; variante test : `go:<import_path>[test]@<goos>/<goarch>`. Exemple : `go:example@linux/amd64`.
**Profil sémantique :** `go-<version sans préfixe "go">-<goos>-<goarch>` (ex. `go-1.22.5-linux-amd64`).
**Noms qualifiés :** `<import_path>.<Nom>` ; méthode : `<import_path>.<Type>.<Méthode>` (récepteur pointeur : `<import_path>.(*Type).<Méthode>`) ; closure : `<fonction parente>$<n>` (suffixes de `go/ssa`) ; instance générique : `<import_path>.Max[int]` (forme `go/ssa`).

## 9.2. Couverture syntaxique obligatoire et **règles de comptage des décisions source**

Les décisions source sont énumérées par une **traversée AST** (`go/ast`). Les spans sont des spans de **condition**, colonnes en octets, fin exclusive (§5.1).

| Construct | Décision(s) émise(s) | Issues (ordre = ordinal) | Span |
|---|---|---|---|
| `if cond { … }` (avec ou sans `else`) | 1 décision `IF` | `TRUE`, `FALSE` (`FALSE` = chute de contrôle s'il n'y a pas de `else`) | l'expression `cond` |
| `else if cond2` | une décision `IF` **distincte** (imbriquée dans le `else`) | idem | `cond2` |
| condition contenant `&&` / `\|\|` | la décision `IF` **plus** une décision `LOGICAL_OPERAND` par **atome** (sous-expression maximale qui n'est ni `&&`, ni `\|\|`, ni parenthèse/`!` autour d'une telle expression), `parent_decision_id` = l'`IF` ; atomes dans l'ordre source | `TRUE`, `FALSE` chacun | l'atome |
| `&&`/`\|\|` en **contexte valeur** (hors condition d'`if`/`for`, ex. `return a && b`) | une décision `LOGICAL_OPERAND` par atome **sauf le dernier** (aucun branchement ne dépend de sa valeur) ; `parent_decision_id` vide | `TRUE`, `FALSE` | l'atome |
| `switch tag { case … default … }` | 1 décision `SWITCH` | une issue par clause `case` non-`default` dans l'ordre source : `CASE:0`, `CASE:1`, … (`case 2, 3:` = **une** issue) ; puis `DEFAULT` **toujours en dernier** (explicite ou implicite, même si `default` figure en premier dans le source) | l'expression `tag` (ou le mot-clé `switch` s'il n'y a pas de tag) |
| `switch x := v.(type)` | 1 décision `TYPE_SWITCH` | idem `switch` | l'expression `v` |
| `for cond { … }` | 1 décision `LOOP_CONDITION` | `ENTER_BODY`, `EXIT` | `cond` (+ décisions `LOGICAL_OPERAND` si `&&`/`\|\|`) |
| `for { … }` (sans condition) | aucune | — | — |
| `for range e` | 1 décision `RANGE_ITERATION` | `ENTER_BODY`, `EXIT` | l'expression `e` |
| `select { … }` | 1 décision `SELECT` | une issue par `case` : `CASE:0`…, puis `DEFAULT` **seulement s'il existe** | le mot-clé `select` |
| `return`, `panic(…)` | aucune décision (sorties : `RETURN`, `PANIC` en IR) | — | — |
| `defer`, `go` | aucune décision ; `DEFER_PUSH`/`SPAWN` en IR | — | — |
| fonction littérale (closure) | traitée comme **fonction distincte** : ses décisions lui appartiennent | — | — |

**Fonctions génériques :** chaque instance `go/ssa` et l'origine générique reçoivent leurs propres décisions (mêmes spans, `function_id` différents).

**Compteur indépendant (`expected_source_decisions`)** : calculé par une **seconde** traversée de l'AST typé écrite séparément (`countDecisions`, pas de code partagé avec l’émetteur). Pour une fonction générique, ce compteur compte une fois le corps d'origine et **une fois par instance réellement émise**, en utilisant l'inventaire des instanciations de `types.Info.Instances` et des fonctions SSA chargées ; aucune instanciation absente de la compilation ne peut être supposée. Le compteur et l'émetteur DOIVENT s'accorder sur cet inventaire d'instances, mais le comptage des constructions syntaxiques ne doit pas appeler l'émetteur. Le moteur d'émission et le compteur DOIVENT donner le même nombre sur toutes les fixtures (test unitaire) ; l'adaptateur envoie le résultat du compteur dans `EndUnit.expected_source_decisions` et le nombre de `SourceDecisionDef` réellement émis dans `emitted_source_decisions`.

**Issue réellement supprimée par la compilation** (aucune arête IR correspondante) : émettre l’issue avec `elided = true`, **sans** lien, et le diagnostic `OUTCOME_ELIDED_BY_LOWERING` (INFO) ; elle reste dans l’inventaire. **`go/ssa` conserve délibérément un `ssa.If` pour une condition constante (`if false`) et ses deux arêtes** : dans ce cas, `elided=false`, liens `DIRECT` dont la qualité suit §9.5 (une constante n'a pas de `DebugRef` : l'`If` est en général non ancré, donc lié en `APPROX` par la règle 4 de §9.5), et le moteur pourra classer l'issue **IR** morte `PROVEN_UNREACHABLE` si les conditions sont satisfaites. Mapper `untyped bool` vers `go:bool`. Ne jamais inférer `elided` de la seule constance de la condition.

**Fonctions synthétiques Go** : émettre les fonctions `<package>.init` comme fonctions CIR non externes avec leur(s) décision(s) IR (ex. test de garde d'initialisation), **sans décision source correspondante**. Émettre les wrappers synthétiques uniquement lorsqu'ils sont atteignables via une cible d'appel ou nécessaires à une résolution ; signaler chaque fonction synthétique émise (y compris `<package>.init`) par le diagnostic `SYNTHETIC_FUNCTION` (INFO, §5.6) et conserver leur absence de décision source. Exclure ces décisions IR des compteurs AST indépendants, jamais du `ir_decision_count`. Le journal de capacité rapporte les catégories synthétiques présentes.

**Panics implicites** (nil, indice hors bornes, assertion de type, division par zéro, écriture dans map nil, fermeture de canal) : v0.2 **ne les modélise pas** comme arêtes ; un diagnostic `IMPLICIT_PANIC_UNMODELED` (INFO) **par fonction** concernée, avec le nombre de sites dans le message **et un inventaire de ces sites (`implicit_exit_sites`) conservant leur span, kind et état `UNMODELED`** (`exit_kind` ∈ `NIL_DEREFERENCE`, `INDEX_OUT_OF_RANGE`, `TYPE_ASSERTION`, `DIVISION_BY_ZERO`, `NIL_MAP_WRITE`, `CLOSED_CHANNEL`, `NEGATIVE_SHIFT`, ou `X_<…>` ; ids `<function_id>/x<n>`, n = rang par (ligne, colonne, `exit_kind`)) ; un tel site interdit de conclure que tous les comportements de la fonction ont été reconstruits. (`DIV`/`REM` et décalages gardent néanmoins leur successeur de panic dans le moteur symbolique, §6.3.)

## 9.3. Pipeline et ordre de sortie déterministe

```text
go/packages (profil) ─► packages typés
   ├─ AST : décisions source (+ compteur indépendant)
   ├─ types : TypeDef/SymbolDef
   └─ SSA (ssautil.AllPackages, mode ssa.InstantiateGenerics|ssa.GlobalDebug ; ni NaiveForm ni BareInits) ─► CFG
        ├─ normalisation des instructions (§9.4)
        ├─ arêtes (§4.7) + liens source→IR **et cas d’activation** (§9.5)
        ├─ cibles d'appel (§9.6)
        └─ diagnostics
   ─► flux protobuf (Header, UnitDef, SourceFileDef*, TypeDef*, …, EndUnit)
```

**Déterminisme** : tri explicite (fichiers par chemin, packages par import path, fonctions par nom qualifié, blocs par `Index` SSA, etc.) ; **aucune** dépendance à l'ordre d'itération d'une `map` Go. Test : deux extractions successives de la même unité produisent des flux **identiques octet par octet**.

Ne pas compter uniquement les `ssa.If` pour déduire la couverture source : les abaissements créent/suppriment des branches et `&&`/`||` demandent la vue syntaxique.

## 9.4. Mapping SSA → CIR (**normatif**)

| SSA Go | CIR | Note |
|---|---|---|
| `ssa.Const` | `ValueDef` `CONST` | type et bits exacts ; `nil` ⇒ `null_value` |
| `ssa.Parameter` | `ValueDef` `PARAM` (+ `name`) | |
| `ssa.Global` | `ValueDef` `GLOBAL` (type pointeur) | |
| `ssa.Function` (valeur) | `ValueDef` `GLOBAL` (symbole fonction) | |
| `ssa.BinOp` `+ - * / % & \| ^ << >>` | `ADD SUB MUL DIV REM BIT_AND BIT_OR XOR SHL SHR` | `overflow_mode=WRAP` ; `&^` ⇒ `BIT_NOT` puis `BIT_AND` (2 instructions) |
| `ssa.BinOp` `== != < <= > >=` | `EQ NE LT LE GT GE` | flottants ⇒ `OPAQUE_OP` ; chaînes `<` etc. ⇒ `OPAQUE_OP` ; chaînes `==`/`!=` ⇒ `EQ`/`NE` |
| `ssa.BinOp` `+` sur chaînes, flottants, complexes | `OPAQUE_OP` | `source_construct` renseigné |
| `ssa.UnOp` `-` / `!` / `^` | `NEG` / `LOGICAL_NOT` / `BIT_NOT` | |
| `ssa.UnOp` `*` (déréférencement) | `LOAD` | |
| `ssa.UnOp` `<-` | `CHANNEL_RECV` | |
| `ssa.Phi` | `PHI` (`phi_inputs`) | |
| `ssa.If` | `BRANCH` | 2 arêtes explicites |
| `ssa.Jump` | `JUMP` | |
| `ssa.Return` | `RETURN` | valeurs multiples conservées dans l'ordre |
| `ssa.Panic` | `PANIC` | |
| `ssa.Call` avec `Common().StaticCallee() != nil` (fonction ou closure directe) | `CALL` `STATIC` | + `CallTargetDef` (§9.6) |
| `ssa.Call` (mode invoke, interface) | `CALL` `INTERFACE` | cibles par CHA |
| `ssa.Call` (valeur fonction sans callee statique) | `CALL` `DYNAMIC` | cibles selon §9.6 (CHA par signature, `world_closed=false`) |
| `ssa.Call` builtin `len` | `LENGTH` | autres builtins (`cap`, `append`, `copy`, `delete`, `new`, `print`, `recover`…) ⇒ `OPAQUE_OP` (`recover` ⇒ `RECOVER`) |
| `ssa.Alloc` | `ALLOC` | |
| `ssa.Store` | `STORE` | |
| `ssa.FieldAddr` / `ssa.Field` | `FIELD_ADDR` / `EXTRACT_FIELD` | `FieldAccess` |
| `ssa.IndexAddr` / `ssa.Index` | `INDEX_ADDR` / `INDEX` | |
| `ssa.Extract` | `EXTRACT_FIELD` sur `TUPLE` | retours multiples |
| `ssa.Convert`, `ssa.ChangeType` | entiers ⇒ `SIGN_EXTEND`/`ZERO_EXTEND` (élargissement selon la signature **de la source**), `TRUNCATE` (rétrécissement), `BITCAST` (même largeur) ; sinon `OPAQUE_OP` | |
| `ssa.MakeInterface` | `MAKE_INTERFACE` | |
| `ssa.TypeAssert` | `TYPE_TEST` (comma_ok) ou `DOWNCAST` | |
| `ssa.MakeClosure` | `MAKE_CLOSURE` | |
| `ssa.Go` | `SPAWN` | `CONCURRENCY_PARTIAL` |
| `ssa.Defer` | `DEFER_PUSH` | `DEFER_UNMODELED` |
| `ssa.RunDefers` | `RUN_DEFERS` | `DEFER_UNMODELED` |
| `ssa.Send` | `CHANNEL_SEND` | |
| `ssa.Select` | `OPAQUE_OP` (`source_construct="select"`) | le `ssa.If`/chaîne qui suit est mappé sur la décision `SELECT` |
| `ssa.MakeChan/MakeMap/MakeSlice/Slice/Lookup/MapUpdate/Range/Next/ChangeInterface/MultiConvert/SliceToArrayPointer` | `OPAQUE_OP` | |
| `ssa.DebugRef` | ignoré (aucune instruction CIR ; ne consomme **aucun** ordinal) | sert uniquement à l'ancrage source→IR (§9.5) |
| tout autre | `OPAQUE_OP` | diagnostic `OPAQUE_OP_EMITTED` (une fois par `(fonction, source_construct)`) |

**Contrat de complétude Go v0.2 :** les unités et décisions source des fixtures sont **toutes** recensées ; toute construction non traduite précisément génère un diagnostic et empêche `COMPLETE` pour la dimension touchée (§3.5).

## 9.5. Liens source → IR ET activation (**normatif, v1.2**)

`source_ir_links` servent **uniquement à la traçabilité** ; la faisabilité d’une issue source repose sur les cas d’activation de §4.5.1. Un « lien » n’est jamais une formule logique et ne peut prouver une inaccessibilité.

Pour chaque `ssa.If` `I` dans le bloc `B` (SSA construit avec `ssa.GlobalDebug`, §9.3) :
1. **Ancre** de `I`, déterminée dans cet ordre :
   - **a.** s'il existe dans `B`, avant `I`, une instruction `*ssa.DebugRef` `d` telle que `d.X == I.Cond`, prendre la **dernière** : l'ancre est le nœud AST `d.Expr` (parenthèses retirées) ;
   - **b.** sinon, si `I.Cond` est une **instruction** dont `Pos()` est valide et située dans le corps de la fonction : l'ancre est cette position (cas des tests `case` produits par `emitCompare`) ;
   - **c.** sinon, la plus grande `Pos()` valide parmi les instructions de `B` précédant `I`, **seulement** si elle tombe dans une zone d'ancrage (règle 3) ;
   - **d.** sinon `I` est **non ancré**. Un `ssa.Parameter`, `ssa.FreeVar`, `ssa.Global` ou `ssa.Const` n'apporte jamais de position utilisable par lui-même (sa `Pos()` est celle de sa déclaration, ou `NoPos`).
2. Une condition constante (`I.Cond` est un `ssa.Const`) conserve ses deux arêtes : **ne jamais** marquer l'issue `elided` (§9.2) ni générer de pseudo-arête ; l'ancre suit la règle 1 (en pratique souvent 1.d).
3. **Zones d'ancrage** : `IF`, `LOOP_CONDITION`, `LOGICAL_OPERAND` : le span de la condition ou de l'atome ; `SWITCH`/`TYPE_SWITCH` : la liste d'expressions de **chaque** clause `case` (la clause `k` donne `CASE:k` sur l'arête `TRUE` ; l'arête `FALSE` du **dernier** test de la chaîne est liée à `DEFAULT`, explicite ou implicite) ; `SELECT`/`RANGE_ITERATION` : l'instruction entière, avec appariement structurel de la chaîne produite par `ssa.Select` (constante `k` comparée à l'indice ⇒ `CASE:k`) ou par la boucle de range ; à défaut, règle 4. Associer `I` à la décision **la plus interne** dont la zone contient l'ancre ; renseigner les liens `DIRECT`/`SHORT_CIRCUIT`/`SWITCH_CASE`/`LOOP_CONDITION` de provenance avec la polarité effective (parité du nombre de `!` entre la racine de la condition et l'atome). Les issues de la décision englobante d'une condition composée (ex. `IF "a && b"`) sont liées aux arêtes qui **terminent** chacun de leurs cas d'activation (`SHORT_CIRCUIT`, cf. P-SHORT-And). Un ancrage univoque par 1.a ou 1.b, ou un appariement structurel univoque `SELECT`/`RANGE_ITERATION`, donne `quality = EXACT` ; un ancrage par 1.c donne `APPROX`.
4. `I` non ancré : apparier dans l'ordre (décisions/atomes non encore appariés de la fonction par rang `sdN` ; `ssa.If` non appariés triés par `(Block.Index, position dans le bloc)`), avec `quality = APPROX` ; aucun mapping approximatif ne suffit pour une conclusion source `PROVEN_UNREACHABLE`.
5. Pour des atomes inversés, `inverted=true` relie la polarité source à la polarité IR correspondante.
6. **Construire ensuite les `SourceOutcomeActivationCaseDef` par parcours des chemins d’évaluation de l’expression source dans l’AST typé**. Préserver courts-circuits, négations et sous-expressions parenthésées. `a || b` TRUE possède **deux** alternatives (`a.TRUE` et `a.FALSE,b.TRUE`), et non seulement le résultat TRUE de `b`. Pour `a && b` FALSE, il existe également **deux** alternatives.
7. Pour une décision source dont **toutes** les alternatives ont été identifiées et mappées sur des issues IR avec `quality=EXACT`, marquer chaque `SourceOutcomeDef.activation_mapping_complete=true`. Sinon `false` + diagnostic `SOURCE_ACTIVATION_INCOMPLETE` et rendre l’inaccessibilité source **non démontrable** ; l’inventaire peut rester `COMPLETE` si toutes les issues source sont bien présentes.
8. Un `source_ir_link` `HEURISTIC` est interdit dans les fixtures de référence ; ne jamais remplacer un mapping manquant par un lien silencieux. Les commentaires `go/ssa` `Block.Comment` restent des indices non normatifs.
9. Une condition de boucle évaluée plusieurs fois doit associer les cas à une occurrence précise de la décision source (les anciennes issues empruntées ne satisfont pas un nouveau cas).

Sur `examples/go-branch-lab`, les règles 1.a/1.b DOIVENT ancrer toutes les décisions `IF`, `LOGICAL_OPERAND`, `LOOP_CONDITION` et `SWITCH` (liens `EXACT`, exigés notamment par AC-0.2-10 et P-INVERT-Check : dans `if !enabled`, `I.Cond` est le `ssa.Parameter` `enabled`, ancré par son `DebugRef`). Une autre méthode d'ancrage n'est admise que par ADR et si elle produit les mêmes liens.

**Contre-exemples normatifs (§15.1) :** `OrDecision(true,false) == true`, `AndDecision(false,true) == false`, `MixedDecision(true,false,false) == true` ; aucun de ces cas n’évalue le second opérande lorsqu’il est court-circuité. Après ingestion, comparer les cas exacts attendus (ordre des termes, nombre d’alternatives, polarités). L’absence de mapping fiable ne peut jamais transformer une issue en `PROVEN_UNREACHABLE`.

## 9.6. Cibles d'appel

- Appel statique (`StaticCallee() != nil`) : cible exacte (`resolution_kind = STATIC`), `dispatch_kind = STATIC`, `world_closed = true` (requis pour `RESOLVED_UNIQUE`, AC-0.4-01).
- Appel via interface ou valeur fonction : `callgraph/cha` restreint aux packages de l'unité et ses dépendances **chargées** ; `resolution_kind = CHA` ; `world_closed = false` ; `CALL_TARGET_APPROX` (INFO).
- Callee hors des sources chargées (stdlib non extraite, module absent) : **stub externe** dans l'unité + `EXTERNAL_CALL_OPAQUE` (INFO) ; `call_sites.resolution_status = EXTERNAL`.
- Aucun callee résoluble pour un appel dynamique : `CALL_TARGET_UNRESOLVED`.
- Ne **jamais** assimiler un call graph approximé à une fermeture du monde.

## 9.7. Organisation du module `adapters/go/` (**normative**)

```text
adapters/go/
├── go.mod, go.sum
├── gen/                        # code protobuf généré (committé ; scripts/gen-proto.sh)
├── cmd/ske-adapter-go/main.go  # lit 1 AdapterRequest (stdin), écrit des AdapterResponse (stdout), logs JSON (stderr)
└── internal/
    ├── frame/                  # framing 4 octets BE
    ├── profile/                # parsing/validation de build-profile.json
    ├── load/                   # go/packages sécurisé (§9.1)
    ├── decisions/              # émetteur ET compteur indépendant (fichiers séparés)
    ├── lower/                  # SSA -> CIR
    ├── link/                   # liens source->IR (§9.5)
    ├── calls/                  # cibles d'appel
    └── emit/                   # sérialisation ordonnée
```

`go vet ./...`, `gofmt -l .` (sortie vide) et `go test ./...` DOIVENT passer. Binaire : `go build -o target/ske-adapter-go ./cmd/ske-adapter-go` (c'est `scripts/verify.sh` qui le construit ; l'adaptateur **n'est jamais** construit par `ske` lui-même). `ske ingest go` localise le binaire par `--adapter PATH` ou, à défaut, `./target/ske-adapter-go`.

---

# 10. Adaptateurs Rust et Java — contrats d'extension (v0.6)

Ces adaptateurs ne sont **pas** à implémenter avant la clôture de la v0.5. Les contraintes ci-dessous sont normatives pour la v0.6 ; les critères détaillés (§19, v0.6) sont à **affiner par ADR** au démarrage de la v0.6 une fois les toolchains épinglés.

## 10.1. Rust

MIR (`rustc`) après extraction des informations source/macro et de la configuration `cfg`, features, target, profils de build. Les API `rustc_interface`/`rustc_driver` sont instables : adaptateur **isolé** dans `adapters/rust/`, toolchain **épinglée** (`rust-toolchain.toml` propre), matrice de compatibilité testée, échec lisible si la version diffère. Les crates cœur **ne dépendent pas** de `rustc_private`. Cas obligatoires pour l'inventaire : `if`, `match`, boucles, `?`, `Result`/`Option` quand ils modifient le contrôle, opérations pouvant paniquer, `Drop` et chemins d'unwind/abort, `async`/`await`, dispatch de traits, closures, macros et attribution de spans. `unsafe`, FFI et appels externes restent partiels. **Risque documenté :** le MIR est abaissé et parfois optimisé ; une correspondance source→IR fiable exige une passe de provenance dédiée ; un `match` source n'est pas une arête MIR unique (utiliser `SWITCH` et `SWITCH_CASE`, §4.7).

## 10.2. Java

**SootUp** + Jimple pour bytecode/CFG/call graph ; structures source via un parseur Java quand les sources existent. Configurer : version de JVM cible, classpath/module-path, annotations, dépendances, contrats des bibliothèques, profils framework. Cas obligatoires : branches, `switch`, `try/catch/finally` (`INVOKE` + arêtes `EXCEPTION`), exceptions vérifiées/non vérifiées, appels virtuel/interface, lambdas, initialisation de classe, synchronisation, collections. `reflection`, proxys dynamiques, injection et classloaders rendent le call graph potentiellement incomplet : **inconnus explicites**, pas de monde fermé sans preuve. (Auditer les licences des dépendances Java avant intégration : ADR dédié.)

**Comparaison Go/Rust/Java :** la conformité porte sur des **propriétés comportementales communes**, pas sur l'égalité du nombre de blocs. Inventaires source, IR et diagnostics sont comparés séparément.

---

# 11. CLI, formats d'entrée et sortie JSON

## 11.1. Options globales et conventions

- `--db PATH` (ou variable `SKE_DB`) : obligatoire sauf `--help`/`--version`. Pour `ingest go`, `--profile` absent ⇒ `<repo>/build-profile.json` s’il existe, sinon profil implicite capturant l’OS/l’architecture de la toolchain. `--yes-run-adapter` confirme explicitement l’exécution du binaire d’adaptateur enfant ; `--redact` masque les chemins absolus dans les logs/JSON, sans changer les empreintes source. `--format text|json` (défaut `text`). `--limit N` (défaut 100, max 10000), `--offset N` (défaut 0).
- **stdout** : résultat (JSON valide si `--format json`, **rien d'autre**). **stderr** : logs/erreurs lisibles ; en JSON, l'erreur est aussi émise sur stdout sous la forme `{"error":{"code":"E_…","message":"…","candidates":[…]}}`.
- `--snapshot ID|latest` (défaut : `latest` pour toute commande qui lit un snapshot) : `latest` = **dernier snapshot `CLOSED` du dépôt courant** (le dépôt est donné par `--repository NAME` ; s'il est omis et qu'il n'existe qu'un seul dépôt, c'est celui-là ; s'il en existe plusieurs ⇒ `E_AMBIGUOUS` (exit 2) avec la liste des dépôts candidats et la commande explicite à utiliser).
- `--function NAME` : correspondance **exacte** sur `qualified_name` parmi les fonctions du snapshot (sans les stubs externes) ; 0 résultat ⇒ `E_NOT_FOUND` (7) ; > 1 ⇒ `E_AMBIGUOUS` (2) avec `candidates` (qualified_name + unit_key).
- Les ids exposés par le JSON sont les **ids de flux** (sans le préfixe `unit_key|`), avec `unit_key` à côté.
- Les spans sont rendus `chemin:ligne:colonne-ligne:colonne` (fin exclusive).

## 11.2. Commandes

```bash
ske init --db .ske/knowledge.sqlite
ske ingest cir --db DB --input F.cir.pb [--input G.cir.pb …] [--repository NAME] [--source-root DIR] [--require-complete]   # --input répétable : 1 unité par fichier, un seul snapshot
ske ingest go  --db DB --repo DIR [--profile DIR/build-profile.json] [--adapter PATH] [--yes-run-adapter] [--max-message-bytes N] [--redact] [--repository NAME] [--require-complete]
ske snapshots list --db DB
ske inspect --db DB --snapshot latest --format json
ske functions list --db DB --snapshot latest [--include-external]
ske functions cfg  --db DB --snapshot latest --function example.Decide --format json
ske branches list  --db DB --snapshot latest --function example.Decide [--run RUN_ID|latest] --format json
ske branches analyze --db DB --snapshot latest --entry example.Decide [--budget budget.toml] [--context context.toml] [--require-complete] --format json
ske runs list --db DB [--snapshot latest]
ske runs mark-failed --db DB --run N
ske paths explore --db DB --snapshot latest --source example.A --target example.B           # v0.4
ske behaviors reconstruct --db DB --snapshot latest --entry example.Check --format json    # v0.5
ske rules list --db DB --snapshot latest --with-evidence
ske rules stale-check --db DB --repository NAME --from A --to B                 # v0.5
ske db check --db DB [--format json]
```

Jalons : `init`, `ingest cir`, `snapshots list`, `inspect`, `functions list|cfg`, `branches list`, `db check` en **v0.1** ; `ingest go` en **v0.2** ; `branches analyze`, `runs *` en **v0.3** ; `paths explore` en **v0.4** ; `behaviors`, `rules` en **v0.5**.

**Dépôts.** `ske ingest cir` utilise `name = --repository` (défaut `cir-fixtures`) et `root_uri = "ske:" + name` (défaut `ske:cir-fixtures`). `ske ingest go` utilise `root_uri = "file://" + chemin absolu de --repo` et `name = --repository` (défaut `basename(--repo)`). Un dépôt existant de même `root_uri` est réutilisé ; si son `name` diffère d'un `--repository` explicite, ou si le `name` visé appartient déjà à un autre `root_uri` ⇒ `E_USAGE` (exit 2), message proposant `--repository`. Sortie JSON d'`ingest` : `{"snapshot_id":N,"status":"CLOSED","inventory_status":"COMPLETE","units":[{"unit_key":"…","inventory_status":"COMPLETE"}],"diagnostics":{"INFO":0,"WARNING":0,"ERROR":0}}`.

## 11.3. Codes de sortie (**normatifs**)

| Code | Sens |
|---|---|
| `0` | succès (une analyse/un inventaire `PARTIAL` est un succès **sauf** avec `--require-complete`) |
| `1` | **réservé : ne jamais utiliser** |
| `2` | arguments/configuration invalides, ambiguïté (`E_USAGE`, `E_AMBIGUOUS`) |
| `3` | entrée/adaptateur invalide (`E_*` d'import, §5.5) ; snapshot `ABORTED` |
| `4` | résultat `PARTIAL`/non `COMPLETE` alors que `--require-complete` est demandé |
| `5` | erreur interne |
| `6` | intégrité de la base (`ske db check`, migration modifiée) |
| `7` | objet introuvable (`E_NOT_FOUND`) |
| `130` | interruption (SIGINT/SIGTERM) pendant une analyse |

## 11.4. Formats de fichiers d'entrée (**normatifs**)

**`build-profile.json`** (Go). Clés inconnues ⇒ `E_USAGE` ; défauts entre parenthèses :
```json
{
  "profile_version": 1,
  "language": "go",
  "go_version": null,
  "goos": "linux",
  "goarch": "amd64",
  "tags": [],
  "include_tests": false,
  "cgo": false,
  "patterns": ["./..."],
  "go_work": null
}
```
`goos`/`goarch` DOIVENT figurer dans `go tool dist list` ; `patterns` non vide ; `go_version`, si non null, doit égaler la version du toolchain après normalisation : supprimer le préfixe `go` de `go version` (`go1.22.5` → `1.22.5`) ; tous les profils emploient le numéro avec patch lorsqu’il est connu.

**`budget.toml`** : table `[budget]` avec les clés et défauts du tableau §6.7. Clé inconnue ⇒ `E_USAGE`. Tout champ absent prend le défaut ; la valeur **effective** est enregistrée dans `analysis_runs.budget_json` (JSON, toutes les clés).

**`context.toml`** (contexte d'entrée de l'analyse) :
```toml
name = "default"                     # nom du contexte (défaut "default")

[[param_range]]                      # 0..n ; seulement pour des paramètres BITVECTOR
param = "age"                        # nom d'un PARAM de la fonction d'entrée
min = 0                              # entier décimal, inclusif, dans la plage du type
max = 150
```
Ajoute `min <= param <= max` (comparaison signée/non signée selon le type) aux contraintes initiales. Paramètre inexistant ou non entier ⇒ `E_USAGE`. `context_fingerprint = sha256_hex(JSON canonique du contexte)`. Sans fichier : contexte vide (tous les paramètres libres).

## 11.5. Formats de sortie JSON (**normatifs** : clés et types exacts ; l'ordre des tableaux est celui indiqué)

**`ske inspect`** :
```json
{
  "snapshot": {"id": 1, "repository": "cir-fixtures", "status": "CLOSED", "inventory_status": "COMPLETE",
               "source_fingerprint": "…", "config_fingerprint": "…", "created_at": "…", "closed_at": "…"},
  "units": [{"unit_key": "go:example@linux/amd64", "language": "go", "semantic_profile": "go-fixture-linux-amd64",
             "semantic_profile_hash": "…", "build_fingerprint": "…", "toolchain_version": "…",
             "adapter": "cir-fixture-gen@1", "capability_flags": [], "inventory_status": "COMPLETE",
             "counts": {"functions": 1, "blocks": 5, "instructions": 7, "source_decisions": 2, "source_outcomes": 4,
                        "ir_decisions": 2, "ir_outcomes": 4, "diagnostics": 0}}],
  "analysis_defaults": {"max_wall_time_ms": 60000, "max_states": 10000, "max_instructions": 1000000, "max_call_depth": 8, "loop_unroll": 3, "loop_mode": "unroll", "max_recursion_unfold": 2, "max_solver_queries": 5000, "solver_rlimit": 5000000, "solver_wall_timeout_ms": 10000, "solver_seed": 0, "max_memory_mb": 1024},
  "diagnostics_summary": {"INFO": 0, "WARNING": 0, "ERROR": 0}
}
```
`units` triées par `unit_key`. `counts.functions` **exclut** les stubs externes. `analysis_defaults` reprend **exactement** les 12 clés et défauts du tableau §6.7.

**`ske functions list`** : `{"snapshot_id":1,"functions":[{"qualified_name":"…","unit_key":"…","is_external":false,"inventory_status":"COMPLETE","blocks":5,"source_decisions":2,"ir_outcomes":4}],"page":{"limit":100,"offset":0,"total":1,"truncated":false}}` — triées par `qualified_name` puis `unit_key`.

**`ske functions cfg`** :
```json
{"function": "example.Decide", "unit_key": "…", "entry": "example.Decide/b0",
 "blocks": [{"id": "example.Decide/b0", "ordinal": 0,
   "instructions": [{"id": "example.Decide/b0/i0", "ordinal": 0, "opcode": "LT",
                     "operands": ["example.Decide/v0", "example.Decide/k0"], "result": "example.Decide/v1",
                     "span": "decide.go:4:5-4:10"}],
   "successors": [{"edge": "example.Decide/b0/i1/e0", "target": "example.Decide/b1", "kind": "BRANCH_TRUE",
                   "label": "TRUE", "guard": {"value": "example.Decide/v1", "negated": false}}]}]}
```
Blocs par `ordinal`, instructions par `ordinal`, successeurs par `outcome_ordinal`. `guard` = `null` si aucune garde stockée.

**`ske branches list`** (avant analyse, sur `Decide`) :
```json
{
  "snapshot_id": 1, "function": "example.Decide", "unit_key": "go:example@linux/amd64",
  "inventory": {"source_status": "COMPLETE", "ir_status": "COMPLETE",
                "source_decisions": 2, "source_outcomes": 4, "ir_decisions": 2, "ir_outcomes": 4},
  "analysis": null,
  "source_decisions": [
    {"id": "example.Decide/sd0", "kind": "IF", "span": "decide.go:4:5-4:10",
     "outcomes": [
       {"id": "example.Decide/sd0/o0", "label": "TRUE",  "elided": false, "status": "UNANALYZED",
        "links": [{"ir_outcome": "example.Decide/b0/i1/e0", "mapping_kind": "DIRECT", "quality": "EXACT", "inverted": false}]},
       {"id": "example.Decide/sd0/o1", "label": "FALSE", "elided": false, "status": "UNANALYZED",
        "links": [{"ir_outcome": "example.Decide/b0/i1/e1", "mapping_kind": "DIRECT", "quality": "EXACT", "inverted": false}]}]}
  ],
  "ir_outcomes": [
    {"id": "example.Decide/b0/i1/e0", "label": "TRUE", "edge_kind": "BRANCH_TRUE",
     "status": "UNANALYZED", "explanation": null, "evidence": []}
  ],
  "page": {"limit": 100, "offset": 0, "total": 4, "truncated": false}
}
```
(`source_decisions` et `ir_outcomes` complets dans la vraie sortie.) Avec `--run` : `analysis` = `{"run_id":N,"run_status":"…","exploration_closed":true,"counts":{"UNANALYZED":0,"POSSIBLE":3,"WITNESSED":0,"PROVEN_UNREACHABLE":1,"UNKNOWN":0,"TIMEOUT":0,"UNSUPPORTED":0}}`, et chaque `ir_outcomes[i]` porte `status`, `soundness`, `explanation`, `evidence: [{"kind":"arrival","id":N,"query_id":N|null}]` (+ `{"kind":"proof","id":N}` si `PROVEN_UNREACHABLE`). Le statut d'une **issue source** est dérivé (§6.6). `page.total` = nombre d'`ir_outcomes` de la fonction.

**`ske branches analyze`** : `{"run_id":N,"run_status":"COMPLETED","exploration_closed":true,"cut_count":0,"assumptions":["SEQUENTIAL_EXECUTION","CLOSED_ENTRY_CONTEXT","CALLEE_PANIC_EXITS_NOT_EXPLORED"],"counts":{…comme ci-dessus…},"metrics":{"states_created":N,"instructions":N,"solver_queries":{"total":N,"sat":N,"unsat":N,"unknown":N,"timeout":N},"wall_ms":N}}`.

**Extension JSON v1.2 (normative dès v0.3) :** tout résultat `PROVEN_UNREACHABLE` porte `verification_level: "MODEL_UNREACHABLE"` dans son objet `evidence[].proof` (ou `proof` si le DTO est plat), ainsi que les hypothèses et la version du solveur ; le contenu de `proof_records` est immuable. Les réponses représentant une issue source ajoutent `activation_mapping_complete` et `activation_case_count` ; si ces données sont absentes/approximatives, la qualification de preuve source est interdite.

## 11.6. Pagination et troncature

Toute réponse listant des éléments contient `page`. Toute exploration (`paths explore`, etc.) accepte `depth`, `max_nodes`, `max_paths`, `timeout`, `context`, `assumptions`, `include_unknowns` et **DOIT** représenter une troncature : `truncated=true`, `truncation_reason`, `visited_count`, `frontier_count`. **Une sortie tronquée n'est jamais présentée comme complète.**

## 11.7. Couche query

`ske-query` expose des DTO indépendants de SQLite (en v0.7 traduits en HTTP/JSON et outils MCP) : `inspect_snapshot`, `list_functions`, `list_branches`, `get_function_cfg`, `explore_paths`, `get_function_summary`, `reconstruct_behaviors`, `list_rules`, `get_rule_evidence`, `compare_snapshots`. Le CLI n'accède **jamais** directement à `rusqlite`.

---

# 12. Algorithmes graphes (`ske-graph`)

Implémenter et tester :
1. Parcours BFS/DFS **déterministe** d'un CFG (ordre stable : arêtes par `outcome_ordinal`).
2. Dominance et post-dominance intra-fonction (§4.8, mêmes cinq cas de test).
3. SCC (Tarjan) pour boucles et graphe d'appels.
4. Arêtes de retour DFS (§6.8).
5. Reachability et plus court chemin sur le graphe **statique**, avec statut `POTENTIALLY_REACHABLE` tant que la faisabilité n'est pas résolue.
6. Parcours interprocédural respectant l'appariement appel/retour ou résumés (ne **pas** relier tout appelant à tous les successeurs d'une fonction).
7. Diff de graphes par fingerprints et correspondances source, avec rapport d'ambiguïtés.

SQLite sert au stockage et aux requêtes indexées ; les parcours complexes se font en Rust sur des graphes chargés par portions. Les CTE récursives restent utiles pour des requêtes simples, pas comme unique moteur d'exploration.

---

# 13. Incrémentalité et versionnement

- Un `snapshot` est **immuable** une fois clos (§5.4, §8.1) ; chaque `ske ingest` crée un nouveau snapshot.
- `source_fingerprint` et `config_fingerprint` sont **distincts** (formules §5.4). `structural_fingerprint` par fonction (§4.6). `summary_fingerprint` (§7.1) dépend des hypothèses, modèles et résumés appelés.
- Les dépendances d'analyse sont persistées dans `analysis_dependencies` (`dependent_kind='SUMMARY'`, `dependent_key=<summary_fingerprint>`, `dependency_kind='FUNCTION'|'SUMMARY'`, `dependency_key=<structural_fingerprint|summary_fingerprint>`).
- **v0.1 à v0.4 : aucune réutilisation entre snapshots.** Chaque snapshot est analysé intégralement ; l'exactitude prime sur la vitesse.
- **v0.5 : copie-sur-réutilisation.** Avant de calculer le résumé d'une fonction `f` (contexte `c`) dans un snapshot `B`, calculer son `summary_fingerprint` ; s'il existe déjà une ligne `function_summaries` (dans n'importe quel snapshot du même dépôt) avec **le même** `(summary_fingerprint, context_fingerprint)`, **copier** ses lignes (`function_summaries`, `summary_cases`, `summary_case_returns`, `summary_case_outcomes`, `observable_effects`) vers `B` en réécrivant les références (fonction, issues IR, expressions) par correspondance de `structural_fingerprint` + position ; la nouvelle ligne porte `reused_from_summary_id`. Si la copie ne peut pas être faite sans ambiguïté, **recalculer** (jamais deviner). Les métriques du run indiquent `summaries_reused` et `summaries_computed`.
- Un changement du type system, du classpath, des options de build ou du profil sémantique change `config_fingerprint`, donc invalide **toute** réutilisation.
- Les règles obsolètes deviennent `STALE` (jamais réattribuées automatiquement à un nouveau snapshot) : `ske rules stale-check --repository <nom_du_dépôt> --from A --to B` marque `STALE` (colonne `validation_status`) les règles de `A` dont la fonction a disparu de `B` ou dont le `structural_fingerprint` a changé.

---

# 14. Observabilité, performance et sûreté

**Métriques minimales** (enregistrées dans `analysis_runs.metrics_json` pour les analyses ; affichées par `ske inspect` pour l'ingestion) : unités/symboles/fonctions/blocs/instructions/décisions/issues ; diagnostics par catégorie ; temps d'ingestion et d'analyse (`wall_ms`) ; nombre de nœuds du DAG ; requêtes SMT (total, SAT, UNSAT, UNKNOWN, TIMEOUT, durée cumulée) ; états créés, dédupliqués (`seen`), coupés (`cut_count`) ; frontières inexplorées ; taille SQLite ; pic mémoire estimé.

**Budgets :** petits budgets conservateurs configurables, consignés dans `analysis_runs`. Une reprise après interruption n'est garantie que si des points de reprise ont été persistés ; sinon un nouveau run repart du début (et, en v0.5, réutilise les résumés sûrs).

**Sécurité technique (normatif) :**
- chemins relatifs normalisés, rejet de `../` (`E_PATH_INVALID`) ;
- pas de désérialisation non bornée : limite de longueur de message (défaut 16 MiB), limite du nombre de messages ;
- SQL **paramétré** uniquement (aucune concaténation de valeurs dans une requête) ; les fichiers `sql/checks/*.sql` sont statiques ;
- aucune macro/script exécuté silencieusement ; adaptateurs lancés avec un environnement **filtré** (liste blanche) ;
- aucun secret du source dans les logs ; la rédaction de valeurs (`ske … --redact`) est configurable pour les traces et requêtes sauvegardées ;
- **consentement :** le premier lancement d'un adaptateur sur un dépôt demande une confirmation interactive ou le drapeau `--yes-run-adapter` (sans terminal interactif et sans ce drapeau : exit `2`) ; le consentement est consigné dans `<répertoire de --db>/.ske/consent.json` (dépôt, adaptateur, version, date).

**Faux positifs :** choisir l'approximation conservatrice et expliquer l'inconnue plutôt qu'une conclusion fonctionnelle fausse.

---

# 15. Fixtures, valeurs attendues et tests de conformité (**normatif — source de vérité des critères**)

## 15.1. Projet Go de référence `examples/go-branch-lab`

Module `example` (`go.mod` : `module example` puis `go 1.22` ou la version installée si plus récente). **Code réellement compilable, gofmt-clean, sans réseau.** Les fichiers ci-dessous sont à écrire **exactement** (indentation par **tabulations**, fin de ligne `\n`, un `\n` final). Numérotation de lignes = celle du fichier.

**Arborescence :**
```text
examples/go-branch-lab/
├── go.mod
├── decide.go  check.go  adjusted.go  simple_if.go  short_circuit.go  nested_if.go
├── switch.go  loop.go  integer_overflow.go  two_returns.go  interfaces.go
├── memory.go  error_paths.go  concurrency.go  generics.go  library_calls.go
├── boolean_activation.go  allocation_loop.go  switch_fixture.go  global_read.go
├── partial/panic_defer.go                 # package partial
├── tags/tag_linux.go  tags/tag_other.go   # package tags
└── build-profile.json                     # profil par défaut (linux/amd64, patterns ["./..."])
```

### 15.1.1. `decide.go` — **fixture de référence de la faisabilité**

```go
package example

func Decide(x int) int {
	if x < 0 {
		return -1
	}
	if x < -10 {
		return -2
	}
	return 0
}
```
Spans de référence (ligne:colonne-ligne:colonne, fin exclusive) : fonction `3:1-11:2` ; condition 1 `4:5-4:10` (`x < 0`) ; condition 2 `7:5-7:12` (`x < -10`) ; `return -1` `5:3-5:12` ; `return -2` `8:3-8:12` ; `return 0` `10:2-10:10`.

### 15.1.2. `check.go`

```go
package example

import "errors"

var ErrUnderage = errors.New("underage")
var ErrDisabled = errors.New("disabled")

func Check(age int, enabled bool) error {
	if age < 18 {
		return ErrUnderage
	}
	if !enabled {
		return ErrDisabled
	}
	return nil
}
```
Spans : fonction `8:1-16:2` ; globales `ErrUnderage` `5:5-5:16`, `ErrDisabled` `6:5-6:16` ; condition 1 `9:5-9:13` (`age < 18`) ; condition 2 `12:5-12:13` (`!enabled`) ; `return ErrUnderage` `10:3-10:21` ; `return ErrDisabled` `13:3-13:21` ; `return nil` `15:2-15:12`.

### 15.1.3. `adjusted.go`

```go
package example

func Adjusted(x int) int {
	y := 0
	if x > 10 {
		y = 3
	}
	return x - y
}
```
Spans : fonction `3:1-9:2` ; condition `5:5-5:11` (`x > 10`) ; `return x - y` `8:2-8:14`.

### 15.1.4. Autres fichiers (contenu exact)

`simple_if.go`
```go
package example

func Abs(x int) int {
	if x < 0 {
		return -x
	}
	return x
}
```

`short_circuit.go`
```go
package example

var sideEffects int

func touch() bool {
	sideEffects++
	return true
}

func And(a, b bool) bool {
	if a && b {
		return true
	}
	return false
}

func Or(a, b bool) bool {
	if a || b {
		return true
	}
	return false
}

func AndValue(a, b bool) bool {
	return a && b
}

func AndCall(a bool) bool {
	if a && touch() {
		return true
	}
	return false
}
```

`nested_if.go`
```go
package example

func Classify(a, b int) int {
	if a > 0 {
		if b > 0 {
			return 1
		}
		return 2
	}
	return 3
}
```

`switch.go`
```go
package example

func Day(n int) string {
	switch n {
	case 1:
		return "one"
	case 2, 3:
		return "few"
	default:
		return "many"
	}
}

func Sign(n int) int {
	switch {
	case n < 0:
		return -1
	case n > 0:
		return 1
	}
	return 0
}
```

`loop.go`
```go
package example

func Sum(n int) int {
	s := 0
	for i := 0; i < n; i++ {
		s += i
	}
	return s
}

func Count3() int {
	c := 0
	for i := 0; i < 3; i++ {
		c++
	}
	return c
}

func SumRange(xs []int) int {
	s := 0
	for _, x := range xs {
		s += x
	}
	return s
}
```

`integer_overflow.go`
```go
package example

func AddOne(x int8) int8 {
	y := x + 1
	if y < x {
		return 1
	}
	return 0
}

func Big(x int) bool {
	return x+1 < x
}
```

`two_returns.go`
```go
package example

import "errors"

var ErrZero = errors.New("zero")

func DivMod(a, b int) (int, error) {
	if b == 0 {
		return 0, ErrZero
	}
	return a / b, nil
}

func UseDivMod() int {
	q, err := DivMod(4, 2)
	if err != nil {
		return -1
	}
	return q
}
```

`interfaces.go`
```go
package example

type Shape interface{ Area() int }

type Square struct{ side int }

func (s Square) Area() int { return s.side * s.side }

type Rect struct{ w, h int }

func (r Rect) Area() int { return r.w * r.h }

func Total(s Shape) int {
	return s.Area()
}
```

`memory.go`
```go
package example

func MemNoCall(p *int) int {
	if *p < 0 {
		return -1
	}
	if *p < -10 {
		return -2
	}
	return 0
}

var Hook func()

func MemOpaque(p *int) int {
	if *p < 0 {
		return -1
	}
	Hook()
	if *p < -10 {
		return -2
	}
	return 0
}

func MemAlias(p, q *int) int {
	*p = 1
	if *q == 1 {
		return 1
	}
	return 0
}

func MemAlloc(p *int) int {
	*p = 1
	a := new(int)
	*a = 2
	if *p == 1 {
		return 1
	}
	return 0
}
```

`error_paths.go`
```go
package example

import "strconv"

func Parse(s string) (int, error) {
	n, err := strconv.Atoi(s)
	if err != nil {
		return 0, err
	}
	return n, nil
}
```

`concurrency.go`
```go
package example

func Pipe(n int) int {
	ch := make(chan int)
	done := make(chan struct{})
	go func() {
		ch <- n
		close(done)
	}()
	select {
	case v := <-ch:
		return v
	case <-done:
		return -1
	}
}
```

`generics.go`
```go
package example

import "cmp"

func Max[T cmp.Ordered](a, b T) T {
	if a > b {
		return a
	}
	return b
}

func UseMax() (int, string) {
	return Max(1, 2), Max("a", "b")
}
```

`library_calls.go`
```go
package example

import "strings"

func HasA(s string) bool {
	if strings.HasPrefix(s, "a") {
		return true
	}
	return false
}
```

`partial/panic_defer.go`
```go
package partial

type errString string

func (e errString) Error() string { return string(e) }

var errPanic = errString("panic")

func Safe(f func()) (err error) {
	defer func() {
		if r := recover(); r != nil {
			err = errPanic
		}
	}()
	f()
	return nil
}

func Boom() {
	panic("boom")
}
```

`tags/tag_linux.go`
```go
//go:build linux

package tags

func Platform() string { return "linux" }
```

`tags/tag_other.go`
```go
//go:build !linux

package tags

func Platform() string { return "other" }
```

### 15.1.4-bis. Fixtures de régression v1.2 (à ajouter au module Go de référence)

`boolean_activation.go` :

```go
package example

func OrDecision(a, b bool) bool {
	if a || b {
		return true
	}
	return false
}

func AndDecision(a, b bool) bool {
	if a && b {
		return true
	}
	return false
}

func MixedDecision(a, b, c bool) bool {
	if a || (b && c) {
		return true
	}
	return false
}
```

`allocation_loop.go` :

```go
package example

func AllocTwice() bool {
	var first *int
	for i := 0; i < 2; i++ {
		p := new(int)
		if i == 0 {
			first = p
			*p = 1
		} else {
			*p = 2
		}
	}
	if *first != 1 {
		return false
	}
	return true
}
```

**Vérité indépendante :** `OrDecision(true,false) == true`, `AndDecision(false,true) == false`, `MixedDecision(true,false,false) == true`, `AllocTwice() == true` (sous exécution Go ordinaire et modèle séquentiel). Les `&&`/`||` court-circuitent : ne pas exécuter `b` lorsque `a` détermine le résultat. `AllocTwice` représente deux objets distincts issus du **même site d’allocation** dans deux itérations. Son résultat concret doit être vrai, mais la v0.3 n’est **pas** tenue de prouver toutes les issues du programme si son abstraction M1 est insuffisante : `UNKNOWN` est préférable à une fausse preuve.

### 15.1.5. Décisions source attendues (**transcription littérale — à écrire à la main dans les golden files, jamais générée par SKE**)

Notation : `kind "texte de condition" [issues]`. Les décisions d'une fonction sont numérotées `sd0, sd1, …` dans l'ordre **(ligne, colonne de début, fin décroissante, kind)** ; `↳ parent sdN` pour un `LOGICAL_OPERAND`. Noms : `X` ≡ `example.X` ; `partial.X` ≡ `example/partial.X` ; `tags.X` ≡ `example/tags.X` (noms qualifiés §9.1 ; les golden emploient la forme complète). Le test **relit le texte source au span** de chaque décision et vérifie qu'il est **égal** au texte indiqué.

| Fonction | Décisions attendues | Total décisions / issues |
|---|---|---|
| `Decide` | sd0 `IF "x < 0"` [TRUE,FALSE] ; sd1 `IF "x < -10"` [TRUE,FALSE] | 2 / 4 |
| `Check` | sd0 `IF "age < 18"` [TRUE,FALSE] ; sd1 `IF "!enabled"` [TRUE,FALSE] | 2 / 4 |
| `Adjusted` | sd0 `IF "x > 10"` [TRUE,FALSE] | 1 / 2 |
| `Abs` | sd0 `IF "x < 0"` | 1 / 2 |
| `And` | sd0 `IF "a && b"` ; sd1 `LOGICAL_OPERAND "a"` ↳sd0 ; sd2 `LOGICAL_OPERAND "b"` ↳sd0 | 3 / 6 |
| `Or` | sd0 `IF "a \|\| b"` ; sd1 `LOGICAL_OPERAND "a"` ↳sd0 ; sd2 `LOGICAL_OPERAND "b"` ↳sd0 | 3 / 6 |
| `OrDecision` | sd0 `IF "a || b"` ; sd1 `LOGICAL_OPERAND "a"` ↳sd0 ; sd2 `LOGICAL_OPERAND "b"` ↳sd0 | 3 / 6 |
| `AndDecision` | sd0 `IF "a && b"` ; sd1 `LOGICAL_OPERAND "a"` ↳sd0 ; sd2 `LOGICAL_OPERAND "b"` ↳sd0 | 3 / 6 |
| `MixedDecision` | sd0 `IF "a || (b && c)"` ; sd1 `LOGICAL_OPERAND "a"` ↳sd0 ; sd2 `LOGICAL_OPERAND "b"` ↳sd0 ; sd3 `LOGICAL_OPERAND "c"` ↳sd0 | 4 / 8 |
| `AllocTwice` | sd0 `LOOP_CONDITION "i < 2"` ; sd1 `IF "i == 0"` ; sd2 `IF "*first != 1"` | 3 / 6 |
| `AndValue` | sd0 `LOGICAL_OPERAND "a"` (sans parent) | 1 / 2 |
| `AndCall` | sd0 `IF "a && touch()"` ; sd1 `LOGICAL_OPERAND "a"` ↳sd0 ; sd2 `LOGICAL_OPERAND "touch()"` ↳sd0 | 3 / 6 |
| `touch`, `Big`, `Total`, `Square.Area`, `Rect.Area`, `UseMax`, `ReadGlobal`, `ReadChangingGlobal`, `partial.Boom`, `partial.errString.Error` | aucune | 0 / 0 |
| `example.init`, `partial.init`, `tags.init` (synthétiques, `SYNTHETIC_FUNCTION`) | aucune décision source ; 1 décision IR chacune (garde `init$guard`, P-IRDEC-n) | 0 / 0 |
| `Classify` | sd0 `IF "a > 0"` ; sd1 `IF "b > 0"` | 2 / 4 |
| `Day` | sd0 `SWITCH "n"` [CASE:0, CASE:1, DEFAULT] | 1 / 3 |
| `DaySwitch` | sd0 `SWITCH "n"` [CASE:0, CASE:1, DEFAULT] | 1 / 3 |
| `Sign` | sd0 `SWITCH "switch"` [CASE:0, CASE:1, DEFAULT] (span = le mot-clé `switch`) | 1 / 3 |
| `Sum` | sd0 `LOOP_CONDITION "i < n"` [ENTER_BODY, EXIT] | 1 / 2 |
| `Count3` | sd0 `LOOP_CONDITION "i < 3"` [ENTER_BODY, EXIT] | 1 / 2 |
| `SumRange` | sd0 `RANGE_ITERATION "xs"` [ENTER_BODY, EXIT] | 1 / 2 |
| `AddOne` | sd0 `IF "y < x"` | 1 / 2 |
| `DivMod` | sd0 `IF "b == 0"` | 1 / 2 |
| `UseDivMod` | sd0 `IF "err != nil"` | 1 / 2 |
| `Parse` | sd0 `IF "err != nil"` | 1 / 2 |
| `HasA` | sd0 `IF "strings.HasPrefix(s, \"a\")"` | 1 / 2 |
| `MemNoCall` | sd0 `IF "*p < 0"` ; sd1 `IF "*p < -10"` | 2 / 4 |
| `MemOpaque` | sd0 `IF "*p < 0"` ; sd1 `IF "*p < -10"` | 2 / 4 |
| `MemAlias` | sd0 `IF "*q == 1"` | 1 / 2 |
| `MemAlloc` | sd0 `IF "*p == 1"` | 1 / 2 |
| `Pipe` | sd0 `SELECT "select"` [CASE:0, CASE:1] (pas de `DEFAULT`) ; `Pipe$1` : aucune | 1 / 2 |
| `Max` (origine) | sd0 `IF "a > b"` | 1 / 2 |
| `Max[int]`, `Max[string]` | idem chacune (mêmes spans) | 1 / 2 chacune |
| `partial.Safe` | aucune ; `partial.Safe$1` : sd0 `IF "r != nil"` | 0 ; 1 / 2 |
| `tags.Platform` | aucune | 0 |

**Propriétés IR attendues** (vérifiées par code de test, en plus des tableaux ci-dessus) :

| ID | Propriété |
|---|---|
| P-LINK-ALL | toute issue source non `elided` a ≥ 1 lien vers une issue IR |
| P-NO-HEUR | aucun lien `HEURISTIC` dans `examples/go-branch-lab` |
| P-IRDEC-n | nombre d'`ir_decisions` : `Decide`=2, `Check`=2, `Adjusted`=1, `Abs`=1, `Classify`=2, `And`=2, `Or`=2, `AndValue`=1, `AndCall`=2, `Day`=3, `Sign`=2, `Sum`=1, `Count3`=1, `AddOne`=1, `DivMod`=1, `UseDivMod`=1, `Parse`=1, `HasA`=1, `MemNoCall`=2, `MemOpaque`=2, `MemAlias`=1, `MemAlloc`=1, `OrDecision`=2, `AndDecision`=2, `MixedDecision`=3, `AllocTwice`=3, `Max`=1, `Max[int]`=1, `Max[string]`=1 ; `SumRange` ≥ 1 ; `Pipe` ≥ 1 ; `DaySwitch` ≥ 1 (nombre exact consigné dans un golden **après observation indépendante de go/ssa avec la toolchain épinglée**, sans valeur prédéfinie non vérifiée) ; `<package>.init` = 1 pour chaque unité Go observée par SSA |
| P-INVERT-Check | dans `Check`, l'issue source `sd1/o0` (`!enabled` vrai) est liée à une issue IR dont `edge_kind = BRANCH_FALSE` avec `inverted = 1` ; `sd1/o1` à `BRANCH_TRUE` avec `inverted = 1` |
| P-SHORT-AndCall | dans `AndCall`, le bloc contenant l'instruction `CALL` vers `example.touch` est **dominé** par la cible de l'arête IR liée à l'issue source `sd1/o0` (`a` vrai) : `touch()` n'est évalué que si `a` est vrai |
| P-SHORT-And | dans `And`, `sd0/o0` (`a && b` vrai) est lié à **1** arête IR ; `sd0/o1` (faux) est lié à **2** arêtes IR (faux de `a` et faux de `b`) |
| P-ACT-OR | `OrDecision.sd0.TRUE` : EXACT cas `[a.TRUE]` et `[a.FALSE,b.TRUE]` ; FALSE : `[a.FALSE,b.FALSE]` ; `activation_mapping_complete=1` pour les deux issues ; l’arête de `b` peut ne pas être évaluée |
| P-ACT-AND | `AndDecision.sd0.TRUE` : `[a.TRUE,b.TRUE]` ; FALSE : `[a.FALSE]` et `[a.TRUE,b.FALSE]` ; toutes les alternatives EXACT |
| P-ACT-MIXED | `MixedDecision.sd0.TRUE` : `[a.TRUE]`, `[a.FALSE,b.TRUE,c.TRUE]` ; FALSE : `[a.FALSE,b.FALSE]`, `[a.FALSE,b.TRUE,c.FALSE]` ; EXACT, ordre conservé |
| P-ACT-NEGATIVE | Un mapping `HEURISTIC` ou incomplet est accepté comme fait de provenance mais **ne peut** conduire à `PROVEN_UNREACHABLE` pour une issue source |
| P-SWITCH-Day | `CASE:0` lié à ≥ 1 arête ; `CASE:1` lié à **exactement 2** arêtes (`n==2` vrai, `n==3` vrai) ; `DEFAULT` lié à ≥ 1 arête ; `mapping_kind = SWITCH_CASE` |
| P-TWORET | dans `DivMod`, les deux `RETURN` ont **2 opérandes** ; dans `UseDivMod`, ≥ 2 `EXTRACT_FIELD` d'indices 0 et 1 |
| P-INTERFACE-Total | `Total` contient 1 `CALL` `dispatch_kind=INTERFACE`, `resolution_status = RESOLVED_POSSIBLE`, `world_closed = 0`, et ses `call_targets` **contiennent** `example.Square.Area` et `example.Rect.Area` |
| P-EXT-HasA | `HasA` contient 1 `CALL` `STATIC` vers un stub externe `strings.HasPrefix` : `resolution_status = EXTERNAL` |
| P-OVERFLOW-width | dans `AddOne`, le type de `x` est `BITVECTOR(8, signed)` ; dans `Big` avec profil `goarch=amd64` le type `int` est `BITVECTOR(64, signed)` et avec `goarch=386` `BITVECTOR(32, signed)` |
| P-CONC-Pipe | `Pipe` contient `SPAWN` ; `Pipe$1` contient `CHANNEL_SEND` ; le diagnostic `CONCURRENCY_PARTIAL` est présent |
| P-GEN | `functions list` contient `example.Max`, `example.Max[int]`, `example.Max[string]` ; l'origine produit `GENERIC_ORIGIN_OPAQUE` |
| P-DET | deux extractions successives du même projet donnent des flux identiques octet par octet |

**Diagnostics attendus** (codes ; en plus, tout diagnostic *non listé* de sévérité `WARNING`/`ERROR` sur `example` est un échec) :

| Cible | Diagnostics attendus |
|---|---|
| `example` (package racine, hors `Pipe` ; les fonctions synthétiques `example.init` sont incluses et inventoriées comme IR sans décision source) | uniquement des `INFO` (`IMPLICIT_PANIC_UNMODELED`, `EXTERNAL_CALL_OPAQUE`, `CALL_TARGET_APPROX`, `OPAQUE_OP_EMITTED`, `GENERIC_ORIGIN_OPAQUE`) ; **inventory_status de l'unité = `COMPLETE`** |
| `Pipe`, `Pipe$1` | `CONCURRENCY_PARTIAL` (WARNING, impact CONCURRENCY) ; n'abaisse pas l'inventaire |
| `partial.Safe` | `DEFER_UNMODELED` (WARNING, EFFECTS) |
| `partial.Safe$1` | `RECOVER_UNMODELED` (WARNING, **INVENTORY**) ⇒ fonction `PARTIAL` ⇒ unité `example/partial` **`PARTIAL`** |
| `tags` (linux) | `BUILD_TAG_EXCLUDED_FILE` (INFO) pour `tags/tag_other.go` ; sources de l'unité : `tags/tag_linux.go` uniquement |
| `tags` (darwin) | `BUILD_TAG_EXCLUDED_FILE` (INFO) pour `tags/tag_linux.go` ; sources : `tags/tag_other.go` uniquement |

**Si un compte IR (`P-IRDEC-n`) diffère de ce qu'observe l'adaptateur :** (1) vérifier que l'écart est un abaissement légitime de `go/ssa` (afficher `fn.String()` et `fn.WriteTo`) et non un bug de l'adaptateur ; (2) l'écrire dans `docs/open-questions.md` ; (3) ne modifier la valeur attendue qu'avec un ADR `golden-correction` citant la sortie `go/ssa`. Les **décisions source** et les spans, eux, ne se corrigent jamais sans validation du commanditaire (ils ne dépendent pas de `go/ssa`).


### 15.1.4-ter. Fichier Go supplémentaire de référence pour la fixture `SWITCH`

Le fichier **`examples/go-branch-lab/switch_fixture.go`** est source de vérité de `switch3.cir.pb`. Il est compilable et gofmt-clean et ne modifie pas les spans des fichiers existants ; ses **12 lignes** sont impératives :

```go
package example

func DaySwitch(n int) int {
	switch n {
	case 1:
		return 10
	case 2, 3:
		return 20
	default:
		return 30
	}
}
```

La fixture `switch3.cir.pb` représente **conceptuellement** un `SWITCH` CIR à trois issues, non la forme SSA spécifique produite par l'adaptateur Go. L'adaptateur réel émet la traduction SSA de `DaySwitch` (nombre d'IR decisions spécifique à Go, à confirmer sur toolchain épinglée), tandis que la décision source et ses trois issues correspondent à la source ci-dessus. Ajouter `DaySwitch` aux golden source Go ; aucun compteur de la fixture CIR (§15.2.2) ne change.


### 15.1.4-quater. Fixture de lecture de globale mutable

`examples/go-branch-lab/global_read.go` (fichier additionnel créé en v0.1, gofmt-clean) :

```go
package example

var MutableTotal int

func ReadGlobal() int {
	return MutableTotal
}

func ReadChangingGlobal() (int, int) {
	a := ReadGlobal()
	MutableTotal++
	b := ReadGlobal()
	return a, b
}
```

Sous exécution séquentielle, `ReadChangingGlobal` renvoie deux valeurs liées à deux états globaux différents. Son objectif est de vérifier que **`is_pure_proven` seul ne permet pas de réutiliser un résultat déterministe sans arguments**. Le test appelle `ReadGlobal` de part et d'autre d'une écriture mémoire ; toute abstraction doit conserver cette dépendance au snapshot mémoire, ou rendre le résultat inconnu. Ne pas changer les spans ni les compteurs des fichiers de fixtures CIR existants.

## 15.2. Fixtures CIR codées à la main (jalon v0.1)

Générées **par code** : `cargo run -p ske-cir --example gen_fixtures -- --out tests/fixtures/cir` (déterministe : deux exécutions ⇒ fichiers identiques octet par octet ; les fichiers générés sont **committés** et un test compare la génération fraîche aux fichiers committés). Les `SourceFileDef.content_sha256` sont calculés depuis les **vrais fichiers** de `examples/go-branch-lab`. Tous les fixtures utilisent : `unit_key = "go:example@linux/amd64"`, `Header{cir_major=1, cir_minor=3, adapter_name="cir-fixture-gen", adapter_version="1", language="go", toolchain_version="fixture", build_fingerprint=sha256_hex("fixture"), semantic_profile="go-fixture-linux-amd64"}`, types : `go:bool` (BOOL), `go:int@amd64` (BITVECTOR 64 signé), etc. Notation des types fonction : `fn(T…)→R` = `TypeDef FUNCTION` (`param_type_ids`, `result_type_ids`) ; `fn()` et `fn()→void` = `FUNCTION` **sans** `result_type_ids` ; aucune fixture n'émet de `TypeDef VOID`.

**Activation des issues sources dans les fixtures (v1.2).** Pour chaque issue source possédant **exactement un** lien direct `EXACT` vers une issue IR, la fixture DOIT émettre `SourceOutcomeActivationCaseDef{case_ordinal=0, ordered_control_edge_ids=[edge], quality="EXACT"}` et `SourceOutcomeDef.activation_mapping_complete=true`. Il s’agit d’une **affirmation de correspondance sémantique de la fixture**, validée par la vérité de référence. Les issues `elided` ne portent pas de cas. Les fixtures `decide`, `check`, `adjusted`, `switch3`, `mem_nocall`, `mem_opaque` ont respectivement **4, 4, 2, 3, 4, 4** cas d’activation simples (autant de termes). `invoke` n’a pas d’issue source. Les noms/ordres d’issues et golden tests originaux ne sont pas modifiés ; seules les tables de nouveaux cas sont remplies.

**Liste (fichiers produits) :** `decide.cir.pb`, `check.cir.pb`, `adjusted.cir.pb`, `switch3.cir.pb`, `invoke.cir.pb`, `mem_nocall.cir.pb`, `mem_opaque.cir.pb` (committés) et les fichiers `bad_<code>.cir.pb` avec `--negative` (committés). `big.cir.pb` est créé **uniquement avec `--big`**, **jamais committé** et exclu du diff de reproductibilité. Un défaut `E_UNIT_DUPLICATE` est testé par une commande à deux `--input`, non par un fichier `bad_*.cir.pb`.

### 15.2.1. Définitions

**Identifiants exacts des fixtures (§4.6)** : les noms courts ci-dessous désignent des **alias de documentation**, jamais des ids sérialisés. Pour `Decide` : `x=example.Decide/v0`, `c1=.../v1`, `c2=.../v2`, `k0=example.Decide/k0`, `km1=.../k1`, `km10=.../k2`, `km2=.../k3`. Pour toutes les fonctions : les `PARAM` sont numérotés `v0…v(p−1)` dans l'ordre des paramètres ; les `SSA` **continuent** la même numérotation (`vp`, `vp+1`, …) dans l'ordre `(ordinal de bloc, ordinal d'instruction)` de leur instruction de définition (les alias `v1`, `v3`… des descriptions ci-dessous sont des noms de lecture : ex. dans `mem_opaque`, `vh` = `v3`, l'alias `v3` = `v4` et l'alias `v4` = `v5`) ; `CONST` de `k<n>` par première apparition dans la liste de valeurs de la fixture ; `GLOBAL` de `global:<qualified_name>` (§4.6). Un générateur utilise exactement cette règle, sans inventer des alias. Les golden JSON emploient ces identifiants complets ; toute incohérence avec un exemple est corrigée en faveur de la règle §4.6.

**`decide.cir.pb`** (source : `decide.go`). Types (3) : `bool`, `int`, `fn(int)→int`. Symboles (2) : `example` (PACKAGE), `example.Decide` (FUNCTION, span `3:1-11:2`). Valeurs (7) : `x` (PARAM, name `x`), constantes `k0=0`, `km1=-1`, `km10=-10`, `km2=-2` (type int), `c1`, `c2` (SSA bool). Blocs : `b0..b4`.
```text
b0: i0: c1 = LT x, k0            span 4:5-4:10
    i1: BRANCH c1                span 4:5-4:10     e0 -> b1 [BRANCH_TRUE, "TRUE", guard c1]
                                                    e1 -> b2 [BRANCH_FALSE, "FALSE", guard c1 negated]
b1: i0: RETURN km1               span 5:3-5:12
b2: i0: c2 = LT x, km10          span 7:5-7:12
    i1: BRANCH c2                span 7:5-7:12     e0 -> b3 [TRUE]   e1 -> b4 [FALSE]
b3: i0: RETURN km2               span 8:3-8:12
b4: i0: RETURN k0                span 10:2-10:10
```
Décisions source : `example.Decide/sd0` (`IF`, span `4:5-4:10`, issues `o0 TRUE`, `o1 FALSE`) ; `sd1` (`IF`, `7:5-7:12`). Liens (4, tous `DIRECT`, `EXACT`, `inverted=0`) : `sd0/o0↔b0/i1/e0`, `sd0/o1↔b0/i1/e1`, `sd1/o0↔b2/i1/e0`, `sd1/o1↔b2/i1/e1`. `EndUnit{expected_source_decisions=2, emitted_source_decisions=2, emitted_ir_edges=4, inventory_status="COMPLETE"}`.

**`check.cir.pb`** (source : `check.go`). Types (5) : `bool`, `int`, `error` (INTERFACE, descripteur `go:error`), `*error` (POINTER vers `error`), `fn(int,bool)→error`. Symboles (4) : `example` (PACKAGE), `example.ErrUnderage` (GLOBAL, type `*error`, span `5:5-5:16`), `example.ErrDisabled` (GLOBAL, `6:5-6:16`), `example.Check` (FUNCTION, `8:1-16:2`). Valeurs (9) : `age` (PARAM int), `enabled` (PARAM bool), `k18` (CONST int 18), `nil_err` (CONST error, `null_value`), `g_under` (GLOBAL `*error`, symbole `ErrUnderage`), `g_dis` (GLOBAL), `c1` (SSA bool), `e1`, `e2` (SSA error).
```text
b0: i0: c1 = LT age, k18         span 9:5-9:13
    i1: BRANCH c1                span 9:5-9:13     e0 -> b1 [TRUE, guard c1]   e1 -> b2 [FALSE, guard c1 negated]
b1: i0: e1 = LOAD g_under
    i1: RETURN e1                span 10:3-10:21
b2: i0: BRANCH enabled           span 12:5-12:13   e0 -> b3 [BRANCH_TRUE, "TRUE", guard enabled]
                                                    e1 -> b4 [BRANCH_FALSE, "FALSE", guard enabled negated]
b3: i0: RETURN nil_err           span 15:2-15:12
b4: i0: e2 = LOAD g_dis
    i1: RETURN e2                span 13:3-13:21
```
Décisions source : `sd0` (`IF`, `9:5-9:13`, TRUE/FALSE) ; `sd1` (`IF`, `12:5-12:13`, TRUE/FALSE). Liens : `sd0/o0↔b0/i1/e0` (inverted 0), `sd0/o1↔b0/i1/e1` (0), `sd1/o0↔b2/i0/e1` (**inverted 1**), `sd1/o1↔b2/i0/e0` (**inverted 1**) — le `BRANCH` de `b2` est son unique instruction `b2/i0`. Spans distincts (8) : fonction, 2 globales, 2 conditions, 3 retours.

**`adjusted.cir.pb`** (source : `adjusted.go`) : voir §4.10. Types (3) : `bool`, `int`, `fn(int)→int`. Valeurs (7) : `x`, `k0`, `k10`, `k3`, `c`, `y`, `r`. Blocs : `b0: i0 c = GT x, k10 ; i1 BRANCH c (e0→b1 TRUE, e1→b2 FALSE)` ; `b1: i0 JUMP (e0→b2 UNCONDITIONAL, label "NEXT")` ; `b2: i0 y = PHI [(b0,k0),(b1,k3)] ; i1 r = SUB x, y (payload BinarySemantics{operator="SUB", overflow_mode="WRAP", signedness="SIGNED"}) ; i2 RETURN r`. Décision source : `sd0` (`IF`, `5:5-5:11`) ; liens `sd0/o0↔b0/i1/e0`, `sd0/o1↔b0/i1/e1`. Spans : fonction `3:1-9:2`, condition `5:5-5:11` (sur `GT` et `BRANCH`), retour `8:2-8:14` (sur `RETURN`).

**`switch3.cir.pb`** (source : `switch_fixture.go`, voir §15.1.4-ter) : fonction `example.DaySwitch(n int) int`. Types : `int`, `fn(int)→int`. Valeurs : `n` (PARAM), `k1=1`, `k2=2`, `k3=3`, `k10=10`, `k20=20`, `k30=30` (CONST int). Blocs : `b0: i0 SWITCH n` avec `e0 SWITCH_CASE case_value_ids=[k1] → b1 (label "CASE:0")`, `e1 SWITCH_CASE case_value_ids=[k2,k3] → b2 ("CASE:1")`, `e2 SWITCH_DEFAULT → b3 ("DEFAULT")` ; `b1: RETURN k10` ; `b2: RETURN k20` ; `b3: RETURN k30`. Décision source `example.DaySwitch/sd0` (`SWITCH`, span `4:9-4:10`, issues `CASE:0`, `CASE:1`, `DEFAULT`) ; 3 liens `SWITCH_CASE`/`EXACT`. Spans : fonction `3:1-12:2`, tag `4:9-4:10`.

**`invoke.cir.pb`** (aucun fichier source, aucun span) : types `int`, `fn()→int`, `fn()→void` ; symboles `example`, `example.risky` (FUNCTION, externe), `example.Try` ; fonctions : `example.Try` (3 blocs) et le stub `example.risky` (`is_external=true`) ; valeurs : `g_risky` (GLOBAL, symbole `example.risky`), `k0=0`, `km1=-1`. `b0: i0 INVOKE g_risky (payload CallSemantics{dispatch_kind=STATIC, may_raise=true, may_mutate_memory=true})` avec `e0 NORMAL → b1 ("NORMAL")`, `e1 EXCEPTION → b2 ("EXCEPTION")` ; `b1: RETURN k0` ; `b2: RETURN km1`. `CallTargetDef{call_instruction_id=b0/i0, target_function_id=example.risky, resolution_kind=STATIC}`. Aucune décision source.

**`mem_nocall.cir.pb`** (source : `memory.go`, fonction `example.MemNoCall`) : types : `bool`, `int`, `*int`, `fn(*int)→int` ; valeurs : `p` (PARAM `*int`), `k0`, `km1`, `km10`, `km2`, `v1`, `v3` (SSA int), `v2`, `v4` (SSA bool). `b0: i0 v1=LOAD p ; i1 v2=LT v1,k0 ; i2 BRANCH v2 (e0→b1, e1→b2)` ; `b1: RETURN km1` ; `b2: i0 v3=LOAD p ; i1 v4=LT v3,km10 ; i2 BRANCH v4 (e0→b3, e1→b4)` ; `b3: RETURN km2` ; `b4: RETURN k0`. Deux décisions source `IF` (conditions `*p < 0` `4:5-4:11` et `*p < -10` `7:5-7:13`), 4 liens `DIRECT`. Spans : fonction `3:1-11:2`, les 2 conditions (sur `LT` et `BRANCH`), les 3 `RETURN` (`5:3-5:12`, `8:3-8:12`, `10:2-10:10`) = 6.

**`mem_opaque.cir.pb`** (source : `memory.go`, fonction `example.MemOpaque`) : comme ci-dessus, avec en plus le symbole `example.Hook` (GLOBAL, type `*fn()`, span `13:5-13:9`), la valeur `g_hook` (GLOBAL) et, dans `b2`, **avant** le second `LOAD` : `i0 vh = LOAD g_hook` (type `fn()`) ; `i1 CALL vh` (payload `CallSemantics{dispatch_kind=DYNAMIC, may_raise=true, may_mutate_memory=true}`, aucun `CallTargetDef`) ; puis `i2 v3=LOAD p ; i3 v4=LT v3,km10 ; i4 BRANCH v4`. Types (6) : `bool`, `int`, `*int`, `fn(*int)→int`, `fn()`, `*fn()`. Spans (7) : fonction `15:1-24:2`, `Hook` `13:5-13:9`, conditions `16:5-16:11` et `20:5-20:13`, retours `17:3-17:12`, `21:3-21:12`, `23:2-23:10`. (Le générateur **relit** `memory.go` pour vérifier le hash ; les positions ci-dessus font foi.)

### 15.2.2. Compteurs attendus après `ske ingest cir` (vérifiés par `SELECT COUNT(*)` ; toutes les autres tables = 0 sauf `repositories`=1, `snapshots`=1, `compilation_units`=1)

| Table | decide | check | adjusted | switch3 | invoke | mem_nocall | mem_opaque |
|---|---|---|---|---|---|---|---|
| `source_files` | 1 | 1 | 1 | 1 | 0 | 1 | 1 |
| `source_spans` | 6 | 8 | 3 | 2 | 0 | 6 | 7 |
| `cir_types` | 3 | 5 | 3 | 2 | 3 | 4 | 6 |
| `symbols` | 2 | 4 | 2 | 2 | 3 | 2 | 3 |
| `functions` | 1 | 1 | 1 | 1 | 2 | 1 | 1 |
| `values_ir` | 7 | 9 | 7 | 7 | 3 | 9 | 11 |
| `basic_blocks` | 5 | 5 | 3 | 4 | 3 | 5 | 5 |
| `instructions` | 7 | 8 | 6 | 4 | 3 | 9 | 11 |
| `instruction_operands` | 9 | 9 | 6 | 4 | 3 | 11 | 13 |
| `phi_inputs` | 0 | 0 | 2 | 0 | 0 | 0 | 0 |
| `control_edges` | 4 | 4 | 3 | 3 | 2 | 4 | 4 |
| `control_edge_case_values` | 0 | 0 | 0 | 3 | 0 | 0 | 0 |
| `source_decisions` | 2 | 2 | 1 | 1 | 0 | 2 | 2 |
| `source_outcomes` | 4 | 4 | 2 | 3 | 0 | 4 | 4 |
| `ir_decisions` | 2 | 2 | 1 | 1 | 1 | 2 | 2 |
| `ir_outcomes` | 4 | 4 | 2 | 3 | 2 | 4 | 4 |
| `source_ir_links` | 4 | 4 | 2 | 3 | 0 | 4 | 4 |
| `source_outcome_activation_cases` | 4 | 4 | 2 | 3 | 0 | 4 | 4 |
| `source_outcome_activation_terms` | 4 | 4 | 2 | 3 | 0 | 4 | 4 |
| `call_sites` | 0 | 0 | 0 | 0 | 1 | 0 | 1 |
| `call_targets` | 0 | 0 | 0 | 0 | 1 | 0 | 0 |
| `diagnostics` | 0 | 0 | 0 | 0 | 0 | 0 | 0 |

(Pour `decide`, `check`, `adjusted`, `switch3`, `mem_*` l'ingestion se fait avec `--source-root examples/go-branch-lab` : aucun diagnostic `SOURCE_NOT_VERIFIED`. Pour `invoke` : pas de source ⇒ aucun diagnostic non plus.)
Spans de `mem_nocall` : fonction + 2 conditions + 3 retours = 6 ; `mem_opaque` : + le symbole `Hook` = 7.

Statuts attendus : unité `COMPLETE` ; snapshot `COMPLETE` ; `functions.inventory_status = COMPLETE` pour toutes les fonctions non externes.

### 15.2.3. Fixtures négatives (**une par code**, dérivées par mutation ; chaque test vérifie : exit `3`, code JSON exact, **aucune ligne ajoutée** dans aucune table)

Base = `decide` sauf indication. Nom du fichier : `bad_<code en minuscules sans "E_">.cir.pb`.

| Code | Mutation |
|---|---|
| `E_PROTO_DECODE` | fichier tronqué de 3 octets |
| `E_MSG_TOO_LARGE` | les 4 premiers octets du fichier valent `ff ff ff ff` |
| `E_HEADER_MISSING` | `Header` supprimé (le 1er record est `UnitDef`) |
| `E_RECORD_ORDER` | `UnitDef` placé en 3ᵉ position |
| `E_CIR_MAJOR_UNSUPPORTED` | `Header.cir_major = 2` |
| `E_END_UNIT_MISSING` | `EndUnit` supprimé |
| `E_DUPLICATE_ID` | `BlockDef` `example.Decide/b1` émis deux fois (détecté par la map d'ids **avant** l'insertion SQL ; ne pas laisser `UNIQUE(function_id, ordinal)` produire `E_ORDINAL_DUPLICATE`) |
| `E_ORDINAL_DUPLICATE` | ajouter un second `BlockDef` avec **id nouveau** mais `ordinal=1` dans la même fonction (autres champs valides) ; contrainte SQLite traduite en `E_ORDINAL_DUPLICATE` |
| `E_PARAM_ORDER` | la seule `ValueDef` PARAM `x` de `Decide` possède `param_index=1` au lieu de `0` (valeur SQL valide, mais ordre/bijection avec `FunctionDef.parameter_value_ids` incorrect) |
| `E_EMPTY_ID` | une `ValueDef` avec `id = ""` |
| `E_UNKNOWN_OPCODE` | opcode de `b0/i0` = `FROBNICATE` |
| `E_UNKNOWN_KIND` | `ValueDef.role = "WEIRD"` |
| `E_DANGLING_REF` | l'opérande 1 de `b0/i0` = `example.Decide/v99` |
| `E_ARITY_MISMATCH` | `b0/i0` (`LT`) n'a qu'un opérande |
| `E_TYPE_MISMATCH` | `LT x, kt` : ajouter une `ValueDef` CONST bool `kt=true` définie au niveau fonction (évite une erreur de dominance) ; `EndUnit` inchangé |
| `E_PAYLOAD_MISSING` | base `adjusted` : `SUB` sans payload |
| `E_PAYLOAD_KIND` | base `adjusted` : `SUB` avec un payload `CallSemantics` |
| `E_NO_TERMINATOR` | `RETURN` de `b1` supprimé |
| `E_MULTI_TERMINATOR` | second `RETURN` ajouté dans `b1` (le CHECK SQLite `idx_one_terminator_per_block` déclenche d'abord : mapper impérativement vers `E_MULTI_TERMINATOR`) |
| `E_TERMINATOR_NOT_LAST` | dans `b0` : supprimer l'instruction `LT` (`b0/i0`) et la `ValueDef` `c1` ; ajouter une `ValueDef` CONST bool `kt=true` ; le `BRANCH` garde l'id `example.Decide/b0/i1`, prend `ordinal=0` et utilise `kt` comme opérande et comme `guard_value_id` de ses deux arêtes ; ajouter l'instruction `example.Decide/b0/i2` = `OPAQUE_OP` sans résultat, `ordinal=1`, payload `OpaqueSemantics{source_construct="fixture", reason="negative"}` |
| `E_EDGE_SOURCE_MISMATCH` | `e0` de `b0/i1` a `source_block_id = b1` |
| `E_EDGE_CROSS_FUNCTION` | ajouter `example.Other` (1 bloc `RETURN`) ; `e0` pointe vers son bloc |
| `E_EDGE_SHAPE` | l'arête `e1` du `BRANCH` de `b0` est supprimée (1 seule arête), **ainsi que tous les `SourceIrLinkDef` et cas d'activation** qui la référencent ; poser `activation_mapping_complete=false` sur `sd0/o1` ; corriger `EndUnit.emitted_ir_edges=3` |
| `E_ENTRY_BLOCK` | `FunctionDef.entry_block_id = ""` pour `example.Decide` (non externe) |
| `E_SSA_MULTI_DEF` | `b2/i0` a le même `result_value_id` que `b0/i0` (`c1`) ; **la `ValueDef` `c2` est supprimée et tous ses usages (opérande de `b2/i1`, `guard_value_id` des arêtes de `b2/i1`) sont redirigés vers `c1`**, afin de ne pas produire en premier `E_DANGLING_REF` |
| `E_SSA_DOMINANCE` | ajouter `n = NEG x` avec `BinarySemantics{operator="NEG",overflow_mode="WRAP",signedness="SIGNED"}` dans `b1` **avant le terminateur** et faire `RETURN n` dans `b3` (b1 ne domine pas b3) |
| `E_PHI_PRED_MISMATCH` | base `adjusted` : `phi_inputs` ne contient que `(b0,k0)` |
| `E_PATH_INVALID` | `SourceFileDef.path = "../decide.go"` |
| `E_SPAN_INVALID` | un span avec `end_line < start_line` |
| `E_LITERAL_WIDTH` | constante `int` encodée sur 4 octets |
| `E_COUNTER_MISMATCH` | `EndUnit.emitted_ir_edges = 5` |
| `E_SOURCE_HASH_MISMATCH` | `content_sha256` = 64 zéros (ingestion avec `--source-root`) |
| `E_UNIT_DUPLICATE` | `ske ingest cir --input decide.cir.pb --input decide.cir.pb` (même `unit_key` deux fois ; `--input` est répétable) |
| `E_SOURCE_ACTIVATION_INVALID` | le cas d’activation de `sd0/o0` a `ordered_control_edge_ids` vide (la variante « `activation_mapping_complete=true` avec un cas `HEURISTIC` » est couverte par un test unitaire de `ske-ingest`) |

## 15.3. Résultats d'analyse attendus (v0.3), budget par défaut et contexte vide sauf indication

Statuts par **issue IR** (ordre : `ordinal` de décision, puis d'issue). `P` = `POSSIBLE`, `PU` = `PROVEN_UNREACHABLE`. `run_status` / `exploration_closed` indiqués.

| Entrée | Issues IR → statut | Run |
|---|---|---|
| **`Decide`** | `sd0` TRUE **P** (témoin ex. `x=-1`) ; `sd0` FALSE **P** ; `sd1` TRUE **PU** ; `sd1` FALSE **P** | `COMPLETED`, closed |
| `Abs` | TRUE **P**, FALSE **P** | `COMPLETED` |
| `Adjusted` | TRUE **P**, FALSE **P** | `COMPLETED` |
| `Classify` | 4 issues **P** | `COMPLETED` |
| `Check` | 4 issues **P** | `COMPLETED` |
| `AddOne` (`int8`) | `y < x` TRUE **P** avec témoin `x = 127` (`witness_json` = `{"x":"127"}`) ; FALSE **P** | `COMPLETED` |
| `Day` | 6 issues IR (`n==1`, `n==2`, `n==3`, chacune T/F) toutes **P** | `COMPLETED` |
| `Count3`, `loop_unroll=3` | ENTER_BODY **P**, EXIT **P** | `COMPLETED`, closed, `cut_count=0` |
| `Count3`, `loop_unroll=2` | ENTER_BODY **P**, EXIT **UNKNOWN** (`EXPLORATION_NOT_CLOSED`) | `PARTIAL`, non closed, `cut_count ≥ 1`, aucun **PU** |
| `Sum`, `loop_unroll=3` | ENTER_BODY **P**, EXIT **P** | `PARTIAL`, non closed, `cut_count ≥ 1`, aucun **PU** |
| `MemNoCall` | `sd0` T **P**, F **P** ; `sd1` TRUE **PU** ; `sd1` FALSE **P** | `COMPLETED` |
| `MemOpaque` | `sd0` TRUE/FALSE **P** (`EXACT` pour ces arrivées) ; `sd1` TRUE/FALSE **P** (`OPAQUE`) | `COMPLETED` |
| `MemAlias` | TRUE **P**, FALSE **P** | `COMPLETED` |
| `MemAlloc` | TRUE **P** ; FALSE **PU** | `COMPLETED` |
| `HasA` | TRUE **P**, FALSE **P** (soundness `OPAQUE`) | `COMPLETED` |
| `Parse` | TRUE **P**, FALSE **P** | `COMPLETED` |
| `DivMod` | `b == 0` TRUE **P**, FALSE **P** (le successeur « division par zéro » de `DIV` est `UNSAT` et n'est pas une issue) | `COMPLETED` |

**Tests de budget et de solveur (déterministes, sur `Decide`)** — numérotation des requêtes dans l'ordre d'appel du solveur (BFS, §6.5) :

| Test | Configuration | Résultats attendus | Run |
|---|---|---|---|
| T-BUDGET-STATES | `max_states = 2` | `sd0` TRUE **P** ; `sd0` FALSE **P** ; `sd1` TRUE `UNANALYZED` ; `sd1` FALSE `UNANALYZED` | `PARTIAL`, non closed, `cut_count = 1`, diag `BUDGET_EXHAUSTED` |
| T-BUDGET-QUERIES | `max_solver_queries = 1` | `sd0` TRUE **P** ; `sd0` FALSE `TIMEOUT` ; `sd1` TRUE `TIMEOUT` ; `sd1` FALSE `TIMEOUT` (arrivées `result=TIMEOUT`, `query_id` NULL) | `PARTIAL` |
| T-SCRIPT-UNKNOWN | `ScriptedSolver` : `[Unknown, Sat, Sat, Sat]` | `sd0` TRUE `UNKNOWN` ; `sd0` FALSE **P** ; `sd1` TRUE **P** ; `sd1` FALSE **P** | `PARTIAL`, non closed |
| T-SCRIPT-TIMEOUT | `ScriptedSolver` : `[Timeout(Rlimit), Unsat]` | `sd0` TRUE `TIMEOUT` ; `sd0` FALSE `UNKNOWN` (`EXPLORATION_NOT_CLOSED`) ; `sd1` TRUE `UNANALYZED` ; `sd1` FALSE `UNANALYZED` ; **aucun** `PROVEN_UNREACHABLE` | `PARTIAL` |
| T-VERIFIER-REJECT | exploration : `ScriptedSolver` `[Unsat, Unsat]` ; **vérificateur : Z3 réel** | exploration fermée ; chaque issue candidate à `PROVEN_UNREACHABLE` est rejetée par rejeu : statut `UNKNOWN`, `explanation` commence par `PROOF_REJECTED` ; **0 ligne** `PROVEN_UNREACHABLE` | `COMPLETED` (exploration fermée) mais résultats `UNKNOWN` |
| T-SOLVER-FAIL | solveur qui renvoie une erreur | diagnostic `SOLVER_UNAVAILABLE` ; résultats déjà persistés conservés | `FAILED` |
| T-TRIGGER | `INSERT` SQL direct d'un `branch_analysis_results` `PROVEN_UNREACHABLE` sans `proof_id` | échec avec le message `E_PROOF_REQUIRED` ; idem avec `proof_id` pointant un proof `REJECTED` ; idem pour `WITNESSED` (`E_WITNESSED_NOT_PRODUCIBLE`) | — |

### 15.3-bis. Contre-tests obligatoires v1.2

| Fixture | Garantie recherchée | Résultat attendu |
|---|---|---|
| `OrDecision(true,false)` | `||` : `a.TRUE` suffit ; `b` non évalué | résultat concret `true`, issue `IF.TRUE` compatible avec le modèle, aucune fausse preuve d’inaccessibilité |
| `AndDecision(false,true)` | `&&` : `a.FALSE` suffit ; `b` non évalué | résultat concret `false`, issue `IF.FALSE` compatible avec le modèle |
| `MixedDecision(true,false,false)` | court-circuit imbriqué | `true`, pas de fausse preuve d’inaccessibilité du `IF.TRUE` |
| `AllocTwice()` | deux allocations au même site (deux itérations) | `true` en concret ; abstraction peut répondre `UNKNOWN`, jamais `PROVEN_UNREACHABLE` à tort |
| run avec preuve acceptée | immuabilité rétroactive | UPDATE/DELETE preuve et mutation des faits support refusés |
| `ReadChangingGlobal()` (`global_read.go`) | non-transparence référentielle | deux résultats non obligatoirement égaux si la globale peut changer |

Ces fixtures sont **des contre-tests**, pas des remplacements des golden files existants ; le moteur peut rendre `UNKNOWN` lorsqu'une abstraction conservatrice ne permet pas de conclure. Les runs imposant un contexte d’entrée utilisent explicitement `context.toml`/une grille de valeurs ; aucune preuve n’est déduite du seul résultat concret.

## 15.4. Validations obligatoires de bout en bout

```bash
cargo test --workspace
(cd adapters/go && go vet ./... && test -z "$(gofmt -l .)" && go test ./...)
./scripts/verify.sh
```

`scripts/verify.sh` (bash, `set -euo pipefail`, termine par la ligne `VERIFY OK <version>` et sort `0`) DOIT, **dans cet ordre** :
1. `cargo fmt --all -- --check` ; `cargo clippy --workspace --all-targets -- -D warnings` ; `./scripts/check-deps.sh` ; `./scripts/gen-proto.sh && git diff --exit-code -- schemas/ adapters/go/gen/` (générateur Go épinglé ; en v0.1 ne vérifier que les fichiers générés déjà dans le dépôt, sans utiliser un outil facultatif installé) ;
2. créer une base temporaire, `ske init`, vérifier 47 tables et 21 triggers ;
3. générer les fixtures CIR et vérifier qu'elles sont identiques aux fichiers committés ;
4. pour **chaque** fixture positive parmi `decide`, `check`, `adjusted`, `switch3`, `invoke`, `mem_nocall`, `mem_opaque`, créer **une base temporaire neuve**, importer **cette unique** fixture, vérifier ses compteurs §15.2.2 et exécuter `ske db check` (exit 0) ; ne jamais ingérer les 7 fichiers dans le même snapshot (`E_UNIT_DUPLICATE`) ;
5. pour chaque `bad_*`, vérifier exit 3, code d'erreur, base inchangée ;
6. lire fonctions, branches, CFG et provenance en JSON et les comparer aux **golden files écrits à la main** (`tests/golden/`) ;
7. **à partir de v0.2 seulement, si `go` est disponible** : construire `target/ske-adapter-go`, `ske ingest go` sur `examples/go-branch-lab`, comparer aux tableaux §15.1.5 (décisions, propriétés, diagnostics) ;
8. (v0.3) exécuter les analyses du §15.3 : un cas `SAT`, un cas `UNSAT` (via `PROVEN_UNREACHABLE`), un cas `UNKNOWN`, un cas `TIMEOUT` (ScriptedSolver) ;
9. vérifier qu'un échec/timeout laisse un statut **partiel explicite**, jamais `COMPLETED`/`COMPLETE` ;
10. deux ingestions consécutives du même fichier ⇒ 2 snapshots distincts, mêmes fingerprints.

Si `go` est absent, l'étape 7 est **sautée avec le message `SKIP go-adapter (go not found)`** et le script se termine par `VERIFY OK <version> (go adapter skipped)` ; les critères AC-0.2-* ne sont alors **pas** validés (le jalon v0.2 ne peut pas être déclaré terminé).

## 15.5. Format des golden files

Dans `tests/golden/` : un JSON par fonction ou fixture, **écrit à la main** (jamais produit par SKE). Exemple `tests/golden/go/short_circuit.And.json` :

```json
{
  "function": "example.And",
  "source_decisions": [
    {"id": "sd0", "kind": "IF", "cond_text": "a && b", "parent": null, "outcomes": ["TRUE", "FALSE"]},
    {"id": "sd1", "kind": "LOGICAL_OPERAND", "cond_text": "a", "parent": "sd0", "outcomes": ["TRUE", "FALSE"]},
    {"id": "sd2", "kind": "LOGICAL_OPERAND", "cond_text": "b", "parent": "sd0", "outcomes": ["TRUE", "FALSE"]}
  ],
  "ir_decisions": 2,
  "properties": ["P-LINK-ALL", "P-NO-HEUR", "P-SHORT-And"],
  "diagnostics_forbidden_severity": ["WARNING", "ERROR"]
}
```
Le harnais de test compare : kinds, textes aux spans (relus dans le fichier source), parents, étiquettes d'issues, nombres, cibles et statuts. Pour l'analyse symbolique, **ne pas** comparer des formules SMT textuellement : comparer des statuts, des témoins (`x=127`), et, si besoin, l'équivalence de formules via le solveur.

---

# 16. Architecture de tests et quality gates

**Unitaires :** parseurs/validation CIR (un test par code `E_*`), arithmétique typée (§4.9), hash-consing du DAG, construction CFG, dominance (5 cas), sérialisation protobuf (aller-retour Go↔Rust : comparer les `Record` décodés sémantiquement, sans imposer les mêmes octets), schéma SQLite (47 tables, 21 triggers), transfert de contraintes, solveur (SAT/UNSAT/UNKNOWN via `ScriptedSolver`), encodage SMT (chaque opcode de §4.3 « Oui » a un test comparant le résultat SMT aux vecteurs §4.9).

**Intégration :** sous-processus adaptateur, framing, import transactionnel (rollback total d'une unité en échec), lecture/écriture concurrente SQLite raisonnable (un lecteur pendant une ingestion), échec contrôlé, CLI JSON (`assert_cmd` + `serde_json`, schéma des clés de §11.5).

**Différentiels :** (a) la sortie Go réelle pour `decide.go` DOIT correspondre à `decide.cir.pb` sur la **projection sémantique** : mêmes décisions source (kind, span, issues), même nombre d'issues IR (4), mêmes liens source→IR (modulo les ids) ; idem `check.go` ; (b) exécuter **uniquement** les petites fixtures de confiance (`Decide`, `Check`, `AddOne`) dans un environnement isolé et comparer les sorties observées aux issues autorisées par le modèle : une exécution observée **non autorisée** par le modèle (ex. `Decide(-5)` atteignant une issue marquée `PROVEN_UNREACHABLE`) révèle une erreur de soundness critique et DOIT faire échouer le test (ce test **n'exécute que du code de fixture écrit dans ce dépôt**, jamais du code analysé tiers).

**Négatifs :** corruption protobuf, opcode inconnu, référence manquante, cycle DAG (CHK-18), FK invalide (CHK-01), chemin `../`, collision de clés, compilation incomplète, solveur indisponible, budget épuisé, cible d'appel inconnue, jointure inter-snapshot (CHK-02/03/16/17/21).

**Propriétés de sécurité des résultats (chacune = un test nommé `prop_*`) :**
- `prop_unsupported_never_exact` : `UNSUPPORTED` ne devient jamais silencieusement `EXACT` ;
- `prop_inventory_never_shrinks` : une branche inventoriée n'est jamais supprimée parce qu'elle est irréalisable (le nombre d'issues avant et après analyse est identique) ;
- `prop_json_possible_vs_witnessed` : la sortie JSON ne confond jamais `POSSIBLE` et `WITNESSED` ;
- `prop_budget_diag` : une requête qui dépasse le budget produit un diagnostic ;
- `prop_no_complete_on_error` : aucune erreur d'adaptateur/compilateur n'entraîne un snapshot `COMPLETE` ou un run `COMPLETED` ;
- `prop_no_llm_proof` : aucune règle `LLM_INFERRED` ne peut être référencée comme preuve (v0.7).

**Toolchain / CI :** `.github/workflows/ci.yml` exécute sur Linux : `cargo fmt/clippy/test`, `go fmt/vet/test`, `check-deps.sh`, `gen-proto.sh` + diff, `verify.sh` ; sur macOS : au minimum `cargo test --workspace` (si l'environnement de développement cible macOS). Dépendances verrouillées ; prérequis locaux documentés dans `README.md`. Intégration Java uniquement quand le jalon v0.6 est actif.

---

# 17. Critères non fonctionnels et mesures

Seuils = **objectifs à valider sur un environnement explicitement décrit**, pas des garanties.

- **Reproductibilité :** mêmes entrées/profils/toolchains ⇒ inventaires équivalents, résultats déterministes (hors durées et IDs techniques). Test : ingérer deux fois, analyser deux fois (graine fixe) ⇒ `branch_analysis_results` identiques (statut, soundness, explication) ; `TIMEOUT` de type `WALL_CLOCK` exclu de la comparaison.
- **Fidélité structurelle :** 100 % des constructions **comprises par l'adaptateur et présentes dans les golden fixtures** inventoriées, sans issue manquante.
- **Intégrité :** zéro violation `ske db check` sur toute base produite par les tests.
- **Tolérance aux échecs :** interruption d'un run ⇒ résultats validés conservés, analyses manquantes `UNANALYZED`.
- **Scalabilité :** ingestion en **streaming** (jamais tout le flux en mémoire) ; transactions par unité ; `gen_fixtures --big` produit un flux synthétique de 200 000 instructions ; **pic mémoire de `ske ingest cir` < 512 MiB** (mesuré par `/usr/bin/time -v` sous Linux) ; temps et pic consignés dans `tests/benchmarks/README.md`.
- **Coût LLM :** aucun token en v0.1–v0.6. À partir de v0.7 : coût d'indexation, coût par requête, qualité et rappel.
- **Portabilité :** SQLite local sans service distant ; CLI macOS/Linux.

`tests/benchmarks/README.md` décrit un protocole reproductible : taille en LOC, fonctions, branches, toolchain, CPU/RAM, temps, mémoire, taille de la base, nombre de diagnostics, taux d'issues explorées et statut d'exactitude.

---

# 18. Décisions d'architecture (ADR) à enregistrer

Créer `docs/adr/ADR-NNN.md` (sections : Contexte, Décision, Alternatives, Conséquences, Tests de non-régression) pour **au moins** :

| ADR | Décision | Raison |
|---|---|---|
| ADR-001 | Cœur Rust, adaptateurs en sous-processus | isolation des toolchains |
| ADR-002 | CIR typée et sémantique, pas un simple call graph | reconstruire des comportements |
| ADR-003 | Inventaire source distinct de l'inventaire IR | exhaustivité et provenance |
| ADR-004 | SQLite pour le stockage local | portable, transactions, SQL |
| ADR-005 | DAG symbolique canonique (interning structurel) | partage, déduplication |
| ADR-006 | SMT derrière une interface (`Solver`) + `ScriptedSolver` | tests, reproductibilité |
| ADR-007 | Exploration bornée + analyse abstraite progressive | explosion combinatoire, soundness |
| ADR-008 | Modélisation explicite des inconnues | pas de certitudes artificielles |
| ADR-009 | LLM post-analyse et optionnel | provenance, coût |
| ADR-010 | Versionnement snapshot/profil/toolchain | reproductibilité |
| ADR-011 | Intégrité inter-snapshot par contrôles SQL (`ske db check`) plutôt que clés composites | simplicité du DDL ; alternative : FK `(snapshot_id, id)` à reconsidérer si les contrôles s'avèrent insuffisants |
| ADR-012 | Pas de réutilisation d'analyses entre snapshots avant v0.5 ; copie-sur-réutilisation ensuite | exactitude d'abord |
| ADR-013 | Identifiants locaux au flux + stubs externes (pas de référence inter-unités) | simplicité du protocole, streaming |
| ADR-014 | Constantes = `ValueDef`, pas d'instruction `CONST` ; pas de `CAST` | une seule représentation |
| ADR-015 | `PROVEN_UNREACHABLE` par « fermeture globale » + vérificateur indépendant + trigger SQL | soundness vérifiable |
| ADR-016 | Déterminisme par `rlimit` + graine ; timeout mur = filet non déterministe | reproductibilité |
| ADR-017 | Profil sémantique et empreinte de build portés par l'unité | multilangage |
| ADR-018 | Adaptateur Go sans compilation (pas d'export data), environnement filtré | sécurité |
| ADR-019 | Choix de génération protobuf Rust (`prost-build` + `protoc` vendored, ou `protox`) | pas de dépendance système cachée |

---

# 19. Roadmap de livraison implémentable et critères d'acceptation

**Mode d'emploi.** Chaque jalon contient : (1) un **objectif**, (2) des **prérequis**, (3) des **tâches ordonnées** (`T-x.y-nn`) avec leurs livrables (chemins), (4) une table de **critères d'acceptation** `AC-x.y-nn` (critère, commande, résultat attendu), (5) la **Definition of Done**. Un critère est `PASS` seulement si sa commande a été exécutée et a donné le résultat attendu ; consigner la commande et la sortie dans `docs/validation/v0.X.md` (gabarit §20). Un jalon est terminé quand **tous** ses `AC` sont `PASS` (ou `WAIVED` par ADR validé par le commanditaire) **et** que les `AC` des jalons précédents le sont toujours (non-régression : `./scripts/verify.sh`).

Variables des commandes : `DB=$(mktemp -d)/k.sqlite` ; `FX=tests/fixtures/cir` ; `LAB=examples/go-branch-lab`. Le binaire est `target/debug/ske` (ou `cargo run -p ske-cli --`). Un test « nommé » signifie une fonction de test dont le nom contient exactement cette chaîne ; `cargo test --workspace -- --list` doit la montrer.

---

## v0.1 — CIR + import SQLite + navigation structurelle

**Objectif :** un programme CIR de référence est importé dans SQLite ; toutes ses décisions et issues sont consultables ; l'intégrité est vérifiable ; la CLI fonctionne.

**Prérequis :** Rust stable installé (`rustc --version`), accès réseau pour `cargo` **une fois** (dépendances), `go` non requis.

### Tâches ordonnées

| ID | Tâche | Livrables |
|---|---|---|
| T-0.1-01 | Créer le workspace, `rust-toolchain.toml`, `.github/workflows/ci.yml`, `scripts/check-deps.sh` (avec `--selftest`), `scripts/gen-proto.sh`, `scripts/verify.sh` (squelette) | `Cargo.toml`, `scripts/*`, CI |
| T-0.1-02 | `ske-common` : ids, enum d'erreurs `E_*` (§5.5, avec code de sortie), registre de diagnostics (§5.6), `sha256_hex`, formatage de spans | `crates/ske-common/` |
| T-0.1-03 | Copier `cir.proto` et `adapter.proto` (§5.1, §5.2) ; génération Rust (`prost-build` + `protoc` vendored ou `protox`, ADR-019) ; génération Go si `protoc-gen-go` présent, sinon message explicite | `schemas/`, code généré, ADR-019 |
| T-0.1-04 | `ske-cir` : types (§4.2), registre d'opcodes **comme table de données** (§4.3 : nom, arité, payload, résultat, indicateur terminateur, nb d'arêtes), `BitVec` + vecteurs (§4.9), vérificateurs locaux (arité, types, payloads, forme des terminateurs et arêtes, `LiteralValue`) | `crates/ske-cir/`, `docs/cir-semantics.md` (une section `### OPCODE` par opcode avec sémantique et exemples) |
| T-0.1-05 | `ske-graph` : CFG, DFS déterministe, dominance (§4.8), arêtes de retour (§6.8), SCC | `crates/ske-graph/` |
| T-0.1-06 | Écrire **dès v0.1** `examples/go-branch-lab` avec le contenu exact §15.1 (source seule, sans lancer de build) et son `build-profile.json` afin que `gen_fixtures` puisse calculer et vérifier les hash/spans ; `ske-cir` example `gen_fixtures` : les 7 fixtures (§15.2.1), `--big` non committé, `--negative` (33 fixtures `bad_*` §15.2.3 + 1 scénario CLI `E_UNIT_DUPLICATE`) ; sortie déterministe | `crates/ske-cir/examples/gen_fixtures.rs`, `tests/fixtures/cir/*` (committés) |
| T-0.1-07 | `ske-storage` : runner de migrations (§8.1), `migrations/0001_initial.sql` **copié tel quel** (§8.2), `Storage::open` (pragmas), `OpenSnapshotWriter`/`ClosedSnapshot`, fichiers `sql/checks/chk_01..22.sql`, `db_check()` | `crates/ske-storage/`, `migrations/` |
| T-0.1-08 | `ske-ingest` : lecture encadrée (`u32` BE), procédure d'import (§5.4), validations §4.8 et §5.3 (dominance via `ske-graph`), dérivation `ir_decisions/ir_outcomes`, `call_sites`, statuts (§3.5), clôture (fingerprints) | `crates/ske-ingest/` |
| T-0.1-09 | `ske-query` + `ske-cli` : commandes `init`, `ingest cir`, `snapshots list`, `inspect`, `functions list/cfg`, `branches list`, `db check` ; sortie JSON (§11.5) ; codes de sortie (§11.3) ; résolution `latest` et `--function` (§11.1) | `crates/ske-query/`, `crates/ske-cli/` |
| T-0.1-10 | Tests (unitaires, intégration, négatifs, golden **écrits à la main** dans `tests/golden/`), `docs/*`, ADR-001..019, `verify.sh` complet v0.1, `docs/validation/v0.1.md` | tout |

### Critères d'acceptation v0.1

| ID | Critère | Commande de vérification | Résultat attendu |
|---|---|---|---|
| AC-0.1-01 | Compilation sans avertissement | `RUSTFLAGS="-D warnings" cargo build --workspace --all-targets` | exit 0 |
| AC-0.1-02 | Clippy propre | `cargo clippy --workspace --all-targets -- -D warnings` | exit 0 |
| AC-0.1-03 | Formatage | `cargo fmt --all -- --check` | exit 0 |
| AC-0.1-04 | Règles de dépendances (§2.2) | `./scripts/check-deps.sh` puis `./scripts/check-deps.sh --selftest` | 1ʳᵉ commande : exit 0 et sortie `DEPS OK` ; 2ᵉ : exit 0 et sortie contenant `FORBIDDEN_DEP` détecté sur le jeu de test synthétique (le selftest *réussit* quand la violation est détectée) |
| AC-0.1-05 | Code protobuf à jour | `./scripts/gen-proto.sh && git diff --exit-code -- schemas/ adapters/go/gen/` (générateur Go épinglé ; en v0.1 ne vérifier que les fichiers générés déjà dans le dépôt, sans utiliser un outil facultatif installé) | exit 0 |
| AC-0.1-06 | Tests nommés présents | `cargo test --workspace -- --list 2>/dev/null \| grep -c 'negative_'` | nombre ≥ 34 (33 fichiers `bad_*` + scénario CLI `E_UNIT_DUPLICATE`) |
| AC-0.1-07 | Tous les tests passent | `cargo test --workspace` | exit 0, 0 test `ignored` hors liste « non bloquant » |
| AC-0.1-08 | Vecteurs arithmétiques (§4.9) | `cargo test -p ske-cir bitvec_vectors` | exit 0 ; le test contient les **27** vecteurs du tableau |
| AC-0.1-09 | Registre documenté | `cargo test -p ske-cir registry_documented` | exit 0 : chaque opcode du registre a une section `### <OPCODE>` dans `docs/cir-semantics.md`, ≥ 1 test positif et ≥ 1 test négatif d'arité/type (arité variable : négatif payload plutôt que arité) |
| AC-0.1-10 | Dominance (5 cas) | `cargo test -p ske-graph dominance_` | exit 0, 5 tests (diamant, boucle simple, boucle imbriquée, irréductible, inatteignable) |
| AC-0.1-11 | Framing | `cargo test -p ske-ingest framing_` | exit 0 : `framing_u32_be`, `framing_zero_length`, `framing_oversize` |
| AC-0.1-12 | Initialisation | `ske init --db $DB` | exit 0 ; fichier créé |
| AC-0.1-13 | Schéma exact | `cargo test -p ske-storage schema_tables_match` | exit 0 : 47 tables (liste §8.2), 21 triggers, noms identiques sur migration v1 ; 50 tables pour version v2 (v0.5+) |
| AC-0.1-14 | Migrations | `cargo test -p ske-storage migrations_` | exit 0 : version max = 1 ; `journal_mode = wal` ; relance idempotente (`ske init` 2ᵉ fois : exit 0, sortie contient `already up to date`, `COUNT(*) FROM schema_migrations` = 1) ; checksum modifié ⇒ exit 6 `E_MIGRATION_CHECKSUM` |
| AC-0.1-15 | FK actives sur chaque connexion | `cargo test -p ske-storage connection_has_fk_on` | exit 0 : `PRAGMA foreign_keys` = 1 ; un `INSERT` orphelin est refusé |
| AC-0.1-16 | Triggers de soundness | `cargo test -p ske-storage trigger_` | exit 0 : `PROVEN_UNREACHABLE` sans preuve ⇒ `E_PROOF_REQUIRED` (aussi avec preuve `REJECTED`, ou d'une autre issue/run, ou sur run `exploration_closed=0` / `cut_count>0`) ; `WITNESSED` ⇒ `E_WITNESSED_NOT_PRODUCIBLE` ; avec preuve `ACCEPTED` valide ⇒ accepté |
| AC-0.1-17 | Fixtures déterministes | `cargo run -p ske-cir --example gen_fixtures -- --negative --out /tmp/fx1 && cargo run -p ske-cir --example gen_fixtures -- --negative --out /tmp/fx2 && diff -r /tmp/fx1 /tmp/fx2 && diff -r /tmp/fx1 $FX` | exit 0 (aucune différence) |
| AC-0.1-18 | Import de `decide` | `ske ingest cir --db $DB --input $FX/decide.cir.pb --source-root $LAB --format json` | exit 0 ; JSON : `"snapshot_id":1`, `"status":"CLOSED"`, `"inventory_status":"COMPLETE"`, 1 unité `go:example@linux/amd64`, `"diagnostics":{"INFO":0,"WARNING":0,"ERROR":0}` |
| AC-0.1-19 | Compteurs des 7 fixtures | `cargo test -p ske-cli --test counts` | exit 0 : 7 tests `counts_decide`, `counts_check`, `counts_adjusted`, `counts_switch3`, `counts_invoke`, `counts_mem_nocall`, `counts_mem_opaque` ; **chaque** valeur du tableau §15.2.2 et zéro ligne dans les tables non listées |
| AC-0.1-20 | Intégrité | `ske db check --db $DB --format json` (après AC-0.1-18) | exit 0 ; `"ok":true` ; tableau `checks` de **22** entrées, toutes `"violations":0` |
| AC-0.1-21 | Détection de corruption | `cargo test -p ske-cli --test db_check_corruption` | exit 0 : (a) supprimer un terminateur **et ses arêtes** dans une copie de DB avec FK actives ⇒ CHK-04 ≥ 1 ; (b) modifier `source_block_id` d'une arête ⇒ CHK-07 ≥ 1 ; (c) `UPDATE compilation_units SET source_decision_count=99` ⇒ CHK-13 ≥ 1 ; (d) insérer `expression_operands` avec opérande ≥ parent ⇒ CHK-18 ≥ 1 ; chaque cas ⇒ exit 6 |
| AC-0.1-22 | `inspect` | `ske inspect --db $DB --snapshot latest --format json` | JSON conforme à §11.5 : `units[0].counts` = `{"functions":1,"blocks":5,"instructions":7,"source_decisions":2,"source_outcomes":4,"ir_decisions":2,"ir_outcomes":4,"diagnostics":0}` |
| AC-0.1-23 | `functions list` | `ske functions list --db $DB --snapshot latest --format json` | 1 fonction `example.Decide`, `blocks`=5, `source_decisions`=2, `ir_outcomes`=4, `page.total`=1 |
| AC-0.1-24 | CFG | `ske functions cfg --db $DB --snapshot latest --function example.Decide --format json` | `entry` = `example.Decide/b0` ; 5 blocs ; `b0` a 2 `successors` de `kind` `BRANCH_TRUE` puis `BRANCH_FALSE`, `guard.negated` `false` puis `true` ; `b1`, `b3`, `b4` n'ont aucun successeur |
| AC-0.1-25 | Branches (inventaire) | `ske branches list --db $DB --snapshot latest --function example.Decide --format json` | `analysis` = `null` ; 2 `source_decisions` de `kind` `IF`, `span` `decide.go:4:5-4:10` et `decide.go:7:5-7:12` ; 4 `ir_outcomes` tous `"status":"UNANALYZED"` ; chaque issue source a 1 `links[0]` `DIRECT`/`EXACT`/`inverted:false` |
| AC-0.1-26 | Polarité inversée | importer `check`, puis `ske branches list … --function example.Check --format json` | l'issue `example.Check/sd1/o0` a **un** lien vers `example.Check/b2/i0/e1` avec `"inverted":true` ; `sd1/o1` vers `…/b2/i0/e0` avec `"inverted":true` |
| AC-0.1-27 | `SWITCH` | importer `switch3`, lister `example.DaySwitch` | 1 décision source `SWITCH` (span `switch_fixture.go:4:9-4:10`), issues `CASE:0`, `CASE:1`, `DEFAULT` ; 3 `ir_outcomes` de mêmes étiquettes ; `edge_kind` `SWITCH_CASE`, `SWITCH_CASE`, `SWITCH_DEFAULT` ; `control_edge_case_values` : 3 lignes |
| AC-0.1-28 | `INVOKE` et stub externe | importer `invoke` | `ir_decisions.kind='INVOKE'` (1) ; `call_sites.resolution_status='EXTERNAL'` ; `ske functions list` ne montre **pas** `example.risky`, `--include-external` le montre (`is_external:true`) |
| AC-0.1-29 | PHI | importer `adjusted` | `phi_inputs` = 2 lignes ; `instructions` `PHI` a 0 ligne dans `instruction_operands` |
| AC-0.1-30 | Fixtures négatives (33 fichiers + 1 scénario CLI) | `cargo test -p ske-cli --test negative` | exit 0 : pour **chaque** `bad_*.cir.pb` et le cas CLI `E_UNIT_DUPLICATE` : exit `3`, `error.code` exact (§15.2.3), **aucune** ligne ajoutée (tous les `COUNT(*)` identiques à avant, y compris `snapshots` et `repositories`) |
| AC-0.1-31 | Immutabilité | `cargo test -p ske-storage closed_snapshot_rejects_structural_writes` | exit 0 : obtenir un writer sur un snapshot `CLOSED` ⇒ `E_SNAPSHOT_CLOSED` |
| AC-0.1-32 | Pas de déduplication | ingérer deux fois `decide`, puis `ske snapshots list --db $DB --format json` | 2 snapshots (ids 1 et 2), `source_fingerprint` et `config_fingerprint` **identiques** entre eux |
| AC-0.1-33 | Résolution de `latest` | `cargo test -p ske-cli --test resolution` | exit 0 : 2 dépôts + `--snapshot latest` sans `--repository` ⇒ exit 2, `E_AMBIGUOUS`, `candidates` listant les 2 dépôts ; avec `--repository` ⇒ OK ; `--function nope` ⇒ exit 7 `E_NOT_FOUND` |
| AC-0.1-34 | Fingerprints (formules §5.4) | `cargo test -p ske-ingest fingerprints_` | exit 0 : valeurs recalculées indépendamment dans le test à partir des fichiers/champs |
| AC-0.1-35 | Streaming et mémoire | `cargo run --release -p ske-cir --example gen_fixtures -- --big --out /tmp/big && /usr/bin/time -v target/release/ske ingest cir --db $(mktemp -d)/b.sqlite --input /tmp/big/big.cir.pb 2>&1 \| grep 'Maximum resident'` | exit 0 de l'ingestion ; « Maximum resident set size » < 524288 (ko). Sous macOS : mesure consignée, critère non bloquant |
| AC-0.1-36 | Codes de sortie | `cargo test -p ske-cli --test exit_codes` | exit 0 : le code `1` n'est jamais renvoyé ; les codes 0, 2, 3, 6, 7 correspondent à §11.3 |
| AC-0.1-37 | Sortie propre | `cargo test -p ske-cli --test stdout_purity` | exit 0 : avec `--format json`, stdout est du JSON valide **uniquement** (même en erreur) ; les logs sont sur stderr |
| AC-0.1-38 | Documentation | `ls docs/architecture.md docs/cir-semantics.md docs/soundness-and-limits.md docs/adapter-contract.md docs/sql-schema.md docs/dependencies.md docs/implementation-status.md docs/open-questions.md docs/validation/v0.1.md && ls docs/adr/ADR-0{01..19}.md` | exit 0 (les 19 ADR existent, chacun avec les 5 sections §18) |
| AC-0.1-39 | CI | `grep -q 'verify.sh' .github/workflows/ci.yml && grep -q 'cargo clippy' .github/workflows/ci.yml` | exit 0 |
| AC-0.1-40 | Verify | `./scripts/verify.sh` | exit 0 ; dernière ligne `VERIFY OK v0.1` (les étapes 7 et 8 de §15.4 ne sont pas encore requises) |
| AC-0.1-41 | Immuabilité des preuves acceptées | `cargo test -p ske-storage proof_immutability_` | `UPDATE proof_records SET verdict='REJECTED'` et `DELETE` refusés avec `E_PROOF_IMMUTABLE` ; sur run ayant preuve acceptée, `INSERT/UPDATE/DELETE` de `solver_queries`, `outcome_arrivals`, `analysis_assumptions` refusés, attributs critiques de `analysis_runs` figés ; `UPDATE analysis_runs SET status='CANCELLED'` (ou toute valeur ≠ `COMPLETED`) refusé avec `E_PROOF_SUPPORT_FROZEN` ; un **nouveau run** reste insérable ; `UPDATE/DELETE branch_analysis_results` déjà `PROVEN_UNREACHABLE` sont refusés ; `UPDATE analysis_runs.metrics_json` reste autorisé et ne change aucun fait de preuve ; les replays du vérificateur ne modifient pas `solver_queries` |
| AC-0.1-42 | Cas d’activation : intégrité SQL | `cargo test -p ske-storage source_activation_integrity` | un cas vide ou inter-unité est détecté par `CHK-22` ; références externes refusées ; inventaire sans cas complets reste interrogeable mais ne justifie pas une preuve source |

**Definition of Done v0.1 :** les 42 critères sont `PASS`, consignés dans `docs/validation/v0.1.md` avec commandes et sorties ; `docs/implementation-status.md` liste chaque fonctionnalité de §1.1 en `DONE/PARTIAL/NOT_STARTED` ; limites restantes écrites.

---

## v0.2 — Adaptateur Go et inventaires vérifiés

**Objectif :** extraire un vrai projet Go (sans LLM, sans compilation) vers le CIR et obtenir des inventaires source/IR **égaux aux tableaux du §15.1.5**.

**Prérequis :** v0.1 entièrement `PASS` ; toolchain Go installé (`go version`) ; `examples/go-branch-lab` écrit **exactement** comme au §15.1.

### Tâches ordonnées

| ID | Tâche | Livrables |
|---|---|---|
| T-0.2-01 | Relire et **ne pas régénérer** `examples/go-branch-lab` créé en v0.1 ; vérifier compilation, vet et gofmt avec la toolchain Go épinglée | `examples/go-branch-lab/` |
| T-0.2-02 | Module Go `adapters/go` : `internal/frame`, `internal/profile` (§11.4), protobuf généré | `adapters/go/` |
| T-0.2-03 | `internal/load` : `go/packages` sécurisé (§9.1) ; commande `list_units` | idem |
| T-0.2-04 | `internal/decisions` : **émetteur** (§9.2) puis, **dans un autre fichier et sans partager de code**, le **compteur indépendant** `countDecisions` | idem |
| T-0.2-05 | `internal/lower` : SSA→CIR (§9.4), types (BITVECTOR selon `GOARCH`, TUPLE, INTERFACE…), valeurs, constantes, PHI, arêtes (§4.7) | idem |
| T-0.2-06 | `internal/link` : liens source→IR **et cas d’activation** (§9.5) ; `internal/calls` : cibles (§9.6) ; diagnostics (§5.6) | idem |
| T-0.2-07 | `internal/emit` : flux ordonné déterministe ; `EndUnit` | idem |
| T-0.2-08 | Côté Rust : lanceur de sous-processus (limites, framing, consentement `--yes-run-adapter`, `.ske/consent.json`), `ske ingest go`, vérification des hash de sources | `crates/ske-ingest/`, `crates/ske-cli/` |
| T-0.2-09 | Golden files **à la main** (`tests/golden/go/`) transcrits du §15.1.5 ; tests de propriétés P-* ; tests différentiels Go↔CIR ; aller-retour protobuf Go↔Rust | `tests/`, `adapters/go/**/_test.go` |
| T-0.2-10 | Mettre à jour `docs/adapter-contract.md`, `docs/validation/v0.2.md`, `verify.sh` (étape 7) ; **tag local** `cir-v1-candidate` | docs, git tag |

### Critères d'acceptation v0.2

| ID | Critère | Commande de vérification | Résultat attendu |
|---|---|---|---|
| AC-0.2-01 | Le laboratoire Go est valide | `(cd examples/go-branch-lab && go build ./... && go vet ./... && test -z "$(gofmt -l .)")` | exit 0 |
| AC-0.2-02 | Adaptateur propre | `(cd adapters/go && go vet ./... && test -z "$(gofmt -l .)" && go test ./...)` | exit 0 |
| AC-0.2-03 | Binaire | `(cd adapters/go && go build -o ../../target/ske-adapter-go ./cmd/ske-adapter-go)` | exit 0, binaire présent |
| AC-0.2-04 | Liste des unités | `cargo test -p ske-ingest adapter_list_units` | exit 0 : exactement `go:example@linux/amd64`, `go:example/partial@linux/amd64`, `go:example/tags@linux/amd64` (dans cet ordre) |
| AC-0.2-05 | Ingestion | `ske ingest go --db $DB --repo $LAB --adapter target/ske-adapter-go --yes-run-adapter --format json` | exit 0 ; 3 unités ; `example` `COMPLETE` ; `example/partial` `PARTIAL` ; `example/tags` `COMPLETE` ; snapshot `PARTIAL` ; aucun `ERROR` |
| AC-0.2-06 | Décisions source = §15.1.5 | `cargo test -p ske-cli --test go_golden_decisions` | exit 0 : **toutes** les lignes du tableau (fonction, rang `sdN`, `kind`, texte relu au span, parent, étiquettes d'issues, totaux) |
| AC-0.2-07 | Compteur indépendant | `(cd adapters/go && go test ./internal/decisions -run TestCounterMatchesEmitter)` et `cargo test -p ske-cli --test go_counters` | exit 0 : pour chaque unité `expected_source_decisions == emitted_source_decisions == COUNT(source_decisions)` |
| AC-0.2-08 | Propriétés IR | `cargo test -p ske-cli --test go_props` | exit 0 : P-LINK-ALL, P-NO-HEUR, P-IRDEC-n, P-INVERT-Check, P-SHORT-AndCall, P-SHORT-And, P-ACT-OR, P-ACT-AND, P-ACT-MIXED, P-ACT-NEGATIVE, P-SWITCH-Day, P-TWORET, P-INTERFACE-Total, P-EXT-HasA, P-OVERFLOW-width, P-CONC-Pipe, P-GEN |
| AC-0.2-09 | Diagnostics | `cargo test -p ske-cli --test go_diagnostics` | exit 0 : tableau « Diagnostics attendus » §15.1.5 ; aucun `WARNING`/`ERROR` non listé sur `example` |
| AC-0.2-10 | Différentiel Go ↔ CIR | `cargo test -p ske-cli --test go_vs_cir` | exit 0 : pour `decide` et `check`, projection sémantique identique (décisions source : kind/span/issues ; 4 issues IR ; mêmes liens modulo les ids ; `inverted` identiques) |
| AC-0.2-11 | Build tags | `cargo test -p ske-cli --test go_build_tags` | exit 0 : profil linux ⇒ sources de `example/tags` = `tags/tag_linux.go` seul ; profil darwin ⇒ `tags/tag_other.go` seul ; 2 snapshots distincts, `config_fingerprint` différents ; `BUILD_TAG_EXCLUDED_FILE` présent |
| AC-0.2-12 | Déterminisme | `cargo test -p ske-cli --test go_deterministic` | exit 0 : deux extractions du même projet ⇒ flux **identiques octet par octet** (P-DET) |
| AC-0.2-13 | Environnement verrouillé | `(cd adapters/go && go test ./internal/load -run TestEnvLocked)` | exit 0 : `GOFLAGS=-mod=readonly`, `GOTOOLCHAIN=local`, `GOPROXY=off`, `CGO_ENABLED=0`, `GOENV=off`, `GOEXPERIMENT=` ; une valeur hostile dans l'environnement parent (`GOPROXY=https://invalid`, `GOFLAGS=-mod=mod`) est **écrasée** |
| AC-0.2-14 | Aucune compilation | `(cd adapters/go && go test ./internal/load -run TestOnlyGoListInvocations)` | exit 0 : un faux `go` placé en tête du `PATH` journalise ses arguments ; seules les invocations `list`, `version`, `env` sont observées (jamais `build`, `test`, `generate`, `run`, `install`) |
| AC-0.2-15 | Aller-retour protobuf | `(cd adapters/go && go test ./internal/frame -run TestRoundTripFixtures)` | exit 0 : chaque `*.cir.pb` **positif** de `tests/fixtures/cir/` est décodé en Go et Rust ; les `Record` décodés sont **sémantiquement égaux** (mêmes champs, valeurs, présence proto3 `optional`, `oneof`, listes ordonnées) après aller-retour. Aucune obligation d’identité des octets Protobuf entre runtimes |
| AC-0.2-16 | Échec d'unité | `cargo test -p ske-cli --test go_failures` | exit 0 : un profil dont le `patterns` ne trouve rien ⇒ snapshot `ABORTED`, exit 3 ; une unité en échec + une réussie ⇒ snapshot `CLOSED`, unité `FAILED` avec diagnostics, exit 0 |
| AC-0.2-17 | Consentement | `cargo test -p ske-cli --test consent` | exit 0 : sans `--yes-run-adapter` et sans terminal ⇒ exit 2 ; avec ⇒ `.ske/consent.json` écrit |
| AC-0.2-18 | Limites du protocole | `cargo test -p ske-ingest adapter_limits` | exit 0 : `--max-message-bytes 1000` ⇒ `E_ADAPTER_LIMIT` ; arrêt du processus enfant ; unité `FAILED` |
| AC-0.2-19 | Vérification des hash | `cargo test -p ske-ingest source_hash_` | exit 0 : modifier un fichier entre extraction et import ⇒ `E_SOURCE_HASH_MISMATCH` |
| AC-0.2-20 | Intégrité | `ske db check --db $DB` après AC-0.2-05 | exit 0 |
| AC-0.2-21 | CIR candidate | `git tag -l cir-v1-candidate` | sortie non vide ; **aucun gel définitif avant validation multilangage** |
| AC-0.2-22 | Verify | `./scripts/verify.sh` | exit 0 ; dernière ligne `VERIFY OK v0.2` (**sans** `go adapter skipped`) |
| AC-0.2-23 | Régression v0.1 | tous les `AC-0.1-*` | `PASS` |
| AC-0.2-24 | Cas d’activation Go | `cargo test -p ske-cli --test go_activation_cases` | `P-ACT-OR`, `P-ACT-AND`, `P-ACT-MIXED`, `P-ACT-NEGATIVE` sont vérifiés ; aucun mapping manquant transformé en lien inventé |
| AC-0.2-25 | Sites de sortie implicite | `cargo test -p ske-cli --test go_implicit_exit_inventory` | `implicit_exit_sites` contient les opérations à panic implicite non modélisées avec spans et statut ; aucune conclusion de complétude comportementale n’ignore ces sites |

**Definition of Done v0.2 :** les fichiers Go du laboratoire sont analysés sans LLM ; les inventaires source/IR égalent les golden files (écrits à la main) ; les cas non supportés restent visibles et bloquent les qualifications de complétude correspondantes (ex. `example/partial` = `PARTIAL`).

---

## v0.3 — Moteur symbolique local + SMT

**Objectif :** `ske branches analyze` produit pour chaque issue IR un statut correct et traçable ; prouve l'impossibilité de la branche contradictoire de `Decide` ; ne produit jamais une preuve sur appel opaque.

**Prérequis :** v0.2 `PASS` ; Z3 utilisable depuis Rust (spike : `cargo add z3` ou `z3-sys` avec la fonctionnalité de construction/linkage documentée ; consigner la procédure d'installation dans `README.md` et `docs/dependencies.md` ; si la compilation de Z3 est trop longue pour la CI, documenter l'utilisation d'une bibliothèque système).

### Tâches ordonnées

| ID | Tâche | Livrables |
|---|---|---|
| T-0.3-01 | `ske-solver` : trait `Solver`, `Z3Solver`, `ScriptedSolver`, encodeur SMT-LIB (§6.4), mapping des raisons `unknown`, `rlimit`/graine | `crates/ske-solver/` |
| T-0.3-02 | `ske-symbolic` : arène d'expressions + interning + simplifications **exhaustives** (§6.2) | `crates/ske-symbolic/` |
| T-0.3-03 | États, environnement, transferts par opcode (§6.3, registre §4.3) ; PHI par prédécesseur réel ; `DIV`/`REM`/décalages avec successeur de panic | idem |
| T-0.3-04 | Mémoire M0/M1 (§6.9) avec `store_log` et alias | idem |
| T-0.3-05 | Exploration (§6.5), arêtes de retour (§6.8), budgets (§6.7), arrivées, classification (§6.6) | idem |
| T-0.3-06 | Vérificateur de preuve indépendant (`verify.rs`), écriture des `proof_records` avant les résultats | idem |
| T-0.3-07 | Persistance (`analysis_runs`, `analysis_assumptions`, `solver_queries`, `outcome_arrivals`, `branch_analysis_results`, `branch_cases`, `symbolic_expressions`), `ske branches analyze`, `ske branches list --run`, `ske runs list/mark-failed`, gestion SIGINT | `crates/ske-storage`, `crates/ske-query`, `crates/ske-cli` |
| T-0.3-08 | Tests d'analyse §15.3 (un test par ligne), tests de budget/solveur, propriétés `prop_*`, test différentiel exécutant `Decide`/`Check`/`AddOne`, docs, `verify.sh` étapes 8–9 | tout |

### Critères d'acceptation v0.3

| ID | Critère | Commande de vérification | Résultat attendu |
|---|---|---|---|
| AC-0.3-01 | Solveur réel | `cargo test -p ske-solver z3_` | exit 0 : `x <s 0 ∧ x <s -10` (64 bits) `Sat` ; `¬(x <s 0) ∧ x <s -10` `Unsat` ; `x + 1 <s x` sur 8 bits `Sat` avec `x = 127` |
| AC-0.3-02 | Encodage = sémantique | `cargo test -p ske-solver encoding_matches_bitvec` | exit 0 : pour **chaque** opcode « Oui » du registre, les vecteurs §4.9 passés au solveur donnent les mêmes bits que `ske-cir::bitvec` |
| AC-0.3-03 | Solveur scripté | `cargo test -p ske-solver scripted_` | exit 0 : rejoue `Unknown`, `Timeout(Rlimit)`, `Sat`, `Unsat` dans l'ordre |
| AC-0.3-04 | DAG | `cargo test -p ske-symbolic expr_` | exit 0 : `expr_interning_structural` (deux constructions identiques ⇒ même id ; deux nœuds de même hash mais structures différentes ⇒ ids distincts), `expr_simplify_each_rule` (une assertion par règle §6.2), `expr_no_extra_simplification` (`x+0`, `x*1`, `a+b` vs `b+a` **non** simplifiés) |
| AC-0.3-05 | `Decide` (statuts) | importer `decide.cir.pb` puis `ske branches analyze --db $DB --snapshot latest --entry example.Decide --format json` | exit 0 ; `run_status:"COMPLETED"`, `exploration_closed:true`, `cut_count:0` ; `counts` = `{"UNANALYZED":0,"POSSIBLE":3,"WITNESSED":0,"PROVEN_UNREACHABLE":1,"UNKNOWN":0,"TIMEOUT":0,"UNSUPPORTED":0}` ; `assumptions` ⊇ `SEQUENTIAL_EXECUTION`, `CLOSED_ENTRY_CONTEXT` |
| AC-0.3-06 | `Decide` (détail) | `ske branches list --db $DB --snapshot latest --function example.Decide --run latest --format json` | `ir_outcomes` : `b0/i1/e0` `POSSIBLE`, `b0/i1/e1` `POSSIBLE`, `b2/i1/e0` **`PROVEN_UNREACHABLE`** (avec evidence `proof`), `b2/i1/e1` `POSSIBLE` ; issues source dérivées : `sd1/o0` `PROVEN_UNREACHABLE`, les 3 autres `POSSIBLE` |
| AC-0.3-07 | Preuve tracée | `cargo test -p ske-verify decide_proof_row` (ne nécessite pas de CLI sqlite3 externe) | exactement 1 ligne : `ACCEPTED`, 0, `EXACT` ; `solver_queries` contient ≥ 4 lignes dont 1 `UNSAT` avec `smtlib` non NULL |
| AC-0.3-08 | Table §15.3 | `cargo test -p ske-cli --test analysis_table` | exit 0 : 1 test par ligne de la table §15.3 (`analysis_decide`, `analysis_abs`, `analysis_adjusted`, `analysis_classify`, `analysis_check`, `analysis_addone`, `analysis_day`, `analysis_count3_u3`, `analysis_count3_u2`, `analysis_sum_u3`, `analysis_mem_nocall`, `analysis_mem_opaque`, `analysis_mem_alias`, `analysis_mem_alloc`, `analysis_hasa`, `analysis_parse`, `analysis_divmod`) ; statuts, `run_status`, `exploration_closed`, `cut_count` conformes ; les fonctions Go sont ingérées depuis `$LAB` (v0.2) et `mem_*`/`decide`/`check` **aussi** depuis les fixtures CIR (résultats identiques) |
| AC-0.3-09 | Témoin entier 8 bits | test `analysis_addone` | `branch_cases.witness_json` de `y < x` TRUE = `{"x":"127"}` ; `validation_method='SMT_MODEL'` |
| AC-0.3-10 | Mémoire | `cargo test -p ske-symbolic memory_` | exit 0 : `memory_two_loads_same_expr` ; `memory_strong_update` (`*p=1; *p` = 1) ; `memory_alias_param_param` (cf. `MemAlias`) ; `memory_alloc_distinct` (cf. `MemAlloc`) ; `memory_havoc_epoch` (appel ⇒ lectures ultérieures = nouveaux symboles), `memory_same_site_two_iterations`, `memory_summary_never_strong_update`, `memory_alias_chain_soundness` |
| AC-0.3-11 | Budgets et solveur scripté | `cargo test -p ske-symbolic -- budget_ && cargo test -p ske-symbolic -- scripted_` | exit 0 : `T-BUDGET-STATES`, `T-BUDGET-QUERIES`, `T-SCRIPT-UNKNOWN`, `T-SCRIPT-TIMEOUT` conformes **ligne par ligne** au tableau §15.3 |
| AC-0.3-12 | Vérificateur indépendant | `cargo test -p ske-verify verifier_` | exit 0 : `T-VERIFIER-REJECT` (0 ligne `PROVEN_UNREACHABLE`, statuts `UNKNOWN` avec explication `PROOF_REJECTED…`) ; `verifier_accepts_decide` ; le vérificateur n'importe pas le module de classification (`check-deps` ou test de visibilité) |
| AC-0.3-13 | Triggers (E2E) | test `T-TRIGGER` (`cargo test -p ske-storage trigger_`) | exit 0 |
| AC-0.3-14 | Solveur indisponible | test `T-SOLVER-FAIL` | exit 0 : `run_status = FAILED`, diag `SOLVER_UNAVAILABLE`, résultats déjà persistés conservés |
| AC-0.3-15 | Boucles | tests `analysis_count3_u3` / `_u2` / `analysis_sum_u3` | conformes §15.3 ; **aucun** `PROVEN_UNREACHABLE` quand `exploration_closed=false` (propriété vérifiée sur **tous** les runs de la suite : `prop_no_proof_when_not_closed`) |
| AC-0.3-16 | Code de sortie 4 | `ske branches analyze … --entry example.Sum --require-complete` | exit **4** ; sans `--require-complete` : exit 0 et JSON `run_status:"PARTIAL"` |
| AC-0.3-17 | Annulation | `cargo test -p ske-cli --test cancel_sigint` (Unix) | exit 0 : SIGINT pendant une analyse longue ⇒ exit 130, `run_status='CANCELLED'`, arrivées déjà écrites conservées |
| AC-0.3-18 | Runs | `ske runs list --db $DB --format json` ; test `runs_mark_failed` | liste les runs avec statut ; un run laissé `RUNNING` passe à `FAILED` après `ske runs mark-failed --run N` ; `ske db check` signale le `RUNNING` avant (CHK-19, WARNING) |
| AC-0.3-19 | Déterminisme | `cargo test -p ske-cli --test analysis_deterministic` | exit 0 : deux analyses de `Decide` (graine fixe) ⇒ mêmes statuts, mêmes explications, mêmes `query_fingerprint` (liste ordonnée identique) |
| AC-0.3-20 | Propriétés de sécurité | `cargo test --workspace prop_` | exit 0 : `prop_unsupported_never_exact`, `prop_inventory_never_shrinks`, `prop_json_possible_vs_witnessed`, `prop_budget_diag`, `prop_no_complete_on_error` |
| AC-0.3-21 | Différentiel concret | `cargo test -p ske-cli --test differential` (exécute `(cd tests/differential && go run .)` : programme de ce dépôt, module Go séparé avec `replace example => ../../examples/go-branch-lab`, appelant `Decide`, `Check`, `AddOne` sur une grille d'entrées et affichant du JSON) | exit 0 : aucune valeur observée n'est exclue par le modèle (ex. `Decide` ne renvoie jamais `-2` ; `AddOne(127)` renvoie `1`) |
| AC-0.3-22 | Intégrité après analyse | `ske db check --db $DB` | exit 0 (22 contrôles) |
| AC-0.3-23 | Documentation | `docs/soundness-and-limits.md` mis à jour (liste des approximations de v0.3 : M1, CALL havocké, boucles bornées, panics d'appel non explorés, flottants/chaînes non encodés) ; `docs/validation/v0.3.md` | fichiers présents, chaque limite de §6 y figure |
| AC-0.3-24 | Verify | `./scripts/verify.sh` | exit 0 ; `VERIFY OK v0.3` ; étapes 8 et 9 exécutées (messages `SAT case ok`, `UNSAT case ok`, `UNKNOWN case ok`, `TIMEOUT case ok`, `PARTIAL never COMPLETE ok`) |
| AC-0.3-25 | Régression | tous les `AC-0.1-*` et `AC-0.2-*` | `PASS` |
| AC-0.3-26 | Allocations même site et alias conservateur | `cargo test -p ske-symbolic memory_` (vérifier les trois tests nommés dans `-- --list`) | les allocations répétées sont distinctes lorsqu’identifiées ; aucune mise à jour forte sur objet récapitulatif ; aucun comportement réel exclu par simplification d’alias |
| AC-0.3-27 | Source outcome sans faux verdict | `cargo test -p ske-cli --test source_activation_regression` | Or/And/Mixed respectent les cas d’activation et les contraintes conjointes ; aucun `PROVEN_UNREACHABLE` obtenu par agrégation d’arêtes ; mappings incomplets → `UNKNOWN`, `elided` → `UNANALYZED` |
| AC-0.3-28 | Transparence référentielle | `cargo test -p ske-symbolic call_not_referentially_transparent` | deux appels sans arguments lisant un état global mutable ne partagent pas un résultat SMT : `is_pure_proven` seul n’autorise pas `UNINTERPRETED` déterministe |
| AC-0.3-29 | Niveau de garantie explicite | `cargo test -p ske-cli --test verification_level` | `PROVEN_UNREACHABLE` n'est accompagné que de `MODEL_UNREACHABLE` + hypothèses ; `CERTIFIED_UNREACHABLE` impossible en v0.x ; `SAT` reste `POSSIBLE` |

**Definition of Done v0.3 :** le moteur prouve l'impossibilité de la branche contradictoire de `Decide` (avec `proof_record` accepté) ; retrouve des modèles satisfaisables pour les branches faisables ; n'invente aucune preuve sur appel opaque (`MemOpaque` : 4 × `POSSIBLE`, dont **2 EXACT avant appel** et **2 OPAQUE après appel**) ; respecte les budgets sans perdre les résultats validés ; toute exploration non fermée reste `PARTIAL` sans `PROVEN_UNREACHABLE`.


---

## v0.4 — Analyse interprocédurale

**Objectif :** composer des résumés de fonctions, restituer depuis un point d'entrée une règle conditionnée dans une fonction appelée, traiter récursion et appels inconnus sans fausse conclusion.

**Prérequis :** v0.3 `PASS`. **Les points marqués ★ sont à détailler dans un ADR au démarrage du jalon** (choix d'implémentation laissés ouverts), sans changer les critères.

**Fixture ajoutée** `examples/go-branch-lab/interproc.go` (ajouter un fichier ne DOIT pas modifier les golden de v0.2 : les tests de v0.2 sont par fonction) :
```go
package example

func Entry(a int) int {
	return helper(a)
}

func helper(x int) int {
	if x > 100 {
		return 1
	}
	return 0
}

type Doer interface{ Do(n int) int }

func CallDoer(d Doer, n int) int {
	if d.Do(n) > 0 {
		return 1
	}
	return 0
}

func Fact(n int) int {
	if n <= 1 {
		return 1
	}
	return n * Fact(n-1)
}

func Divide2(a, b int) (int, error) {
	q, err := DivMod(a, b)
	if err != nil {
		return 0, err
	}
	return q, nil
}
```
Décisions source attendues : `helper` : `IF "x > 100"` ; `CallDoer` : `IF "d.Do(n) > 0"` ; `Fact` : `IF "n <= 1"` ; `Divide2` : `IF "err != nil"` ; `Entry` : aucune. `CallDoer` : appel `INTERFACE` **sans aucune implémentation** de `Doer` dans le package ⇒ `call_targets` vide, `resolution_status = UNRESOLVED`, diagnostic `CALL_TARGET_UNRESOLVED`.

### Tâches ordonnées

| ID | Tâche |
|---|---|
| T-0.4-01 | Graphe d'appels persistant et requêtable (`call_sites`, `call_targets`), SCC (Tarjan) ; `ske paths explore` (statique) |
| T-0.4-02 | Résumés de fonctions (§7.1) : calcul, persistance (`function_summaries`, `summary_cases`, `summary_case_returns`, `summary_case_outcomes`, `observable_effects`), **retours multiples** |
| T-0.4-03 | Application des résumés aux sites d'appel (substitution mémoïsée), propagation paramètres/retours/mémoire ; arêtes d'exception des appels ; cibles inconnues ⇒ havoc |
| T-0.4-04 | Récursion : dépliage borné `max_recursion_unfold = 2` (clé du budget), cas résiduel `PARTIAL` ; ★ cache de résumés (clé §7.1) |
| T-0.4-05 | Domaine d'intervalles (`ske-abstract`) et mode de boucle `loop_mode = "intervals"` (★ algorithme ci-dessous) |
| T-0.4-06 | Interpréteur **concret** du CIR (`ske-symbolic::concrete`) pour valider les résumés et les intervalles sur des entrées aléatoires |
| T-0.4-07 | Exploration de chemins : `ske paths explore` ; JSON §19.v0.4 ; tests, docs, `verify.sh` |

**Mode de boucle `intervals`** (activé par `loop_mode = "intervals"` dans `budget.toml` ; **défaut : `"unroll"`**, comportement v0.3 inchangé). À la `(loop_unroll+1)`ᵉ arrivée en tête de boucle par l'arête de retour : (1) calculer les intervalles de chaque valeur entière modifiée dans la boucle par itération de Kleene avec **élargissement** (une borne instable passe à la borne du type) puis **un** pas de rétrécissement ; toute opération pouvant déborder donne la plage complète du type ; (2) construire un **état générique** : chaque `PHI` d'en-tête dont l'entrée de retour diffère devient un symbole frais `loopvar:<phi>` contraint à son intervalle ; toute adresse écrite dans la boucle est havockée ; (3) soundness de l'état = `OVERAPPROX` ; (4) l'explorer **une fois** (un état d'en-tête déjà vu avec `abstracted=true` est abandonné : subsumption) ; (5) ajouter l'hypothèse `LOOP_INTERVAL_ABSTRACTION` et le diagnostic `LOOP_ABSTRACTED` (INFO). Dans ce mode **aucune coupure** n'est comptée pour cette boucle : l'exploration peut être fermée ; les `PROVEN_UNREACHABLE` obtenus portent `soundness = OVERAPPROX` et `origin ABSTRACT_DERIVED` côté règles.

**Sortie JSON de `ske paths explore`** :
```json
{"source": "example.Entry", "target": "example.helper",
 "status": "POTENTIALLY_REACHABLE",
 "incomplete": false,
 "paths": [{"calls": [{"caller": "example.Entry", "callsite": "example.Entry/b0/i0",
                       "callee": "example.helper", "resolution": "STATIC"}]}],
 "truncated": false, "truncation_reason": null, "visited_count": 2, "frontier_count": 0,
 "page": {"limit": 100, "offset": 0, "total": 1, "truncated": false}}
```
`status` ∈ {`POTENTIALLY_REACHABLE`, `NO_PATH_FOUND`} (jamais « inatteignable ») ; `incomplete = true` si l'exploration a rencontré un appel `UNRESOLVED`/`EXTERNAL`/`DYNAMIC` ; options `--max-depth`, `--max-paths`, `--max-nodes`.

### Résultats attendus (référence)

| Fonction | Résumé attendu |
|---|---|
| `Entry` | 2 cas, `exit_kind=RETURN` : garde `a > 100` ⇒ retour `[1]` ; garde `a <= 100` ⇒ retour `[0]` ; `referenced_outcomes` = les 2 issues IR de `helper` (respectivement) ; `precision=EXACT` |
| `CallDoer` | tous les `ir_outcomes` `POSSIBLE` (soundness `OPAQUE`) ; `precision` du résumé = `OVERAPPROX` ; **aucun** `PROVEN_UNREACHABLE` ; `unknowns` contient l'appel non résolu |
| `Fact` | **4 cas** : `n <= 1` ⇒ `1` ; `n > 1 ∧ n-1 <= 1` ⇒ `n * 1` ; `n > 1 ∧ n-1 > 1 ∧ n-2 <= 1` ⇒ `n * ((n-1) * 1)` ; `n > 1 ∧ n-1 > 1 ∧ n-2 > 1` ⇒ `n * ((n-1) * H)` avec `H` symbole havoc, cas `PARTIAL` ; **exactement 1** cas `PARTIAL` ; `complete = 0` ; `unknowns_json` contient `RECURSION_BOUND` |
| `DivMod` | 2 cas : `b == 0` ⇒ `[0, ErrZero]` ; `b != 0` ⇒ `[a / b, nil]` ; `summary_case_returns` : 2 lignes par cas |
| `Divide2` | **3 cas** (la valeur initiale de la globale `ErrZero` est un symbole libre, donc possiblement `nil`) : `b == 0 ∧ ErrZero != nil` ⇒ `[0, ErrZero]` ; `b == 0 ∧ ErrZero == nil` ⇒ `[0, nil]` ; `b != 0` ⇒ `[a / b, nil]` ; ordre par span de sortie, puis chemin d’issues IR lexicographique (§7.2) |
| `Sum` avec `loop_mode="intervals"` | `run_status=COMPLETED`, `exploration_closed=true`, ENTER_BODY **P**, EXIT **P**, aucun **PU**, diagnostic `LOOP_ABSTRACTED` |

### Critères d'acceptation v0.4

| ID | Critère | Commande de vérification | Résultat attendu |
|---|---|---|---|
| AC-0.4-01 | Graphe d'appels | `cargo test -p ske-cli --test callgraph` | exit 0 : `Entry→helper` `RESOLVED_UNIQUE` ; `CallDoer` `UNRESOLVED` ; `Entry→helper` a `world_closed=true` pour autoriser `RESOLVED_UNIQUE` ; `Total` (v0.2) toujours `RESOLVED_POSSIBLE` ; SCC de `Fact` = `{Fact}` (auto-boucle) |
| AC-0.4-02 | Chemins statiques | `ske paths explore --db $DB --source example.Entry --target example.helper --format json` ; puis `--source example.helper --target example.Entry` | 1ʳᵉ : `POTENTIALLY_REACHABLE`, 1 chemin ; 2ᵉ : `NO_PATH_FOUND`, `incomplete:false` (aucun appel inconnu rencontré depuis `helper`) ; `CallDoer`→`x` : `incomplete:true` |
| AC-0.4-03 | Résumé `Entry` | test `summary_entry` | conforme au tableau (2 cas, gardes, retours, issues référencées) |
| AC-0.4-04 | Règle conditionnée restituée | test `entry_precondition` | depuis `Entry`, la condition `x > 100` de `helper` apparaît comme `a > 100` (substitution) |
| AC-0.4-05 | Appel polymorphique non résolu | test `summary_calldoer` | conforme ; `UNKNOWN`/`POSSIBLE` mais jamais prouvé |
| AC-0.4-06 | Récursion | test `summary_fact` | conforme au tableau (4 cas, 1 `PARTIAL`, `complete=0`, `RECURSION_BOUND`) |
| AC-0.4-07 | Retours multiples | tests `summary_divmod`, `summary_divide2` | conformes ; **aucune** colonne « retour unique » ; `summary_case_returns` porte les 2 retours |
| AC-0.4-08 | Résumé = comportement concret | `cargo test -p ske-symbolic summary_matches_concrete` | exit 0 : pour `helper`, `DivMod`, `Fact` (n de 0 à 6) et 200 entrées aléatoires (graine fixe) l'interpréteur concret (CIR) exécute la fonction ; **exactement un** cas du résumé est satisfait par l'entrée ; s'il est `EXACT`, les retours égalent le concret ; s'il est `PARTIAL`, ne pas comparer les retours. Pour `Fact`, limiter les entrées aléatoires à `[-10,20]` ; toute exécution concrète doit avoir une borne d'étapes et de récursion (échec explicite plutôt que blocage). `DivMod` reçoit une globale `ErrZero` concrète prédéfinie pour le comparateur ; ne pas exécuter le stub `errors.New`. Jamais de cas contredit |
| AC-0.4-09 | Intervalles sûrs | `cargo test -p ske-abstract` | exit 0 : transfert (`ADD/SUB/MUL` avec détection de débordement ⇒ plage complète), élargissement/rétrécissement, `intervals_contain_concrete` (200 exécutions concrètes de `Sum` : chaque valeur observée de `i` et `s` est dans l'intervalle de sa valeur) |
| AC-0.4-10 | Boucle en mode intervalles | test `analysis_sum_intervals` | conforme au tableau (`COMPLETED`, closed, 2×**P**, `LOOP_ABSTRACTED`) |
| AC-0.4-11 | Défaut inchangé | tous les tests `analysis_*` de v0.3 | `PASS` sans modification (le mode par défaut reste `unroll`) |
| AC-0.4-12 | Persistance | `ske db check` après calcul de résumés | exit 0 ; `SELECT COUNT(*) FROM summary_case_returns` cohérent avec les tableaux ci-dessus (par ex. `DivMod` : 4 lignes) |
| AC-0.4-13 | Déterminisme | test `summary_deterministic` | deux calculs ⇒ mêmes `summary_fingerprint` |
| AC-0.4-14 | Verify et régression | `./scripts/verify.sh` ; tous les AC précédents | `VERIFY OK v0.4` ; `PASS` |

**Definition of Done v0.4 :** une règle conditionnée dans une fonction appelée est restituée depuis le point d'entrée avec la bonne précondition ; un appel polymorphique non résolu reste `UNKNOWN`/`POSSIBLE` et jamais une fausse conclusion prouvée.

---

## v0.5 — Connaissance comportementale

**Objectif :** produire, sans LLM, des règles comportementales exactes (sous les hypothèses) avec provenance, et une première analyse incrémentale.

**Tâches :** T-0.5-07 appliquer `migrations/0002_capabilities.sql` (trois tables, index ; mise à jour des attentes de schéma) ; T-0.5-01 extraction des cas (§7.2) et déduplication conservatrice ; T-0.5-02 rendu des gardes R1–R6 ; T-0.5-03 persistance (`behavior_scenarios`, `behavior_cases`, `behavior_case_outcomes`, `functional_rules`, `knowledge_evidence`) ; T-0.5-04 commandes `ske behaviors reconstruct` (JSON/YAML) et `ske rules list --with-evidence`, `ske rules stale-check` ; T-0.5-05 copie-sur-réutilisation des résumés (§13) ; T-0.5-06 tests, docs, `verify.sh`.

**Sortie JSON de `ske behaviors reconstruct --entry example.Check`** (l'identifiant de run `7` n'est qu'une valeur **illustrative** : les tests utilisent le vrai run créé ; les listes `evidence:[]` des cas 1 et 2 ci-dessous sont abrégées uniquement pour la lisibilité et **doivent être peuplées** conformément aux 11 lignes de preuve du tableau suivant ; un JSON d'API réel ne renvoie jamais une liste vide pour une règle persistée) (`--format yaml` produit le même contenu en YAML) :
```json
{"function": "example.Check", "run_id": 7, "precision": "EXACT",
 "assumptions": ["SEQUENTIAL_EXECUTION", "CLOSED_ENTRY_CONTEXT", "CALLEE_PANIC_EXITS_NOT_EXPLORED"],
 "cases": [
  {"ordinal": 0, "rule_key": "example.Check@run7#case0", "when": "age < 18", "then": "return ErrUnderage",
   "exit_kind": "RETURN", "precision": "EXACT",
   "source_outcomes": ["example.Check/sd0/o0"],
   "evidence": [{"kind": "summary_case", "id": "example.Check@run7#case0", "relation": "DERIVED_FROM"},
                {"kind": "ir_outcome", "id": "example.Check/b0/i1/e0", "unit_key": "go:example@linux/amd64", "relation": "GUARD_OUTCOME"},
                {"kind": "source_span", "span": "check.go:10:3-10:21", "relation": "RESULT_SITE"}]},
  {"ordinal": 1, "rule_key": "example.Check@run7#case1", "when": "age >= 18 AND enabled == false", "then": "return ErrDisabled", "exit_kind":"RETURN", "precision":"EXACT", "source_outcomes":["example.Check/sd0/o1","example.Check/sd1/o0"], "evidence":[{"kind":"summary_case","id":"example.Check@run7#case1","relation":"DERIVED_FROM"},{"kind":"ir_outcome","id":"example.Check/b0/i1/e1","unit_key":"go:example@linux/amd64","relation":"GUARD_OUTCOME"},{"kind":"ir_outcome","id":"example.Check/b2/i0/e1","unit_key":"go:example@linux/amd64","relation":"GUARD_OUTCOME"},{"kind":"source_span","span":"check.go:13:3-13:21","relation":"RESULT_SITE"}]},
  {"ordinal": 2, "rule_key": "example.Check@run7#case2", "when": "age >= 18 AND enabled == true", "then": "return nil", "exit_kind":"RETURN", "precision":"EXACT", "source_outcomes":["example.Check/sd0/o1","example.Check/sd1/o1"], "evidence":[{"kind":"summary_case","id":"example.Check@run7#case2","relation":"DERIVED_FROM"},{"kind":"ir_outcome","id":"example.Check/b0/i1/e1","unit_key":"go:example@linux/amd64","relation":"GUARD_OUTCOME"},{"kind":"ir_outcome","id":"example.Check/b2/i0/e0","unit_key":"go:example@linux/amd64","relation":"GUARD_OUTCOME"},{"kind":"source_span","span":"check.go:15:2-15:12","relation":"RESULT_SITE"}]}],
 "page": {"limit": 100, "offset": 0, "total": 3, "truncated": false}}
```

### Résultats attendus (référence, textes exacts)

| Entrée | Cas (ordre) : `when` ⇒ `then` ; issues source référencées |
|---|---|
| **`Check`** | 0 : `age < 18` ⇒ `return ErrUnderage` ; `sd0/o0` — 1 : `age >= 18 AND enabled == false` ⇒ `return ErrDisabled` ; `sd0/o1`, `sd1/o0` — 2 : `age >= 18 AND enabled == true` ⇒ `return nil` ; `sd0/o1`, `sd1/o1` |
| `Decide` | 0 : `x < 0` ⇒ `return -1` ; `sd0/o0` — 1 : `x >= 0 AND x >= -10` ⇒ `return 0` ; `sd0/o1`, `sd1/o1`. **Pas de cas pour `return -2`** (inatteignable) ; l'issue `sd1/o0` reste dans l'inventaire et `PROVEN_UNREACHABLE` |
| `Adjusted` | 0 : `x > 10` ⇒ `return x - 3` — 1 : `x <= 10` ⇒ `return x - 0` (aucune simplification algébrique en v0.5) |
| `Entry` | 0 : `a > 100` ⇒ `return 1` — 1 : `a <= 100` ⇒ `return 0` |
| `DivMod` | 0 : `b == 0` ⇒ `return 0, ErrZero` — 1 : `b != 0` ⇒ `return a / b, nil` |

Lignes `knowledge_evidence` pour `Check` : cas 0 → 3 lignes (1 `summary_case`, 1 issue IR, 1 span) ; cas 1 → 4 ; cas 2 → 4 ; **total 11**. `functional_rules` : 3 lignes `origin='DETERMINISTIC'`, `validation_status='UNVALIDATED'`, `rule_key` `example.Check@run<run_id>#case0..2`.

### Critères d'acceptation v0.5

| ID | Critère | Commande de vérification | Résultat attendu |
|---|---|---|---|
| AC-0.5-01 | Rendu des gardes | `cargo test -p ske-behavior render_guard_` | exit 0 : un test par règle R1–R6 + les chaînes exactes du tableau (`age < 18`, `age >= 18 AND enabled == false`, `x >= 0 AND x >= -10`, `a / b`…) |
| AC-0.5-02 | `Check` | `ske behaviors reconstruct --db $DB --entry example.Check --format json` | **3** cas, textes exacts du tableau, `precision:"EXACT"`, issues source listées comme au tableau, 4 issues source distinctes couvertes |
| AC-0.5-03 | Règles et preuves | `ske rules list --db $DB --snapshot latest --with-evidence --format json` | 3 règles ; **toute** règle a ≥ 1 evidence et un `validation_status` ; `knowledge_evidence` = 11 lignes (`Check` seul) ; CHK ok |
| AC-0.5-04 | Branche impossible conservée | test `behavior_decide` | 2 cas (pas de `return -2`) ; `ske branches list --run latest` montre toujours 4 issues IR dont 1 `PROVEN_UNREACHABLE` ; le nombre d'issues source avant/après est identique (`prop_inventory_never_shrinks`) |
| AC-0.5-05 | Autres références | tests `behavior_adjusted`, `behavior_entry`, `behavior_divmod` | conformes au tableau |
| AC-0.5-06 | Déduplication conservatrice | test `behavior_dedup` | fusion seulement si `exit_kind`, retours, delta mémoire et effets structurellement égaux ; jamais entre `RETURN` et `PANIC` ; l'union des issues référencées est conservée |
| AC-0.5-07 | Effets inconnus | test `behavior_unknown_effect` (fonction `MemOpaque`) | cas de précision ≤ `OVERAPPROX`, jamais `EXACT` |
| AC-0.5-08 | Pas d'interprétation métier | test `behavior_no_business_text` | `description` ne contient que `WHEN … THEN …` généré ; aucun champ libre |
| AC-0.5-09 | Réutilisation | test `summaries_reuse` : ingérer `$LAB` deux fois (snapshots A, B), calculer les résumés dans A puis dans B | dans B : `metrics.summaries_reused` = nombre de fonctions résumées, `summaries_computed` = 0, `reused_from_summary_id` non NULL, **résultats identiques** à A (modulo ids) ; si un `structural_fingerprint` diffère : recalcul |
| AC-0.5-10 | Obsolescence | test `rules_stale_check` : créer **un seul dépôt temporaire** en copiant `$LAB`, ingérer une première fois (snapshot A), puis modifier `decide.go` **dans ce même répertoire**, réingérer (snapshot B), `ske rules stale-check --repository <nom_du_dépôt> --from A --to B` | règles de `Decide` ⇒ `STALE` ; règles de `Check` inchangées (`UNVALIDATED`) ; les règles de A ne sont jamais réattribuées à B |
| AC-0.5-11 | YAML | `ske behaviors reconstruct … --format yaml` | YAML valide, mêmes valeurs que JSON (test de parse) |
| AC-0.5-12 | Verify et régression | `./scripts/verify.sh` ; tous les AC précédents **y compris AC-0.5-13** | `VERIFY OK v0.5` ; `PASS` |
| AC-0.5-13 | Migration de capacités | `cargo test -p ske-storage migration_0002_capabilities` | exit 0 : migration 2 appliquée exactement une fois ; **50 tables** au total (47 v0.1 + 3 nouvelles), 21 triggers ; `ske db check` inchangé et sans violations ; le test `schema_tables_match` vérifie la table attendue selon la version de migration (v1 : 47 ; v2 : 50) |

**Definition of Done v0.5 :** modèle exact du programme `Check` (3 cas, 4 issues source liées aux cas), exploitable sans LLM (JSON/YAML), avec statut de précision et preuve à chaque règle.

---

## v0.6 — Adaptateurs Rust et Java

**Objectif :** mêmes services `ingest/inspect/branches` pour Go, Rust, Java, **sans fuite de types internes de compilateur** dans le noyau.

**★ À détailler par ADR au démarrage du jalon** (toolchains épinglés, API exacte de MIR/SootUp) ; les critères ci-dessous sont fixes. Aucune modification du noyau (`ske-cir`, `ske-storage`, `ske-symbolic`) n'est permise pour accueillir un langage, **sauf** ajout d'opcodes/kinds par extension compatible et tests (sinon ADR).

**Fixtures** (à écrire exactement) :

`examples/rust-branch-lab/src/lib.rs`
```rust
pub fn decide(x: i32) -> i32 {
    if x < 0 {
        -1
    } else if x < -10 {
        -2
    } else {
        0
    }
}

pub fn kind(n: u8) -> &'static str {
    match n {
        0 => "zero",
        1..=9 => "small",
        _ => "big",
    }
}

pub fn parse_len(s: &str) -> Result<usize, std::num::ParseIntError> {
    let n: usize = s.parse()?;
    Ok(n)
}
```

`examples/java-branch-lab/src/main/java/example/Decide.java`
```java
package example;

public final class Decide {
    public static int decide(int x) {
        if (x < 0) {
            return -1;
        }
        if (x < -10) {
            return -2;
        }
        return 0;
    }

    public static String day(int n) {
        switch (n) {
            case 1:
                return "one";
            case 2:
            case 3:
                return "few";
            default:
                return "many";
        }
    }

    public static int guarded(Runnable r) {
        try {
            r.run();
            return 0;
        } catch (RuntimeException e) {
            return -1;
        }
    }
}
```


**Fixtures additionnelles obligatoires v0.6** (elles font partie de `examples/java-branch-lab/` ; aucun téléchargement ni exécution de code du dépôt étudié) :

`examples/java-branch-lab/src/main/java/example/ReflectionUnknown.java` :

```java
package example;

public final class ReflectionUnknown {
    public static Class<?> load(String className) throws ClassNotFoundException {
        return Class.forName(className);
    }
}
```

`examples/java-branch-lab/src/main/java/example/MultiCatchFinally.java` :

```java
package example;

public final class MultiCatchFinally {
    public static int guarded(String input) {
        try {
            return Integer.parseInt(input);
        } catch (NumberFormatException ex) {
            return -1;
        } catch (RuntimeException ex) {
            return -2;
        } finally {
            System.out.println("done");
        }
    }
}
```

Les deux sources sont testées **sans** présumer d'un CFG identique entre versions de bytecode. `EXCEPTION_FLOW_PARTIAL` porte `impact=INVENTORY` si une issue exceptionnelle n'est pas inventoriée ; `impact=EFFECTS` si seules les gardes sont imprécises (sans omission d'issue). Le test `java_multi_catch_finally` accepte `PARTIAL` quand les sémantiques ne sont pas modélisées, mais interdit un `COMPLETE` mensonger.

### Critères d'acceptation v0.6

| ID | Critère | Commande / test | Résultat attendu |
|---|---|---|---|
| AC-0.6-01 | Adaptateur Rust isolé | `(cd adapters/rust && cargo build)` ; `docs/adapter-contract.md` | compile avec **sa propre** `rust-toolchain.toml` ; les crates cœur ne dépendent pas de `rustc_private` (`check-deps.sh`) |
| AC-0.6-02 | Adaptateur Java isolé | `(cd adapters/java && ./gradlew build)` (ou Maven, ADR) | exit 0 ; versions épinglées |
| AC-0.6-03 | Rust : décisions source | test `rust_golden_decisions` | `decide` : `IF "x < 0"`, `IF "x < -10"` ; `kind` : `MATCH "n"` [`CASE:0`, `CASE:1`, `DEFAULT`] ; `parse_len` : `ERROR_PROPAGATION "s.parse()?"` [`SUCCESS`, `ERROR`] ; spans exacts relus dans la source |
| AC-0.6-04 | Java : décisions source | test `java_golden_decisions` | `decide` : 2 `IF` ; `day` : `SWITCH "n"` [`CASE:0` (1), `CASE:1` (2, 3), `DEFAULT`] ; `guarded` : `TRY_CATCH` [`SUCCESS`, `EXCEPTION`] |
| AC-0.6-05 | Types | test `lang_types` | Rust `i32` ⇒ `BITVECTOR(32,signed)` ; Java `int` ⇒ `BITVECTOR(32,signed)` ; Rust `u8` ⇒ `BITVECTOR(8,unsigned)` |
| AC-0.6-06 | Propriétés comportementales communes | test `cross_language_decide` : ingérer les 3 laboratoires, analyser `decide` (Go, Rust, Java) | **même relation comportementale** sur la grille de tests et mêmes conditions symboliques lorsqu’elles sont modélisées ; 2 décisions source `IF` et 4 issues **inventoriées** par langage. Pour une issue supprimée lors de l’abaissement (`elided`), attendre `UNANALYZED` (`explanation="ELIDED"`), **jamais** forcer un `PROVEN_UNREACHABLE`. Comparer séparément les IR, les hypothèses et les statuts effectivement démontrables ; ne pas exiger le même CFG ni le même nombre d’arêtes. |
| AC-0.6-07 | `SWITCH`/`INVOKE` réels | tests `java_switch_ir`, `java_invoke_exception` | `day` utilise un terminateur `SWITCH` avec `case_value_ids` ; `guarded` utilise `INVOKE` avec arêtes `NORMAL`/`EXCEPTION` |
| AC-0.6-08 | Inconnus explicites | test `java_reflection_unknown` (petite classe avec `Class.forName`) | cible `UNRESOLVED`, diagnostic `CALL_TARGET_UNRESOLVED`, jamais `world_closed=1` |
| AC-0.6-09 | Toolchain incompatible | test `rust_toolchain_mismatch` | échec lisible `AdapterError{code:"TOOLCHAIN_MISMATCH"}`, exit 3 |
| AC-0.6-10 | Précision affichée par langage | `ske inspect` | `units[].semantic_profile` et `capability_flags` par langage ; limites dans `docs/soundness-and-limits.md` (section par langage) |
| AC-0.6-11 | Intégrité et IPC | `ske db check` ; tests de framing Go↔Rust↔Java/Rust adapter | exit 0 ; mêmes règles de framing |
| AC-0.6-12 | Verify et régression | `./scripts/verify.sh` ; tous les AC précédents | `VERIFY OK v0.6` |
| AC-0.6-13 | Gel CIR après pilotes multilangages | `git tag -l cir-v1.0-frozen` ; `cargo test --workspace cir_compat_` | tag créé **seulement après** réussite des tests Go/Rust/Java, exceptions, `match`, types, contrôle de flux ; protocole majeur gelé seulement ici |
| AC-0.6-14 | Exceptions Java multiples | test `java_multi_catch_finally` | chemins `NORMAL`, plusieurs `EXCEPTION` filtrés, ordre des handlers conservé ; `finally` exécuté sur les issues pertinentes ou diagnostic `EXCEPTION_FLOW_PARTIAL`, jamais `COMPLETE` par omission |

**Definition of Done v0.6 :** mêmes services `ingest/inspect/branches` pour Go/Rust/Java ; précision et limitations affichées par langage.

---

## v0.7 — API de connaissance, MCP, LLM optionnel

**Objectif :** un agent peut demander « Quelles règles conditionnent l'opération X ? », obtenir un inventaire traçable avec inconnues, et consulter chaque preuve sans rouvrir arbitrairement tous les fichiers source.

**★ Détails d'API à fixer par ADR** (framework HTTP/MCP léger, exposant `ske-query`). Critères fixes :

| ID | Critère | Commande / test | Résultat attendu |
|---|---|---|---|
| AC-0.7-01 | Query API indépendante de SQLite | `cargo test -p ske-query dto_` ; `test "$(cargo tree -p ske-api --depth 1 -e normal | grep -c rusqlite || true)" -eq 0` | tests exit 0 ; le second compte vaut 0 (pas de dépendance directe) |
| AC-0.7-02 | Pagination et limites | test `api_limits` | toute réponse a `page` ; une exploration tronquée a `truncated=true`, `truncation_reason`, `visited_count`, `frontier_count` ; jamais présentée comme complète |
| AC-0.7-03 | Outils MCP | test `mcp_tools_list` | outils exposés exactement : `inspect`, `branches`, `explain`, `rules`, `impact` ; schémas JSON d'entrée/sortie fournis ; sortie `rules` pour `Check` = les 3 règles du §v0.5 avec leurs evidence |
| AC-0.7-04 | `explain` | test `mcp_explain_decide` | pour l'issue `example.Decide/sd1/o0` : statut `PROVEN_UNREACHABLE`, hypothèses du run, arrivées `UNSAT` avec **texte SMT-LIB**, verdict du vérificateur ; sans lire `decide.go` |
| AC-0.7-05 | `impact` conservateur | test `mcp_impact_helper` | modifier `helper` ⇒ résultat contient `Entry` (via graphe d'appels) et marque `incomplete:true` si des appels non résolus existent sur le chemin ; jamais de « pas d'impact » affirmé quand le call graph est incomplet |
| AC-0.7-06 | Context Builder | test `context_builder_budget` | avec `max_tokens=500` (estimation `ceil(caractères/4)`) : sortie ≤ 500 ; la sélection se fait **par liens structurels** (règle → cas → issues → preuves), puis troncature ; toute inconnue (`UNKNOWN`/`TIMEOUT`/`UNSUPPORTED`) pertinente est **incluse en priorité** ; `truncated` indiqué |
| AC-0.7-07 | LLM jamais requis | test `no_llm_default` | `ske behaviors reconstruct` fonctionne sans configuration LLM ; aucune requête réseau (test avec réseau bloqué) |
| AC-0.7-08 | LLM : provenance et séparation | test `llm_mock_provenance` (serveur factice compatible OpenAI) | lignes `functional_rules` `origin='LLM_INFERRED'`, `validation_status='UNVALIDATED'`, `prompt_version`, `llm_model`, `model_parameters` renseignés ; les règles déterministes sont **inchangées** (hash des lignes avant/après identique) ; `prop_no_llm_proof` |
| AC-0.7-09 | LLM : confidentialité | test `llm_no_source_upload` | sans `--allow-source-upload` (+ consentement enregistré), le corps de requête ne contient **aucun** extrait de fichier source (uniquement des faits structurés) |
| AC-0.7-10 | Benchmark | `tests/benchmarks/README.md` + `ske-bench` | protocole §17 exécuté sur `$LAB` avec/sans représentation structurée : coût d'indexation, coût par requête, qualité et rappel consignés |
| AC-0.7-11 | Verify et régression | `./scripts/verify.sh` ; tous les AC précédents | `VERIFY OK v0.7` |

---

# 20. Plan de travail attendu de l'agent exécutant

Pour **chaque jalon**, fournir dans cet ordre :

1. **Plan de modifications** : fichiers/crates à créer ou modifier, contrat public touché, risques.
2. **Implémentation complète** : pas de `TODO` sur un chemin nominal, pas de fausses données substituées à une vraie extraction.
3. **Tests** : unitaires, intégration, golden (écrits à la main), négatifs.
4. **Migrations** : testées, idempotentes ; jamais destructives en silence.
5. **Documentation** : `README.md` avec commandes **exactes**, architecture, ADR, limitations.
6. **Validation exécutée** : `docs/validation/v0.X.md` avec, pour **chaque** `AC` : la commande, la sortie résumée, `PASS`/`FAIL`/`WAIVED`.
7. **État de conformité** : tableau `DONE / PARTIAL / NOT_STARTED` dans `docs/implementation-status.md`, diagnostics ouverts, prochaines dépendances.

**Ne pas modifier une hypothèse structurante en silence.** Proposer un ADR (alternatives, risques, effet sur schéma/API, tests de non-régression).

**Gabarit `docs/validation/v0.X.md` :**
```markdown
# Validation v0.X — <date>
Environnement : OS, rustc, go, z3, sqlite (versions réelles)
| AC | Statut | Commande exécutée | Sortie (extrait) |
|---|---|---|---|
| AC-0.X-01 | PASS | `…` | `…` |
## Écarts et dérogations (WAIVED) — ADR liés
## Limites restantes
```

**Format du rapport d'étape (à la fin d'un jalon) :**
```markdown
## Version v0.X
### Livré
- ...
### Non couvert / approximations
- ...
### Tests exécutés
- `cargo test --workspace` : PASS/FAIL (détails)
- `go test ./...` : PASS/FAIL (détails)
### DDL / API modifiés
- ...
### Décisions d'architecture
- ADR-... : ...
### Prochaine étape
- ...
```

**Conduite en cas de difficulté.** (1) Relire la section normative concernée et son `AC`. (2) Si un `AC` semble contradictoire avec le texte : §0.3. (3) Si un outil externe se comporte autrement que décrit (ex. `go/ssa`) : consigner dans `docs/open-questions.md`, tester l'hypothèse minimale, ne **jamais** modifier une valeur attendue sans ADR. (4) Si bloqué : rapport `BLOCKED` (§0.6), **ne pas** déclarer le jalon terminé.

---

# 21. Points de vigilance et pièges à éviter

1. **Confondre présence de branche et chemin réalisable.** L'inventaire vient d'abord ; la faisabilité est relative aux hypothèses.
2. **Énumérer tous les chemins.** Potentiellement infini : garder un graphe, des contraintes et des résumés.
3. **Prendre `SAT` pour une preuve empirique.** Un modèle abstrait peut admettre des traces non exécutables.
4. **Prouver `UNREACHABLE` depuis un seul état éliminé.** Il faut une exploration **fermée** et un vérificateur indépendant (§6.6).
5. **Écraser les branches source par la SSA.** Décision source et branche IR ne sont pas en relation 1:1.
6. **Traiter un appel externe comme pur.** Il peut changer la mémoire, lever une exception, affecter le contrôle.
7. **Ignorer les modes d'arithmétique.** Largeur de type, débordement, division, flottants et `NaN` ont des conséquences réelles (§4.9).
8. **Oublier les sorties alternatives.** `panic`, `defer`, unwind, exceptions, retours multiples.
9. **Appeler « complète » une extraction partielle.** `COMPLETE` est vérifiable par unité/dimension (§3.5).
10. **Créer trois moteurs Go/Rust/Java.** Un seul moteur CIR ; des adaptateurs.
11. **Stocker tous les états en SQL.** Ne persister que faits, DAG réutilisables, résumés, arrivées, preuves, résultats.
12. **Produire des règles sans provenance.** Une règle sans evidence est une hypothèse.
13. **Confondre approximation et imprécision non signalée.** Toute surapproximation est traçable (`soundness`).
14. **Ignorer la configuration de compilation.** Tags Go, features Rust, classpath Java changent le programme.
15. **Supposer que l'analyse statique mesure la couverture des tests.**
16. **Mélanger snapshots.** Les FK ne suffisent pas : `ske db check` est obligatoire.
17. **Tests tautologiques.** Un golden produit par SKE n'est pas un test (§0.4).
18. **« Corriger » une valeur attendue pour faire passer un test.** Interdit sans ADR.
19. **Laisser le moteur dépendre du temps mur.** Utiliser `rlimit` et graine fixe (§6.4).
20. **Lancer `go build`/`go generate` depuis l'adaptateur.** Interdit (§9.1).
21. **Oublier qu'une constante n'est pas une instruction** (`CONST` n'existe pas) et que `PHI` n'a pas d'opérandes.
22. **Confondre `end_column` inclusif/exclusif.** Exclusif partout (§5.1).

---

# 22. Décisions encore ouvertes — à traiter sans bloquer le MVP

Les valeurs suivantes sont des **défauts recommandés** ; chaque choix concret fait l'objet d'un ADR.

| Question | Défaut initial | Quand décider |
|---|---|---|
| Génération protobuf Rust | `prost-build` avec `protoc` vendored (ou `protox`) | v0.1 (ADR-019) |
| Algorithme d'appel Go | statique + CHA, soundness explicitée | v0.2 |
| Bibliothèque SMT Rust | crate `z3` ; procédure de build documentée | v0.3 (spike au démarrage de v0.3) |
| Modèle d'aliasing | M1 conservateur de §6.9 | v0.3 |
| Abstractions de boucle | intervalles + élargissement (§v0.4) | v0.4 |
| Stratégie de résumé contextuel | cache à clé abstraite stable (§7.1) | v0.4 |
| Toolchain MIR | toolchain Rust épinglée et testée | v0.6 |
| Parser source Java | complémentaire facultatif | v0.6 |
| Framework HTTP/MCP | léger, exposant `ske-query` | v0.7 |
| LLM / embeddings | facultatif, configurable, aucune donnée envoyée sans accord | v0.7 |
| Licence du logiciel | **non choisie** (le commanditaire décide) ; audit des licences des dépendances en ADR | avant toute publication |

---

# 23. Références techniques (à revérifier à l'implémentation)

Les références documentent des outils et des concepts, pas une garantie de compatibilité d'une version future ; re-vérifier les API et versions à l'implémentation.

- Go SSA : https://pkg.go.dev/golang.org/x/tools/go/ssa
- Go package loading : https://pkg.go.dev/golang.org/x/tools/go/packages
- Go call graph : https://pkg.go.dev/golang.org/x/tools/go/callgraph
- Rust MIR : https://rustc-dev-guide.rust-lang.org/mir/index.html
- `rustc_interface` / instabilité des API internes : https://rustc-dev-guide.rust-lang.org/rustc-driver/intro.html
- SootUp : https://soot-oss.github.io/SootUp/latest/callgraphs/ et https://soot-oss.github.io/SootUp/latest/concepts/
- SQLite : https://www.sqlite.org/foreignkeys.html, https://www.sqlite.org/lang_with.html, https://www.sqlite.org/wal.html, https://www.sqlite.org/lang_createtrigger.html
- Z3 : https://github.com/Z3Prover/z3 ; SMT-LIB : https://smt-lib.org/
- Joern / Code Property Graph : https://docs.joern.io/code-property-graph/
- Dominance (Cooper, Harvey, Kennedy, « A Simple, Fast Dominance Algorithm ») ; Interprétation abstraite (Cousot & Cousot) pour widening/narrowing.

---

# 24. Première instruction à envoyer à l'IA de développement

> **Implémente SKE en respectant strictement ce cahier des charges (v1.4). Lis d'abord §0, §3, §4, §5 et §8, puis exécute le jalon v0.1 tâche par tâche (T-0.1-01 à T-0.1-10). Crée le monorepo Rust (arborescence §2.1), le contrat CIR v1 (§5), la migration `0001_initial.sql` copiée telle quelle (§8.2), l'import transactionnel des fixtures CIR (§15.2), les validations d'invariants (§4.8), les commandes CLI de consultation (§11) et les tests. Fournis du code compilable et des commandes reproductibles. Ne simule pas les résultats d'analyse ; n'écris jamais un golden à partir de la sortie du programme. Un critère n'est `PASS` qu'après exécution de sa commande avec le résultat attendu, consigné dans `docs/validation/v0.1.md`. N'attaque ni Rust/Java ni la couche LLM. Après v0.1 validé, poursuis v0.2 (Go) puis v0.3 (analyse symbolique) dans cet ordre. Avant toute conclusion d'inaccessibilité, respecte les distinctions `POSSIBLE`, `WITNESSED`, `PROVEN_UNREACHABLE`, `UNKNOWN` et la règle de fermeture globale avec vérificateur de preuve. Si une limitation apparaît, rends-la explicite et persistée plutôt que de masquer l'incertitude. En cas de contradiction ou de blocage, applique §0.3 et §0.6.**

**Résultat cible :** un outil local, reproductible, extensible et auditable, capable de reconstruire progressivement la connaissance fonctionnelle d'un codebase tout en conservant chaque issue de branche identifiable et les limites exactes de son analyse.

---

# Annexe A — Liste de contrôle de fin de jalon (à copier dans `docs/validation/v0.X.md`)

- [ ] Tous les `AC-0.X-*` du jalon sont `PASS` avec commande et sortie consignées
- [ ] Tous les `AC` des jalons précédents sont toujours `PASS` (`./scripts/verify.sh`)
- [ ] Aucun test `ignored`/`skipped` hors liste « non bloquant »
- [ ] Aucun golden généré par SKE ; aucun `todo!()` sur chemin nominal
- [ ] `ske db check` : 0 violation `ERROR` sur toutes les bases produites par les tests
- [ ] Aucun `UNKNOWN`/`TIMEOUT`/`SAT` converti en `PROVEN_UNREACHABLE`/`WITNESSED` (tests `prop_*`)
- [ ] `docs/implementation-status.md`, `docs/soundness-and-limits.md`, `docs/open-questions.md` à jour
- [ ] ADR ajoutés pour toute décision structurante ou dérogation
- [ ] Rapport d'étape (§20) rédigé

# Annexe B — Correspondance rapide statuts ↔ sources de vérité

| Statut | Défini en | Produit par | Garanti par |
|---|---|---|---|
| `inventory_status` | §3.5 | cœur (recalculé) | `ske db check` CHK-13/14/20 |
| `run_status` | §6.7 | moteur | tests §15.3 |
| `reachability_status` | §3.2, §6.6 | classification §6.6 | triggers SQL, vérificateur §6.6, CHK-15 |
| `soundness` | §3.1 | moteur (monotone) | `prop_unsupported_never_exact` |
| `origin` / `validation_status` | §3.4 | v0.5 / humain | `prop_no_llm_proof` |

# Annexe C — Contrôle qualité de la spécification v1.4 (15 octobre 2026)

Cette annexe documente **ce qui a réellement été vérifié sur les artefacts présents dans ce Markdown** lors de la relecture v1.4. Elle **ne constitue pas** une attestation d'exécution du logiciel SKE, qui n'existe pas encore.

| Contrôle documentaire/exécutable (relecture v1.4) | Résultat constaté | Portée exacte |
|---|---|---|
| `cir.proto` et `adapter.proto` extraits du document et compilés par `protoc` (libprotoc 35.1, `-Ischemas`) | **PASS** | Syntaxe Protobuf ; contrat inchangé par rapport à la v1.3 (seul un commentaire diffère) ; pas de test des bindings Go/Rust |
| `migrations/0001_initial.sql` exécutée dans SQLite 3.37 avec `PRAGMA foreign_keys=ON` | **PASS** : 46 tables (identiques à la liste normative §8.2) et **21** triggers | Création du schéma |
| `0002_capabilities.sql` appliquée après `0001` | **PASS** : 49 tables (50 avec `schema_migrations`) | Création des tables additionnelles |
| Requêtes CHK-01 à CHK-22 et exemple §8.6 exécutés sur le schéma | **PASS** (exécution sans erreur) | Faux positifs/négatifs à tester en v0.1 |
| Scénario de preuve : acceptation refusée avant `COMPLETED` ; après acceptation, `status` → `CANCELLED`/`FAILED` refusé (nouveau trigger), `cut_count` figé, `proof_records` immuable, `metrics_json`/`finished_at` modifiables, nouveau run insérable | **PASS** | Scénarios ciblés ; ne prouve pas l'absence de toute séquence SQL anormale |
| Spans de référence (`decide.go`, `check.go`, `adjusted.go`, `memory.go`, `switch_fixture.go`) recalculés depuis les blocs Go du document | **PASS** | Colonnes en octets, fin exclusive |
| Compteurs §15.2.2 recalculés à la main depuis les descriptions §15.2.1 (7 fixtures) | **PASS** après correction V14-03 | Cohérence documentaire ; la génération réelle reste à tester (AC-0.1-17/19) |
| Blocs JSON (7) valides ; 148 AC uniques, toutes les références `AC-*` et `§` résolues ; blocs de code équilibrés ; codes de diagnostic utilisés présents au registre §5.6 | **PASS** | Cohérence éditoriale |
| Laboratoire Go : **non recompilé** pendant cette relecture (aucune toolchain Go disponible) | Non exécuté | La v1.3 rapportait `go build`/`go vet` **PASS** avec Go 1.23.2 ; la v1.4 ne modifie aucun fichier `.go` |

**Hypothèses sur `go/ssa` introduites ou rendues explicites en v1.4** (à confirmer sur la toolchain épinglée, selon la procédure du §15.1.5 en cas d'écart) : `ssa.GlobalDebug` émet un `ssa.DebugRef` pour chaque expression non constante évaluée par le builder (donc pour chaque atome de condition) sans modifier le reste de la SSA ; `Parameter.Pos()` désigne la déclaration ; un `switch` taggé est abaissé en tests `emitCompare` positionnés sur l'expression du `case` ; une condition constante conserve son `ssa.If` ; `<package>.init` contient un test `init$guard` hors mode `BareInits`.

**Contrôles non effectués à ce stade :** compilation des futurs crates Rust, implémentation et validation Z3, génération croisée Go↔Rust des messages, adaptateurs Rust/Java, vérification exhaustive des attentes `go/ssa` sur la toolchain épinglée, validation sémantique complète des 148 AC, performance sur de grands dépôts. Tout écart observé lors de l'implémentation DOIT être documenté suivant §0.3 et §0.6 ; **aucun résultat attendu ne doit être modifié pour masquer un échec**. Les tests du jalon font seuls foi pour autoriser son passage au jalon suivant.
