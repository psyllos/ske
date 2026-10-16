# Import/query/CLI v0.1 — compte rendu du lot T08/T09/T10

## Architecture livrée

- `ske-ingest::ingest_many` ouvre un seul `BEGIN IMMEDIATE`, active les FK
  différées après ce `BEGIN`, crée dépôt/snapshot et un savepoint par fichier.
  Tout échec, même au deuxième fichier ou au `COMMIT`, annule toutes les lignes.
- Le lecteur consomme les frames u32-BE une par une, avec limite de 16 MiB.
  Chaque définition est contrôlée localement, déclarée dans son espace d'IDs,
  puis **insérée immédiatement** dans les tables structurelles. Une référence
  en avant réserve un entier et reste marquée non déclarée jusqu'à sa définition.
  `EndUnit` ferme les maps et vérifie les références.
- Une table SQLite `TEMP` indexée conserve les protobuf nécessaires aux
  validations ultérieures et aux dérivations. Elle n'est pas un buffer Rust de
  l'unité et ne sert plus au replay des insertions structurelles. Les corps de
  fonctions sont chargés et validés séparément ; seules les maps d'identité
  persistent en mémoire pendant l'unité.
- Les validateurs locaux `ske-cir` restent l'autorité pour le registre des
  opcodes/types/payloads/arêtes. La dominance utilise `ske_graph::Graph`.
- Le cœur dérive décisions/issues IR, provenance, cas d'activation, sites
  d'appel et cibles, effets conservateurs, inventaire et fingerprints. Les stubs
  externes ne sont liés qu'à une unique fonction réelle de même nom/langue dans
  le snapshot.
- `ske-query::Query` fournit les résultats JSON sans exposer de connexion SQLite
  à la CLI. `latest` sélectionne uniquement un snapshot `CLOSED`; les fonctions
  sont sélectionnées par nom qualifié exact, sans les stubs.
- `ske-cli` dépend normalement de query/common, clap et serde_json, jamais
  directement de rusqlite. Les tests CLI peuvent ouvrir SQLite pour contrôler
  les tables et introduire les corruptions spécifiées.

## Commandes exécutées et preuves

Sur macOS, après intégration CIR/storage/graph/fixtures :

```sh
cargo test -p ske-cli -p ske-ingest -p ske-query
cargo clippy -p ske-cli -p ske-ingest -p ske-query --all-targets -- -D warnings
```

Premier checkpoint final : 55 tests CLI et 9 tests ingest réussis, aucun ignoré.
Trois régressions de résolution/pagination ont ensuite été ajoutées et exécutées
avec `cargo test -p ske-cli --test resolution` : 5 tests réussis au total dans
cette suite (58 tests CLI présents désormais). Le filtre exact
`cargo test -p ske-ingest fingerprints_` réussit aussi, ainsi que
`./scripts/check-deps.sh` (`DEPS OK`).
Les suites CLI comprennent les 7 compteurs exhaustifs, 34 erreurs négatives
avec comparaison de **toutes** les tables avant/après, 4 corruptions SQL, sorties
de consultation, 5 golden littéraux, codes de sortie et pureté stdout.

Les tests ingest supplémentaires vérifient la déclaration entièrement inversée
de `adjusted`, les deux fingerprints recalculés indépendamment depuis les
fichiers/champs, le rollback global d'une base déjà peuplée et réutilisable,
le rejet d'un cas d'activation `HEURISTIC` attesté complet, l'inventaire
recalculé et l'identité du dépôt indépendante de `source-root`.

Mesure de streaming exécutée sur le générateur `--big` final de 200 000
instructions (2 000 fonctions), sans racine source :

```sh
cargo build --release -p ske-cli
cargo run --release -p ske-cir --example gen_fixtures -- --big --out "$BIG"
/usr/bin/time -l target/release/ske ingest cir \
  --db "$DB" --input "$BIG/big.cir.pb" --format json
```

Résultat réel : exit 0, snapshot `CLOSED`/`COMPLETE`, 5,97 s de temps réel,
**68 157 440 octets de RSS maximum** (~65 MiB). La commande macOS `time -l`
remplace explicitement le `time -v` Linux ; le critère Linux n'a pas été exécuté
ici. La mesure précède les derniers contrôles généraux de valeurs et de
diagnostics ; le parent doit mesurer sa version intégrée finale.

## Limites et intégration

Ce lot n'autorise pas de conclure que les 42 AC globaux sont validés :
le parent possède le registre `docs/validation/v0.1.md`, les scripts globaux et
la validation workspace. Aucun travail v0.2/v0.3 n'est commencé.

La première implémentation utilisait un spool puis replay d'insertion. Elle a
été remplacée : **la version livrée réserve les références et insère au fil de
l'eau**. Aucune dérogation à la réservation streaming n'est demandée.
