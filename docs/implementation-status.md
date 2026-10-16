# SKE — état durable d'implémentation

Source de vérité : `SKE_Specification_Implementation_v1.4.md`, notamment §24.

## Reprise

Lire ce fichier, puis `docs/work-log.md`, puis les rapports de validation.
Ne jamais supposer qu'un test passé vaut validation de tous les critères.
Ne pas commencer v0.2 avant les 42 AC v0.1 PASS ou dérogation approuvée.

| Jalon | État | Condition |
|---|---|---|
| v0.1 — CIR, SQLite, import, navigation | DONE | 42 AC PASS après revue et intégration ; voir validation/v0.1.md |
| v0.2 — Go | DONE | 25 AC PASS après correction des six défauts de revue ; 42 AC v0.1 toujours PASS |
| v0.3 — analyse symbolique et preuves | DONE | 29 AC PASS ; non-régression 42 AC v0.1 + 25 AC v0.2 ; voir validation/v0.3.md et le dernier journal |
| v0.4 — analyse interprocédurale | DONE | 14 AC PASS ; 42 AC v0.1 + 25 AC v0.2 + 29 AC v0.3 PASS ; voir validation/v0.4.md et les frontières de précision dans soundness-and-limits.md |
| v0.5 — connaissance comportementale | PARTIAL | Implémentation et tests ciblés présents ; revue de sûreté et validation complète encore ouvertes. Voir le dernier journal et validation/v0.5.md |
| v0.6–v0.7 | NOT_STARTED | Hors périmètre de la demande actuelle |

## Fonctionnalités §1.1

`PARTIAL` signifie seulement la partie du périmètre cible déjà prise en charge.

| Fonctionnalité | État | Livré / frontière |
|---|---|---|
| Inventaire sources/packages/modules/symboles | DONE (Go v0.2) | Extraction Go source/SSA et import CIR, provenance conservée |
| Décisions source | DONE (Go v0.2) | Émetteur AST et compteur indépendant, liens et activation bornée |
| Décisions IR | DONE (CIR v0.1) | Dérivation des terminateurs multi-issues dans SQLite |
| CFG intra-fonction | DONE (CIR v0.1) | Navigation JSON, dominance CHK, DFS et SCC |
| Graphe d'appels | DONE (v0.4) | Go statique/CHA, stubs et inconnues explicites ; SCC Tarjan itératif ; `paths explore` borné, paginé, sans affirmation d'inaccessibilité |
| Propagation symbolique | DONE (v0.3) | Arène typée, interning, simplifications §6.2 ; exploration FIFO, PHI par prédécesseur, budgets, fermeture globale |
| Vérification SMT | DONE (v0.3) | Z3 réel lié, encodeur SMT-LIB §6.4, solveur scripté, requêtes persistées avec texte et empreinte |
| Boucles/récursivité | DONE (v0.4, périmètre borné) | mode `intervals` explicite, défaut `unroll` inchangé ; récursion bornée et cas résiduel PARTIAL ; allocations en boucle restent coupées conservativement |
| Résumés interprocéduraux | DONE (v0.4, périmètre documenté) | `ske-summary`, substitution inter-arènes, retours multiples, globales et deltas connus ; persistance summary_* ; effets non modélisés explicites |
| Modèle de comportements | NOT_STARTED | v0.5 |
| Connaissance fonctionnelle/règles | NOT_STARTED | v0.5 |
| Adaptateurs Rust/Java | NOT_STARTED | Interdits dans la première instruction §24 |
| API/MCP/LLM | NOT_STARTED | Aucune donnée envoyée à un LLM |
| Analyse incrémentale | PARTIAL | Snapshots et fingerprints sans réutilisation |

## Répartition

- Coordinateur : workspace, ske-common, ske-graph, scripts, intégration, validation.
- Agent CIR : protobuf, ske-cir, validations locales, arithmétique, registre.
- Agent stockage : migration exacte, ske-storage, contrôles SQL, tests de preuve.
- Agent fixtures : sources Go exactes, générateur CIR, fixtures négatives.
- Agent import/query : import transactionnel, ske-query, CLI et tests E2E.

Les contrats d'intégration et les identifiants de threads sont dans `docs/work-log.md`.
