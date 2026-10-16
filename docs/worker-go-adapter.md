# Worker Go adapter — checkpoint v0.2 (en cours)

Ownership : `adapters/go/**`, `scripts/gen-proto-go.sh`. Aucune modification du
laboratoire, des schémas, des golden manuels ou du workspace Rust.

## Contrats

- Binaire : `target/ske-adapter-go`, compilé depuis `adapters/go` avec
  `go build -o ../../target/ske-adapter-go ./cmd/ske-adapter-go`.
- Un seul `AdapterRequest` protobuf dans une frame u32BE ; réponses
  `AdapterResponse` framées. Protocole majeur 1. Pas de texte sur stdout.
- Unités : `go:<import_path>@<goos>/<goarch>` ; variantes test
  `go:<import_path>[test]@<goos>/<goarch>`.
- Protobuf généré committé sous `adapters/go/gen`, import
  `ske/adapters/go/gen/{cir,adapter}/v1`. `M` mappings protoc ; aucun `go_package`
  ajouté aux schémas normatifs.
- SSA réel : `ssautil.AllPackages`, `InstantiateGenerics|GlobalDebug`. Compteur
  AST séparé de l'émetteur ; même inventaire des fonctions réellement émises.
- Sources hashées SHA256 exactes, chemins relatifs ; empreinte build SHA256 JSON
  trié (profil, version, racine et modules hashés ; work hashé si configuré).
- Environnement go/packages fermé ; aucune export data, aucun build du projet.
  Typage source explicite des `GoFiles` si `go/packages` ne remplit pas la syntaxe.

## Versions réellement résolues

- Go système : `go1.27.1 darwin/arm64`.
- `golang.org/x/tools v0.52.0`, `x/mod v0.42.0`, `x/sync v0.24.0`.
- `google.golang.org/protobuf v1.36.11` (runtime et `protoc-gen-go`).
- protoc système : `libprotoc 36.2`.
- Outil installé uniquement dans `target/tools`.

## Vérifications déjà exécutées

- `go test ./internal/...` : compteur/émetteur et chargement sécurisé passent.
- Vérification finale exécutée, exit 0 :
  `(cd adapters/go && go vet ./... && test -z "$(gofmt -l .)" && go test ./... && go build -o ../../target/ske-adapter-go ./cmd/ske-adapter-go)`.
  Frame/profil/calls/lower inclus ; `internal/link` n'a pas encore de tests propres.
- `go test ./internal/emit -run TestExtractLab -v` : vraies extractions répétées,
  flux byte-identiques, compteurs 50/50 racine, 1/1 partial, 0/0 tags.
  Le test vérifie une extraction réelle, **pas** la conformité de toutes les
  propriétés/golden ni la validité des flux selon le validateur Rust.
- Dernière observation : avis `COMPLETE`, `PARTIAL`, `COMPLETE` pour racine,
  partial, tags ; 119, 6, 3 arêtes IR respectivement. Ces nombres ne remplacent pas
  les attendus normatifs par fonction. Log local : `target/go-checkpoint-extract.log`.

## Hashes du checkpoint

SHA256 :

| Fichier | Hash |
|---|---|
| `adapters/go/go.mod` | `f9fd56c71f9d6b67704b33af977cfce1bc4672e161fc1a0c4495538eb3651f38` |
| `adapters/go/go.sum` | `40d85fecf6a1d8c37e2f850b002e2dbe5213439032a3b24a64c90070d20afd06` |
| `schemas/cir/v1/cir.proto` (inchangé) | `8da4f809c020ae59224eda0de18b884f4d602981cc3251d979cbc2b78979628e` |
| `schemas/adapter/v1/adapter.proto` (inchangé) | `1953b1470c47cfc9adda3e0f00a7f0ed2535fc61e1e4ae78dcecc139b805d460` |
| `target/ske-adapter-go` (local, non committé) | `f754c6be989c5819ae20132b27db6c623d4a91ce74b1e984b1963d3c38c18067` |

Dernière source worker lower intégrée :
`9aebdf45-8712-4697-a0b2-b8bf2bca1eb3.371018`.

## Limites à fermer avant validation finale

