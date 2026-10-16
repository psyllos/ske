# Correction CI — classification de la fin du processus adaptateur

## Échec distant constaté

- `main`, commit `8004d2f`, run
  https://github.com/psyllos/ske/actions/runs/38878648861 :
  job Ubuntu en échec, macOS annulé par la matrice.
- `ske/implementation`, commit `ab7b6bb`, run
  https://github.com/psyllos/ske/actions/runs/38900342322 :
  même échec confirmé dans le job Ubuntu `116970746398`.
- Test : `adapter_limits_memory_stops_allocating_child_before_timeout`,
  obtenu `E_RECORD_ORDER`, attendu `E_ADAPTER_LIMIT`.

## Cause et correction

Linux impose `RLIMIT_AS` via `prlimit` avant exec. L'allocation du pilote
Python peut échouer avant que le moniteur RSS n'observe un dépassement.
L'EOF sans `ListDone` remontait immédiatement comme erreur de protocole,
sans examiner la sortie non nulle du processus.

Le lecteur mémorise maintenant l'EOF, y compris au milieu d'une trame.
Après EOF, le lanceur attend le statut du processus avec le watchdog actif :
une sortie non nulle ou un dépassement surveillé donne `E_ADAPTER_LIMIT`.
Une sortie réussie conserve l'erreur de protocole initiale. Une trame invalide
sans EOF reste rejetée immédiatement, sans attendre la sortie du processus.
Les plafonds mémoire et assertions existants ne sont pas assouplis.

Trois régressions supplémentaires couvrent :

1. stdout fermé avant une sortie non nulle, réponse vide ou tronquée ;
2. trame invalide suivie d'un processus dormant : rejet immédiat ;
3. stdout fermé et processus dormant : attente bornée par le watchdog.

## Validation locale

macOS arm64, Rust 1.99.0, Go 1.27.2, Z3 système 5.1.0.

- Avant correction : nouvelle régression
  `cargo test -p ske-ingest --test adapter_ipc adapter_limits_failed_child_exit_takes_precedence_over_incomplete_response --locked`
  reproduit `E_RECORD_ORDER` au lieu de `E_ADAPTER_LIMIT` (exit 101).
- `cargo fmt --all -- --check` : PASS.
- `cargo test -p ske-ingest --locked` : 33 tests PASS, aucun ignoré,
  dont 18 tests IPC.
- `cargo clippy -p ske-ingest --all-targets -- -D warnings` : PASS.
- `cargo test --workspace --locked` : PASS, aucun échec ni test ignoré.
- `./scripts/verify.sh` : exit 0, dernière ligne `VERIFY OK v0.3` ;
  inclut Clippy workspace strict, génération protobuf sans diff, build,
  tests Go/Rust et cas de conformité symbolique.

Logs locaux : `target/validation-ci/{workspace,verify}.log`.

## Limites et suite

Le worktree attaché est basé sur `main` / `8004d2f`, pas sur le jalon v0.4
de la branche distante. Aucun changement v0.4 n'a été récupéré ou écrasé.
Pas de runtime Docker Linux disponible (daemon non démarré) ; la régression
d'ordre EOF/sortie est reproduite sur macOS, mais l'application effective de
`prlimit` reste à confirmer en CI Linux après publication autorisée du correctif.
Aucun push ni merge partagé effectué, workflow inchangé.
