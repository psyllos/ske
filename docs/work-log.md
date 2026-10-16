# Journal de reprise SKE

## Demande

Exécuter §24 de la spécification v1.4 ; déléguer ; conserver l'état pour ne pas
repartir de zéro après une limite de contexte. Commencer v0.1 puis seulement
après validation poursuivre v0.2 et v0.3.

## État initial

Le dépôt n'avait aucun commit et aucune implémentation. La spécification était
non suivie dans le checkout principal, donc absente du worktree Delta. Elle a
été copiée à l'identique dans le worktree ; le checkout principal n'a pas été modifié.

Outils observés : Rust/Cargo 1.99.0, Go 1.27.1, protoc 36.2, Z3 5.1.0,
macOS arm64. Versions Cargo vérifiées avec `cargo search`, figées dans le workspace.

## Contrats entre lots

- `ske_common::{Error::new(code, message), Result<T>, sha256_hex(bytes)}`.
- Types prost publics dans `ske_cir::proto`, messages du schéma exact §5.
- `ske_cir::registry` et validations locales ne dépendent pas du stockage.
- `ske_graph` expose un graphe indexé et dominance ; coordination par messages.
- `ske_storage::Storage` possède la connexion et migre à l'ouverture.
- `ske-ingest` importe dans la connexion du stockage ; SQL reste hors du CLI.
- `ske-query` est l'unique façade applicative du CLI.

## Validation

Aucun critère n'est encore PASS. Consigner les commandes réelles dans
`docs/validation/v0.1.md`. Pas de résultats simulés, pas de golden généré
depuis la sortie SKE. Les étapes non implémentées restent explicitement ouvertes.

## Threads délégués

- CIR : `ksQQUFcPQtXkTD6TTuMAxza-4pLOAAPF-pIBxBDZiRjivQ9OQqTS_tRBC-14`
- Stockage : `ksQQUFcPQtXkTD6TTuMAxza-4pLOAAPGK5IBxBDZiRjivQ9OQqTS_tRBC-14`
- Fixtures : `ksQQUFcPQtXkTD6TTuMAxza-4pLOAAPGPZIBxBDZiRjivQ9OQqTS_tRBC-14`
- Import/query/CLI : `ksQQUFcPQtXkTD6TTuMAxza-4pLOAAPGe5IBxBDZiRjivQ9OQqTS_tRBC-14`

## API graphes

`ske_graph::Graph::new(node_count, edges: IntoIterator<Item=(usize,usize)>)
-> Result<Graph,String>`. Méthodes `dfs(entry)`, `dominators(entry)`,
`back_edges(entry)`, `strongly_connected_components()`, `predecessors(node)`,
`successors(node)`. `Dominators::dominates(def_block,use_block)` et champ
`immediate: Vec<Option<usize>>`. Indices de blocs denses définis par l'appelant.
Dominance CHK ; bloc inatteignable sans dominateur. Cinq tests normatifs dédiés.

## Checkpoint d'intégration

- Migration et schémas protobuf comparés indépendamment octet à octet aux
  blocs normatifs : EXACT (32 082 / 8 729 / 1 480 octets).
- CIR : registre de 66 opcodes, 27 vecteurs machine ; correction de l'appel
  direct `validate_edges` sans opérande (plus de panic).
- SQLite : 12 tests de stockage réels, 47 tables runner inclus, 21 triggers.
- Premier workspace intégré : 7 counts, 34 négatifs, 4 corruptions, résolution,
  codes de sortie et pureté JSON passent. Clippy workspace a passé après
  correction CIR, avant dernière réécriture de l'import.
- Import réécrit par l'agent pour respecter §5.4 : réservation des IDs sur
  référence, marquage declared, insertion directe en streaming ; les corps
  protobuf TEMP servent uniquement à la validation par fonction et dérivation.
  Le coordinateur n'a pas encore revalidé ce dernier snapshot.
- `--big` corrigé à 200 000 instructions exactes ; pas de grand fichier committé.
- `scripts/validate-v01.py` exécute les 42 AC et sauvegarde commandes, exits et
  sorties brutes dans `target/validation-v01/`. Son exit n'est pas une
  dérogation à la revue ni une approbation d'écart architectural.
