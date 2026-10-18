# Revue v0.7 — défauts ouverts et correctifs ciblés

Revue harnais/benchmark en lecture seule :
`ksQQDqrjX0u2SvmXE0lJRqe6aZLOACOY_5IBxBDZiRjivQ9OQqTS_tRBC-14`.
Pas de campagne longue exécutée par le reviewer.

## Constats

1. AC-11 supprimait le résultat v0.6 mais lançait seulement v0.5. Le
   résultat requis ne pouvait donc jamais être régénéré. Corrigé : AC-11
   invoque désormais v0.6 avec le marqueur v0.7 ; test unitaire ajouté.
   La campagne réelle reste à exécuter après fermeture des autres lacunes.
2. `--only 11` ne reprend pas une campagne : les autres AC sont NOT_RUN
   dans l'invocation et bloquent la régression. Les anciennes instructions
   proposant de préparer les résultats puis de lancer `--only 11` étaient
   incorrectes. Utiliser une campagne complète une fois prête.
3. Le benchmark retourne zéro après certains échecs de requête/analyse ;
   `--check` n'impose pas de vérification supplémentaire. À corriger.
4. La branche « sans représentation structurée » est un libellé avec
   métriques nulles, pas une campagne comparable réellement exécutée.
   Définir un corpus indépendant de questions/réponses puis deux bras.
5. Les ratios mesurent couverture evidence et classification, pas qualité
   ni rappel des réponses. Le dénominateur doit venir d'un oracle indépendant.
   L'absence de UNANALYZED ne permet pas de déclarer EXACT.
6. RSS Linux stockée comme octets alors que `time -v` retourne des KiB.
   RAM, répétition de déterminisme et contrôle d'intégrité manquent.
7. Le binaire n'est reconstruit que s'il est absent : risque de mesure
   attribuée au commit courant avec un exécutable périmé.
8. Le validateur v0.5 ne vérifie pas l'ensemble exact des identifiants AC
   des artefacts prédécesseurs. La chaîne répète verify.sh et régénère des
   documents suivis ; séparer artefacts et promotion documentaire reste ouvert.

## Correctifs intégrés et vérifiés par le coordinateur

- Impact : lot isolé intégré, cinq régressions SQLite et un test MCP
  exécutés avec succès. Détails : `docs/worker-impact-v07.md`.
- Budget Context Builder : coût du texte rendu incluant les séparateurs.
- Confidentialité : snippets embarqués et requêtes mutées contrôlés avant envoi.
- Harnais : lacunes de couverture restent PARTIAL, pas PASS au seul succès
  du filtre. Le chaînage AC-11 passe par v0.6.
- Promotion documentaire séparée (point 8) : les validateurs v0.3–v0.5
  écrivent leur rapport sous `target/validation-v0X/report.md` et ne
  modifient plus `docs/validation/v0.X.md` qu'avec `SKE_PROMOTE_DOC=1`.
  Une campagne (y compris la chaîne de non-régression) ne peut plus écraser
  l'encadrement « validée » maintenu à la main. La vérification exacte des
  identifiants AC des artefacts prédécesseurs reste ouverte.

Validation ciblée exécutée :
`cargo test -p ske-api --lib -- --test-threads=1` (10 tests),
`cargo test -p ske-query impact_ -- --test-threads=1` (5 tests),
`cargo test -p ske-cli --test mcp mcp_impact_helper -- --test-threads=1`
(1 test), `python3 -m unittest discover -s scripts -p 'test_validate_v0*.py'
-v` (17 tests du harnais, commandes substituées), Clippy strict ske-api et
ske-query, fmt --check et diff --check. Tous réussis.

Aucun de ces contrôles ne constitue une validation complète de la v0.7.
