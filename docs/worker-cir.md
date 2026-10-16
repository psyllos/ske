# Checkpoint durable — v0.1 T03/T04

## Livré
- Schémas CIR et adaptateur copiés octet à octet des blocs normatifs §5.
- `ske-cir`, génération `prost-build` avec `protoc-bin-vendored` dans OUT_DIR.
- Types publics `ske_cir::proto`, `ske_cir::adapter_proto`.
- Registre déclaratif des 66 opcodes non réservés; métadonnées arité, résultat,
  payload, terminateur et arêtes. Les inconnus ne sont jamais des no-op.
- BitVec largeurs 1..128; 27 vecteurs §4.9, dont compte u16=256, division zéro,
  minimum signé divisé par -1 et décalage signé négatif.
- Validations locales types, valeurs/littéraux, instructions et arêtes.
- `docs/cir-semantics.md`, une section et exemples par opcode; ADR-019.

## API communiquée au parent
Toutes les fonctions suivantes sont exportées à la racine de `ske_cir`:

```rust,ignore
validate_type(&TypeDef) -> ske_common::Result<()>
validate_literal(&LiteralValue, &TypeDef) -> ske_common::Result<()>
validate_value(&ValueDef, &HashMap<String, TypeDef>) -> ske_common::Result<()>
validate_instruction(&InstructionDef, &HashMap<String, TypeDef>,
                     &HashMap<String, ValueDef>) -> ske_common::Result<()>
validate_edges(&InstructionDef, &[ControlEdgeDef],
               &HashMap<String, ValueDef>) -> ske_common::Result<()>
```

Les maps sont fournies par l'appelant; aucun buffering d'unité dans cette crate.
`registry::{OPCODES, lookup}` expose la table. `call_effects` et `opaque_effects`
rendent `(sortie_anormale_possible, mutation_memoire_possible)`; `None` devient
toujours `true`, même si un autre drapeau prétend la pureté.

## Preuves exécutées
- `cargo test -p ske-cir`: PASS initial puis revalidé après revue (voir ci-dessous).
- `cargo test -p ske-cir bitvec_vectors`: PASS, 27 vecteurs.
- `cargo test -p ske-cir registry_documented`: PASS, boucle sur tout le registre:
  présence documentation, cas valide, cas invalide d'arité ou payload.
- `cargo clippy -p ske-cir --all-targets -- -D warnings`: PASS.
- `RUSTFLAGS="-D warnings" cargo build -p ske-cir --all-targets`: PASS.
- `cargo fmt -p ske-cir -- --check`: PASS.
- `rustfmt --edition 2021` sur build.rs, src et tests de ce lot.
- Script Python comparant chaque schéma au bloc proto extrait §5: EXACT pour les deux.

Premier essai `--locked` bloqué car lock absent; génération Cargo.lock ensuite
autorisée explicitement par le parent. Premier build bloqué sur ske-common/sha2:
correctif du parent importé (aucune modification manuelle de common).

## Frontières / reste intégration parent
L'importeur valide l'appartenance aux fonctions, les références inter-unités,
l'unicité SSA, les paramètres/retours selon FunctionDef, les terminateurs par bloc,
la dominance et les prédécesseurs PHI. `validate_edges` valide la source du
terminateur mais ne dispose pas de la map des blocs pour contrôler leur fonction.
Les références TypeDef et les filtres d'exception sont résolus par l'importeur.
Les décisions/activations source et compteurs ne font pas partie des validateurs
locaux T04.

Les fixtures et le script gen-proto appartiennent aux autres lots. Aucun AC global
workspace, CLI, SQLite ni `verify.sh` revendiqué ici. Aucun travail v0.2/v0.3.

## Correctif après signalement import
`validate_edges` vérifie désormais l'arité indépendamment de `validate_instruction`,
car l'import peut vérifier les arêtes en premier. Le test
`edge_validation_empty_operands_never_panics` appelle directement ce validateur
avec BRANCH/SWITCH sans opérandes et attend `E_ARITY_MISMATCH`.
Après ce correctif: `cargo test -p ske-cir`, Clippy all-targets avec `-D warnings`
et `cargo fmt -p ske-cir -- --check` passent.

## Correctifs de revue sûreté
- Callee POINTER GLOBAL refusé: le rôle GLOBAL ne rend pas arbitrairement appelable.
  Pour un type FUNCTION, les arguments doivent correspondre aux paramètres,
  et CALL/INVOKE retournent le type déclaré (TUPLE ordonné pour retours multiples).
  SPAWN/DEFER_PUSH valident les arguments mais abandonnent les valeurs retournées.
  Un récepteur INTERFACE conserve la résolution dynamique; sa signature concrète
  n'est pas inventée ici.
- Dispatch CallSemantics hors registre refusé par `E_UNKNOWN_KIND`;
  transparence référentielle sans pureté refusée par `E_TYPE_MISMATCH`.
- NEG exige WRAP; ADD conserve les quatre modes autorisés.
- INVOKE refuse catch-all non final, donc aussi plusieurs catch-all.
- BitVec encapsule `width` et `bits`: `new` refuse width 0/129 et masque les bits.
  API lecture `width()` / `bits()`, négation via `std::ops::Neg` (`-value`).
  Ce changement d'accès garantit l'invariant plutôt que de valider tardivement
  chaque opération; les types et noms des deux composantes §4.9 sont conservés.
  Aucun nouvel allow Clippy ajouté: l'ancien allow de `neg` est supprimé.
- Régressions réelles ajoutées pour les probes de revue, signatures multi-retour,
  constructeur et impossibilité d'instanciation directe (doctest compile_fail).

Revalidation finale après ces corrections: `cargo test -p ske-cir` PASS
(1 unitaire, 11 intégration, 1 doctest compile_fail); les commandes exactes
`cargo test -p ske-cir bitvec_vectors`, `cargo test -p ske-cir registry_documented`,
`cargo clippy -p ske-cir --all-targets -- -D warnings`,
`RUSTFLAGS="-D warnings" cargo build -p ske-cir --all-targets` et
`cargo fmt -p ske-cir -- --check` passent toutes.
