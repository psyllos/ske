# T-0.3-01 — solveur SMT réel

## Périmètre et état

Implémenté dans `crates/ske-solver` suivant §4.3, §4.9 et §6.4.
Ce rapport ne valide que T-0.3-01 / AC-0.3-01..03, pas le jalon v0.3
complet. v0.1/v0.2 n'ont pas été réimplémentées ni revalidées.
Aucun schéma, migration, golden, workspace `Cargo.toml`, push ou merge partagé.
Cargo a mis à jour `Cargo.lock` avec les dépendances du nouveau crate.

## Contrat d'intégration

```rust
pub trait Solver {
    fn check(&mut self, query: &Query, budget: &SolverBudget) -> CheckResult;
    fn check_smtlib(&mut self, text: &str, budget: &SolverBudget) -> CheckResult;
    fn name(&self) -> &str;
    fn version(&self) -> String;
}
```

`CheckResult`, `TimeoutKind`, `SolverBudget` ont les variantes/champs normatifs.
`check` accepte une `Query { smtlib, variables }`, sans dépendance vers l'arène
symbolique. `variables` contient les noms **non quotés** et les sortes exactes
des déclarations du texte, à projeter dans le modèle. Le producteur de texte
doit maintenir ce contrat ; utiliser `Encoder::finish` le garantit.
`Model` est un `BTreeMap<String, ModelValue>` ; valeurs `Bool(bool)`,
`BitVec { width, bits: u128 }`, `Uninterpreted(String)`.
Le moteur connaît la signedness CIR pour convertir les bits en témoin décimal.
`check_smtlib` sert au replay de preuve et ne demande aucune projection de modèle
(`Sat { model: Some(empty_map) }`). Le moteur doit utiliser `check` pour les témoins.
Un témoin SMT n'est jamais une validation par exécution.

Constructeurs : `Z3Solver::new()` et
`ScriptedSolver::new(impl IntoIterator<Item = CheckResult>)`.
Le script conserve `queries` et `budgets` ; son épuisement donne `Unknown`.
L'encodeur produit toujours le texte avant l'appel, même pour le script.

L'API d'encodage est indépendante du backend :

- `Encoder::{declare,nil,assert,finish}` pour les déclarations et requêtes ;
- `Encoder::op(opcode, &[Term], &OpOptions)` retourne
  `EncodedOp { value, normal_guard }` ;
- `Encoder::phi(predecessor, &[(predecessor, Term)])` choisit l'entrée PHI
  par prédécesseur, sans inventer des opérandes CIR ;
- `Sort::{Bool,BitVec(u32),Uninterpreted(type_id)}`, largeurs 1..=128 ;
- `OpOptions` : `signed` de la valeur, `count_signed` du compte,
  `overflow_wrap` (défaut true), `result_sort` pour les conversions ;
- `Term::bitvec` normalise les bits à la largeur, `Term::bool` encode BOOL.

Tous les opcodes « Oui » sont pris en charge : COPY, PHI, SELECT,
ADD/SUB/MUL/DIV/REM/NEG, EQ/NE/LT/LE/GT/GE, BIT_AND/BIT_OR/XOR/BIT_NOT,
SHL/SHR, LOGICAL_NOT, SIGN_EXTEND/ZERO_EXTEND/TRUNCATE/BITCAST.
Les opérations non WRAP sont refusées avec `UNSUPPORTED_OVERFLOW_MODE` pour
permettre au moteur de créer HAVOC + diagnostic ; pas d'approximation silencieuse.
Les autres opcodes sont refusés, jamais traités comme no-op.
Les arités/types/largeurs/réinterprétations sont contrôlés.

DIV/REM et shifts retournent une garde normale ; le moteur doit l'utiliser
pour la sortie normale et sa négation pour PANIC. Les valeurs SMT totalisées
en dehors de cette garde **ne représentent pas** un résultat CIR normal.
Le compte de shift non tronqué est comparé à la largeur avant réduction.
La comparaison elle-même représente la largeur, même pour un compte u1.
SHR arithmétique surdimensionné remplit le signe.

Les sortes non interprétées ont des noms `S_<hex UTF-8 du type_id>` :
nettoyage injectif, sans collision `a-b` / `a_b`. `nil_<sorte>` est déclaré
une fois ; le préfixe de variable `nil_` est réservé. Les identifiants de
variables sont quotés, et `|`, `\`, contrôles et noms vides sont rejetés.
Les fragments `Term.smt` fournis manuellement doivent être des expressions
SMT-LIB valides et correctement typées ; ce n'est pas un parseur d'entrée CIR.

## Z3 : spike et dépendances système

Machine testée : macOS arm64. Dépendances Rust épinglées :
`z3 = "=0.21.1"`, `z3-sys = "=0.13.1"` ; `sha2` du workspace.
`ske-cir` est uniquement une dev-dependency pour la comparaison sémantique.
Aucun appel au programme CLI Z3 pour résoudre : bindings Rust vers libz3.
Pas de téléchargement natif/build vendored dans le crate.

Commandes réellement exécutées :

```text
pkg-config --modversion z3
5.1.0.0
pkg-config --libs --cflags z3
-I/opt/homebrew/Cellar/z3/5.1.0/include -L/opt/homebrew/Cellar/z3/5.1.0/lib -lz3
z3 --version
Z3 version 5.1.0 - 64 bit
otool -L target/debug/deps/solver-e476d66e8eae3ca6
  /opt/homebrew/opt/z3/lib/libz3.5.1.dylib
  (compatibility version 5.1.0, current version 5.1.0)
