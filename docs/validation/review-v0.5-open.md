# Revue v0.5 — clôturée, jalon validé

Revue indépendante :
Une revue indépendante a été menée sur le checkpoint antérieur aux
corrections locales de reprise (thread
`ksQQEfcnmHasRuOdMYFaCCg3kpLOABYZgJIBxBDZiRjivQ9OQqTS_tRBC-14`).
Aucune approbation de fin de jalon n'avait été donnée à ce stade.
Après les corrections de la reprise et la validation complète (voir la
section « Preuves exécutées »), tous les constats sont traités et v0.5 est
déclarée validée par l'exécution réelle de `python3 scripts/validate-v05.py`
(exit 0, 13/13 AC+non-régression 110 AC antérieurs).

| Constat | Traitement / preuve |
|---|---|
| Fusion sur rendus identiques et effets omis | Retours comparés par empreintes structurelles, effets chargés, précision dégradée conservativement. Test `behavior_dedup_distinguishes_opaque_returns_and_preserves_precision` ; ancres de tous les cas fusionnés conservées via `summary_case_ids` (`behavior_dedup_retains_every_summary_and_result_site` : 2 DERIVED_FROM + 2 RESULT_SITE). |
| Lookup d'issues non borné au snapshot | SQL borné au snapshot, LIKE supprimé, ids numériques d'outcome conservés de bout en bout (`outcome_ids`, `outcome_identity`), rejet d'ambiguïté par unité (`E_AMBIGUOUS`), ordre par décision/issue numériques dans le chemin IR. |
| Reconstruction non transactionnelle et pagination avant persistance | Transaction ajoutée ; tous les cas persistés avant pagination. Tests `behavior_pagination_persists_all_cases_without_renumbering` et `behavior_failure_rolls_back_scenario_run_and_rules`. |
| Configuration du snapshot absente de la clé de cache | Empreinte de configuration du snapshot + budget + namespace `ske-summary-scalar-0.5-cache-v2` (test : build tag sans effet de corps invalide la réutilisation). Canonicalisation §4.6 `structural()` indépendante des ids de flux, des spans et des types inutilisés (`structural_fingerprint_ignores_flow_ids_spans_and_unused_types`) ; lignes `analysis_dependencies` inscrites (FUNCTION/SUMMARY, vérifiées dans `summaries_reuse_across_snapshots`). |
| Hypothèses perdues lors de la copie | Hypothèses copiées/unionnées avant réhydratation. Test imposant les deux hypothèses obligatoires dans chaque run réutilisé. |
| FIELD toujours réhydraté avec index 0 | Payload parsé comme u32 dans les deux lecteurs, erreur sur payload invalide. Test `reused_field_hydration_preserves_nonzero_index`. |
| Ordre des cas non normatif | Port de paires numériques décision/issue dans `ir_path` et fingerprint structurel indépendant de l'ordre d'exploration ; tri par span puis chemin numérique (tests `behavior_case_ordering_by_span_then_ir_path`). |
| R1/R5 : déduplication de texte des contraintes, garde vide | R1 respecté strictement : plus aucune assimilation par texte (répétition `b != 0 AND b != 0` conservée, `render_guard_r1_preserves_repeated_atoms`) ; la double garde DIV n'est plus dupliquée dans l'arène (`same_boolean_predicate` dans le moteur, garde vide rendue `true`). |
| stale-check : correspondance par qualified_name seulement | Correspondance par qualified_name dans l'unité correspondante (`unit_key`) ; `E_AMBIGUOUS` si plusieurs fonctions cibles ; transaction avec rollback. Test `rules_stale_check_detects_function_change`. |

## Preuves exécutées (validation finale v0.5)

- `python3 scripts/validate-v05.py` (sans `--only`), `SKE_EXPECT_VERIFY_VERSION=v0.5` : **13/13 AC PASS**, exit 0.
- AC-0.5-12 rejoue `./scripts/verify.sh` (fmt, clippy `-D warnings`, check-deps, gen-proto, build `-D warnings`, db check 50 tables/21 triggers, fixtures, compteurs, Go vet/test/build, ingest go, db check, `cargo test --workspace`, `validate-v03.py --verify-cases`, snapshots) une seule fois puis `validate-v04.py` en mode `SKE_INHERIT_VERIFY=1` : **14/14 AC PASS**, et en cascade v0.3 (29), v0.2 (25), v0.1 (42) — tous PASS, verify.sh exécuté une seule fois pour toute la chaîne. Voir `target/validation-v0*/results.json`.
- Tests v0.5 : ske-behavior 12 + ske-cli behavior 14 + ske-query 16 (incl. `structural_fingerprint_...` et `reused_field_hydration_preserves_nonzero_index`), aucun ignoré.
- Logs complets : `target/validation-v05/AC-0.5-*.log`.

La validation finale constitue l'acceptation de conformité v0.5 ; aucun AC
n'est déclaré PASS sans exécution de sa vérification.
