# Réconciliation ciblée v0.5 sur main

## Périmètre autorisé

Après `/land`, détection de deux implémentations v0.5 indépendantes.
Le commanditaire demande de conserver l'architecture de `main` et de ne
porter que les corrections manquantes, puis de valider le candidat et sa CI.

Base : `72e1db9b77aabc74ae236f30b34302cb6619cf42`.
Checkpoint indépendant conservé localement : `5e75a1f`.
Branche d'intégration : `ske/v05-main-fixes`.

Conservés : modules/API `behavior.rs`, runs BEHAVIOR distincts, persistance
relationnelle et copie-sur-réutilisation de main, liens capacités/scénarios,
frontières `EXTERNAL_UNKNOWN`, migrations 0001/0002, schéma 50 tables/21 triggers.
Pas de second moteur comportemental ni migration 0003.

## Corrections et preuves ciblées

`cargo test -p ske-cli --test v05_reconciliation -- --test-threads=1` :
les trois premiers tests ont réellement échoué avant correction sur main
(`target/land-v05-main/regressions-before.log`) :

- Entry reste UNVALIDATED après modification de helper (`stale=0`, attendu 2).
- Deux retours identiques, dont un après Hook(), fusionnent à tort (1 cas,
  attendu 2) car UNKNOWN_EFFECT était attribué à chaque cas de la fonction.
- Evidence ne distingue pas le run producteur du résumé du run BEHAVIOR.

Après correction, les 4 tests passent :

1. Stale-check compare les callees connus transitifs, les profils sémantiques
   et empreintes de build ; Check reste inchangé, Entry devient STALE.
2. `Exit.effects` conserve les sites exécutés, en ordre et par chemin ;
   persistance et copie des effets gardées dans les tables existantes.
3. Le DTO BEHAVIOR conserve son propre run et expose `summary_run_id` ;
   les ancres summary_case nomment leur véritable producteur et ordinal,
   avec `producer_run_id` et `summary_case_id`.
4. Une nouvelle version/clé de cache exige le modèle d'effets courant ;
   les anciens résumés ne sont ni réinterprétés ni réécrits.

L'absence de trace d'effets ne suffit pas à prouver la composabilité d'un
résumé réhydraté non exact : son application reste opaque conservativement.
Les limites de copie/réhydratation de main ne sont pas remplacées par celles
de l'implémentation indépendante.

Runner SQLite : transaction immédiate par fichier de migration, checksum et
historique atomiques pour ce fichier. Test d'une vraie migration v1 suivie
d'une migration invalide : 47 tables et checkpoint v1 conservés ; reprise
avec 0002 valide ⇒ 50 tables. SQL normatif inchangé.

Validateur : échec préalable bloque la campagne finale ; heartbeat et durée
visibles ; arrêt du groupe de processus enfant sur interruption. `--only`
conserve la distinction NOT_RUN, sans prétendre valider le jalon complet.

## Commandes exécutées à ce stade

- `cargo test -p ske-cli --test v05_reconciliation -- --test-threads=1` :
  4 PASS, aucun ignoré.
- `cargo test -p ske-storage` : 1 unitaire + 13 intégration PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` : PASS avant la
  dernière régression cache ; la campagne complète rejoue ce contrôle.
- `python3 scripts/test_validate_v05.py` : 3 PASS.
- `cargo fmt --all`, `git diff --check`, `bash -n scripts/verify.sh` : exit 0.

## Validation complète du candidat intégré

`RUST_TEST_THREADS=1 python3 scripts/validate-v05.py` exécuté sans `--only`,
avec logs dans `target/land-v05-main/validate-v05.log` : exit 0.
Les cinq `target/validation-v0*/results.json` ont été relus et vérifiés :
42 v0.1 + 25 v0.2 + 29 v0.3 + 14 v0.4 + 13 v0.5, tous PASS.
Cette preuve vient du candidat intégré basé sur main, pas de l'ancien
checkpoint indépendant. Durée environ 14 minutes ; phase verify/régression
800,91 s. Un seul marqueur `VERIFY OK v0.5` dans le log.

La campagne inclut formatage, Clippy strict, protoc/fixtures, Go vet/test,
workspace complet (dont les quatre nouvelles régressions) et intégrité SQL.
Elle hérite explicitement de la preuve verify dans la chaîne v0.4→v0.1,
sans supprimer de contrôle ni ignorer de test.

Publication du candidat et CI Linux/macOS : prochaine étape avant landing.
