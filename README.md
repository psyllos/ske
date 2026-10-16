# SKE — Software Knowledge Engine

SKE construit une connaissance structurelle traçable à partir d'une CIR typée,
stockée dans SQLite. La source normative est
`SKE_Specification_Implementation_v1.4.md` ; les critères §19 et fixtures §15
priment sur les exemples.

## État

**v0.1 validée : 42 AC PASS**, checkpoint local `f18e6e8`.
**v0.2 validée : 25 AC PASS**, après correction des six défauts de revue ;
non-régression v0.1 : 42 AC PASS. Adaptateur Go réel, IPC borné et tests E2E
intégrés.
**v0.3 validée : 29 AC PASS** (moteur symbolique + SMT), `VERIFY OK v0.3`,
non-régression 42 AC v0.1 + 25 AC v0.2. Les limites et approximations de v0.3
sont explicites dans `docs/soundness-and-limits.md`.
**v0.4 validée : 14 AC PASS**, `VERIFY OK v0.4` ; non-régression
42 AC v0.1 + 25 AC v0.2 + 29 AC v0.3. Graphe d'appels, résumés bornés,
intervalles et interprétation concrète CIR sont disponibles dans le périmètre
documenté. Voir [la validation v0.4](docs/validation/v0.4.md).
Lire [le journal de reprise](docs/work-log.md) et
[le rapport de blocage](docs/validation/BLOCKED-v0.2-quota.md).
Il n'existe pas de livraison v0.7 ni de promesse d'API/MCP opérationnels.

## Environnement

Monorepo Rust édition 2021, toolchain épinglé 1.99.0, environnement de référence
macOS arm64. Go 1.27.1 est présent mais non requis pour l'import CIR v0.1.
Le build Rust utilise `prost-build` et `protoc-bin-vendored` 3.3.0, pas le
`protoc` système 36.2. Z3 5.1.0 est présent mais n'est pas intégré à v0.1.
Voir [dépendances](docs/dependencies.md).

## Installation Z3 pour v0.3

Le workspace lie une bibliothèque Z3 **système** via `z3 = 0.21.1` et
`z3-sys = 0.13.1` ; il ne compile ni ne télécharge Z3 au build.
Installer `z3` et `pkg-config` (`brew install z3 pkg-config` sur macOS ;
`sudo apt-get install libz3-dev z3 pkg-config` sur Debian/Ubuntu).
La machine de validation utilise Z3 5.1.0, macOS arm64.
Pour un préfixe non standard, renseigner `PKG_CONFIG_PATH` vers son
répertoire `lib/pkgconfig` et le chemin de chargement dynamique approprié.

```sh
pkg-config --modversion z3
cargo test -p ske-solver z3_
```

Après import CIR ou Go :

```sh
target/debug/ske branches analyze --db "$DB" --snapshot latest --entry example.Decide --format json
target/debug/ske branches list --db "$DB" --function example.Decide --run latest --format json
target/debug/ske runs list --db "$DB" --format json
```

`POSSIBLE` n'est pas un témoin exécuté. `PROVEN_UNREACHABLE` est une preuve
dans le modèle sous hypothèses, de niveau `MODEL_UNREACHABLE`, jamais une
certification absolue du logiciel source. SIGINT conserve les arrivées déjà
persistées ; un run laissé `RUNNING` après un crash exige un
`ske runs mark-failed --db "$DB" --run N` explicite.

## Parcours v0.1 depuis la racine

Le parcours CIR est disponible ; `verify.sh` inclut maintenant le Go et termine
par `VERIFY OK v0.4`. Ne pas exécuter
le code du projet étudié : les fixtures Go servent ici de sources et provenance.

```sh
cargo build --workspace --all-targets
cargo run -p ske-cir --example gen_fixtures -- --negative --out tests/fixtures/cir
DB="$(mktemp -d)/k.sqlite"
FX=tests/fixtures/cir
LAB=examples/go-branch-lab
target/debug/ske init --db "$DB"
target/debug/ske ingest cir --db "$DB" --input "$FX/decide.cir.pb" --source-root "$LAB" --format json
target/debug/ske inspect --db "$DB" --snapshot latest --format json
target/debug/ske functions list --db "$DB" --snapshot latest --format json
target/debug/ske functions cfg --db "$DB" --snapshot latest --function example.Decide --format json
target/debug/ske branches list --db "$DB" --snapshot latest --function example.Decide --format json
target/debug/ske snapshots list --db "$DB" --format json
target/debug/ske db check --db "$DB" --format json
./scripts/verify.sh
```

`gen_fixtures` produit les entrées, jamais les golden attendus : ceux-ci doivent
être transcrits indépendamment de §15. L'inventaire v0.1 expose `analysis: null`
et des issues `UNANALYZED`, sans inventer de résultats symboliques.
`VERIFY OK v0.1` a été constaté au checkpoint v0.1 ; la validation actuelle
v0.2 inclut sa non-régression. `python3 scripts/validate-v02.py` rejoue les 25 AC.

## Analyse interprocédurale v0.4

Après ingestion Go du laboratoire :

```sh
target/debug/ske paths explore --db "$DB" --source example.Entry --target example.helper --format json
python3 scripts/validate-v04.py
```

`paths explore` accepte `--max-depth`, `--max-paths`, `--max-nodes` et
la pagination `--limit`/`--offset`. Un appel inconnu rend `incomplete=true`,
jamais une preuve d'inaccessibilité.

Les résumés persistés sont accessibles via l'API Rust applicative :

```rust
let query = ske_query::Query::open(db_path)?;
let summary = query.summarize_scalar(
    "latest", None, "example.Entry", &Default::default(),
)?;
```

La composition des globales connues préserve l'état mémoire entre les appels.
La récursion résiduelle reste `PARTIAL` ; les effets non modélisés restent
opaques. Aucun code hôte du projet étudié n'est exécuté.
Pour les boucles, un fichier budget contenant
`[budget]` puis `loop_mode = "intervals"` active l'abstraction ;
le défaut reste `"unroll"`.

## Documentation

- [Architecture](docs/architecture.md)
- [Soundness et limites](docs/soundness-and-limits.md)
- [Contrat adaptateur](docs/adapter-contract.md)
- [ADR](docs/adr/ADR-001.md) (001 à 021)
- [Mesures](tests/benchmarks/README.md)
