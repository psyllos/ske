# Soundness et limites

## Ce que v0.1 permet

Importer et consulter des inventaires structurels et leur provenance est
validé en v0.1 (42 AC, rapport séparé). Une base
structurellement valide ne démontre pas la sémantique du programme source.
v0.2 est validée (25 AC), v0.3 (29 AC) et v0.4 (14 AC) également. La conformité structurelle Go
ne doit pas être présentée comme une connaissance comportementale complète.
En particulier, les nil Go hors du contrat littéral CIR sont explicitement
opaques et ne permettent aucune déduction précise de nullité (ADR-020).

## Axes distincts

| Axe | Valeurs |
|---|---|
| Inventaire | COMPLETE, PARTIAL, FAILED |
| Exécution d'analyse | PENDING, RUNNING, COMPLETED, PARTIAL, FAILED, CANCELLED |
| Accessibilité | UNANALYZED, POSSIBLE, WITNESSED, PROVEN_UNREACHABLE, UNKNOWN, TIMEOUT, UNSUPPORTED |
| Soundness | EXACT, OVERAPPROX, OPAQUE, UNSUPPORTED |

La soundness ne peut que se dégrader dans l'ordre du tableau. `EXACT` signifie
exact sous hypothèses, pas vérité absolue sur le logiciel. `COMPLETE` concerne
l'inventaire dans le périmètre déclaré, jamais tous les chemins concrets.
Le cœur recalcule les compteurs et peut dégrader COMPLETE en PARTIAL ; il ne
promeut pas un PARTIAL déclaré par l'adaptateur. Un snapshot sans unité est
FAILED ; sinon l'agrégation respecte §3.5.

## Aucune preuve simulée

En v0.1 les issues sont UNANALYZED et `analysis` vaut null.
`SAT` ne produit pas WITNESSED ; WITNESSED n'a aucun producteur en v0.x.
UNKNOWN et TIMEOUT ne sont jamais convertis en inaccessibilité.
Une requête UNSAT isolée ne suffit pas à PROVEN_UNREACHABLE.

Le mécanisme v0.3 impose fermeture globale, absence de coupes,
preuves acceptées, vérification indépendante de la classification et protection
SQL. Son niveau maximal est MODEL_UNREACHABLE, sous hypothèses du run, pas une
preuve absolue sur les sources. SOLVER_REPLAYED ne certifie pas la couverture ;
CERTIFIED_UNREACHABLE est interdit en v0.x.

## Hypothèses et confiance

Tout run doit persister au minimum SEQUENTIAL_EXECUTION et
CLOSED_ENTRY_CONTEXT avant COMPLETED. L'état global initial est libre.
Les hypothèses supplémentaires appartiennent au registre §3.3 ; les garanties
de mapping source et d'effets adaptateur doivent être explicites.
Un inventaire sans cas d'activation complets reste consultable mais ne justifie
aucune preuve source. Ni concurrence, ni comportement opaque, ni effets inconnus
ne peuvent être éliminés pour obtenir artificiellement une preuve.

## Approximations et frontières v0.3

- **Mémoire M0/M1** : pas de modèle de heap complet. Les allocations dynamiques
  identifiées sont distinctes, y compris au même site ; paramètres/globaux
  peuvent aliaser. Les écritures ambiguës sont faibles, ordonnées dans le
  journal ; aucune mise à jour forte sur un résumé multi-instance. La perte
  d'identité ou une cible inconnue provoque un havoc conservateur.
- **CALL havocké** : aucun résumé ni exploration interprocédurale. Résultat
  frais et mémoire invalidée lorsque les effets ne sont pas prouvés absents.
  Un appel pur n'est pas automatiquement référentiellement transparent :
  lire un état mutable interdit le partage déterministe de résultat.
- **Panics d'appel non explorés** : hypothèse explicite
  `CALLEE_PANIC_EXITS_NOT_EXPLORED`. Les sorties anormales quittent la fonction ;
  elles ne constituent pas des issues IR inventées. Les forks DIV/REM par zéro
  et décalage signé négatif sont, eux, vérifiés par SMT.
- **Boucles bornées** : comptage des arêtes de retour DFS ; une coupe rend
  l'exploration globale non fermée. Aucun `PROVEN_UNREACHABLE` dans un tel run,
  même pour une issue dont les arrivées rencontrées sont toutes UNSAT.
  Aucun point fixe, widening, résumé récursif ou invariant inductif en v0.3.
- **Flottants, chaînes, tableaux et opérations non encodées** : sortes opaques
  ou valeurs HAVOC, avec perte de précision ; pas de raisonnement arithmétique
  ou lexical exact. Seules les opérations documentées BOOL/BITVECTOR et
  l'égalité sur sortes non interprétées ont un encodage SMT.
