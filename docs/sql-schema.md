# Stockage SQLite v0.1

`migrations/0001_initial.sql` est la copie octet pour octet du bloc SQL §8.2
de `SKE_Specification_Implementation_v1.4.md`, sans les délimiteurs Markdown.
Ne pas reformater cette migration publiée : SHA-256 des octets UTF-8 embarqués
est conservé dans `schema_migrations`.

Le schéma installé contient 50 tables : les 46 tables de la migration
`0001_initial.sql` (copie exacte §8.2) + les 3 tables de `0002_capabilities.sql`
+ `schema_migrations` créée par le runner. Il contient 21 triggers.
Les identifiants numériques SQLite ne sont pas des identifiants CIR stables ;
les clés locales portent le préfixe `unit_key|` qui doit être retiré à la
frontière JSON.

## Ouverture et migration

`Storage::open(path)` active les FK sur chaque connexion, demande WAL et fixe
le busy timeout à 5000 ms. `open_with_busy_timeout` permet de changer ce délai.
SQLite conserve `journal_mode=memory` pour `:memory:` ; le test WAL utilise un
vrai fichier temporaire.

Le runner embarque la migration (`include_str!`), prend `BEGIN IMMEDIATE`,
compare le checksum déjà enregistré ou applique SQL et version dans la même
transaction. Une seconde ouverture ne réapplique rien. Une divergence produit
`E_MIGRATION_CHECKSUM`, exit 6. Les migrations suivantes devront être ajoutées
au registre embarqué ; aucune découverte de SQL sur disque à l'exécution.

## Intégrité et frontières

Les fichiers `crates/ske-storage/sql/checks/chk_01.sql` à `chk_22.sql`
retournent les violations de §8.5. CHK-11 combine les deux requêtes par
`UNION ALL`. CHK-22 est copié octet pour octet du bloc normatif.
`Storage::db_check()` renvoie toujours le rapport complet lors de violations :
`ok`, puis 22 entrées `checks` ordonnées, avec `id`, `severity`, `violations`.
`rows` est absent sans violation et contient au plus 20 valeurs sinon
(tableau de colonnes pour `foreign_key_check`). Toutes les lignes sont comptées.
CHK-19 seul est WARNING et ne rend pas `ok=false`.
Le CLI doit convertir `ok=false` en `E_DB_INTEGRITY` / exit 6.

Les triggers imposent la preuve acceptée pour `PROVEN_UNREACHABLE`,
l'absence de `WITNESSED` et l'immuabilité des preuves/faits support.
L'acceptation mathématique est une responsabilité du futur vérificateur,
pas de SQLite. Les contraintes inter-snapshot sont vérifiées par les checks,
pas intégralement par les FK.

`open_snapshot_writer(id)` refuse CLOSED et ABORTED avec `E_SNAPSHOT_CLOSED`.
Le writer revérifie OPEN sous transaction immédiate (ou savepoint d'ingestion)
avant chaque exécution. `closed_snapshot(id)` expose l'état d'inventaire et
la création de runs, sans API d'écriture structurelle.
L'importeur conserve la responsabilité de relier son SQL au snapshot du writer.
La connexion publique est une échappatoire explicite du contrat imposé :
elle ne peut pas garantir l'immuabilité contre du SQL brut.
