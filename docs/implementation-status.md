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
| v0.4 — analyse interprocédurale | DONE | 14 AC PASS ; 42 AC v0.1 + 25 AC v0.2 + 29 AC v0.3 PASS ; voir validation/v0.4.md et les frontières de précision dans soundness-and-limits.md. Correctif SIGINT, parsing SMT par lots et priorité d'erreur adaptateur intégrés (CI Linux/macOS verte) |
| v0.5 — connaissance comportementale | DONE | 13 AC PASS ; non-régression 42 AC v0.1 + 25 AC v0.2 + 29 AC v0.3 + 14 AC v0.4 PASS. Voir validation/v0.5.md, le dernier journal et les frontières dans soundness-and-limits.md |
| v0.6 — adaptateurs Rust et Java | DONE | 14 AC PASS sur `python3 scripts/validate-v06.py` (commit `22f0399`, tag `cir-v1.0-frozen`) ; 123 AC antérieurs rejoués ; main poussé à `a4a17db`, CI Linux/macOS verte (run 39026189933). Limites §10.1 documentées par ADR |
| v0.7 — API/MCP/LLM, incréments | DONE | 11 AC PASS sur `python3 scripts/validate-v07.py` complet (exit 0) : DTO indépendant rusqlite + 10 services §11.7, pagination/troncature, outils MCP, explain/impact, Context Builder, LLM avec mock HTTP/consentement/prop_no_llm_proof, benchmark comparatif, et non-régression complète v0.1–v0.6 rejouée (137 AC) + `verify.sh` `VERIFY OK v0.7`. Voir `docs/validation/v0.7.md` |
| Couverture de limitations (post-v0.7, sans nouveau jalon) | LIVRE | 4 lanes TDD : Rust `/` `%` représentables (assert MIR → BRANCH+PANIC) ; Go pré-modules (`gopath_import`, fallback GOPATH) ; Java heap configurable (`SKE_JAVA_XMX`, défaut 2g) + fix `E_SSA_DOMINANCE` handlers déconnectés ; harnais benchmark re-vérifié. Voir `docs/limitations-coverage.md` et le dernier journal. Non-régression : `cargo test --workspace`, adaptateurs Go/Java/Rust verts, clippy/fmt propres, `check-doc-claims` 19 PASS, 37 tests harnais |
| v0.8 — scénarios causaux et documentation fonctionnelle | EN COURS (cycles 1+2 livrés) | Cycle 1 : `ske scenarios list\|link\|show` (liens CALL CONFIRMED idempotents, callee non reconstruit jamais inventé, chaîne causale, frontière EXTERNAL_UNKNOWN) + `ske docs render` (fiches Markdown package/fonction, run autoritatif dédupliqué, branches prouvées inaccessibles). Cycle 2 : `ske capabilities list\|nominate\|reject\|show` (nomination humaine `title` + `HUMAN_VALIDATED`/`REJECTED`, jamais automatique) + `ske docs render --capabilities` (`CAPABILITIES.md` top-down). 9 tests d'intégration `cargo test -p ske-cli --test scenarios_docs` PASS ; non-régression `cargo test --workspace` 380 PASS / 0 FAIL. Voir `docs/validation/v0.8.md` et ADR-024. Frontières : pas d'interservice, pas de nomination LLM, pas de stale-check de capacités. Aucun push/merge |

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
| Modèle de comportements | DONE (v0.5) | Rendu R1–R6 §7.2, déduplication conservatrice, reconstruction behavior_scenarios/cases/outcomes, scénario/capacité §7.5, pagination et rollback ; 13 AC PASS |
| Connaissance fonctionnelle/règles | DONE (v0.5) | `ske behaviors reconstruct`, `ske rules list --with-evidence`, `ske rules stale-check` (§11.2) ; evidence DERIVED_FROM/GUARD_OUTCOME/RESULT_SITE ancrée au snapshot |
| Adaptateurs Rust/Java | DONE (v0.6, périmètre borné) | Pousseurs réels MIR (Rust) et SootUp/Jimple (Java) isolés, mêmes services ingest/inspect/branches ; inconnus et mappings partiels explicites, unités PARTIAL, audit LGPL Java requis avant redistribution |
| API/MCP/LLM | DONE (v0.7, périmètre borné) | `ske-api` DTO SQLite-indépendant, `ske-mcp` (outils inspect/branches/explain/rules/impact + serveur stdio JSON-RPC), Context Builder budgeté, client LLM optionnel scripté ; HTTP/JSON exposé par `ske-api` ; benchmark comparatif réel. Limites : qualité/rappel LLM non mesurée (aucun token dépensé), HTTPS réel hors loopback non testé, adapter Query du Context Builder futur. 11 AC v0.7 PASS |
| Analyse incrémentale | DONE (v0.5, périmètre borné) | Copie-sur-réutilisation des résumés (§13) entre snapshots du même dépôt ; clé canonique §4.6 indépendante des ids de flux/spans, config de snapshot, lignes analysis_dependencies, metrics summaries_reused/computed |

## Répartition

- Coordinateur : workspace, ske-common, ske-graph, scripts, intégration, validation.
- Agent CIR : protobuf, ske-cir, validations locales, arithmétique, registre.
- Agent stockage : migration exacte, ske-storage, contrôles SQL, tests de preuve.
- Agent fixtures : sources Go exactes, générateur CIR, fixtures négatives.
- Agent import/query : import transactionnel, ske-query, CLI et tests E2E.

Les contrats d'intégration et les identifiants de threads sont dans `docs/work-log.md`.
