# Checkpoint T08 — Rust Go IPC, CLI et query

Statut : **bootstrap compilable, NON VALIDÉ v0.2**. Arrêt demandé par le
parent après épuisement du quota de l'agent Go. Aucun travail v0.3 commencé.

## Implémenté dans ce fork

- `ske-ingest::adapter::{RunLimits,list_units,extract_unit}` : vrais processus,
  une requête protobuf `AdapterRequest` encadrée u32BE, fermeture stdin,
  réponses `AdapterResponse`. Extraction streaming via callback `Record`.
- Taille et nombre de messages contrôlés avant allocation, ordre des réponses
  contrôlé ; découverte limitée à 100000 unités et 16 MiB de descripteurs.
- Watchdog timeout et RSS du groupe ; groupe Unix dédié, SIGKILL du groupe,
  kill/wait de l'enfant, drainage stderr avec conservation maximale de 64 KiB.
  Sur Linux, `prlimit --as` ajoute une limite dure avant exec.
- Environnement adaptateur filtré (PATH/HOME/GOCACHE/GOMODCACHE/GOROOT),
  paramètres Go de sûreté explicites. Aucun build du code étudié ajouté.
- Importer CIR factorisé en source de records ; les fichiers CIR gardent la
  transaction globale et les savepoints existants. Go crée le snapshot puis
  une transaction par unité, rollback avant persistance FAILED/UNIT_FAILED
  séparée, poursuit les autres unités ; clôture/fingerprints/stubs partagés.
- `GoOptions` et `Query::ingest_go`, CLI `ske ingest go --repo DIR
  --profile FILE --adapter target/ske-adapter-go --yes-run-adapter
  --max-message-bytes N --max-messages N --timeout-ms N --max-memory-mb N
  --redact --require-complete`. Profil optionnel avec défauts §11.4,
  contrôle strict des clés/types et cible via `go tool dist list`, JSON canonique.
- Consentement sous `<parent DB>/.ske/consent.json`, repository et adaptateur
  absolus, empreinte binaire comme version, date Unix ; refus sans terminal
  sans consentement existant ni `--yes-run-adapter`.

## Validation réellement exécutée

Sur macOS, dans ce fork :

```sh
cargo check -p ske-cli -p ske-ingest -p ske-query
cargo test -p ske-ingest -p ske-cli -p ske-query
cargo clippy -p ske-ingest -p ske-cli -p ske-query --all-targets -- -D warnings
```

Tous exit 0. Tests : CLI existants 58, ingest 11 (dont deux nouveaux tests
profil), query 0. Les tests existants couvrent notamment le rollback global
CIR, les fingerprints et 34 erreurs négatives. Ceci **ne valide pas**
l'ingestion Go ni les nouveaux processus enfants : aucun mockchild ou
workflow adaptateur réel n'a été exécuté.

Formatage ciblé des fichiers de ces trois crates effectué. Aucun changement
root Cargo/lock, schéma, migration, fixture ou golden.

## Reprise obligatoire avant acceptation

1. Fusionner le bootstrap dans le parent et coordonner le Harness T09 :
   `--profile FILE` confirmé ; déterminisme à tester sur flux adaptateur brut.
   Les messages directs au thread T09 ont échoué (« no thread known »).
2. Ajouter les suites Rust IPC mockchild demandées : `adapter_list_units`,
   `adapter_limits`, `source_hash_`, CLI `consent` et `go_failures`.
   Ne pas annoncer ces filtres PASS : ils n'existent pas encore.
3. Revoir/corriger les limites avant production :
   - macOS RSS est échantillonné via `/bin/ps`, donc pas une limite dure
     sans dépassement transitoire ; échec de ps donne actuellement zéro.
   - watchdog 5 ms plus coût de ps ; message timeout générique pour RSS aussi.
   - `go tool dist list` de validation profil utilise `Command::output`
     sans timeout/cap dédié ; profil/consentement lus entièrement.
   - le collecteur stderr garde 64 KiB mais ne les expose pas ; détails
     AdapterError conservés dans UNIT_FAILED sauf `--redact`.
4. Vérifier que les chemins source découverts sont normalisés ; l'import CIR
   garde sa validation existante et vérifie les hashes, mais lit actuellement
   chaque fichier source entier pour son SHA-256. Ajouter tests symlinks et
   changements de source entre extraction/import.
