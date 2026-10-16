# Architecture

## Périmètre et hypothèses

Contrat normatif : spécification v1.4, §0, §2, §3, §18–20 et §24.
Monorepo Rust édition 2021 ; développement local macOS/Linux ; environnement
de référence macOS arm64, Rust 1.99.0. SQLite local est le seul stockage de
référence. Pas d'interface graphique. Go 1.27.1 est disponible pour un futur
adaptateur indépendant, pas nécessaire à l'import de fixtures.

**État : v0.1 validée, 42 AC PASS** (`validation/v0.1.md`). CIR/prost,
stockage, import, query, CLI et graphes (dominance CHK, DFS, SCC) sont intégrés.
v0.2 Go est validée (25 AC, régression v0.1 incluse) ; v0.3 non commencée.
Le journal et `validation/v0.2.md` détaillent le checkpoint courant.

## Flux v0.1

Fixtures CIR protobuf encadrées → validation locale et graphes → import
transactionnel → snapshot SQLite clôturé → services query → CLI JSON.
Les inventaires source et IR sont conservés séparément avec leurs liens,
polarités et cas d'activation. Aucun calcul de faisabilité n'est annoncé.

La migration initiale est la copie exacte de §8.2 : **47 tables, 21 triggers**.
La présence de tables d'analyse prépare des contrats ultérieurs sans constituer
un moteur symbolique. L'intégrité inter-snapshot exige les 22 contrôles SQL,
pas uniquement les clés étrangères.

## Frontières de dépendances

`ske-common` est sans dépendance workspace. `ske-cir` dépend de common ;
graph et storage de common/CIR ; ingest de common/CIR/storage/graph.
Query orchestre les services ; la CLI ne dépend normalement que de query et
common, sans accès direct à rusqlite. Le contrôle normatif est
`scripts/check-deps.sh` via `cargo metadata` (§2.2), dépendances normales
uniquement. Les adaptateurs ne connaissent que les schémas publics.

À terme, solver restera séparé du stockage ; symbolic ne dépendra ni du
stockage ni des adaptateurs ; verify restera indépendant de symbolic et de
sa classification. Ces composants sont **prévus, non implémentés dans ce lot**.

## Reproductibilité et sécurité

Chaque unité porte profil sémantique, hash du profil et empreinte de build ;
chaque analyse future sera liée au snapshot, toolchain et hypothèses.
Les snapshots clôturés ne permettent plus d'écriture structurelle.
Deux imports identiques créent deux snapshots : pas de déduplication v0.1.
La CIR v1 reste provisoire jusqu'à validation multilangage v0.6.

Ne pas exécuter les sources étudiées, tests ou scripts de génération.
Les futurs adaptateurs seront des sous-processus consentis avec environnement
filtré, budgets et stdout réservé au protocole.
Voir les ADR et le contrat adaptateur pour les décisions et tests prévus.
