# ADR-022 — Intégration multilangage v0.6

## Statut

Accepté pour l'interface d'intégration ; validation du jalon en attente.
Les choix internes des adaptateurs et l'audit des licences Java font
l'objet d'ADR dédiés. Cet ADR n'autorise aucune modification normative
de schéma, de migration ou de protocole.

## Contexte

La v0.5 est validée sur `17036c0`. La v0.6 exige les mêmes services
ingest/inspect/branches pour Go, Rust et Java, sans types internes de
compilateur dans le cœur. L'IPC et la validation CIR existent déjà.
Dupliquer le lanceur par langage multiplierait les règles de consentement,
de limites, de rollback et de persistance des erreurs.

## Décision

Mutualiser l'ingestion des processus adaptateurs derrière une sélection
explicite de langage. Conserver l'API Go publique existante ; ajouter les
points d'entrée Rust et Java sans modifier le moteur symbolique, le stockage
ou le registre CIR pour contourner une difficulté d'extraction.

La découverte et le header d'extraction doivent correspondre au langage
demandé. Les inconnus et pertes de mapping se manifestent dans les
diagnostics, profils et capacités, jamais par des attestations artificielles.
L'import CIR direct conserve son contrat multilangage.

Profils fermés, canoniques, plats :

```json
{"profile_version":1,"language":"rust","target":null,"features":[],"cfg":[],"entry":"src/lib.rs"}
```

```json
{"profile_version":1,"language":"java","class_path":["target/classes"],"source_roots":["src/main/java"],"target_release":17}
```

Clé inconnue ou langage incohérent : erreur de configuration.
Le profil complet intervient dans les empreintes existantes.
Les chemins doivent rester dans la racine autorisée ; aucun profil ne
contient une commande à exécuter.

Rust invoque uniquement le compilateur épinglé sur la source, sans Cargo,
build.rs ou chargement de proc macros tiers. Java lit du bytecode déjà
compilé et les sources disponibles, sans lancer le build du dépôt étudié.
Les tests peuvent compiler les fixtures de confiance du dépôt SKE.

## Alternatives

- Trois moteurs d'ingestion : rejeté, duplique les mécanismes de sûreté.
- Convertir les adaptateurs en générateurs de fixtures : rejeté, ne vérifie
  ni le protocole réel ni l'inventaire générique.
- Modifier le noyau pour chaque langage : rejeté hors extension compatible
  nécessaire et justifiée par des tests/ADR.

## Conséquences et vérification

Les adaptateurs ont leur propre build et leur propre contrat de toolchain.
La présence locale d'un compilateur ou le succès d'un build ne valide pas
la provenance source→IR. Les tests croisés doivent comparer les relations
comportementales et inventorier séparément source/IR/statuts.

Les 14 AC v0.6 et 123 AC antérieurs doivent être réellement exécutés avant
clôture. Aucun tag `cir-v1.0-frozen` avant les pilotes multilangages et
les contrôles d'exceptions/match/types/flot de contrôle. Aucun push ni
merge partagé n'est autorisé.