- SELECT/RANGE : mappings structurels à compléter ; incomplets explicitement
  diagnostiqués, jamais qualifiés EXACT par simple rang lexical.
- CHA dynamique : signatures resserrées chez worker lower ; vérification
  intégrée des cibles de `MemOpaque` et des wrappers `Total` encore nécessaire.
- Statuts inventaire vs effets/concurrence : seule une dégradation INVENTORY doit
  abaisser l'avis d'inventaire.
- Tests frame/profil passent ; golden manuels, propriétés complètes et validation
  Rust des flux émis restent à exécuter. Les fixtures CIR positives sont décodées/
  réencodées en Go avec égalité sémantique ; pas de test croisé Rust supplémentaire.
- Divergence de contrat à arbitrer : `nil` SSA de SLICE/FUNCTION/map/chan demande
  `null_value` selon §9.4 ; le validateur Rust existant n'accepte que
  POINTER/REFERENCE/INTERFACE. Aucune fausse conversion en pointeur n'est faite.
- Les attestations de complétude d'activation ne sont pas encore validées sur
  toutes les expressions/négations/instances ; elles doivent être auditées avant
  toute preuve source. Aucun moteur de preuve n'est lancé dans ce checkpoint.
- Fichiers exclus par build tags et cgo : diagnostics à compléter.
- `include_tests` et `go_work` : support profil présent, tests bout-en-bout restants.
- Fingerprint : inventaire complet des contraintes/paths compilés à consolider.
- Les limites OS mémoire/temps sont aussi assurées par le lanceur Rust ; la
  construction SSA n'est pas actuellement interruptible en interne.

Aucun AC v0.2 global n'est déclaré PASS ici.
Arrêt au checkpoint à la demande du parent ; ne pas démarrer v0.3.

## Reprise après `9cc8436` — SSA/CIR importable et conformité Go

Le contenu ci-dessus décrit le checkpoint interrompu. Cette reprise ferme les
échecs observés sur le laboratoire, sans modifier les golden, le laboratoire,
les schémas ou la migration. v0.1 n'a pas été réimplémentée ; v0.3 reste interdite.
Le correctif de tri des unités et `sha256_reader` ont été **importés du parent**,
pas réécrits dans ce lot. `frame_test.go`, scripts et IPC restent au parent.

### Règles corrigées

- `nil` conserve `null_value=true` et son vrai type `SLICE`, `FUNCTION` ou
  `OPAQUE` map/chan. Les maps/chans nommés gardent leur ID nominal ; leur
  `canonical_descriptor` décrit le type Go sous-jacent (`go:map[...]...`,
  `go:chan ...`, `go:chan<- ...`, `go:<-chan ...`). Le validateur Rust admet
  ces seules familles opaques, pas tous les `OPAQUE`. `nullable` reste réservé
  à POINTER/REFERENCE selon §4.2. Aucune conversion en pointeur, aucun schéma
  modifié. Régressions positives/négatives Go et Rust ajoutées.
- Les IDs des signatures de méthodes incluent le récepteur : `types.TypeString`
  seul l'omet, alors que les arguments CIR le passent explicitement. Les appels
  gardent le callable puis les arguments ordonnés ; la pile runtime supplémentaire
  de `ssa.Defer` n'est plus un argument source. Les tuples de retours et les
  paramètres d'environnement des closures restent conservés et testés.
- CHA reste ouvert, avec cibles signature-compatibles. Plusieurs objets SSA de
  wrapper peuvent désigner le même symbole : une déclaration CIR par identité
  qualifiée, une cible par `(instruction, fonction)` après canonicalisation.
  Cela corrige réellement les erreurs `E_DUPLICATE_ID` puis UNIQUE call_targets
  observées après le tri parent ; aucune erreur d'import n'est tolérée.
- `ssa.Function.Recover` est une entrée implicite sans prédécesseur normal.
  Ses dépendances SSA cross-block sont matérialisées par des valeurs locales
  `OPAQUE_OP` (« recover environment »), avec diagnostic INFO. Le bloc et ses
  instructions restent présents ; aucune arête d'exception fictive ni relaxation
  de dominance Rust. Le recover lui-même reste WARNING/INVENTORY dans la closure.
