# Revue v0.5 — non validée

Revue indépendante :
`ksQQEfcnmHasRuOdMYFaCCg3kpLOABYZgJIBxBDZiRjivQ9OQqTS_tRBC-14`.
Lecture seule sur le checkpoint antérieur aux corrections locales de reprise.
Aucune approbation de fin de jalon n'a été donnée.

| Constat | Traitement / reste à faire |
|---|---|
| Fusion sur rendus identiques et effets omis | Retours comparés par empreintes structurelles, effets chargés, précision dégradée conservativement. Test ajouté sur deux retours opaques distincts. Restent la canonicalisation mémoire et tous les ancrages des cas fusionnés. |
| Lookup d'issues non borné au snapshot | SQL borné au snapshot, LIKE supprimé. Reste à conserver les ids numériques de bout en bout et à rejeter les ambiguïtés inter-unités plutôt que sélectionner la première ligne. |
| Reconstruction non transactionnelle et pagination avant persistance | Transaction ajoutée ; tous les cas sont persistés avant pagination. Tests rollback et offset=1/limit=1 exécutés. |
| Configuration du snapshot absente de la clé de cache | Empreinte de configuration du snapshot ajoutée, namespace moteur v0.5. Restent la formule normative complète, la canonicalisation §4.6 et les lignes analysis_dependencies. |
| Hypothèses perdues lors de la copie | Hypothèses copiées/unionnées avant réhydratation. Test impose les deux hypothèses obligatoires dans chaque run réutilisé. |
| FIELD toujours réhydraté avec index 0 | Payload parsé comme u32 dans les deux lecteurs, erreur sur payload invalide. Test unitaire de réhydratation d'un FIELD d'index 3 exécuté. |
| Ordre des cas non normatif | **Ouvert** : porter des paires numériques décision/issue et un fingerprint structurel indépendant de l'ordre d'exploration. |
| R1/R5 : déduplication de texte des contraintes, garde vide incorrecte | Garde vide corrigée en `true` et testée. **Ouvert** : supprimer l'assimilation de contraintes opaques par texte ; traiter la double garde DIV sans violer R1 (pas de simplification additionnelle). |
| stale-check : correspondance par qualified_name seulement | **Ouvert** : inclure l'identité stable d'unité et refuser toute correspondance ambiguë. |

## Preuves exécutées après corrections locales

- `cargo test -p ske-behavior` : 11 tests PASS, 0 ignoré.
- `cargo test -p ske-cli --test behavior` : 13 tests PASS, 0 ignoré.
- `cargo test -p ske-query reused_field_hydration_preserves_nonzero_index` :
  1 test PASS, 0 ignoré.
- `cargo clippy --workspace --all-targets -- -D warnings` : exit 0.
- `cargo fmt --all` puis `git diff --check` : exit 0.

Les points ouverts sont bloquants pour v0.5, même si les tests existants passent.
Le rapport d'AC partiel `v0.5.md` ne constitue pas une validation de conformité.