- `scripts/verify.sh` inclut explicitement les tests de l'exemple générateur.
- Le coordinateur a ajouté un oracle indépendant pour dominance :
  tous les 4 096 graphes dirigés à 4 sommets, chaque point d'entrée ; PASS.

### Attention intégration en cours

La fusion de la réécriture import avec le formatage coordinateur a produit
des marqueurs de conflit dans `ske-ingest/src/{lib,insert,validate}.rs` et
`ske-query/src/lib.rs`. L'agent import en est propriétaire et résout ces
conflits en conservant sa nouvelle sémantique, puis renverra un checkpoint
compilable. Ne pas lancer la validation finale ni déclarer v0.1 terminé tant
que ces conflits ne sont pas résolus et tous les contrôles exécutés.

### Autres threads

- Documentation terminée : `ksQQUFcPQtXkTD6TTuMAxza-4pLOAARVHZIBxBDZiRjivQ9OQqTS_tRBC-14`
- Revue sûreté : `ksQQUFcPQtXkTD6TTuMAxza-4pLOAASfIpIBxBDZiRjivQ9OQqTS_tRBC-14`

## v0.1 validée — passage à v0.2

Le checkpoint précédent est historique : les conflits sont résolus. Les
correctifs de sûreté CIR sont intégrés, avec tests de régression. Dernière
exécution `python3 scripts/validate-v01.py` : **42 PASS, exit 0**.
`./scripts/verify.sh` termine par `VERIFY OK v0.1`. Rapport durable complet
dans `docs/validation/v0.1.md` ; logs locaux dans `target/validation-v01/`.
103 tests workspace + 4 tests exemple passent, aucun ignoré.
Grand flux : 200 000 instructions, 6,82 s, RSS 68 812 800 octets sous macOS
(`/usr/bin/time -l`, distinct de Linux `-v`).

La commande AC38 exacte est exécutée sous zsh sur macOS, car Bash 3.2
ne conserve pas les zéros de l'expansion `01..19`. Aucun faux ADR ajouté.

Prochaine étape : v0.2 seulement (adaptateur Go, IPC sûr, conformité Go).
Ne pas commencer v0.3 avant les 25 AC v0.2 PASS. Aucun tag/push partagé fait.

## Checkpoint Git local

Branche dédiée `ske/implementation`, commit racine `f18e6e8` :
`Implement validated SKE v0.1 foundation` (228 fichiers).
Cette branche n'est pas poussée ; aucun merge dans le `main` principal.

## Threads v0.2

- Adaptateur Go : `ksQQUFcPQtXkTD6TTuMAxza-4pLOAAUv3pIBxBDZiRjivQ9OQqTS_tRBC-14`
- IPC Rust/CLI : `ksQQUFcPQtXkTD6TTuMAxza-4pLOAAU1YZIBxBDZiRjivQ9OQqTS_tRBC-14`
- Conformité Go indépendante : `ksQQUFcPQtXkTD6TTuMAxza-4pLOAAVDy5IBxBDZiRjivQ9OQqTS_tRBC-14`

Bootstrap adaptateur annoncé : module `ske/adapters/go`, imports
`ske/adapters/go/gen/{cir,adapter}/v1`, script `scripts/gen-proto-go.sh`
avec M mappings, binaire `target/ske-adapter-go`. Protocoles exacts du schéma.
IPC prévu : `ske_ingest::adapter::{Limits,list_units,extract_unit}` et
`ingest_go(storage,repo,profile_json,adapter,limits,repository)`.
Ces contrats doivent être confirmés après fusion, pas traités comme code livré.

## Dernier état — interruption fournisseur v0.2

L'agent Go a reçu l'erreur réelle `Your ChatGPT subscription has reached its
usage limits`. Aucun remplacement de modèle/fournisseur/agent n'a été tenté.
Rapport : `docs/validation/BLOCKED-v0.2-quota.md`.

Sauvegarde principale :

- Bootstrap Go + protobuf figé et AST décisions/compteur + SSA lower/calls
  récupérés indirectement via checkpoint Go
  `d98918e2-bd0f-4e42-a4d2-fed4410bed78.359401`.
- `go test ./...`, `go vet ./...`, contrôle gofmt sur l'adaptateur partiel : PASS.
- `cargo test --workspace --locked` après sauvegarde : PASS.
- Il manque encore l'intégration `internal/link`, `internal/emit`,
  `cmd/ske-adapter-go`, l'IPC Rust complet et les tests E2E.
