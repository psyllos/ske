# Soundness et limites

## Ce que v0.1 permet

Importer et consulter des inventaires structurels et leur provenance est
validé en v0.1 (42 AC, rapport séparé). Une base
structurellement valide ne démontre pas la sémantique du programme source.
v0.2 est validée (25 AC) ; v0.3 est en validation. La conformité structurelle Go
ne doit pas être présentée comme une connaissance comportementale complète.
En particulier, les nil Go hors du contrat littéral CIR sont explicitement
opaques et ne permettent aucune déduction précise de nullité (ADR-020).

## Axes distincts

| Axe | Valeurs |
|---|---|
| Inventaire | COMPLETE, PARTIAL, FAILED |
| Exécution d'analyse | PENDING, RUNNING, COMPLETED, PARTIAL, FAILED, CANCELLED |
| Accessibilité | UNANALYZED, POSSIBLE, WITNESSED, PROVEN_UNREACHABLE, UNKNOWN, TIMEOUT, UNSUPPORTED |
| Soundness | EXACT, OVERAPPROX, OPAQUE, UNSUPPORTED |

La soundness ne peut que se dégrader dans l'ordre du tableau. `EXACT` signifie
exact sous hypothèses, pas vérité absolue sur le logiciel. `COMPLETE` concerne
l'inventaire dans le périmètre déclaré, jamais tous les chemins concrets.
Le cœur recalcule les compteurs et peut dégrader COMPLETE en PARTIAL ; il ne
promeut pas un PARTIAL déclaré par l'adaptateur. Un snapshot sans unité est
FAILED ; sinon l'agrégation respecte §3.5.

## Aucune preuve simulée

En v0.1 les issues sont UNANALYZED et `analysis` vaut null.
`SAT` ne produit pas WITNESSED ; WITNESSED n'a aucun producteur en v0.x.
UNKNOWN et TIMEOUT ne sont jamais convertis en inaccessibilité.
Une requête UNSAT isolée ne suffit pas à PROVEN_UNREACHABLE.

Le mécanisme v0.3 impose fermeture globale, absence de coupes,
preuves acceptées, vérification indépendante de la classification et protection
SQL. Son niveau maximal est MODEL_UNREACHABLE, sous hypothèses du run, pas une
preuve absolue sur les sources. SOLVER_REPLAYED ne certifie pas la couverture ;
CERTIFIED_UNREACHABLE est interdit en v0.x.

## Hypothèses et confiance

Tout run doit persister au minimum SEQUENTIAL_EXECUTION et
CLOSED_ENTRY_CONTEXT avant COMPLETED. L'état global initial est libre.
Les hypothèses supplémentaires appartiennent au registre §3.3 ; les garanties
de mapping source et d'effets adaptateur doivent être explicites.
Un inventaire sans cas d'activation complets reste consultable mais ne justifie
aucune preuve source. Ni concurrence, ni comportement opaque, ni effets inconnus
ne peuvent être éliminés pour obtenir artificiellement une preuve.

## Approximations et frontières v0.3

- **Mémoire M0/M1** : pas de modèle de heap complet. Les allocations dynamiques
  identifiées sont distinctes, y compris au même site ; paramètres/globaux
  peuvent aliaser. Les écritures ambiguës sont faibles, ordonnées dans le
  journal ; aucune mise à jour forte sur un résumé multi-instance. La perte
  d'identité ou une cible inconnue provoque un havoc conservateur.
- **CALL havocké** : aucun résumé ni exploration interprocédurale. Résultat
  frais et mémoire invalidée lorsque les effets ne sont pas prouvés absents.
  Un appel pur n'est pas automatiquement référentiellement transparent :
  lire un état mutable interdit le partage déterministe de résultat.
- **Panics d'appel non explorés** : hypothèse explicite
  `CALLEE_PANIC_EXITS_NOT_EXPLORED`. Les sorties anormales quittent la fonction ;
  elles ne constituent pas des issues IR inventées. Les forks DIV/REM par zéro
  et décalage signé négatif sont, eux, vérifiés par SMT.
- **Boucles bornées** : comptage des arêtes de retour DFS ; une coupe rend
  l'exploration globale non fermée. Aucun `PROVEN_UNREACHABLE` dans un tel run,
  même pour une issue dont les arrivées rencontrées sont toutes UNSAT.
  Aucun point fixe, widening, résumé récursif ou invariant inductif en v0.3.
- **Flottants, chaînes, tableaux et opérations non encodées** : sortes opaques
  ou valeurs HAVOC, avec perte de précision ; pas de raisonnement arithmétique
  ou lexical exact. Seules les opérations documentées BOOL/BITVECTOR et
  l'égalité sur sortes non interprétées ont un encodage SMT.
- **Effets opaques/concurrence** : `RECOVER`, defer, SPAWN, canaux et effets
  inconnus ne sont pas explorés exhaustivement ; havoc mémoire et soundness
  OPAQUE sous `SEQUENTIAL_EXECUTION`, pas de garantie sur le programme concurrent.
- **Budgets et solveur** : rlimit/graine déterministes ; timeout mural de
  sécurité non reproductible. Unknown/Timeout restent visibles, l'exploration
  continue conservativement mais devient PARTIAL. L'estimation mémoire n'est
  pas un hard cap OS. Les requêtes identiques réutilisent leur preuve de requête
  dans le run sans nouvel appel ni nouvelle ligne SQL.
- **Activation source** : une preuve IR isolée ne devient pas une preuve source
  par agrégation de statuts. Les cas d'activation ordonnés doivent être complets
  et exacts ; POSSIBLE requiert un chemin conjoint SAT. La dérivation reste
  conservatrice (UNKNOWN) si des contradictions conjointes ne sont pas
  justifiées par les preuves persistées. `elided` reste UNANALYZED.
- **Rejeu de preuve** : le vérificateur SQL indépendant relit fermeture,
  arrivées, hypothèses et toutes les requêtes UNSAT du run, puis utilise Z3
  neuf à graine 1. Cela vérifie la cohérence dans le modèle, non la traduction
  source→CIR. Les affirmations de l'adaptateur restent des hypothèses de confiance.
- **Annulation** : entre instructions, ou après le délai du solveur en cours ;
  les arrivées sont écrites au fil de l'eau. Après finalisation COMPLETED,
  le run ne change plus de statut ; les candidats non vérifiés deviennent
  UNKNOWN si une annulation interrompt la vérification.

Interprocédural v0.4, comportements/réutilisation v0.5, Rust/Java v0.6,
API/MCP/LLM v0.7 restent hors livraison. Aucun code étudié arbitraire n'est
exécuté ; seul le runner concret de confiance du dépôt est autorisé par les
tests différentiels. Les commandes réellement exécutées sont dans les rapports
de validation ; les obligations décrites dans un ADR ne sont pas des PASS.
