---
name: ske-knowledge-navigate
description: Navigate an SKE knowledge base (.sqlite produced by `ske ingest` or `ske init`). Use whenever you need to explore or query what SKE knows about a codebase: list snapshots, units, functions, reconstructed scenarios, capabilities, functional rules, causal chains, or turn them into readable Markdown. Provides the `ske-navigate.py` script (no build required) plus the `ske` CLI mapping and the schema/discipline rules.
---

# Naviguer une base de connaissances SKE (SQLite)

SKE génère une base SQLite (`--db` / `SKE_DB`, défaut `.sqlite`) qui contient
l'inventaire structuré d'un codebase : unités de compilation, fonctions, CFG,
décisions, et surtout la **connaissance fonctionnelle** reconstruite
(scénarios, capacités, règles, preuves). Ce skill donne à une IA les moyens de
s'y retrouver vite, soit via le CLI `ske`, soit — sans builder — via
`scripts/ske-navigate.py`.

## Quand l'utiliser

- « Que sait SKE sur ce dépôt / cette base ? » → `summary` puis `schema`.
- « Quelle est la liste des fonctions / scénarios / capacités / règles ? »
- « Montre-moi le comportement (règles SI… ALORS) d'une fonction donnée. »
- « Génère la documentation fonctionnelle lisible par un humain. »
- Toute question où tu dois *interroger* le graphe de connaissances, plutôt que
  de relire le code source.

## Démarrage rapide

```sh
DB=/chemin/vers/knowledge.sqlite   # ou via SKE_DB
python3 scripts/ske-navigate.py summary "$DB"           # vue d'ensemble
python3 scripts/ske-navigate.py schema "$DB"            # tables + nombre de lignes
python3 scripts/ske-navigate.py functions "$DB"         # fonctions du snapshot courant
python3 scripts/ske-navigate.py scenarios "$DB"         # scénarios reconstruits
python3 scripts/ske-navigate.py capabilities "$DB"      # capacités + titres humains
python3 scripts/ske-navigate.py rules "$DB" --with-evidence
```

Toutes les sorties sont **déterministes** (triées par clé canonique) ; les clés
locales `unit_key|…` sont affichées sans leur préfixe (convention du DTO).

## Modèle de connaissances (schéma) en bref

| Table | Rôle |
|---|---|
| `repositories`, `snapshots` | dépôts et états d'analyse (OPEN/CLOSED/ABORTED) |
| `compilation_units` | unités compilées (langage, profil sémantique, inventaire) |
| `functions`, `symbols`, `basic_blocks`, `instructions` | inventaire structurel |
| `source_decisions`/`source_outcomes`, `ir_outcomes` | décisions source et issues IR |
| `function_summaries`, `summary_cases` | résumés symboliques interprocéduraux |
| `behavior_scenarios`, `behavior_cases` | **scénarios** reconstruits + cas SI→ALORS |
| `functional_capabilities`, `capability_scenarios` | **capacités** (titre nommé par un humain) |
| `scenario_links` | liens causaux (CALL CONFIRMED / interservice INFERRED) |
| `functional_rules`, `knowledge_evidence` | **règles métier** + preuves ancrées |
| `proof_records`, `solver_queries` | preuves SMT (MODEL_UNREACHABLE…) |

## Interroger avec le CLI `ske` (source de vérité)

Quand le binaire `target/debug/ske` existe, préfère-le : le DTO JSON est la
frontière publique (préfixe `unit_key|` retiré, pagination, mêmes requêtes que
toutes les autres couches). Toute requête SQL directe doit rester **en
lecture seule** (le skill n'écrit jamais dans la base).

```sh
SKE_DB="$DB" ske snapshots list
SKE_DB="$DB" ske inspect --snapshot latest          # unités + compteurs
SKE_DB="$DB" ske functions list --snapshot latest
SKE_DB="$DB" ske scenarios list --snapshot latest
SKE_DB="$DB" ske scenarios show --entry example.Check --depth 3
SKE_DB="$DB" ske capabilities list --snapshot latest
SKE_DB="$DB" ske capabilities show --entry example.Check
SKE_DB="$DB" ske rules list --snapshot latest --with-evidence
```

### Documentation lisible par un humain

```sh
SKE_DB="$DB" ske docs render --snapshot latest --out docs/functional
SKE_DB="$DB" ske docs render --snapshot latest --out docs/functional --capabilities
```

Écrit `INDEX.md` (« comment lire » + index des fiches), une fiche par fonction
ayant un scénario reconstruit, et `CAPABILITIES.md` (vue métier top-down). Les
titres de capacités viennent **uniquement** de la nomination humaine
(`ske capabilities nominate --entry E --title "…"`) : ne les invente jamais.

## Règles de discipline (AC-0.5-08)

- **Ne jamais inventer d'interprétation métier.** Les `guard_text`/`result_text`
  sont les rendus R1–R6 : tu peux les reformuler en « Si X alors Y » pour les
  présenter, mais tu ne peux pas affirmer un sens métier qui n'y figure pas.
- `functional_capabilities.title` est `NULL` tant qu'un humain ne nomme pas ;
  un `validation_status` `REJECTED` garde son titre (audit), un `STALE` signale
  une source modifiée depuis l'analyse.
- Ne jamais transformer une limite d'analyse (`PARTIAL`, `OVERAPPROX`,
  `EXTERNAL_UNKNOWN`, `UNANALYZED`) en certitude.
- Lecture seule sur la base SQLite ; aucune migration ni écriture directe.

## Références rapides

- schéma : `migrations/0001_initial.sql` + `0002_capabilities.sql`
- couche de requêtes (SQL réel) : `crates/ske-query/src/`
- légende des statuts (guide de lecture) : générée dans `INDEX.md` par `docs render`
- fichier de validation : `docs/validation/v0.8.md`, ADR-024
