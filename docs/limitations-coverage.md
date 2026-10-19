# Couverture des limitations — inventaire consolidé et solutions (TDD)

Source de vérité croisée : `docs/soundness-and-limits.md`, les ADR, la campagne
grandeur réelle (`docs/validation/realworld.md`), la revue v0.7
(`docs/validation/review-v0.7-open.md`) et le journal (`docs/work-log.md`).

Principe : chaque limitation documentée est cartographiée vers une solution
concrète, avec plan TDD. Une limitation est « couverte » quand son comportement
est soit levé par une implémentation testée, soit explicitement borné avec un
diagnostic honnête (jamais un faux PASS, jamais un test affaibli).

| # | Limitation (source) | Solution proposée | Statut |
|---|---|---|---|
| L01 | Rust : `/` et `%` → `MIR_UNSUPPORTED` (assert MIR rejeté, toute la fonction tombe) — ripgrep/regex | Abaisser `assert(<cond>) -> [success, unwind continue]` en BRANCH gardé vrai→panic / faux→succès ; `Div`/`Rem` restent OPAQUE_OP ; le moteur gère déjà le panic DIV/REM | **Livré** (adapters/rust/src/lower.rs, test `rust_division_and_remainder_representable`, 13/13 adaptateur) |
| L02 | Go : projets pré-modules (pas de `go.mod`) → `E_ADAPTER_LIMIT`/ABORTED (go-cache, way, pat, columnize) | Fallback GOPATH (`GO111MODULE=off`) avec GOPATH temporaire + profil `gopath_import` ; modules absents non inventés | **Livré** (adapters/go/internal/load, tests `TestPreModule*`, byte-for-byte module) |
| L03 | Java : gros dépôts >100 classes → `E_ADAPTER_LIMIT` (`-Xmx512m` launcher) | Heap configurable `SKE_JAVA_XMX` + défaut `-Xmx2g` ; `--max-heap` ; transmission par `ske ingest java` | **Livré** (launcher.sh.in, Main.java, Test `launcher_heap_is_configurable`) |
| L04 | Java : `E_SSA_DOMINANCE` sur code généré (json-simple Yylex, commons-codec) | Fix adaptateur : dispatch SWITCH conservateur pour handlers déconnectés (règle SSA intacte, diagnostic `EXCEPTION_FLOW_PARTIAL`) | **Livré** (Extractor.java, test `implicit_exception_handlers_keep_ssa_dominated`, json-simple CLOSED/PARTIAL) |
| L05 | Harnais v0.7 : sortie zéro après échec analyse/requête ; `--check` sans effet ; RSS octets vs KiB ; binaire périmé ; bras « sans structuré » non comparable | Re-vérifié : points 3/4/6/7 déjà satisfaits ; 17 tests de garde-fou + confirmé par campagne réelle | **Livré** (scripts/test_bench_harness.py ; seule lacune : Linux non rejoué ici) |
| L06 | LLM réel : transport HTTPS OpenAI non fourni (`LlmTransport` = extension documentée) | Implémenter un `OpenAIHttpsTransport` derrière le trait (testé via serveur loopback TLS) ; zéro token en validation ; confidentialité inchangée | À planifier (ADR) |
| L07 | Intervalles v0.4 : pas de raffinement systématique par les gardes ; débordement → plage type complète | Abstract interpretation : raffinement garde-guidé (widening/narrowing), raffinement SMT par les BRANCH | À planifier (moteur, ADR) |
| L08 | Récursion : 2 dépliages par défaut, pas de point fixe récursif | Point fixe avec widening sur les résumés (chaîne de contextes bornée) | À planifier (moteur, ADR) |
| L09 | Mémoire M0/M1 : pas de modèle de heap complet ; writes faibles ; havoc conservateur | Heap abstrait (sites d'allocation + alias), mises à jour fortes sous non-alias prouvé | À planifier (moteur, ADR) |
| L10 | Tableaux/chaînes/flottants : sortes opaques, pas de raisonnement lexical exact | Sorts opaques documentés (actuel) ; encodage borné sous-ensemble si besoin métier avéré | Borné volontairement |
| L11 | Concurrence : SPAWN/channels/defer/RECOVER non explorés | Frontière conservatrice persistée (actuel) ; modèle événementiel explicite si besoin | Borné volontairement |
| L12 | Context Builder : estimation `ceil(caractères/4)`, pas une tokenisation réelle | Tokenizer déterministe réel (ex. BPE) configurable par modèle + budget | À planifier |
| L13 | Reproducibilité toolchain Rust sur machine neuve non démontrée (ADR-022-rust) | Vérifier `rustup toolchain install nightly-2026-10-15` + correspondance hash complète | À vérifier |
| L14 | Go `any`/`byte`/rune/signatures/type-switch : mapping partiel (pas d'attestation type-switch) | Améliorations ciblées déjà livrées ; type-switch source reste partiel (gain marginal) | Partiel assumé |
| L15 | Go nil slices/maps/chans opaques (ADR-020) | Frontière OPAQUE_OP conservatrice (actuel), invariants testés | Borné volontairement |
| L16 | Scénario/capacité et règles fonctionnelles `UNVALIDATED` (pas de validation humaine/LLM) | Modèle de validation externe (hook) avec marqueurs explicites | À planifier |
| L17 | Qualitude/rappel LLM non mesurés (zéro token dépensé) ; benchmark « qualité » est de la récupération lexicale | Corpus oracle indépendant + métriques précision/rappel honnêtes (déjà en place), bras LLM réel hors périmètre coût | Partiel assumé |
| L18 | Récursion/effectifs v0.4 : sorties exceptionnelles et appels non composés (résumés composables seulement à sorties RETURN) ; panics d'appels non explorés | Étendre la composition aux sorties PANIC/THROW connues + effet d'état documenté ; hypothèses conservées par le vérificateur | À planifier (moteur, ADR) |
| L19 | Rust v0.6 : inventaire d'adaptateur incomplet (match/`?` non prouvés complets, Drop/unwind/abort, macros, async, closures, unsafe/FFI, dépendances Cargo, toolchain neuve ADR-022) | Pilotage MIR élargi pas à pas, chaque capacité avec TDD et rejet fail-closed faute de support | À planifier (adaptateur) |
| L20 | Java v0.6 : `world_closed=false` (réflexion/cibles dynamiques), gardes d'exceptions non modélisées, chargement sans build du projet étudié | Modèle d'ouverture explicite (resolveurs externes), gardes d'exceptions bornées, diagnostic conservateur | À planifier (adaptateur) |
| L21 | Chemins/prouve : `NO_PATH_FOUND` ≠ inaccessibilité ; rejeu de preuve ne certifie pas la traduction source→CIR | Conservations par design (graphe + fermeture + vérificateur indépendant) ; ne pas convertir UNKNOWN en preuve | Borné volontairement |
| L22 | Boucles : pas de point fixe récursif ni invariant inductif hors `unroll`/`intervals` ; pas de `PROVEN_UNREACHABLE` sur run non fermé | Invariants inductifs candidats via itération d'intérêts + raffinement SMT ; garder la règle de fermeture | À planifier (moteur, ADR) |

## Règles de conduite appliquées

- TDD : test rouge d'abord, correction minimale, non-régression après.
- Aucun golden produit par SKE ; aucun test affaibli ; aucun PASS sans exécution.
- Aucun changement de schéma/migration normatif silencieux ; les choix structurants
  passent par ADR.
- Aucun push/merge vers une branche partagée sans instruction explicite.

## Prochaines étapes

1. Réception des 4 lanes (A-D) : fusion des worktrees, non-régression séquentielle.
2. Campagne de validation ciblée + mise à jour `docs/implementation-status.md` et `docs/work-log.md`.
3. Choisir avec le commanditaire les items moteur (L07-L09, L12, L16) à planifier en jalon.
