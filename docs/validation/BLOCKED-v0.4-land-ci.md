# BLOCKED — landing v0.4 : annulation SIGINT

## Diagnostic ciblé : propriété du gestionnaire SIGINT

Z3 installe par défaut son propre gestionnaire SIGINT pendant `check`
(`ctrl_c=true`, `api_solver.cpp` / `scoped_ctrl_c`). Le signal interrompt
alors Z3 sans positionner le drapeau d'annulation de SKE.

Régression `solving_preserves_application_sigint_handler` : un vrai solveur
sur pigeonhole reste actif pendant 200 ms ; le watchdog observe la disposition
SIGINT via `sigaction`, sans la modifier ni envoyer de signal au processus.
FAIL réel avant correctif : « Z3 replaced the application's SIGINT handler
during solving ». PASS après `params.set_bool("ctrl_c", false)`.
Les 12 tests ske-solver passent ; le test CLI existant passe 3/3, sans
modification d'assertion ni de délai. Clippy strict workspace passe.

Ce correctif est local, non publié. La CI Linux/macOS reste à relancer ;
le landing reste bloqué jusqu'à sa validation.

## Rectification après la CI de `2b22558`

Le run https://github.com/psyllos/ske/actions/runs/38923701495 échoue encore
sur `cancel_sigint_preserves_streamed_arrivals` après 120 s sous Linux ;
macOS est annulé. La section « Résolution » ci-dessous décrit une tentative
locale, **pas une résolution du blocage CLI**. Les validations locales restent
historiques ; aucun landing sur main n'a été effectué.

Les reproductions manuelles précédentes ont produit des observations
contradictoires et ne suffisent pas à attribuer une cause. Ne pas conclure
à un défaut de `thread::scope`, de livraison des signaux ou de persistance
sans un test ciblé. Le probe temporaire dépendant de `/tmp` a été retiré.

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

## Résolution — parse SMT-LIB non interruptible (date de la reprise)

Racine du blocage : `Z3_solver_from_string` (parse SMT-LIB) n'est ni
interruptible par `ContextHandle::interrupt()` ni borné par les paramètres
`timeout`/`rlimit`, qui ne s'appliquent qu'au `check`. Le texte SMT croît à
chaque déroulage de boucle ; sur un runner lent, un seul parse peut dépasser
45–120 s sans consulter le drapeau `cancelled` ni être interrompu. Vérifié
expérimentalement : pendant un parse de 1,5 M d'assertions (~6 s), répéter
l'interruption ne l'abrège pas (6,13 s).

Correctif dans `crates/ske-solver/src/backend.rs` : le parse est alimenté par
lots de lignes (`FEED_CHUNK_LINES = 16_384`, une ligne = une commande, format
garanti par `crate::encode`), avec contrôle de `cancelled()` et de l'échéance
`deadline` entre lots ; le watchdog existant continue d'interrompre le `check`
en ré-acerbant jusqu'au retour. L'annulation couvre ainsi tout le parcours
natif (parse + set_params + check) au lieu de seulement `check`.

Régression démontrée `cancellation_interrupts_long_parse`
(`crates/ske-solver/tests/cancel_interrupt.rs`) : annulation 50 ms après le
début d'un parse de ~6 s. Avant correctif FAIL (≈6,28 s), après correctif PASS
(≈73 ms). `cancel_interrupts_solver_promptly`, `cancel_sigint` et les 9 tests
`solver` passent. `cargo test --workspace` : 66 lots ok, aucun échec. Clippy
dur et `fmt --check` verts. Relance `python3 scripts/validate-v04.py` :
AC-0.4-01..14 PASS, y compris AC-0.4-14 (`./scripts/verify.sh &&
validate-v03.py`) et la non-régression v0.2 (AC-0.2-22 à nouveau PASS).

Restent, pour le landing : CI distante verte sur `origin/ske/implementation`,
puis réconciliation avec `main` (`d160ad3` vs `9106713` pour la priorité
d'erreurs adaptateur) avant d'avancer la branche partagée.