5. Déplacer/renforcer les validations d'options avant création de snapshot :
   `RunLimits` invalide atteint actuellement le snapshot puis FAILED plutôt
   qu'un E_USAGE direct ; l'API Go ne normalise pas encore profile_json
   (la CLI le fait).
6. Tester cycle CLOSED/ABORTED, transactions FK différées, erreurs après
   EndUnit et exit enfant nonzero, stubs, fingerprints incluant FAILED,
   absence de rows structurelles résiduelles et `ske db check`.
   L'unité FAILED utilise actuellement toujours les sentinelles même si le
   Header a déjà été reçu (pas encore conservation des métadonnées connues).
7. Consentement à durcir contre courses/symlinks et taille excessive ; hash
   adaptateur est une identité exacte mais pas sa version sémantique déclarée.
8. `--redact` masque les messages UNIT_FAILED, pas encore les diagnostics
   réussis importés : préciser/durcir ce contrat.
9. Après disponibilité du vrai `target/ske-adapter-go`, exécuter commandes
   exactes AC-0.2 et workflow réel, jamais remplacer par une extraction simulée.

Les 25 AC v0.2 restent à évaluer par le parent ; aucune autorisation de passer
à v0.3. Pas de tag, commit de landing ou merge Git réalisé par ce worker.

## Reprise depuis `9cc8436` — IPC et garanties testées

Ce supplément remplace les points ouverts ci-dessus lorsqu'ils sont
explicitement couverts ici. v0.1 n'a pas été réimplémentée ; v0.3 non commencée.
Le tri `list_units` et son test réel `discovery_order.rs` viennent du parent,
fusionnés avec son `sha256_reader` (frontière parent
`ed2514f6-52e5-4283-8c42-3e74553f5129.382712`). Aucun changement du tri par ce lot.

### Changements

- Validation positive et débordements des limites avant snapshot ; profil
  normalisé aussi à la frontière API Go, pas seulement dans la CLI.
- Requêtes IPC bornées, vérification des réponses restant après le callback
  d'extraction ; conservation du contrôle d'exit après EOF (un exit nonzero
  ou une erreur tardive ne valide jamais une unité).
- RSS du groupe échantillonnée toutes les 100 ms, échec de `ps` désormais
  fail-closed plutôt que RSS zéro. L'intervalle initial 5 ms provoquait une
  tempête de processus sous tests parallèles : tests explicitement rouges
  avant correction, pas de skip. Timeout et mémoire ont un diagnostic commun
  honnête `adapter time/memory monitoring limit exceeded`.
- Profil JSON/consentement plafonnés à 1 MiB pendant la lecture ; validation
  `go tool dist list` plafonnée à 64 KiB et 5 secondes, stdin fermé, stderr
  ignoré, environnement filtré, arrêt du groupe et reap. CLI utilise
  `<repo>/build-profile.json` si `--profile` absent et fichier présent.
- Hashes sources et binaire adaptateur calculés en streaming (64 KiB via
  `ske-common` du parent). Sources canoniques refusées hors racine ; fichiers
  non réguliers refusés. Pas de nouvelles dépendances.
- Consentement sérialisé par verrou `create_new`, refus des symlinks directs
  du store ou de `.ske`, temporaire unique `create_new`, mode Unix 0600,
  sync puis rename ; binaire modifié ⇒ consentement à renouveler.
- Échec d'unité après Header valide : conservation de l'identité adaptateur/
  toolchain/profil connue après rollback, sans prétendre la connaître avant
  Header. Diagnostics d'adaptateur, y compris `UNIT_FAILED` émis par Go,
  redacted à la frontière d'import. Les diagnostics dérivés sont masqués avant
  clôture ; le code stable d'erreur des échecs du cœur reste conservé.
  Les erreurs CLI d'`ingest go --redact` masquent détails et candidats.

### Ce que les tests prouvent

`adapter_ipc.rs` utilise de vrais enfants shell avec frames protobuf préparées
par le test ; les cas de replay sont des fixtures de protocole, **pas une
extraction Go simulée revendiquée comme réelle**. Couverture :

