# BLOCKED — landing v0.4 : annulation SIGINT

## État vérifié

- Landing demandé explicitement par `/land`.
- v0.4 publiée sur `origin/ske/implementation`, checkpoint `ab7b6bb`,
  puis correctif mémoire `9106713`.
- Aucun push vers main effectué par cet agent.
- CI de `9106713` :
  https://github.com/psyllos/ske/actions/runs/38901291552
  **FAIL** sur `cancel_sigint_preserves_streamed_arrivals`.
  Le processus reste vivant après SIGINT jusqu'à la borne de 120 secondes.
  Le job macOS est annulé par le fail-fast de la matrice.

## Correctif déjà isolé et testé

Le premier run v0.4
https://github.com/psyllos/ske/actions/runs/38900342322
échouait sur le test de limite mémoire Linux, comme le main initial
`8004d2f`. Le parsing retournait E_RECORD_ORDER avant de reconnaître la
sortie non nulle sous limite dure d'espace d'adressage.

`9106713` corrige la priorité d'erreurs sur le chemin de parsing invalide,
avec grâce bornée de 50 ms. La nouvelle régression est reproduite FAIL
avant correction puis PASS après ; les 16 tests `adapter_ipc` passent
localement. La CI Linux passe désormais ce test et rencontre ensuite le
blocage d'annulation.

## Annulation : pas de correctif validé

La relance locale `python3 scripts/validate-v04.py` a aussi rencontré
l'échec SIGINT dans la régression v0.2 imbriquée (AC-0.2-22), puis
AC-0.3-25 et AC-0.4-14 ont été déclarés FAIL.

Un essai de garde Z3 répétant l'interruption, avec court-circuit des checks
déjà annulés, a passé les tests ciblés solver/CLI. Mais la validation
complète suivante a dépassé la borne de la commande (1 000 000 ms), sans
résultat final AC-0.4-14. Cet essai a été retiré intégralement :
**ne pas le considérer comme une correction démontrée**.

Les anciens 14/14 PASS sont des résultats historiques sur `ab7b6bb`,
pas une preuve de CI verte actuelle. Aucun test n'a été ignoré, aucune
borne augmentée pour masquer l'échec et aucune assertion affaiblie.

## Concurrence avec main et reprise

Pendant l'opération, main a avancé indépendamment à `02df44f`
(`Record passing Linux and macOS CI for adapter fix`), après `d160ad3`
(`Fix adapter exit classification after EOF`). Ces commits ne contiennent
ni `ab7b6bb` ni `9106713` selon `git merge-base --is-ancestor`.
Ne pas écraser ce travail ni forcer un push. Réconcilier le correctif
mémoire déjà arrivé sur main avant une prochaine tentative.

Prochaine action : diagnostiquer et corriger l'annulation SIGINT avec des
preuves de régression, relancer la CI Linux/macOS et la validation locale,
puis reprendre le landing seulement après validation.
