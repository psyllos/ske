# Régressions de la revue indépendante Go v0.2

Les six défauts ont été reproduits par de nouveaux tests **avant** correction :
`go test ./internal/lower ./internal/link -run TestReview -count=1` échouait
sur les cinq scénarios Go ; `cargo test -p ske-cir --test nil_trust` échouait
sur la constante OPAQUE `go:map[`. Aucun test ignoré ou golden réécrit.

| Défaut | Règle corrigée | Régression |
|---|---|---|
| IndexAddr dérivé de *array nil | Inventorier NIL_DEREFERENCE sur IndexAddr pour une base pointeur non prouvée non nulle, en plus des bornes ; pas de doublon sur le LOAD | `TestReviewArrayPointerIndexInventoriesNil` : lecture/écriture/adresse, contre-tests slice/allocation locale |
| DNF exponentielle non bornée | Énumération paresseuse bornée à 4096 **pendant** la construction, appariement des atomes sans énumérer la DNF ; limite atteinte ⇒ attestation incomplète | `TestReviewActivationEnumerationIsBounded`, `TestReviewActivationLimitKeepsInventory` (4096 et 8192 alternatives), test CLI `go_activation_limit_is_persisted_without_shrinking_inventory` |
| Null accepté par préfixe libre | Liste des kinds du cœur stricte, types Go hors contrat matérialisés explicitement en SSA opaque ; ADR-020 | `opaque_descriptors_cannot_authorize_null_constants`, `TestNilablesAndStrictCallSignatures`, test CLI `go_nil_outside_core_literal_contract_is_explicitly_opaque` |
| Négation composée perdue dans la provenance | Conserver la parité des négations englobantes jusqu'au terme terminal et la combiner à celle de l'atome | `TestReviewCompoundNegationProvenance` (négation simple et double) ; `TestReviewLazyPathsPreserveShortCircuitTerms` |
| Range sur canal non mappé | Apparier le résultat comma-ok du receive **au header de ce range**, pas un autre receive de son corps | `TestReviewChannelRangeHeaderMapping` |
| Mauvais impact du diagnostic | `SOURCE_ACTIVATION_INCOMPLETE` ⇒ INFO/SYMBOLIC | `TestReviewIncompleteDiagnosticTriplet`, vérification SQL du test CLI de limite |

## Résultats ciblés exécutés après correction

- `go test ./internal/lower ./internal/link -count=1` : PASS.
- `go test ./internal/link -run TestReview -count=1` : PASS, dont bornes et
  inventaire intact (27 décisions et 54 issues pour 13 groupes booléens).
- `cargo test -p ske-cir` : PASS (unitaire, contrats, nil_trust, doctest).
- `cargo test -p ske-cli --test go_review --test go_diagnostics --test go_vs_cir`
  : PASS, 2 + 1 + 1 tests réels, aucun ignoré.

La validation globale v0.2 est consignée séparément dans `v0.2.md`.

## Validation complète et budget de requête toolchain

Après un échec transitoire de `go tool dist list` au budget de 5 secondes,
la requête est bornée à 30 secondes. Son timeout et la récollection du groupe
restent testés à délai court distinct. `cargo test -p ske-ingest --lib profile_`
passe cinq tests.

L'exécution complète finale `python3 scripts/validate-v02.py` a donné
**25 PASS, exit 0**, dont les **42 AC v0.1 PASS**, 143 tests Rust et aucun
ignoré. `verify.sh` termine par `VERIFY OK v0.2`.
