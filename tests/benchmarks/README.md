# Mesures v0.1

Mesure locale v0.1 effectuée par le coordinateur sur macOS arm64 :
200 000 instructions, 6,82 s d'import, RSS maximum 68 812 800 octets.
Ce n'est pas la mesure Linux `time -v`. Voir `docs/validation/v0.1.md`.
Référence : AC-0.1-35 et §17 de la spécification v1.4.

## Streaming et mémoire

Préparer un build release et une fixture volumineuse hors fichiers committés :

```sh
cargo build --release --workspace
OUT="$(mktemp -d)"
DB="$(mktemp -d)/b.sqlite"
cargo run --release -p ske-cir --example gen_fixtures -- --big --out "$OUT"
```

Sous Linux, commande du critère (le code de sortie de l'import doit aussi être
contrôlé, indépendamment du filtrage de la sortie) :

```sh
/usr/bin/time -v target/release/ske ingest cir --db "$DB" --input "$OUT/big.cir.pb"
```

Attendu : ingestion exit 0 et Maximum resident set size < 524288 ko.
Sous macOS, utiliser la mesure native suivante et consigner explicitement les
unités (octets pour maximum resident set size), sans la présenter comme
l'exécution de la commande Linux :

```sh
/usr/bin/time -l target/release/ske ingest cir --db "$DB" --input "$OUT/big.cir.pb"
```

Le critère mémoire est non bloquant sous macOS, mais sa mesure doit être
consignée. Noter OS/architecture, CPU/RAM, versions effectives, commit,
taille/compteurs de fixture, commande exacte, exit, durée, RSS et unités.
Répéter avec une base neuve ; ne pas mélanger coût de compilation et import.
Le parent conserve les preuves dans son rapport de validation.

Les benchmarks SMT, adaptateur Go et comportements sont prévus, non implémentés.
Ne pas les commencer avant validation des jalons précédents, dont les 42 AC v0.1.

## v0.7 — protocole §17 (AC-0.7-10)

`scripts/ske-bench.py` exécute le protocole §17 sur `$LAB` et consigne :
coût d'indexation, coût par requête, qualité et rappel, avec et sans
représentation structurée. Composants : SMT (§6.4), adaptateur Go (§9),
comportements/règles (v0.5).

```sh
python3 scripts/ske-bench.py --check
```

Exige au préalable `cargo build -p ske-cli` (binaire `target/debug/ske`) et le
build de l'adaptateur Go de confiance (le script l'exécute sinon) ; `go` et
`cargo` doivent être disponibles. Rapports JSON (reproductibles) dans
`target/bench-v07/bench-report.json` (env, LOC $LAB, durées, pic RSS
indexation via `/usr/bin/time`, taille de la base, diagnostics, distribution
des statuts d'issures, rappel classé, qualité des preuves).

Mesures consignées le jour de validation : voir `docs/validation/v0.7.md`.
Aucune valeur n'est échantillonnée depuis un autre run : chaque chiffre vient
d'une commande exécutée par cette campagne.