- Les agents IPC et conformité ont reçu une demande de checkpoint. Leurs
  rapports suivants doivent être capturés sans supposer leur travail perdu.
- Sous-sous-agents Go découverts via `delta_thread tree` :
  AST `ksQQUFcPQtXkTD6TTuMAxza-4pLOAAU1k5IBxBDZiRjivQ9OQqTS_tRBC-14` ;
  SSA `ksQQUFcPQtXkTD6TTuMAxza-4pLOAAVDcZIBxBDZiRjivQ9OQqTS_tRBC-14`.
  Fusion directe non reconnue par le runtime local, récupération indirecte
  déjà faite pour leur checkpoint disponible dans le parent Go.

La prochaine session doit lire `AGENTS.md` et ce dernier état, reprendre v0.2,
pas recommencer v0.1. Harness `scripts/validate-v02.py` préparé mais non exécuté.
Le tag `cir-v1-candidate` n'est pas créé tant que le checkpoint Go n'est pas prêt.

### Récupération finale des lots interrompus

IPC Rust/CLI et neuf suites de conformité Go sont maintenant fusionnés :
rapports `worker-go-ipc.md` et `worker-go-tests.md`.
Compilation de toutes les suites PASS, Clippy workspace strict PASS,
58 tests CLI v0.1 PASS et 11 tests ingest PASS.
Le premier E2E Go a été exécuté et **échoue explicitement** :
`cargo test -p ske-cli --test go_golden_decisions`, exit 101,
`adapters/go/cmd/ske-adapter-go` absent. Aucun skip ni faux PASS.

Priorités de reprise :

1. Intégrer le vrai binaire Go avec lower + décisions + link + emit.
2. Régler le contrat `nil` des types Go nilables sans faux cast POINTER.
3. Compléter les tests IPC mockchild, consent/go_failures, limites/redact.
4. Exécuter toutes les suites Go, corriger les vrais écarts aux golden inchangés.
5. Finir verify/génération/tag et les 25 AC ; ensuite seulement v0.3.

Le commit v0.1 reste le point vert. Le checkpoint v0.2 sauvegardé contient
des tests rouges connus ; ne pas présenter le workspace courant comme terminé.

### Tout dernier delta Go

Le binaire Go + emit/link ont finalement été récupérés du parent Go :
version `8250d844-05ca-41ca-9172-b50628e47a90.366982`.
L'E2E `go_golden_decisions` échoue maintenant sur un vrai diagnostic :
`E_RECORD_ORDER: unit descriptors must be unique and sorted`.
Le tri Rust compare la clé brute, contrairement au tri par import path §9.3
et l'ordre exact AC-0.2-04. C'est le premier correctif de reprise avec test.
Le binaire n'est donc plus manquant ; les priorités précédentes sont
historiques, à ajuster avec cette dernière observation.

## Reprise demandée après le checkpoint 9cc8436

v0.1 n'est pas réimplémentée. Reprise depuis un arbre Git propre.

- Régression ajoutée avec un vrai processus adaptateur :
  `cargo test -p ske-ingest adapter_list_units`.
  FAIL reproduit avant correction, PASS après correction du tri par
  `(import_path, suffixe de build)` plutôt que clé brute. Les trois unités
  sont vérifiées exactement et dans l'ordre prescrit.
- `ske_common::sha256_reader` ajouté pour hasher sources/binaires avec mémoire
  bornée : erreurs I/O préservées, lectures courtes et interrompues testées
  (2 tests PASS).
- L'E2E `cargo test -p ske-cli --test go_golden_decisions` échoue encore,
  mais plus sur la découverte : racine FAILED pour symbole dupliqué
  `strings.(*Replacer).buildOnce$bound`, partial FAILED pour dominance SSA
  `example/partial.Safe/v1`, tags COMPLETE.
- Diagnostic conservé dans `target/resume-v02/check.sqlite` (artefact local).

Lots parallèles repris :

- Go/CIR/SSA/mappings : `ksQQUFcPQtXkTD6TTuMAxza-4pLOAAW6zJIBxBDZiRjivQ9OQqTS_tRBC-14`
- IPC/consentement/limites/tests : `ksQQUFcPQtXkTD6TTuMAxza-4pLOAAW2dZIBxBDZiRjivQ9OQqTS_tRBC-14`

