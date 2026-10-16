# Contrat adaptateur

Référence : spécification v1.4 §2.3, §3, §5 et §18.
L'import CIR v0.1 est validé (42 AC), ainsi que l'adaptateur Go et son IPC v0.2
(25 AC, après corrections de revue). Voir `validation/v0.2.md` pour les
commandes exécutées et limites conservées, et ADR-020 pour les nil Go opaques.

## Transport

Le futur adaptateur est un sous-processus lancé après consentement explicite et
validation du toolchain. stdin transporte AdapterRequest, stdout
AdapterResponse, stderr des événements JSON, un par ligne. Aucun log sur stdout.
Chaque message est précédé de quatre octets u32 big-endian de longueur,
limite par défaut 16 MiB (`--max-message-bytes`). Zéro est une longueur valide.
EOF suit EndUnit ou ListUnitsDone. Les fixtures `.cir.pb` utilisent le même
encadrement, avec des Record consécutifs et sans en-tête de fichier.

Les limites couvrent taille, temps total, nombre de messages et mémoire si
l'OS le permet. Un dépassement termine le processus avec E_ADAPTER_LIMIT.
Une trame tronquée n'est pas un EOF propre.

## Version et modèle public

Major CIR accepté : 1 ; un major inconnu donne E_CIR_MAJOR_UNSUPPORTED.
Champs, opcodes ou kinds inconnus exigent un diagnostic, jamais une ignorance
silencieuse. CIR v1 est provisoire jusqu'à v0.6 ; une rupture exige ADR,
version mineure, migration et mise à jour des fixtures et documentation.
Le candidat v0.2 n'est pas le gel multilangage v0.6.

Les types publics sont indépendants de Go SSA, rustc et des frameworks Java.
Les identifiants sont locaux au flux d'unité. Les cibles externes sont des
stubs de cette unité, pas des références traversant les unités.
Les constantes sont des ValueDef ; ni opcode CONST ni CAST générique.

## Import et provenance

Le cœur valide les invariants avant clôture et assure l'atomicité : un rejet
ne laisse aucune nouvelle ligne, même dans repositories et snapshots.
Il vérifie hash/spans, types, dominance, terminateurs et arêtes, inventaires,
liens et cas d'activation. Il dérive décisions/issues IR et call sites.
L'unité porte profil sémantique, hash et empreinte de build.
Le profil des fixtures manuelles est exactement `go-fixture-linux-amd64`.
Les compteurs EndUnit sont confrontés aux données importées ; le cœur peut
dégrader le statut mais ne promeut jamais PARTIAL en COMPLETE.

## Sécurité de l'adaptateur Go prévu

Chargement source/type-checking sans compilation ni export data de dépendances ;
ne pas exécuter tests, code, générateurs ou commandes arbitraires du dépôt.
Environnement filtré et configuration de build explicite. La disponibilité de
Go 1.27.1 ne constitue pas une validation de ce contrat.

## Vérifications attendues, non exécutées ici

Framing u32 BE, longueur nulle, dépassement et troncature ; major inconnu ;
diagnostics d'extensions ; rollback des négatifs ; stub externe ; polarité
inversée ; compteurs et cas d'activation ; pureté stdout et codes de sortie.
Les commandes de validation et constats d'AC restent à la charge du parent.
