# ADR-022-java — Java v0.6 : SootUp, protocole et licences

Statut : accepté pour le pilote local ; diffusion binaire publique non autorisée.
Références : spécification v1.4 §10.2, jalon v0.6 lignes 3998–4126.

## Décision

- Adaptateur isolé Maven dans `adapters/java/`, sans dépendance du cœur sur Java.
  Maven **3.10.0**, JDK **24.0.1** (enforcer), compilation adaptateur et fixtures
  en `--release 17`. Le runtime émet `TOOLCHAIN_MISMATCH` sur une autre JVM.
  Bytecode étudié de major > 61 rejeté ; aucun téléchargement de toolchain.
- SootUp **2.0.0** : `JavaClassPathAnalysisInputLocation`, `JavaView`,
  `JavaSootMethod.getBody()`, Jimple `StmtGraph.successors/exceptionalSuccessors`.
  CFG issu du bytecode réel, jamais reconstruit à partir des fixtures.
  Un bloc CIR par statement Jimple, avec blocs synthétiques de propagation.
- JavaParser **3.26.3**, niveau Java 17 : inventaire source et spans UTF-8
  exclusifs, indépendants du CFG. Les comparaisons simples paramètres/entiers
  ont une attestation exacte seulement après comparaison structurelle des
  opérandes, opérateurs et ligne unique du Jimple. Position seule : APPROX.
- ASM **9.7.1** fournit l'ordre original des types de handlers de la méthode,
  car l'itération de la map SootUp n'est pas un contrat d'ordre.
  Dispatch/sous-types non prouvés : `EXCEPTION_FLOW_PARTIAL`, impact EFFECTS.
  Finally/throw/effets implicites non entièrement inventoriés : impact INVENTORY.
  Aucun handler absent ne permet de déclarer COMPLETE.
- Protobuf runtime/protoc **4.29.3**, schémas existants inchangés. IPC identique
  Go : u32 BE, maximum 16 MiB, `AdapterRequest` major 1,
  `AdapterResponse` ; CIR 1.3, Header en premier, EndUnit en dernier.
  `--version` est informatif, pas une étape de calcul du consentement.
- Profil fermé, valeurs par défaut :

  ```json
  {"profile_version":1,"language":"java","class_path":["target/classes"],"source_roots":["src/main/java"],"target_release":17}
  ```

  Les chemins restent dans la racine, y compris après résolution des symlinks.
  Pas de module-path, profils framework, annotation processors ni contrats de
  bibliothèques implicites : demandes hors contrat rejetées comme profil invalide.
  Pas de résolution de dépendances au runtime ; `class_path` est fourni explicitement.
  Une seule unité `java:main`, profil `java-17-jvm`, monde toujours ouvert.
- Maven package génère `ske-adapter-java` à partir de `launcher.sh.in`, avec le
  SHA-256 du jar ombré embarqué dans le lanceur. Le hash de l'exécutable consenti
  change avec le jar ; un remplacement du jar est refusé avant le lancement
  JVM. Aucun exécutable n'est lancé pour dériver l'identité de consentement.
  Heap 512 MiB, metaspace/code/class-space bornés ; variables Java d'injection
  retirées. Les budgets globaux temps/mémoire sont imposés par le parent.

## Alternatives

Gradle/wrapper ajouterait une seconde mécanique de résolution sans bénéfice au
pilote ; Maven est explicitement permis par l'AC-0.6-02 avec ADR. Parser la source
seule perdrait les vrais SWITCH/handlers/abaissements bytecode. Chargement des
classes par réflexion exécuterait potentiellement du code étudié : interdit.
Exécuter Maven/Gradle/javac du dépôt étudié est également interdit. Seuls les
tests de confiance compilent les trois fixtures normatives avec le javac hôte.

## Audit de licences §10.2

Audit effectué sur les POM effectivement résolus (y compris parents) et les
notices embarquées ; pas une simple supposition basée sur le nom des librairies.
Les versions ci-dessous constituent l'arbre runtime résolu au démarrage du pilote.
Les coordonnées exactes sont reproductibles par
`mvn org.apache.maven.plugins:maven-dependency-plugin:3.8.1:tree`.
Les POM et sources correspondants sont disponibles sur
`https://repo.maven.apache.org/maven2/<group avec />/<artifact>/<version>/`.

