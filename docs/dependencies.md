# Dépendances et outils

## Source de vérité

Les versions ci-dessous sont recopiées du Cargo.toml du workspace, versions
fixées après observation par `cargo search` lors de l'implémentation.
Cargo.toml définit les contraintes directes ; Cargo.lock conserve la résolution
transitive. Ne pas déduire une intégration fonctionnelle d'une dépendance listée.

| Dépendance | Version exacte | Rôle |
|---|---|---|
| prost | 0.14.4 | protobuf Rust |
| prost-build | 0.14.4 | génération au build |
| protoc-bin-vendored | 3.3.0 | protoc fourni au build Rust |
| rusqlite | 0.40.2 | SQLite, feature bundled |
| sha2 | 0.11.1 | SHA-256 |
| serde | 1.0.229 | sérialisation, derive |
| serde_json | 1.0.154 | JSON |
| clap | 4.6.7 | CLI, derive et env |
| tempfile | 3.27.0 | fichiers temporaires de test |
| z3 | 0.21.1 | solveur natif Rust, instance neuve par requête |
| z3-sys | 0.13.1 | liaison système libz3, sans build vendored |
| toml | 1.1.8 | budgets et contextes stricts |
| signal-hook | 0.4.5 | annulation SIGINT/SIGTERM |

Les crates SKE sont des dépendances locales path ; édition workspace 2021,
resolver 2, version de package 0.1.0. La liste blanche des dépendances normales
est celle de §2.2, contrôlée par scripts/check-deps.sh.

## Environnement fourni

| Outil | Version / état |
|---|---|
| Rust | 1.99.0 épinglé dans rust-toolchain.toml ; rustfmt et clippy |
| Plateforme de référence | macOS arm64 |
| Go | 1.27.1, adaptateur v0.2 validé |
| protoc système | 36.2 présent, non utilisé par le build Rust |
| Génération Rust | prost-build + protoc-bin-vendored 3.3.0 |
| Z3 | 5.1.0 système, intégration Rust réelle v0.3 |
| SQLite | bibliothèque bundled par rusqlite ; version effective à relever lors de validation |

Le numéro du paquet protoc-bin-vendored n'est pas une assertion de version du
binaire protoc embarqué. La décision détaillée de génération est ADR-019,
livrée par l'agent CIR. La génération Go utilise le même compilateur vendored
et `protoc-gen-go v1.36.11`, épinglé dans le script. Les modules Go directs
sont protobuf 1.36.11 et x/tools 0.52.0 ; `adapters/go/go.mod` et `go.sum`
conservent la résolution réelle.

## Z3 système

Installer `brew install z3 pkg-config` sur macOS, ou
`sudo apt-get install libz3-dev z3 pkg-config` sur Debian/Ubuntu.
Un préfixe spécifique se sélectionne avec `PKG_CONFIG_PATH` ; la bibliothèque
dynamique doit également rester accessible à l'exécution. Le build ne requiert
pas une longue compilation native et ne télécharge pas Z3.
Le spike réel `pkg-config --modversion z3` puis
`cargo test -p ske-solver z3_` et `otool -L` est consigné dans
`worker-solver.md` : bibliothèque `/opt/homebrew/opt/z3/lib/libz3.5.1.dylib`.
Linux et Windows n'ont pas été validés localement ; la CI installe la dépendance
système avant Clippy/tests. Z3 est aussi utilisé comme exécutable de confiance
par les tests indépendants du vérificateur, pas comme backend de production.

## Validation exécutée

Les commandes et sorties figurent dans `validation/v0.1.md` et
`validation/v0.2.md` : 42 AC v0.1 et 25 AC v0.2 PASS.
CI distante et Linux non exécutés ici.
