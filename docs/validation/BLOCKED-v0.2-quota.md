# BLOCKED — v0.2 : limite d'utilisation du fournisseur

**Rapport historique résolu :** les travaux ont été récupérés, les défauts de
revue corrigés et v0.2 validée (25 AC PASS). Voir `v0.2.md` et le dernier journal.
Ne pas reprendre les limitations ci-dessous comme l'état courant.

## Critères concernés

v0.2 non terminée : AC-0.2-02 à AC-0.2-25 non globalement validés.
AC-0.2-01 laboratoire Go PASS. v0.1 reste validée (42 AC).
Ne pas commencer v0.3.

## Opération et sortie exacte

Sous-agent `Implémenter l’adaptateur Go réel` :
`ksQQUFcPQtXkTD6TTuMAxza-4pLOAAUv3pIBxBDZiRjivQ9OQqTS_tRBC-14`.
Événement de terminaison :

```text
Failed: OpenAI error: Your ChatGPT subscription has reached its usage limits.
Check limits, reset times, and other options at
https://chatgpt.com/codex/cloud/settings/analytics#usage.
```

Aucun horaire de réinitialisation n'a été fourni à l'agent coordinateur.
Aucun changement de fournisseur/modèle ni création d'agent de remplacement
n'a été effectué pour contourner cette limite.

## Sauvegarde et hypothèses testées

- Checkpoint v0.1 : branche locale `ske/implementation`, commit `f18e6e8`.
- Bootstrap de l'agent Go fusionné à la version exacte
  `8250d844-05ca-41ca-9172-b50628e47a90.354717` :
  protobuf Go généré, module/sum, frame, profil, load et tests load,
  `scripts/gen-proto-go.sh`.
- Les autres agents ont reçu une demande de checkpoint durable et d'arrêt
  sans nouveau lot. Leurs rapports arrivant ensuite doivent être conservés.
- Deux sous-sous-agents sont découverts via `delta_thread tree/show`, mais
  `merge_worktrees` ne les reconnaît pas dans le workspace courant :
  `no thread with id ... is known to this workspace`.
  Ne pas inventer une fusion réussie. Leurs transcripts sont accessibles.

## Travaux persistés dans des threads séparés

| Lot | Thread |
|---|---|
| AST décisions/compteur | `ksQQUFcPQtXkTD6TTuMAxza-4pLOAAU1k5IBxBDZiRjivQ9OQqTS_tRBC-14` |
| SSA lower/calls | `ksQQUFcPQtXkTD6TTuMAxza-4pLOAAVDcZIBxBDZiRjivQ9OQqTS_tRBC-14` |
| IPC Rust/CLI | `ksQQUFcPQtXkTD6TTuMAxza-4pLOAAU1YZIBxBDZiRjivQ9OQqTS_tRBC-14` |
| Tests Go indépendants | `ksQQUFcPQtXkTD6TTuMAxza-4pLOAAVDy5IBxBDZiRjivQ9OQqTS_tRBC-14` |

Le transcript AST montre `internal/decisions/{emit,count,decisions_test}.go`.
Le transcript SSA montre `internal/{lower,calls}` avec tests.
Ils ne sont pas considérés fusionnés ou validés dans le worktree principal.

### Récupération complémentaire effectuée

Un checkpoint ultérieur du parent Go a permis de récupérer indirectement
`internal/decisions`, `internal/lower` et `internal/calls` :
version `d98918e2-bd0f-4e42-a4d2-fed4410bed78.359401`.
Ces fichiers sont maintenant dans le worktree principal.
`(cd adapters/go && go test ./... && go vet ./... && test -z "$(gofmt -l .)")`
a réussi après cette fusion (packages décisions, load et lower testés).
Cela ne valide pas le binaire complet, absent, ni l'ingestion Go E2E.

## Ce qui manque et reprise

1. Relire `docs/work-log.md`, ce rapport et les rapports worker disponibles.
2. Reprendre les mêmes threads ou récupérer leurs fichiers/checkpoints,
   sans recréer le socle v0.1.
3. Intégrer AST + SSA avec `internal/link`, `internal/emit` et
   `cmd/ske-adapter-go` : le binaire d'adaptateur complet n'est pas encore livré.
4. Intégrer IPC Rust et suites E2E ; exécuter et corriger vrais tests.
5. Mettre à jour génération/verify pour v0.2, créer le tag local candidat
   seulement au checkpoint approprié, exécuter les 25 AC et la régression v0.1.
6. Poursuivre v0.3 seulement après validation complète v0.2.

Le harness `scripts/validate-v02.py` est préparé mais n'a pas encore été
exécuté. Il ne certifie pas de PASS avant exécution.

## Dernière récupération IPC et tests

IPC Rust fusionné depuis
`5bce5661-f147-4b24-9c13-24ff33817351.363024` ; neuf suites de conformité
fusionnées depuis `240f6e67-a732-4f55-81cd-7041cd326a82.364335`.
Voir `docs/worker-go-ipc.md` et `docs/worker-go-tests.md`.

Contrôles du coordinateur après ces fusions :

- `cargo test --workspace --no-run --locked` : PASS, toutes suites compilent.
- `cargo clippy --workspace --all-targets -- -D warnings` : PASS.
- Les sept suites CLI v0.1 explicitement sélectionnées : 58 tests PASS.
- `cargo test -p ske-ingest` : 11 tests PASS.
- `cargo test -p ske-cli --test go_golden_decisions` : **FAIL**, exit 101,
  sans test ignoré, parce que l'intégration du binaire Go manque :

```text
cannot build Go adapter (not the studied project):
stat .../adapters/go/cmd/ske-adapter-go: directory not found
```

Le workspace courant est donc un checkpoint v0.2 partiel, pas une livraison
aux tests tous verts. Le checkpoint v0.1 validé reste `f18e6e8`.

## Dernier checkpoint Go récupéré

Le parent Go a ensuite transmis une version contenant le binaire et le pipeline,
puis son checkpoint final `8250d844-05ca-41ca-9172-b50628e47a90.372464`,
fusionné. `cmd/ske-adapter-go`, `internal/emit` et `internal/link` existent.
Voir `docs/worker-go-adapter.md`. Tous les lots directs sont récupérés.

Nouvelle exécution `cargo test -p ske-cli --test go_golden_decisions` :
**FAIL exit 101**, sur l'ingestion réelle plutôt que le binaire absent.
Diagnostic persistant reproduit dans `target/quota-recovery.sqlite` :

```text
UNIT_FAILED: E_RECORD_ORDER: unit descriptors must be unique and sorted
snapshot ABORTED, inventory FAILED, unité failed:0
```

Cause identifiée à traiter en premier avec régression : le lanceur Rust compare
les clés d'unité brutes lexicalement. §9.3 trie les packages par import path,
et AC-0.2-04 exige `example` avant `example/partial` et `example/tags`.
Le caractère `/` précède `@` dans les clés complètes, ce qui rend ce contrôle
Rust incompatible avec l'ordre attendu. Ne pas changer le tableau d'acceptation.

Les autres limites (nil, mappings SELECT/RANGE, CHA, diagnostics/profils,
tests IPC) restent ouvertes. Le fait que l'extraction Go seule soit déterministe
ne remplace pas sa validation et sa conformité après import Rust.
