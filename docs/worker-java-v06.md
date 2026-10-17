# Worker Java v0.6 — état durable

Base : `17036c0` (v0.5 validée). Périmètre exclusif : `adapters/java/`,
`examples/java-branch-lab/`, ce rapport, ADR suffixé Java.
Aucun changement du cœur, schéma, migration, CLI, journal global ou validation
globale ; intégration CLI/ingest et non-régression confiées au coordinateur.
Aucun push, merge partagé, installation système ni réglage machine.

## Livré

Adaptateur réel SootUp 2.0.0/Jimple → CIR 1.3, JavaParser 3.26.3 pour
inventaire/spans source, protobuf 4.29.3 généré depuis les schémas existants.
Maven autorisé par `docs/adr/ADR-022-java.md` avec versions de plugins épinglées.
JDK 24.0.1/Maven 3.10.0 vérifiés par enforcer, cible bytecode 17.

Les trois sources normatives sont copiées telles que demandées :
`Decide.java`, `ReflectionUnknown.java`, `MultiCatchFinally.java`.
Seuls ces fichiers de confiance et les petits programmes créés par les tests
sont compilés ; jamais de build/code/script du dépôt étudié dans l'adaptateur.

### Interface transmise tôt au coordinateur

```json
{"profile_version":1,"language":"java","class_path":["target/classes"],"source_roots":["src/main/java"],"target_release":17}
```

Profil plat fermé ; clés inconnues/trailing JSON rejetés. Chemins relatifs
résolus dans la racine, symlinks externes refusés. `source_roots: []` autorise
le mode bytecode seul ; ce mode n'invente pas de décisions source.
Les entrées `class_path` sont le périmètre analysé, pas un mécanisme de
téléchargement. Module-path/frameworks/annotation processors hors contrat.

- Exécutable après **Maven package** : `adapters/java/ske-adapter-java`.
- `--version` : `ske-adapter-java 0.6.0 (SootUp 2.0.0; bytecode 17)`.
- Unité : `java:main`, profil `java-17-jvm`.
- Capabilities : `SOURCE_AST`, `JIMPLE_CFG`, `SWITCH`, `INVOKE`, `OPEN_WORLD`.
- Symboles : `example.Decide.decide`, `.day`, `.guarded`,
  `example.ReflectionUnknown.load`, `example.MultiCatchFinally.guarded`.
- stdin/stdout : protocole binaire existant, trames u32 BE, 16 MiB maximum,
  request major 1, responses AdapterResponse, Header/EndUnit, EOF propre.
- Lanceur généré avec le SHA-256 du jar dans son contenu : le consentement
  sur le hash exécutable est lié au contenu Java. Jar modifié : refus avant JVM.
  Variables d'injection JVM retirées, heap 512 MiB et réservations bornées.
  Le runtime n'exécute ni javac, ni Maven/Gradle, ni classe étudiée.

## Extraction et frontières

Le CFG, les constantes, paramètres, comparaisons, SWITCH et INVOKE proviennent
du Jimple réel. Les `case_value_ids` de `day` regroupent `[1]` et `[2,3]`.
Les types Java `int` sont BITVECTOR(32,signed), arithmetic WRAP JVM.
Les références nominales restent OPAQUE (pas un faux modèle de heap/nullité).
Les petits types des locaux JVM sont normalisés au type computationnel int ;
conversions numériques explicites, autres conversions opaques diagnostiquées.

Source : deux IF, SWITCH et deux TRY_CATCH (5 décisions/11 issues).
Spans des conditions `x < 0`, `x < -10`, `n`, mot-clé `try`, positions UTF-8
exclusives relues dans les tests. Compteur AST indépendant de l'émetteur.
Pas de constante d'extraction ni branche conditionnée à un nom de fixture.

Les IF simples ont une activation EXACT seulement si :

1. un seul IF source et un seul Jimple IF sur la ligne ;
2. opérateurs identiques ou inverses prouvés par table explicite ;
3. opérandes identiques : paramètres résolus et littéraux entiers relus ;
4. paramètres Jimple à définition unique.

Les cas d'activation et flags `inverted` sont émis, pas seulement des liens.
Le test générique change nom/paramètre/constantes et refuse l'attestation
après mutation source sans recompilation. Absence de mapping ne devient
jamais `elided` ou une preuve d'inaccessibilité inventée.
Switch/try et constructions complexes : mapping incomplet explicite.
Locaux multi-définition : frais conservateurs et diagnostic, pas de faux PHI.

INVOKE : NORMAL + EXCEPTION filtrées, puis propagation catch-all si nécessaire.
Ordre des types de handlers récupéré dans la table bytecode originale ASM.
Tables à types répétés/plages ambiguës : diagnostic INVENTORY ; cet ordre
global ne certifie pas le dispatch par plage dans tous les bytecodes.
Sous-types/gardes non prouvés : `EXCEPTION_FLOW_PARTIAL` impact EFFECTS.
Finally : copies bytecode et appels println conservés ; effets/propagations
implicites non entièrement inventoriés : diagnostic INVENTORY. Jamais COMPLETE
par omission. Toutes les invocations gardent `world_closed=false`.
Statique/special dans le projet : cibles réelles ; externe : stubs locaux.
Virtuel/interface/dynamic/réflexif : cibles non résolues et diagnostic explicite.

Limites non levées : heap, collections, lambdas, class initialization,
synchronisation, annotations/frameworks, exceptions implicites de toutes les
instructions, dispatch complet de throw, inventaire/mapping exhaustif des
conditions complexes/initializers/classes imbriquées. Les effets concernés
restent opaques/partiels ou produisent une erreur explicite d'extraction.
Pas de fermeture globale de call graph annoncée.