- **Effets opaques/concurrence** : `RECOVER`, defer, SPAWN, canaux et effets
  inconnus ne sont pas explorés exhaustivement ; havoc mémoire et soundness
  OPAQUE sous `SEQUENTIAL_EXECUTION`, pas de garantie sur le programme concurrent.
- **Budgets et solveur** : rlimit/graine déterministes ; timeout mural de
  sécurité non reproductible. Unknown/Timeout restent visibles, l'exploration
  continue conservativement mais devient PARTIAL. L'estimation mémoire n'est
  pas un hard cap OS. Les requêtes identiques réutilisent leur preuve de requête
  dans le run sans nouvel appel ni nouvelle ligne SQL.
- **Activation source** : une preuve IR isolée ne devient pas une preuve source
  par agrégation de statuts. Les cas d'activation ordonnés doivent être complets
  et exacts ; POSSIBLE requiert un chemin conjoint SAT. La dérivation reste
  conservatrice (UNKNOWN) si des contradictions conjointes ne sont pas
  justifiées par les preuves persistées. `elided` reste UNANALYZED.
- **Rejeu de preuve** : le vérificateur SQL indépendant relit fermeture,
  arrivées, hypothèses et toutes les requêtes UNSAT du run, puis utilise Z3
  neuf à graine 1. Cela vérifie la cohérence dans le modèle, non la traduction
  source→CIR. Les affirmations de l'adaptateur restent des hypothèses de confiance.
- **Annulation** : entre instructions, ou après le délai du solveur en cours ;
  les arrivées sont écrites au fil de l'eau. Après finalisation COMPLETED,
  le run ne change plus de statut ; les candidats non vérifiés deviennent
  UNKNOWN si une annulation interrompt la vérification.

Les comportements/réutilisation v0.5 livrés ci-dessous le sont dans leur
périmètre borné ; les frontières v0.6 (Rust/Java) et v0.7 (API/MCP/LLM) sont
détaillées dans leurs sections respectives ci-après.
Aucun code étudié arbitraire n'est exécuté ; seul le runner concret de
confiance du dépôt est autorisé par les tests différentiels. Les commandes
réellement exécutées sont dans les rapports de validation ; les obligations
décrites dans un ADR ne sont pas des PASS.

## Périmètre et frontières v0.4

- **Résumés** : calcul avec paramètres symboliques, un cas par sortie,
  import mémoïsé du DAG dans l'arène appelante, gardes filtrées par SMT,
  retours multiples et provenance IR conservés. Les globales initiales sont
  libres ; les lectures et écritures globales connues sont composées avec
  l'état mémoire du site d'appel (y compris entre deux appels successifs).
  L'API applicative est `Query::summarize_scalar`, pour le contexte par
  défaut ; elle persiste les résumés dans une transaction SQLite.
- **Récursion** : deux dépliages par défaut ; le reste de l'activation devient
  un retour havoc, avec cas `PARTIAL` et `RECURSION_BOUND`. Ce n'est pas un
  point fixe récursif exact. Une limite de profondeur de composition empêche
  une récursion hôte non bornée ; son dépassement échoue explicitement.
- **Effets et exceptions** : seuls les résumés composables à sorties `RETURN`
  sont substitués précisément. Appels ouverts, effets mémoire ambigus,
  concurrence et sorties exceptionnelles non composées gardent le traitement
  havoc/OPAQUE et les hypothèses v0.3. Les effets inconnus sont persistés
  comme `UNKNOWN_EFFECT`, jamais omis pour fabriquer un résumé EXACT.
  Le mode `branches analyze` par défaut conserve la sémantique locale v0.3.
- **Intervalles** : activés par `loop_mode="intervals"`. Les équations SSA
  joignent tous les prédécesseurs, élargissent puis rétrécissent une fois.
  L'absence de raffinement systématique par les gardes peut donner la plage
  complète du type ; tout débordement possible donne cette plage complète.
  La moitié haute des uint128 est arrondie à top (pas tronquée à i128::MAX).
  L'état générique des PHI est exploré une fois, avec mémoire havockée,
  soundness OVERAPPROX, hypothèse et diagnostic explicites. Les boucles avec
  allocations/adresses dynamiques restent dans le traitement borné avec coupe :
  elles ne sont pas annoncées fermées par une abstraction de heap inexistante.
- **Concret** : interprétation CIR locale bornée en instructions et récursion,
  sans exécution de stub hôte (`errors.New`, par exemple). Paramètres,
  globales injectées, appels statiques, PHI et retours multiples sont testés.
  Les opérations non prises en charge retournent une erreur explicite.
- **Cache et persistance** : empreinte calculée sur structure, profil
  sémantique, contexte, dépendances triées, modèles et configuration.
  Déduplication dans un snapshot ; pas de réutilisation incrémentale entre
  snapshots annoncée. Une composition inter-unités avec des types/globaux
  incompatibles échoue plutôt que de les assimiler silencieusement.
