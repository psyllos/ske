# T-0.2-09 — checkpoint des suites CLI Go

## État

Checkpoint demandé par le parent après arrêt de l'agent adaptateur sur quota.
**Aucun AC v0.2 n'est déclaré PASS par ce lot.**
Les neuf suites Rust demandées sont écrites et compilent sur la base v0.1,
sans changement de Cargo.toml, du harness `common`, des sources Go étudiées,
ni des 49 golden manuels.

## Fichiers et oracles

`crates/ske-cli/tests/go_support/mod.rs` est le helper dédié :

- vrai binaire CLI fourni par Cargo (`CARGO_BIN_EXE_ske`) ;
- construction de notre adaptateur seulement, via
  `go build -p=1 -o target/ske-adapter-go ./cmd/ske-adapter-go`
  dans `adapters/go` ; `OnceLock` par exécutable de test ;
- verrou interprocessus de construction, `GOMAXPROCS=2`, bases temporaires
  distinctes ; aucun test ignoré, aucun skip si Go/adaptateur absent ;
- profils explicites linux/amd64, darwin/amd64 et linux/386 ;
- lecture des spans dans les vrais fichiers, colonnes en octets, fin exclusive ;
- requête protobuf ExtractUnit indépendante encadrée par u32 big-endian ;
  comparaison des flux AdapterResponse bruts sur les trois unités ;
- lecteur minimal d'enveloppes protobuf pour les types et indices de champs
  persistés, sans dépendance Rust supplémentaire.

| Suite | Vérifications |
|---|---|
| `go_golden_decisions` | 49 golden manuels, fonctions présentes, compte par fonction, rang sdN, kind, texte source relu, parent, ids et ordinal/label des issues, db check |
| `go_counters` | 3 unités ; compteur indépendant = compteur émis persisté = COUNT réel = total manuel (52 racine, 1 partial, 0 tags) |
| `go_props` | liens, absence HEURISTIC, comptes IR des golden, bornes SSA pour Range/Pipe/DaySwitch, inversion Check, liens And/Day, dominance AndCall, multi-return/extract, interface/externe, largeurs 8/64/32, concurrence, génériques, DNF, provenance négative |
| `go_diagnostics` | diagnostics requis/interdits de tous les golden ; sévérité et impact exacts des warnings ; propagation COMPLETE/PARTIAL ; aucun ERROR, avertissement racine non listé refusé |
| `go_vs_cir` | Go contre les deux fixtures CIR manuelles decide/check, projection source kind/span/texte/issues et liens edge-kind/mapping/quality/inverted, 4 issues IR |
| `go_build_tags` | deux snapshots et fingerprints distincts, tous les source_files tags du snapshot, fichier exclu nommé dans le diagnostic INFO |
| `go_deterministic` | deux extractions brutes successives par unité, identiques octet par octet et non vides |
| `go_activation_cases` | alternatives DNF EXACT complètes et ordonnées OR/AND/MIXED, correspondance vers les arêtes des atomes ; mutation locale explicite incomplet/HEURISTIC, navigation UNANALYZED sans preuve/lien créé |
| `go_implicit_exit_inventory` | témoins source indépendants de nil/division ; sites non vides, spans relus, kind/statut/id ordinal, diagnostic unique par fonction avec nombre de sites ; absence de résultats comportementaux/proofs en v0.2 |

## Vérifications réellement exécutées

1. `rustfmt --edition 2021 crates/ske-cli/tests/go_*.rs crates/ske-cli/tests/go_support/mod.rs` : succès.
2. `cargo test -p ske-cli --test go_golden_decisions --test go_counters --test go_props --test go_diagnostics --test go_vs_cir --test go_build_tags --test go_deterministic --test go_activation_cases --test go_implicit_exit_inventory --no-run` : succès, neuf exécutables compilés.
3. `cargo clippy -p ske-cli --test go_golden_decisions --test go_counters --test go_props --test go_diagnostics --test go_vs_cir --test go_build_tags --test go_deterministic --test go_activation_cases --test go_implicit_exit_inventory -- -D warnings` : succès.
4. `cargo test -p ske-cli --test go_golden_decisions` : **échec attendu de dépendance**, adaptateur non présent dans ce fork ; message lisible
   `Go conformance requires go and adapters/go; adapter must not be silently skipped: ... No such file or directory`.
   0 passed, 1 failed, 0 ignored. Cet échec n'est pas une validation des assertions de conformité.

Les huit autres suites n'ont pas été exécutées : même dépendance absente.
Ni verify complet, ni régression v0.1 exécutés dans ce lot.

## Reprise

1. Fusionner les travaux adaptateur et IPC/CLI v0.2 dans le parent, puis ces tests.
   Le CLI visé est le contrat stable §11 : `ingest go --repo --profile --adapter
   --yes-run-adapter --db --format json`.
2. Exécuter séparément les neuf commandes AC sans `--no-run`, puis clippy
   et verify. Ne pas éditer golden/sources pour satisfaire un test en échec.
3. Examiner les écarts réels avec l'agent adaptateur. Le contrat SQLite attendu
   est la migration normative existante ; le compteur émis est
   `compilation_units.source_decision_count`.
4. P-ACT-NEGATIVE vérifie seulement la frontière v0.2 observable
   (navigation sans preuve et préservation provenance), pas un moteur v0.3.
   La mutation se limite à la base temporaire du test, après extraction réelle.
5. L'inventaire implicite dispose de témoins minimaux indépendants et valide
   tous les sites émis ; il ne constitue pas un oracle exhaustif de tous les
   sites AST possibles. Renforcer cet oracle à la reprise avec les conventions
   de span de l'adaptateur, sans dériver les attendus de sa sortie.
6. Les nombres exacts SSA pour SumRange/Pipe/DaySwitch ne sont volontairement
   pas inventés : §15.1.5 impose observation indépendante de la toolchain
   épinglée avant ajout éventuel d'un golden exact. Les bornes normatives ≥ 1
   sont déjà testées.

Le test de hash source appartient à l'autre lot. Aucun développement v0.3,
aucun nouvel agent, aucun contournement du quota n'a été entrepris.