## Commandes et résultats réellement exécutés

Environnement local macOS arm64, java/javac 24.0.1, Maven 3.10.0.
Journaux non versionnés dans `adapters/java/target/`.

| Commande | Résultat observé |
|---|---|
| `(cd adapters/java && mvn -B package)` | exit 0, compilation et packaging réels ; première commande après ajout des tests : 9 tests PASS |
| `(cd adapters/java && mvn -B verify)` | exit 0 sur la version finale ; 10 unit tests + 3 packaged launcher IT, 0 échec/erreur/skip |
| `adapters/java/ske-adapter-java --version` | exit 0, version attendue |
| Tests `java_golden_decisions`, `lang_types`, `java_switch_ir`, `java_invoke_exception`, `java_reflection_unknown`, `java_multi_catch_finally` | assertions sur extraction réelle, pas de golden généré |
| Tests `framing_and_profile`, `deterministic_extraction`, `local_type_contracts`, `general_extraction_not_fixture_specific` | limites/troncature/EOF/profil, déterminisme, signatures/locals, variantes et source périmée |
| IT `actual_framed_protocol` | sous-processus du lanceur packagé ; list/extract réels, stdout purement protobuf, stderr vide, EOF après EndUnit |
| IT `jar_tampering_refused_before_jvm` | jar remplacé dans un répertoire temporaire de test : exit 1, ADAPTER_BINARY_MISMATCH, aucune version JVM exécutée |
| IT `version_and_bad_frame` | version et trame tronquée : erreur encadrée, exit 1 |

Les tests compilent les fixtures par JavaCompiler de confiance, équivalent :

```sh
(cd examples/java-branch-lab &&
 mkdir -p target/classes &&
 javac --release 17 -g -d target/classes src/main/java/example/*.java)
```

Ils produisent `target/java-lab.cir.pb` par appel direct et
`target/java-ipc.cir.pb` par protocole sous-processus. **Ce sont des sorties
d'extraction pour import, pas des goldens ni des oracles.**

Import avec le cœur inchangé :

```sh
target/debug/ske ingest cir \
  --db adapters/java/target/final-java.sqlite \
  --input adapters/java/target/java-ipc.cir.pb \
  --source-root examples/java-branch-lab --format json
target/debug/ske db check \
  --db adapters/java/target/final-java.sqlite --format json
```

Résultat : CLOSED/PARTIAL, unité java:main PARTIAL, 0 ERROR, 47 WARNING,
7 INFO (diagnostics explicites et recalcul cœur) ; 22 checks à zéro violation.
SQL : 5 décisions source, 11 issues source, 4 cas d'activation exacts ;
2 BRANCH, 1 SWITCH, 10 INVOKE réels.

Analyse réelle sans moteur spécial Java :

```sh
target/debug/ske branches analyze \
  --db adapters/java/target/final-java.sqlite \
  --entry example.Decide.decide --format json
target/debug/ske branches list \
  --db adapters/java/target/final-java.sqlite \
  --function example.Decide.decide --run latest --format json
```

Résultat : COMPLETED, exploration_closed=true, zéro coupe, 4 requêtes Z3
(3 SAT, 1 UNSAT), 3 POSSIBLE + 1 PROVEN_UNREACHABLE, preuves réelles acceptées.
Source, dans l'ordre TRUE/FALSE des deux IF :
`POSSIBLE, POSSIBLE, PROVEN_UNREACHABLE, POSSIBLE`.
IR séparé (comparaisons inversées GE) :
`POSSIBLE, POSSIBLE, POSSIBLE, PROVEN_UNREACHABLE`.
Hypothèses affichées, dont ADAPTER_SOURCE_MAPPING_TRUSTED ; aucune prétention
de preuve absolue. La comparaison multi-langage/grille concrète reste au parent.

### Échecs de développement conservés

- Premier build : import `Value` ambigu, corrigé par imports explicites.
- Premier import : E_TYPE_MISMATCH. Le test de contrat ajouté a reproduit
  les locaux boolean inférés par SootUp depuis literal0 et les signatures de
  stub constructeur dépendant du type concret du receiver ; corrections de
  règle computationnelle/signature déclarée, pas d'exception spécifique fixture.
  Type OPAQUE nullable refusé par CIR : nullable retiré, pas de schéma changé.
- Des Maven lancés simultanément dans le même `target/` ont échoué sur les
  répertoires protoc/shade/checksum. Relance séquentielle finale verte :
  `target/final-verify.log`. Aucun test ignoré pour masquer un échec.
- Deux commandes d'exploration CLI (`import cir`, `analyze` à la racine)
  étaient invalides ; validations finales utilisent `ingest cir` et
  `branches analyze` réellement supportées.

## Critères et suite

Évidence locale pour la partie Java des AC-0.6-02/04/05/07/08/11/14.
Ne pas convertir cette liste en PASS des AC multi-langages/globaux :
CLI `ingest java`, consentement intégré, `inspect`, cross-language grid,
Linux/macOS CI, verify global, tous les anciens AC et gel CIR restent à
valider par le parent. Aucun tag de gel créé.

L'audit licences obligatoire est dans ADR-022-java : SootUp **LGPL-2.1**,
pas Apache. Intégration source/pilote local permise ; distribution du jar
ombré non approuvée sans notices, sources/relink et revue distributeur.