Le coordinateur garde tri/intégration/scripts ; ne pas refaire les lots en cours.
v0.2 reste non validée, v0.3 non commencée.

## Correction des six défauts de revue, à la demande du commanditaire

- Nouveaux tests ajoutés avant correction : les six défauts sont reproduits.
- IndexAddr sur *array : site nil inventorié au producteur de l'adresse.
- DNF : énumération paresseuse bornée à 4096, sans produit intermédiaire
  exponentiel ni DNF lors de l'appariement des atomes ; attestation incomplète
  à la limite, inventaire conservé et ordinaux des cas contigus.
- Nil : cœur strict selon §4.4, pas de confiance dans un préfixe libre ;
  valeurs Go hors contrat explicitement opaques et typées (ADR-020).
- Parité des négations composée conservée dans les liens source→IR.
- Range canal apparié au comma-ok du receive du header, pas à un receive du corps.
- Diagnostic SOURCE_ACTIVATION_INCOMPLETE INFO/SYMBOLIC.

Tests ciblés PASS, y compris ingestion SQLite de 8192 alternatives théoriques
plafonnées à 4096 et tous les types nilables conservés sans faux pointeur.
Détail : `docs/validation/review-v0.2-fixes.md`.
La validation globale des 25 AC reste à relancer entièrement ; pas de v0.3.

## v0.2 terminée et validée — état à reprendre

À la demande explicite de correction puis de validation, les six défauts de la
revue ont été corrigés avec leurs tests, sans réimplémenter v0.1.
Rapport détaillé : `docs/validation/review-v0.2-fixes.md`.

- Dernière exécution `python3 scripts/validate-v02.py` : **25 PASS, exit 0**,
  260,2 secondes, sans interruption.
- Régression incluse : **42 AC v0.1 PASS**, **143 tests Rust**, **0 ignoré**.
- `./scripts/verify.sh` termine par **VERIFY OK v0.2**.
- Toutes les commandes et preuves sont dans `docs/validation/v0.2.md` ;
  logs locaux développés dans `target/validation-v02/` et `validation-v01/`.
- Premier passage complet : 24/25, timeout de `go tool dist list` à 5 s ;
  budget de requête explicite porté à 30 s, test de timeout court conservé,
  puis validation complète intégralement relancée et réussie.
- ADR-020 documente la frontière nil : cœur strict, nil Go hors contrat
  représentés explicitement par SSA opaque, types réels conservés.
- Aucun schéma, migration, source du laboratoire ou golden normatif changé.
- Branche locale `ske/implementation`, candidat CIR local existant ;
  aucun push, aucun merge partagé, aucun changement de tag existant.

v0.3 reste **non commencée**. La demande présente se termine sur la livraison
v0.2 validée ; ne pas refaire les jalons précédents à la prochaine reprise.

## Reprise v0.3 depuis b65786a

État réel initial du worktree attaché : `main`, `d3f5f0d`, arbre propre,
seule la spécification suivie. `AGENTS.md` absent à ce HEAD.
Après inspection des branches/remotes et vérification de l'objet `b65786a`,
`git switch -c ske/implementation b65786a` a restauré le checkpoint demandé.
Le dépôt primaire indiqué par `local` n'a été consulté qu'en lecture
(`for-each-ref`) ; sa branche `main` pointe ailleurs. Pas de push/merge partagé.

Lectures : AGENTS, état durable, dernière section du journal, rapports v0.1,
v0.2, six corrections de revue et BLOCKED historique résolu ; §24 v0.3 et §6.
v0.1/v0.2 ne sont pas réimplémentées. Leurs PASS restent des résultats
historiques jusqu'à la nouvelle non-régression.

### Lots v0.3 et propriété des fichiers

- Solveur : `ksQQNDW_ffujR6OqNqJn4MJ-PJLOAAZ8MJIBxBDZiRjivQ9OQqTS_tRBC-14`
  (`ske-solver`, installation Z3).
- Expressions : `ksQQNDW_ffujR6OqNqJn4MJ-PJLOAAZ6L5IBxBDZiRjivQ9OQqTS_tRBC-14`
  (`ske-symbolic` arène, lib/Cargo).