- `SELECT` : reconnaissance du `ssa.Select` au token exact, extraction de l'index,
  comparaisons avec les ordinals de communication et vérification de la chaîne
  FALSE. Chaque cas conserve les prérequis ordonnés ; DEFAULT utilise les FALSE
  précédents. Aucun appariement lexical présenté comme EXACT.
- `RANGE_ITERATION` : reconnaissance du flux longueur/incrément/phi ou
  Range/Next, relié au `DebugRef` de l'expression range. Un `if` dans le corps
  n'est plus pris pour le header. Les formes ambiguës restent incomplètes avec
  `SOURCE_ACTIVATION_INCOMPLETE`, jamais des liens inventés.
- Instances génériques : leur `Origin().Pkg` identifie l'appartenance source,
  même si `Pkg` est nil. Inventaire racine corrigé de 50 à **52** décisions,
  contre le compteur/golden indépendant inchangé. `GENERIC_ORIGIN_OPAQUE`
  concerne l'origine, pas ses instances.
- Spans d'opérations : expression AST contenante, non vide ; provenance de
  déclaration pour les wrappers synthétiques sans token d'instruction.
  Un IndexAddr réussi n'est pas nil (son risque d'index reste inventorié).
  Diagnostics effets/concurrence dédupliqués par fonction/code ; opérations
  opaques toujours par fonction/construction. Fichiers exclus émis depuis
  `packages.Package.IgnoredFiles`, sans ajout aux sources de l'unité.

### Commandes finales exécutées — toutes exit 0

```sh
(cd adapters/go && go vet ./... && test -z "$(gofmt -l .)" && go test ./... && go build -o ../../target/ske-adapter-go ./cmd/ske-adapter-go)
cargo test -p ske-cir --locked
cargo test -p ske-cli --test go_golden_decisions --test go_props --test go_diagnostics --test go_vs_cir --test go_build_tags --test go_counters --test go_deterministic --test go_activation_cases --test go_implicit_exit_inventory --no-fail-fast
target/debug/ske ingest go --db target/go-worker-final.sqlite --repo examples/go-branch-lab --adapter target/ske-adapter-go --yes-run-adapter --format json
target/debug/ske db check --db target/go-worker-final.sqlite --format json
git diff --check
```

- Go : tests de tous les packages PASS ; nouvelles régressions nil/signatures,
  recovery, mapping SELECT blocking/default, range avec `if` interne, absence
  de mapping, unicité des déclarations/cibles.
- CIR Rust : 1 test unitaire + 12 contrats + 1 doctest PASS.
- Neuf suites E2E ci-dessus : **9 PASS, 0 ignoré**. Golden manuels inchangés ;
  véritables extractions Go puis import Rust, sans résultats simulés.
- Ingestion CLI réelle : snapshot CLOSED/PARTIAL, racine COMPLETE, partial
  PARTIAL, tags COMPLETE ; **0 ERROR, 511 INFO, 4 WARNING**.
- `db check` : `ok=true`, 22 contrôles, 0 violation.
- Debug indépendant disponible via
  `SKE_TEST_DUMP_SSA=1 go test ./internal/emit -run TestExtractLab -v`
  (source/typecheck/SSA uniquement). Sortie locale examinée :
  `target/go-worker-ssa.log`, notamment le bloc Recover sans prédécesseur.

### Frontières restantes

Ce lot ne certifie **pas** les 25 AC v0.2. Le parent porte les tests IPC,
consentement, limites, échecs, hash, interop réelle Go↔Rust, scripts/génération,
tag candidat et validation globale/régression v0.1. Aucune de ces validations
globales n'a été relancée ici. Aucun push/merge partagé ni sous-agent.

Hors laboratoire, les ranges entiers/arrays/chans et selects optimisés sans
`ssa.Select` peuvent encore rester explicitement incomplets. Les tests ajoutés
ne prétendent pas une couverture générale de tous les abaissements Go.
L'environnement de recovery est conservateur, non un modèle d'exécution
defer/recover ; les appels CHA peuvent produire de nombreuses cibles externes
et diagnostics INFO. `include_tests`, `go_work`, cgo, empreinte exhaustive et
limites internes SSA restent à auditer selon le rapport historique.