- **Chemins** : graphe statique, identités SQLite distinctes entre unités,
  chemins simples et budgets explicites. `NO_PATH_FOUND` ne signifie jamais
  « prouvé inaccessible » ; `incomplete` conserve les limites de résolution.

Validation : `python3 scripts/validate-v04.py` exécute les 14 AC, incluant
`summary_matches_concrete` et 200 exécutions CIR concrètes de Sum contre les
intervalles. Les 96 AC précédents sont rejoués. CI distante non exécutée.

## v0.5 — connaissance comportementale (validée)

v0.5 est validée : 13 AC PASS sur `python3 scripts/validate-v05.py`
(`SKE_EXPECT_VERIFY_VERSION=v0.5`), avec non-régression des 110 AC antérieurs
(42 v0.1 + 25 v0.2 + 29 v0.3 + 14 v0.4) rejoués par AC-0.5-12.
Les logs et résultats sont conservés dans `target/validation-v0*/`.

Périmètre livré (spéc. §7.2, §7.5, §8.4, §11.2, §13) :

- **Règles R1–R6** : gardes rendues en conjonction ordonnée par chemin ir,
  sans simplification algébrique et sans assimilation par texte de contraintes
  distinctes (une répétition reste affichée) ; négations poussées, constantes
  décimales signées, paramètres/globales par nom, R6 parenthésé. Garde vide
  rendue `true`. La double garde DIV implicite n'est pas dupliquée dans l'arène
  (dédoublonnage structurel `same_boolean_predicate`), ce qui évite d'inventer
  une contrainte tout en préservant R1 du côté rendu.
- **Déduplication** : clé structurelle (exit_kind, résultat expr, mémoire,
  effets), garde = disjonction des gardes des cas fusionnés, précision = la
  plus faible, provenance de tous les cas fusionnés conservée
  (`summary_case_ids`, evidence DERIVED_FROM/RESULT_SITE par cas). RETURN et
  PANIC ne fusionnent jamais.
- **Ordre des cas** : clé normative `(span, chemin ir numérique
  (décision_ordinal, outcome_ordinal), fingerprint)` ; le fingerprint du cas
  est structurel (gardes, retours, mémoire, effets, chemin ir), indépendant de
  l'ordre d'exploration.
- **Persistance** : behavior_scenarios/cases/outcomes, functional_rules,
  knowledge_evidence, functional_capabilities, capability_scenarios,
  scenario_links (frontière EXTERNAL_UNKNOWN pour les effets inconnus).
  Reconstruction transactionnelle : tous les cas persistés avant pagination,
  rollback en cas d'échec.
