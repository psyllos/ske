# Worker validation v0.6 — orchestration préparée

## Périmètre livré

Uniquement `scripts/validate-v06.py`, `scripts/test_validate_v06.py` et ce rapport.
Ni `verify.sh`, ni adaptateurs, ni tests d'acceptation multilangages modifiés.
Lecture : règles racine, statut, dernier journal, spécification §10 et
AC v0.6, validations v0.1/v0.2 et blocage v0.4 résolu, runners existants.

Le runner conserve les 14 lignes dans `target/validation-v06/results.json`,
avec commandes prévues et réellement lancées, statuts et logs par AC.
`--only` laisse les autres AC `NOT_RUN` ; le retour reste non nul tant que
les 14 AC ne sont pas PASS. Les dépendances doivent passer dans la même
invocation : par exemple `--only 1,3` pour les décisions Rust. Une dépendance
absente/échouée donne FAIL avec explication, sans lancer la commande dépendante.
Le JSON est initialisé avant la première commande : une interruption ne
réutilise pas les statuts d'une campagne précédente.

Chaque filtre Rust doit produire au moins un test réellement passé portant
le nom attendu et un résumé non vide ; tests ignorés refusés. Une compilation
ou un filtre Cargo vide ne valent donc pas PASS.

## Contrat d'intégration à confirmer par le parent

- AC01 : `(cd adapters/rust && cargo build)` puis `check-deps.sh`.
- AC02 : `(cd adapters/java && mvn -B verify)` selon l'ADR Maven attendu.
- AC03–11,14 : cible `ske-cli --test multilang` et les filtres demandés :
  `rust_golden_decisions`, `java_golden_decisions`, `lang_types`,
  `cross_language_decide`, `java_switch_ir`, `java_invoke_exception`,
  `java_reflection_unknown`, `rust_toolchain_mismatch`, `multilang_inspect`,
  `multilang_integrity_framing`, `java_multi_catch_finally`.
- Les tests du parent doivent effectivement vérifier tout le contenu normatif
  des AC : le harness ne peut pas certifier la qualité d'une assertion Rust.
  Audit documentaire des toolchains épinglées, ADR et limites toujours requis.
- AC12 ne commence qu'après AC01–11 et AC14 PASS. Exécute `verify.sh`,
  exige la dernière ligne `VERIFY OK v0.6`, puis lance **tout** v0.5.
  Les anciens résultats v01–v05 sont supprimés avant la campagne et les
  123 identifiants uniques/statuts PASS sont vérifiés ensuite.
- Aucun `SKE_INHERIT_VERIFY` reçu de l'environnement n'est accepté.
  v0.5 reçoit `SKE_EXPECT_VERIFY_VERSION=v0.6`. Le parent confirme avoir
  corrigé ses constantes et la propagation à v0.4 dans son propre lot ;
  cette intégration reste à valider par la campagne réelle. Aucun faux héritage.
- AC13 passe en dernier après tous les autres AC. Il exécute
  `cargo test --workspace cir_compat_`, exige un test réel puis vérifie
  la présence de `cir-v1.0-frozen`. Il ne crée **jamais** le tag.
  Après une première campagne sans tag, le parent peut examiner les preuves,
  créer le tag autorisé puis relancer la campagne complète. Une présence de
  tag seule ne prouve pas historiquement sa date de création.

## Vérification réalisée

`python3 -m unittest discover -s scripts -p test_validate_v06.py -v` :
**10 tests unitaires PASS** (commandes acceptance simulées uniquement dans
les tests unitaires). `git diff --check` : exit 0.

**Aucun AC v0.6 réellement exécuté ; tous restent NOT_RUN.**
Ni build adaptateur, ni campagne v0.5, ni `verify.sh`, ni CI exécutés ici.
Aucun tag, push, merge partagé ou installation système.