- Mémoire : `ksQQNDW_ffujR6OqNqJn4MJ-PJLOAAaVVZIBxBDZiRjivQ9OQqTS_tRBC-14`
  (`ske-symbolic/src/memory.rs`).
- Transferts/exploration : `ksQQNDW_ffujR6OqNqJn4MJ-PJLOAAabNZIBxBDZiRjivQ9OQqTS_tRBC-14`
  (moteur pur et DTO, sans SQL).
- Vérification indépendante : `ksQQNDW_ffujR6OqNqJn4MJ-PJLOAAaEtZIBxBDZiRjivQ9OQqTS_tRBC-14`
  (`ske-verify`, sans dépendance au classifieur).
- Tests d'acceptation indépendants :
  `ksQQNDW_ffujR6OqNqJn4MJ-PJLOAAanvJIBxBDZiRjivQ9OQqTS_tRBC-14`.
- Coordinateur : chargement/persistance SQL, façade Query, CLI, intégration,
  harness de validation et documentation de reprise.

### Contrats retenus

Respecter le graphe de dépendances existant : `ske-solver` ne dépend pas de
`ske-symbolic`. Le solveur accepte des requêtes SMT-LIB complètes ; son encodeur
indépendant fournit les primitives typées. L'arène reste dans `ske-symbolic`.
Le moteur expose des DTO purs ; SQL et orchestration restent côté applicatif.
Le vérificateur relit SQL et rejoue toutes les requêtes UNSAT, sans réutiliser
le classifieur et sans persister les rejeux.

État : implémentation en cours, **aucun AC v0.3 PASS**. Prochaines étapes :
intégrer les contrats livrés, exécuter les tests ciblés, corriger, puis seulement
les 29 AC et la non-régression complète. Rapport : `docs/validation/v0.3.md`.

## v0.3 validée — dossier de la reprise