```

Pour installer sur macOS : `brew install z3 pkg-config`.
Pour Linux : installer Z3 et son paquet de développement avec `pkg-config`,
dans une version compatible avec z3-sys 0.13.1 ; la version réellement testée
ici est **5.1.0**. Vérifier `pkg-config --modversion z3` avant compilation :
les anciennes versions des distributions ne sont pas implicitement validées.
Pour une installation personnalisée, renseigner `PKG_CONFIG_PATH` vers `z3.pc`,
et assurer la disponibilité de la bibliothèque dynamique au chargement.
Windows/Linux et autres versions natives ne sont pas testés dans ce lot.
`z3-sys` livre les bindings : libclang/bindgen n'est pas nécessaire ici.

Une instance de contexte **et** solveur est créée à chaque requête.
`rlimit` et timeout sont posés par paramètres solveur à chaque appel.
Z3 expose ces paramètres comme unsigned 32 bits : les budgets u64 hors plage
sont refusés explicitement, jamais tronqués.
Les graines `smt.random_seed` et `sat.random_seed` sont des paramètres
**globaux de modules Z3**, pas toutes deux des paramètres de solveur :
l'essai initial avec `sat.random_seed` dans `Solver::set_params` échouait.
Le code final sérialise ses appels Z3 avec un mutex, pose les deux graines
avant la création du contexte et restaure les anciennes valeurs par RAII.
Le paramètre local `random_seed` est également posé.
Les autres utilisateurs directs de Z3 dans le processus ne doivent pas
modifier ces paramètres en parallèle hors de cette API.
Les erreurs du parseur et de paramétrage sont vérifiées par `Z3_get_error_code`
(z3.rs désactive le handler abortif, mais `from_string` ne retourne pas d'erreur).

Mapping : `resource`/`rlimit` → Rlimit ; `timeout` → WallClock ;
`canceled` sans autre précision → Budget (ne pas inventer une cause) ;
sinon Unknown avec raison conservée. Timeout mur est non reproductible.
Un véritable rlimit=1 est exercé, pas simulé.
La limite mur est posée réellement ; son mapping est testé, sans assertion
fragile dépendant de la vitesse machine.

`query_fingerprint` applique exactement SHA-256(text + `|seed=...|rlimit=...`).
Le cache **par run**, les compteurs, les lignes SQLite, diagnostics
`SOLVER_UNAVAILABLE`, états FAILED et règles de preuve appartiennent au moteur/
stockage/vérificateur, pas au backend. Les erreurs natives récupérables sortent
comme Unknown avec préfixe `SOLVER_UNAVAILABLE` ; l'intégration doit les faire
échouer, pas les transformer en preuve. Une bibliothèque dynamique absente
empêche le chargement du programme ; un crash natif fatal n'est pas récupérable
par ce binding in-process. Ne pas prétendre à une isolation de processus.

## Validation locale

| Commande exacte | Résultat |
|---|---|
| `cargo test -p ske-solver z3_` | exit 0, 3 tests |
| `cargo test -p ske-solver encoding_matches_bitvec` | exit 0, 3 tests |
| `cargo test -p ske-solver scripted_` | exit 0, 1 test |
| `cargo test -p ske-solver` | exit 0, 9 tests ; 0 ignoré |
| `cargo clippy -p ske-solver --all-targets -- -D warnings` | exit 0 |
| `cargo fmt -p ske-solver -- --check` | exit 0 |
| `RUSTFLAGS='-D warnings' cargo build -p ske-solver --all-targets` | exit 0 |
| `./scripts/check-deps.sh && ./scripts/check-deps.sh --selftest` | DEPS OK, DEPS SELFTEST OK |
| `git diff --check` | exit 0 |

AC01 vérifie les deux conjonctions signées 64 bits imposées et le modèle
8 bits `x=127` pour `x+1 <s x`. AC02 prouve par requêtes Z3 UNSAT que chaque
encodage diffère impossiblement des bits calculés par `ske-cir::bitvec` :
tous les vecteurs §4.9, erreurs via gardes, chaque opcode « Oui », BOOL,
PHI/SELECT, comparaisons signées/non signées, largeurs frontières 1/7/64/128,
comptes de shifts 1/8/128 et valeurs jusqu'à u128::MAX. Aucune constante
spécifique aux fixtures dans l'encodeur. AC03 rejoue les quatre réponses
imposées dans l'ordre et vérifie le texte conservé.
Tests complémentaires : nil/EQ/NE, modèle BOOL/u128, erreurs arité/type,
réinitialisation entre requêtes, SMT-LIB invalide et budgets hors plage.

Restent au parent : intégrer le bridge des expressions, les témoins typés,
le cache par run, les diagnostics/états FAILED et la persistance/preuve.
Pour les dépendances workspace des consommateurs, ajouter
`ske-solver = { path = "crates/ske-solver" }` au root si nécessaire ;
aucune modification root faite par ce lot.