- requête unique puis EOF, version, limites, racine absolue et environnement ;
- ordre de réponses, absence/double ListDone, mauvais body, AdapterError ;
- taille stdout > 1000 avant allocation, nombre de frames, flux tronqués,
  frame vide, exit nonzero, réponse/erreur restant après callback ;
- 4 MiB stderr drainés sans polluer stdout, timeout avec descendant conservant
  les pipes, arrêt mémoire avant timeout (Python 3, témoin d'exécution vérifié,
  allocation réelle 128 MiB sous limite 64 MiB) ;
- unité FAILED sans résidu structurel, erreur tardive après EndUnit avec
  rollback, hash modifié/absent entre fixture et import, fuite symlink hors
  racine, identité Header préservée, `db check` ;
- redaction des diagnostics d'un flux réussi et empreintes inchangées ;
  limites/profil invalides sans snapshot ni enfant lancé.

`consent.rs` teste CLI non interactive exit 2 sans exécution, `--yes` écrit
identité/date/hash, réutilisation, modification binaire, store trop gros et
symlink sans écriture sur la cible.

`go_failures.rs` construit **le vrai adaptateur Go**, et ne compile/exécute
jamais les sources étudiées : projet de confiance temporaire avec package
valide et package en erreur de typage. Il vérifie patterns absents ⇒
ABORTED/exit 3 ; sain + échoué ⇒ CLOSED/exit 0 ; `--require-complete` ⇒ 4 ;
diagnostic FAILED, redaction du chemin réellement émis par Go, `db check`.

### Commandes de validation

Commandes exactes exécutées sur macOS arm64 pour ce lot, toutes exit 0
après corrections (aucun test ignoré) :

```sh
cargo test -p ske-ingest adapter_list_units
cargo test -p ske-ingest adapter_limits
cargo test -p ske-ingest source_hash_
cargo test -p ske-cli --test consent
cargo test -p ske-cli --test go_failures
cargo test -p ske-ingest
cargo clippy -p ske-ingest -p ske-cli --all-targets -- -D warnings
```

Résultats : filtre découverte 3 tests dont le test réel du parent ; limites
8 tests ; hash 3 tests ; consentement 2 tests ; Go failures réel 2 tests.
Suite ingest complète : 29 tests (7 unitaires, 15 IPC, 1 découverte réelle,
6 import), tous PASS. Les tests de timeout bornent lecture, attente après EOF
et écriture stdin bloquée. Une régression réelle de redaction sur le
`UNIT_FAILED` émis par Go a été observée puis corrigée à la frontière d'import ;
le test CLI conserve l'assertion sur le chemin absolu.

Formatage et `rustfmt --check` ciblés avec `--edition 2021 --config
skip_children=true` sur les seuls fichiers possédés par ce lot ; pas de
`cargo fmt --all`. `git diff --check` également exécuté.

### Limites restantes explicites

- Aucune certification globale des 25 AC, aucun `verify.sh` ou workspace E2E
  complet exécuté par ce lot. AC04 réel est le test du parent, pas celui du
  mockchild. L'intégration extraction/golden Go reste à son propriétaire.
- Limite RSS macOS échantillonnée, pas plafond dur : dépassement transitoire
  possible. Linux `prlimit --as` conservé mais non exécuté ici ; limite
  d'adresse par processus distincte de RSS agrégée du groupe.
- Ce n'est pas un sandbox contre un adaptateur hostile qui fait `setsid`
  ou quitte son groupe. Les tests Unix nécessitent shell, ps/kill/dd et
  Python 3 pour la pression mémoire ; aucun test ignoré sur la machine validée.
- Les chemins canoniques et refus de symlinks protègent les liens statiques,
  pas les remplacements concurrents par un acteur ayant accès en écriture
  aux répertoires. Pas de garantie `openat`/descripteurs pinés ni d'exec du
  binaire par descripteur ; course hash consentement→exec reste explicite.
- Le verrou protège des écrivains coopérants. Un crash peut laisser
  `consent.lock` ; vérifier l'absence d'écrivain puis le retirer manuellement.
- `--redact` masque les diagnostics/erreurs de ce workflow, pas les chemins
  stockés de provenance ou le consentement local. Le diagnostic est moins
  détaillé ; les empreintes restent identiques.
- Aucun schéma, migration, golden, root Cargo/lock, push, tag ou landing modifié.