Intégration des lots délégués (solveur, arène, mémoire, moteur, vérificateur,
tests d'acceptation, lecture SQL) dans la branche `ske/implementation` reprise
depuis b65786a. v0.1 et v0.2 n'ont pas été réimplémentées ; leurs fixtures,
schémas, migrations et golden normatifs sont inchangés.

### Corrections d'intégration et de revue

- Type (défauts de revue indépendante) :
  1. `MemoryState::load` ne panique plus sur un rechargement de même adresse
     SSA avec des types différents (BITCAST/union) : lecture fraîche typée
     (représenté en opaques), pas de run laissé RUNNING. Test nommé
     `memory_type_mismatch_reload_never_panics`.
  2. Les CONST non-machine (`StringBytes`) et MachineBitsBe non entiers deviennent
     HAVOC avec dégradation OPAQUE de l'état initial + diagnostic
     `OPAQUE_OP_EMITTED`, au lieu d'une sortie EXACT ou d'une erreur de validation
     d'arène. Tests `string_constant_degrades_to_opaque_and_emits_diagnostic`,
     `float_machine_constant_is_opaque_not_error`.
  3. `may_mutate_memory=false`/`may_raise=false` opposés dans CALL/INVOKE
     enregistrent l'hypothèse `ADAPTER_EFFECT_CLAIMS_TRUSTED`.
  4. `cut_count` au déclenchement de budget = taille de la worklist (§6.5), sans
     double +1.
  5. M1 : un STORE à cible inconnue conserve l'écriture dans le journal, relisible
     par la même référence SSA (cohérence d'identité §6.9), les autres alias
     restant gardés. Test `memory_same_reference_visible_under_unknown_store`.

- Annulation robuste : Z3 n'honore `timeout` qu'aux frontières de tactiques et
  peut dépasser fortement sous charge/rewriting. Ajout de
  `Solver::check_interruptible`, override `Z3Solver` avec un thread de garde
  (scoped) qui appelle `ContextHandle::interrupt` dès que le drapeau
  d'annulation (Sync) est posé ou que le budget mural s'écoule ; ceci rend SIGINT
  immédiat même bloqué dans un solve long. Le moteur et `analysis.rs` passent le
  prédicat `&(dyn Fn() -> bool + Sync)` (les drapeaux internes sont des atomiques).
  Tests : `cancel_interrupts_solver_promptly` (solveur) et
  `cancel_sigint_preserves_streamed_arrivals` (CLI E2E, arrêts avant/pendant).

- Ordonnancement final : en cas d'annulation, la persistance des `branch_cases`
  (témoins/gardes) et la vérification des candidats sont sautées ; les arrivées et
  POSSIBLE écrites au fil de l'eau sont conservées, le run passe `CANCELLED` et le
  processus sort 130 promptement.

### Validation finale

`python3 scripts/validate-v03.py` : **29 AC PASS, exit 0** (dernière exécution
complète). `./scripts/verify.sh` termine par `VERIFY OK v0.3` (étapes 8-9 :
SAT/UNSAT réels, UNKNOWN/TIMEOUT scriptés, PARTIAL jamais COMPLETE).
Non-régression : `SKE_EXPECT_VERIFY_VERSION=v0.3 python3 scripts/validate-v02.py`
**25/25 PASS** (AC-0.2-22 verify v0.3, AC-0.2-23 régression v0.1 42/42).
`cargo test --workspace` : vert, 0 ignoré. Clippy `-D warnings`, fmt, check-deps
et gen-proto : OK.

Z3 système requis : `brew install z3 pkg-config` / `apt install libz3-dev z3
pkg-config` ; procédure détaillée dans `docs/dependencies.md` et
`docs/worker-solver.md` (spike réel sur libz3 5.1.0 macOS arm64).

Rapport avec les commandes et logs :
`docs/validation/v0.3.md` + `target/validation-v03/`.
Pas de commit, push, merge partagé ni changement de tag. Prochain jalon : v0.4,
non commencé. CI distante non exécutée.

# v0.4 — analyse interprocédurale — reprise

## Reprise depuis 8004d2f (le checkpoint v0.1→v0.3 squashe par le commanditaire)

État réel initial : `main` à `8004d2f`, arbre propre, seul commit applicatif
`8004d2f` (« Implement and validate SKE v0.1 through v0.3 »). Le commanditaire
confirme v0.1, v0.2 et v0.3 validées ; le contenu de v0.1→v0.3 dans ce dépôt
chargé se résume à ce commit applicatif (les checkpoints historiques
af0b/9cc8436/b65786a cités par les journaux ne sont pas présents dans ce clone).

Branche de travail : `ske/implementation` créée localement à partir de 8004d2f,
non poussée ; aucun merge dans `main`. Tag local `cir-v1-candidate` présent,
non modifié. Aucun push.

Lectures : AGENTS.md (état corrigé : v0.3 validée), état durable,
dernière section du journal (v0.3 validée), §24, §19 v0.4, §7.1, §8 schéma
(summary_* déjà dans 0001_initial.sql, aucune migration nécessaire),
§2.1-2.2 (arborescence et règles de dépendance : crates `ske-abstract` et
`ske-summary` prévues, whitelist check-deps déjà prête).
v0.1/v0.2/v0.3 ne sont pas réimplémentées.

Baseline : `cargo test --workspace` vert. `cancel_sigint` a flaky sous charge
et passe en isolation ; pas un échec réel.

## Fixture v0.4 et état Git

- `examples/go-branch-lab/interproc.go` ajouté (contenu exact §19 v0.4,
  sans ré-écrire `DivMod`/`ErrZero` qui existent dans `two_returns.go`).
  `gofmt`, `go build`, `go vet` PASS sur le module.
- Les tables de persistance v0.4 existent déjà dans `0001_initial.sql` :
  `call_sites`, `call_targets`, `function_summaries`, `summary_cases`,
  `summary_case_returns`, `summary_case_outcomes`, `observable_effects`.
  `call_sites`/`call_targets` sont déjà peuplées par l'ingestion Go (v0.2).
- Budget moteur et config disposent déjà de `loop_mode` et
  `max_recursion_unfold` ; `analysis_config.rs` refuse encore
  `loop_mode != "unroll"` (levée en T-0.4-05).

## Lots v0.4 et propriété des fichiers

- Lot A — `ske-abstract` (intervalles, T-0.4-05) : `crates/ske-abstract/` seul.
- Lot B — chemins statiques + graphe d'appels (T-0.4-01/07) : `ske-query`
  (methodes paths), `ske-cli` (commande `paths explore`), test CLI `callgraph`.
- Lot C — interpréteur concret (T-0.4-06) : `ske-symbolic/src/concrete.rs` +
  tests du module.
- Coordinateur — résumés moteur (T-0.4-02/03/04), mode intervalles dans le
  moteur, persistance summary_* dans `ske-query`, `ske-summary` si nécessaire,
  tests AC, `verify.sh`, ADR-021, validation.

Contrats retenus — mêmes règles que v0.3 : DTO purs dans les crates pur,
SQL uniquement dans ske-query, vérificateur relit SQL sans réutiliser le
classifieur. ADR-021 détaille la clé de cache des résumés et l'algorithme
des intervalles (points ★ du §19 v0.4).

## État

Implémentation en cours, **aucun AC v0.4 PASS**. Aucun push, aucun merge
partagé, aucun tag modifié. Prochaine étape : déléguer les lots indépendants
puis intégrer le moteur des résumés.

### Threads v0.4 délégués

- Lot A — ske-abstract (intervalles) :
  `ksQQ9uo16AT0RrqL7JdWZgoH9ZLOAAlbU5IBxBDZiRjivQ9OQqTS_tRBC-14`
- Lot B — interpréteur concret :
  `ksQQ9uo16AT0RrqL7JdWZgoH9ZLOAAlbXZIBxBDZiRjivQ9OQqTS_tRBC-14`
- Lot C — chemins statiques + graphe d'appels :
  `ksQQ9uo16AT0RrqL7JdWZgoH9ZLOAAlbZJIBxBDZiRjivQ9OQqTS_tRBC-14`

Coordinateur : intégration du moteur des résumés (T-0.4-02/03/04), mode
intervalles dans le moteur, `ske-summary`, persistance summary_* dans
ske-query, tests AC, `verify.sh`, validation. Fusion des lots via
`merge_worktrees` après leurs rapports.

### Plan moteur des résumés (lot D, après fusion A/B/C)

Le moteur v0.3 havogue tout `CALL`. Pour v0.4 il faut (contrats relus) :

- DTO purs `ske-symbolic::summary` (ou crate `ske-summary`) : `FunctionSummary`
  (function_id, context_fingerprint, summary_fingerprint, precision, complete,
  cases[], assumptions, unknowns) et `SummaryCase` (case_ordinal,
  guard: Vec<ExprId>, returns: Vec<ExprId> UNE par valeur de retour,
  exit_kind, memory_delta, effects, referenced_outcomes, precision).
- Calcul : explorer la fonction avec params symboliques (déjà fait par
  `analyze`), chaque `record_exit` ⇒ un cas (pas de fusion en v0.4, §7.1).
  Les retours multiples : le RESULT d'un CALL à multi-retours est un tuple ;
  `ssa.Extract` devient `EXTRACT_FIELD` (index 0, 1, …) — vérifié dans
  `adapters/go/internal/lower/instructions.go`. La liaison au site d'appel
  suit §7.1 : tuple ⇒ `EXTRACT_FIELD` sur la liste.
- Application aux sites d'appel : substitution mémoïsée param→arguments sur
  le DAG (`ExprArena`), forker l'état par cas dont `contraintes ∧ garde’` est
  `Sat` (`Unknown` ⇒ garder + dégrader soundness), ajouter les effets, lier
  les returns au résultat, `referenced_outcomes` = issues IR du cas.
  Cible inconnue ⇒ havoc (§6.10).
- Récursion : SCC (Tarjan) résumés bas-en-haut ; `max_recursion_unfold=2`
  dépliages puis cas résiduel `PARTIAL`, `unknowns += RECURSION_BOUND`.
- Clé de cache §7.1/ADR-021 : `sha256_hex(structural|semantic|context|
  callees triés|modèles|config)`.
- Persistance : `function_summaries` + `summary_cases` + `summary_case_returns`
  + `summary_case_outcomes` + `observable_effects` (tables déjà présentes dans
  `0001_initial.sql`) ; `analysis_runs.analysis_kind='SUMMARY'`.
- Tests AC cibles : summary_entry, entry_precondition, summary_calldoer,
  summary_fact, summary_divmod, summary_divide2, summary_matches_concrete
  (avec interpréteur concret lot B), summary_deterministic ;
  analyse_sum_intervals (avec ske-abstract lot A).