- **CLI** : `ske behaviors reconstruct` (JSON/YAML, pagination), `ske rules
  list --with-evidence`, `ske rules stale-check` (marque STALE sur disparition
  ou changement d'empreinte structurelle §4.6, jamais ré-attribution).
- **Réutilisation des résumés (§13)** : copie-sur-réutilisation entre
  snapshots du même dépôt, clé = contexte + empreinte structurelle §4.6
  (canonique, indépendante des ids de flux, des spans et des types
  inutilisés) + empreinte sémantique + dépendances triées + modèles +
  configuration (snapshot, budget, contexte, namespace moteur
  `ske-summary-scalar-0.5-cache-v2`). Lignes `analysis_dependencies`
  (FUNCTION/SUMMARY) inscrites ; metrics summaries_reused/computed et
  reused_from_summary_id ; choix de précision conservé. Une réhydratation
  incomplète est recalculée via savepoint (jamais de moitié copiée).

Limites v0.5 (bornes assumées, non des défauts) :

- **Réutilisation** : seulement pour des configurations de snapshot identiques
  (build tags/goos/goarch compris, testé) et le mode moteur v0.5 ; un corps
  différent sous la même config invalide le fingerprint (testé). Cas partiels
  (`PARTIAL`, effets non modélisés) non réutilisés comme résumés complets.
  Pas de graphe de dépendances multi-snapshot exploité au-delà de
  `analysis_dependencies` (informatif, non cyclé).
- **Scénario/capacité §7.5** : un scénario est un groupement technique par
  snapshot+fonction ; l'origine est `DETERMINISTIC` et la validation
  `UNVALIDATED` (aucune validation humaine ou LLM). Une frontière
  `EXTERNAL_UNKNOWN` est déclarée quand des UNKNOWN_EFFECT existent ; elle ne
  prétend pas fermer le comportement.
- **Règles fonctionnelles** : dérivées de résumés ; `validation_status` reste
  `UNVALIDATED` sauf marquage `STALE` après `stale-check`. Le stale-check ne
  déduit rien sur les fonctions disparues (il les marque STALE, il ne les
  ré-attribue pas).
- **Evidence** : ancrage RESULT_SITE aux spans de sortie complets émis par
  l'adaptateur Go de confiance ; l'adaptateur est le seul producteur de
  spans. Aucun texte « métier » n'est inventé (`behavior_no_business_text`).

## v0.6 — frontières par langage (validée)

### Go

Contrat v0.2 et limites de précision v0.3–v0.5 conservés. La mutualisation
du lanceur pour Rust/Java ne transforme pas les attestations Go et ne
supprime aucune inconnue, hypothèse ou coupe.

### Rust

Pilote MIR réel isolé, pas de types rustc dans le cœur. Construction de
l'adaptateur en 1.99.0 ; producteur MIR nightly contrôlé séparément par
identité complète. L'alias daté disponible localement ne correspond pas
au hash attendu ; la reproductibilité sur une machine neuve n'est donc
pas encore démontrée (ADR-022-rust).

Les IF atomiques dont les opérandes/polarités correspondent réellement au
MIR ont une activation exacte. `match` et `?` sont inventoriés à la source,
mais leurs activations restent incomplètes dans le pilote actuel. Aucune
preuve source ne peut être fabriquée à partir de ces mappings incomplets.
Les unités restent PARTIAL. L'inventaire complet des panics implicites,
Drop/unwind/abort, macros, async/closures et dépendances Cargo est encore
à compléter ou explicitement rejeté ; ce pilote ne clôt pas §10.1.
Le chargement ne lance ni Cargo ni build.rs ni proc macros tiers.

### Java

Pilote SootUp/Jimple intégré, bytecode cible 17, runtime 24.0.1 (voir
`docs/validation/v0.6.md` pour les AC rejoués). Lecture de classes et sources,
pas de lancement du build étudié.
La réflexion et les cibles dynamiques restent ouvertes (`world_closed=false`) ;
les mappings source inexacts et gardes d'exceptions non modélisées restent
explicites. La relecture Java est validée dans son périmètre borné, avec les
mêmes inconnues conservatrices que le reste du moteur.

## v0.7 — API de connaissance, MCP et LLM optionnel (validée)

### Couche API/MCP

- `ske-api` ne dépend jamais directement de `rusqlite` (contrôle AC-0.7-01 via
  `cargo tree`) ; toute lecture passe par les DTO de `ske-query`. La DTO indépendante
  de SQLite est testée par les tests `dto_*` (pagination, troncature, clés requises).
- Le transport MCP est **stdlib JSON-RPC 2.0** (aucun service distant) : les cinq
  outils (`inspect`, `branches`, `explain`, `rules`, `impact`) sont testés en
  process et via `ske-mcp-server`. Une troncature de `paths explore`/`explain`
  n'est jamais présentée comme complète (`truncated`/`truncation_reason` /
  `visited_count`/`frontier_count`).
- `explain` ne relit jamais le fichier source ; il rend le statut, les hypothèses
  du run, les arrivées avec le texte SMT-LIB persisté et le verdict du
  vérificateur. `impact` est conservateur : il marque `incomplete:true` dès
  qu'un chemin traverse un appel non résolu/externe/dynamique et ne conclut
  jamais « pas d'impact » sur un graphe d'appels non clos.

### LLM (optionnel)

- Aucun token dépensé dans la validation : le transport injecté (`LlmTransport`)
  est scripté ; le cas testé « serveur compatible OpenAI » est un mock local.
  Sans configuration LLM (`enabled=false`), aucun process/transport n'est appelé
  (AC-0.7-07, réseau bloqué).
- Les règles `LLM_INFERRED` sont persistées avec `prompt_version`, `llm_model`,
  `model_parameters` et `validation_status='UNVALIDATED'` ; les règles
  déterministes sont inchangées (comparaison avant/après). `prop_no_llm_proof` :
  une règle LLM n'est jamais référencée comme preuve (pas de `behavior_case_id`,
  donc hors de toute dérivation de preuve).
- Confidentialité : sans `--allow-source-upload` **et** consentement enregistré,
  le corps de requête ne contient que des faits structurés (AC-0.7-09).

### Limites restantes

- L'enrichissement LLM réel (transport HTTPS OpenAI) n'est pas fourni dans le
  cœur verrouillé : `LlmTransport` est l'extension documentée. Aucune règle
  produit par un LLM n'est utilisée comme preuve d'inaccessibilité.
- Le Context Builder est une borne d'estimation (`ceil(caractères/4)`), pas une
  tokenisation d'un modèle précis.
- Le benchmark `ske-bench` (AC-0.7-10) a été exécuté réellement (deux bras,
  déterminisme ×2, intégrité) et la campagne complète v0.7 validée librement
  rejouée ; ces résultats et les limites de mesure sont documentés dans
  `docs/validation/v0.7.md`.