| Artefacts (version exacte) | Licence / choix |
|---|---|
| `org.soot-oss:sootup.{java.bytecode.frontend,interceptors,java.core,core,analysis.intraprocedural}:2.0.0` | LGPL-2.1 (POM parent `sootup:2.0.0`) |
| `org.jgrapht:jgrapht-core:1.3.1` | LGPL-2.1 ou EPL-2.0 ; choix EPL-2.0 |
| `org.jheaps:jheaps:0.10` | Apache-2.0 |
| `org.ow2.asm:{asm,asm-analysis,asm-commons,asm-tree,asm-util}:9.7.1` | BSD-3-Clause |
| `com.github.javaparser:javaparser-core:3.26.3` | LGPL-3.0 ou Apache-2.0 ; choix Apache-2.0 |
| `com.google.protobuf:protobuf-java:4.29.3`, protoc 4.29.3 (outil) | BSD-3-Clause |
| `com.fasterxml.jackson.core:{jackson-core,jackson-annotations,jackson-databind}:2.18.3` | Apache-2.0 |
| Sources embarquées Jackson : FastDoubleParser, fast_float | Apache-2.0 / MIT ; notices `META-INF/FastDoubleParser-*` et `thirdparty-LICENSE` inspectées |
| `com.google.guava:guava:33.3.0-jre`, `failureaccess:1.0.2`, `listenablefuture:9999.0-empty-to-avoid-conflict-with-guava` | Apache-2.0 ; le dernier jar est vide intentionnellement |
| `com.google.code.gson:gson:2.8.9` | Apache-2.0 |
| `com.google.code.findbugs:jsr305:3.0.2` | Apache-2.0 |
| `com.google.errorprone:error_prone_annotations:2.28.0` | Apache-2.0 |
| `com.google.j2objc:j2objc-annotations:3.0.0` | Apache-2.0 |
| `org.checkerframework:checker-qual:3.43.0` | MIT |
| `commons-io:commons-io:2.16.1`, `org.apache.commons:commons-lang3:3.14.0` | Apache-2.0, notices dans leurs jars |
| `org.slf4j:slf4j-api:2.0.13`, `slf4j-nop:2.0.16` | MIT, texte dans jars ; API 2.0.13 résolue par SootUp |
| JUnit Jupiter 5.11.4 / Platform 1.11.4 (tests uniquement) | EPL-2.0 |
| plugins Apache Maven épinglés, Ant 1.10.12 (outil antrun) | Apache-2.0 |
| `org.xolstice.maven.plugins:protobuf-maven-plugin:0.6.1`, `kr.motd.maven:os-maven-plugin:1.7.1` (outils) | Apache-2.0 |

**Conséquence importante :** SootUp est LGPL, pas Apache. L'isolation IPC évite
un lien du cœur Rust avec SootUp mais n'annule pas les obligations du distributeur
Java. Le jar ombré est un artefact de développement local, pas un paquet autorisé
à publication. Avant distribution : conserver toutes les notices sans collisions
de shading, fournir textes LGPL/EPL et sources exactes des composants concernés,
préserver la possibilité de remplacement/relink des composants LGPL, vérifier les
modalités avec le responsable de la distribution. Préférer alors un assemblage
de jars séparés avec manifeste de hashes et lanceur lié à ce manifeste.
Les warnings de notices du shade sont connus : ils ne sont **pas** une validation
de conformité de redistribution. Aucun package publié, aucun push.

## Limites assumées du pilote

Pas de modèle complet de heap Java, de collections, lambdas, synchronisation,
initialisation de classe, exceptions implicites, hiérarchie chargée ou frameworks.
Ces effets sont opaques avec diagnostics ; toutes les invocations gardent
`world_closed=false`. Réflexion, virtual/interface/dynamic : cibles non résolues
explicites, pas de CHA fermé inventé. Les appels statiques/special résolus dans le
projet pointent vers les fonctions réelles ; les autres vers des stubs externes
de la même unité.

Locaux Jimple multi-définition : valeurs fraîches conservatrices et diagnostic,
pas de faux SSA ; entiers JVM computationnels normalisés et conversions CIR
explicites. Spans/activations des constructions complexes restent partiels.
L'ordre global des types de handlers ne suffit pas à certifier tous les cas de
tables imbriquées/réordonnées : aucune certification de dispatch n'est annoncée.
Le rapport worker documente les checks réellement exécutés ; cet ADR ne valide
pas à lui seul le jalon complet ni la conformité publique des artefacts.
