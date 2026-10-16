# Lot T-0.1-07 — notes d'intégration

## API

- `Storage { pub conn: rusqlite::Connection }`
- `Storage::open(path: impl AsRef<Path>) -> ske_common::Result<Storage>`
- `Storage::open_with_busy_timeout(path, Duration)`
- `Storage::db_check() -> Result<serde_json::Value>` : rapport complet,
  même si `ok=false` ; le CLI doit choisir exit 6 dans ce cas.
- `Storage::open_snapshot_writer(id)` / `closed_snapshot(id)`.
- `OpenSnapshotWriter::snapshot_id()` / `execute(sql, params)`.
- `ClosedSnapshot::snapshot_id()` / `inventory_status()` /
  `add_analysis_run(kind, engine_version, config_fingerprint, assumptions_fingerprint)`.
- `map_sql_error(rusqlite::Error)` conserve les codes des triggers, traduit
  FK et les contraintes ordinales explicitement normées.

Le writer fonctionne dans une transaction d'import déjà ouverte (savepoint)
ou seul (BEGIN IMMEDIATE). L'importeur doit utiliser le writer pour le SQL
structurel, ne pas partager une transaction externe avec une autre connexion,
et vérifier lui-même les références et types CIR avant insertion.
Les violations CHECK de types/opcodes ne peuvent pas être classées correctement
sans contexte CIR : elles sont une erreur interne de persistance si elles
atteignent cette couche. Le validateur d'import fournit le code typé en amont.

## Contradictions et limites normatives non corrigées

Le schéma exact contient bien 46 tables + runner = 47 et 21 triggers.
Les points suivants sont conservés sans retouche du DDL ni de CHK-22 :

1. §8.3 annonce tous les attributs déterminants figés, mais le trigger ne couvre
   pas `analysis_kind`, `started_at`, ni `id`. Les UPDATE support vérifient OLD
   seulement : une ligne d'un autre run peut être réaffectée au run prouvé.
   L'appelant ne doit pas pratiquer ces mutations. Une correction demande une
   nouvelle migration et décision normative, pas une modification silencieuse.
2. CHK-15 ne teste pas `model_soundness <> 'UNSUPPORTED'`, contrairement aux
   triggers d'écriture des résultats. Les triggers restent la protection normale.
3. §8.5 décrit aussi l'ordre des termes d'activation, mais CHK-22 normatif ne
   contrôle pas les trous d'ordinaux ni la validité symbolique des alternatives.
   Les UNIQUE/CHECK garantissent seulement unicité et non-négativité.
4. L'immuabilité structurelle CLOSED est applicative. Le contrat public impose
   une connexion SQL accessible ; le DDL exact n'a aucun trigger CLOSED.
   Un client SQL brut peut donc contourner le writer. Le SQL générique du writer
   doit lui-même cibler son snapshot ; ce n'est pas un analyseur de requêtes.
5. CHK-22 valide les déclarations, ne transforme jamais un inventaire sans
   alternatives complètes en preuve source. Cette couche n'implémente pas de
   moteur ou rejeu solveur.

## Validation

Les tests utilisent SQLite bundled et des insertions/mutations réelles,
sans mocks ni triggers supprimés. Le rapport final de ce lot précise les
commandes exécutées, et distingue les checks package des checks workspace/CLI.

Commandes exécutées depuis la racine, toutes exit 0 après import de la correction
`ske-common::sha256_hex` du parent :

| Commande | Résultat |
|---|---|
| `cargo test -p ske-storage` | 12 tests PASS, 0 ignored |
| `cargo test -p ske-storage schema_tables_match` | 1 PASS ; octets DDL, 47 tables, 21 triggers |
| `cargo test -p ske-storage migrations_` | 1 PASS ; WAL fichier, idempotence, checksum et exit 6 |
| `cargo test -p ske-storage connection_has_fk_on` | 1 PASS ; trois ouvertures, insertions orphelines refusées |
| `cargo test -p ske-storage trigger_` | 3 PASS ; preuves absentes/rejetées/autre run/autre issue, run ouvert/coupé, soundness et frontier invalides, WITNESSED, INSERT/UPDATE valide |
| `cargo test -p ske-storage closed_snapshot_rejects_structural_writes` | 1 PASS ; CLOSED/ABORTED, writer périmé, nouveau run autorisé |
| `cargo test -p ske-storage proof_immutability_` | 1 PASS ; mutations preuve, trois tables support, attributs de run et résultats refusées, métriques autorisées |
| `cargo test -p ske-storage source_activation_integrity` | 1 PASS ; les cinq catégories CHK-22, FK réelles, SQL exact |
| `cargo clippy -p ske-storage --all-targets -- -D warnings` | PASS |
| `RUSTFLAGS='-D warnings' cargo build -p ske-storage --all-targets` | PASS |
| `rustfmt --edition 2021 --check crates/ske-storage/src/lib.rs crates/ske-storage/tests/storage.rs` | PASS |

Les tests supplémentaires couvrent les corruptions SQL inter-snapshot, les
deux moitiés du contrôle PHI, les compteurs, le terminateur, le DAG, les warnings,
la limite de 20 exemples, les savepoints et un writer devenu périmé après clôture
sur une deuxième connexion.

Non exécutés dans ce lot : commandes CLI `ske init` / `ske db check`,
tests workspace complets et `cargo fmt --all -- --check`. Ils appartiennent à
l'intégration parent. L'essai initial `cargo test -p ske-storage --locked`
était bloqué par l'absence de lock ; l'essai scratch initial a exposé le défaut
LowerHex de common, ensuite corrigé par le parent et importé. Le parent a
autorisé la génération locale du lock pour les validations : il doit conserver
ou régénérer son lock workspace lors de l'intégration.
