# SKE — reprise et règles du dépôt

La source normative est `SKE_Specification_Implementation_v1.4.md`.
La demande porte sur sa section 24 et le jalon v0.4 explicitement demandé,
après v0.1 → v0.2 → v0.3, avec délégation et état durable.

## Avant de continuer

1. Lire `docs/implementation-status.md` et la dernière section de
   `docs/work-log.md`.
2. Lire `docs/validation/v0.1.md`, `docs/validation/v0.2.md` et tout rapport
   `BLOCKED-*` pertinent.
3. Examiner les changements Git et les checkpoints des agents avant de
   réimplémenter quelque chose. Ne pas repartir du dépôt vide.

## État à préserver

- v0.1 : 42 critères validés ; checkpoint local `f18e6e8` sur
  `ske/implementation`.
- v0.2 : validée, 25 AC PASS après correction des six défauts de revue ;
  régression v0.1 : 42 AC PASS. Voir la validation finale et le dernier journal.
- v0.3 : validée, 29 AC PASS ; non-régression 42 AC v0.1 + 25 AC v0.2.
  Voir `docs/validation/v0.3.md` et le dernier journal.
- v0.4 : validée, 14 AC PASS ; non-régression 42 AC v0.1 + 25 AC v0.2 +
  29 AC v0.3. Voir `docs/validation/v0.4.md` et la dernière section du journal.

Reprise après v0.4 :

1. Lire `docs/work-log.md` (dernière section), `docs/validation/v0.4.md`
   et `docs/soundness-and-limits.md`.
2. Ne pas réimplémenter les jalons v0.1 à v0.4.
3. `python3 scripts/validate-v04.py` rejoue les 14 AC et les 96 AC précédents.
4. v0.5 n'est pas commencée ; ne l'engager que sur demande.

## Discipline

- Aucun faux PASS, test ignoré pour masquer un échec, golden produit par SKE,
  résultat d'analyse simulé ou constante spécifique aux fixtures dans l'extracteur.
- Ne pas modifier les schémas ni la migration initiale normatifs silencieusement.
- Ne pas exécuter de code étudié arbitraire ; les fixtures de confiance et
  l'adaptateur du dépôt ont leurs commandes de validation propres.
- Conserver commandes, résultats, limites et prochaines étapes dans le dépôt.
- Aucun push ni merge vers une branche partagée n'est autorisé par la demande.
