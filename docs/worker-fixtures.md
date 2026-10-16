# T-0.1-06 — sources, CIR fixtures et golden manuels

## Contrat livré

- `examples/go-branch-lab/` : les **23 fichiers Go** de §15.1, y compris
  `boolean_activation.go`, `allocation_loop.go`, `switch_fixture.go` et
  `global_read.go`, avec tabulations, LF et LF final. `go.mod` déclare
  `module example` et `go 1.22`. Le profil §11.4 est Linux/amd64, sans tests
  ni CGO, `patterns=["./..."]`, sans version de Go inventée.
- `crates/ske-cir/examples/gen_fixtures.rs` : transcription des **7 programmes**
  de §15.2.1. `ske_cir::proto` fournit les messages prost et leurs modules
  `record`, `operation_payload`, `literal_value`.
- `tests/fixtures/cir/` : **40 flux binaires** (7 positifs et 33 négatifs).
  Chaque record protobuf est précédé d'un **u32 big-endian**, pas d'un varint.
- `tests/golden/cir/` : 7 golden écrits à la main depuis §15.2.1/2 et §15.5,
  avec les 22 compteurs du tableau normatif. Les décisions portent leurs IDs
  complets. Les tables non mentionnées doivent rester à zéro, sauf les trois
  lignes repository/snapshot/unit décrites par §15.2.2.
- `tests/golden/go/` : un golden manuel par fonction attendue de §15.1.5,
  y compris closures, instances génériques, fonctions sans décision et `init`.
  Les IDs source locaux `sdN` suivent l'exemple §15.5.

**Aucun golden n'a été dérivé d'une sortie de SKE.** Le générateur ne lit ni
n'écrit les golden. Seuls ses tests les lisent.

## Génération

À la racine du dépôt :

```sh
cargo run -p ske-cir --example gen_fixtures -- --negative --out tests/fixtures/cir
cargo test -p ske-cir --example gen_fixtures
```

Sans `--negative`, seuls les 7 positifs sont écrits. Une destination existante
n'est pas purgée : employer un répertoire neuf pour compter les sorties.
`--out` peut être absolu ou relatif au répertoire courant ; la source Go est
résolue par rapport au manifest du crate, indépendamment du répertoire courant.
Arguments inconnus et `--out` sans argument sont refusés.

Le générateur relit les vrais octets des sources avec `ske_common::sha256_hex`
et les compare à des empreintes de référence pour empêcher une dérive silencieuse
des spans. Ces empreintes sont également placées dans `SourceFileDef`.

| Fichier | SHA-256 des octets réels |
|---|---|
| `decide.go` | `39c3272b8bffb72c31fe2b395b2129f183760662b42a2a8d07aa44e08a9b6972` |
| `check.go` | `eb40f7f8e96a0bbe76aa9e25dfa239c5fd45f806a055acb61f6049b4bf13b4c0` |
| `adjusted.go` | `b2b2a9a1d4da9452dbe7b58484dfb316d479a39e09146e2357cbce746c112631` |
| `memory.go` | `9d76d23b9d1eb104e209a97fcfbc6b3e15856cf44b7395976cc68828b94ae887` |
| `switch_fixture.go` | `1fec02c9436ca998a91a04fc4bce54129a283ced6e0ec5a81dc893f433e42867` |

**Sécurité :** ni le générateur, ni les tests de fixtures n'exécutent Go, un
build du projet source ou un adaptateur. Le profil est une donnée, pas une
permission d'exécution.

## Choix normatifs et corrections v1.4

- `Header` v1.3, profil `go-fixture-linux-amd64`, empreinte de build
  `sha256_hex(b"fixture")`, même `unit_key=go:example@linux/amd64` partout.
  Importer chaque positif dans une base/snapshot neuf, pas les sept ensemble.
- Les IDs PARAM/SSA utilisent une seule séquence `vN`, les constantes `kN`
  suivent l'ordre de déclaration et les GLOBAL sont `global:<nom qualifié>`.
  Exemple `MemOpaque` : LOADs `v1`, `v3` (Hook), `v4` ; comparaisons `v2`, `v5`.
- `Check/sd1/o0` pointe sur `example.Check/b2/i0/e1`, `o1` sur `…/e0`,
  avec `inverted=true`. Il n'existe pas de `b2/i1` dans cette fixture.
- Chaque issue source a exactement un lien exact et un cas d'activation exact
  à un terme ; `invoke` n'a ni source ni cas. `adjusted` garde ses trois arêtes
  CFG mais **deux** issues IR, son JUMP n'étant pas une décision.
- LOAD reçoit `MemorySemantics{access_kind="LOAD"}`, requis par §4.3.
  L'interface `go:error` a `nullable=false` (champ interdit par la table §4.2),
  mais sa constante `null_value=true` est autorisée. Les pointeurs sont nullable.
- `invoke` contient le stub externe `example.risky`, type fonction sans résultat,
  et une cible statique. `may_raise`/`may_mutate_memory` sont explicitement vrais ;
  aucune pureté ou fermeture du monde n'est inventée.
- `switch3` représente le SWITCH CIR conceptuel à trois issues, **pas** un
  résultat observé de go/ssa.

## Négatifs et isolation

La liste `NEGATIVES` du générateur contient les 33 noms de §15.2.3.
Chaque fichier repart d'un positif neuf (`decide`, sauf les trois mutations
de payload/PHI utilisant `adjusted`).

Les mutations composites v1.4 sont conservées : suppression des usages pendants
pour SSA multi-définition, suppression du lien et du cas de l'arête retirée
pour `E_EDGE_SHAPE`, remplacement du comparateur par une constante booléenne
pour `E_TERMINATOR_NOT_LAST`. Le test de dominance ajoute NEG avant RETURN
dans `b1` puis utilise le résultat dans `b3`. Le test interfonction ajoute une
fonction `Other` bien typée, avec son propre paramètre et RETURN.

`E_UNIT_DUPLICATE` n'est **pas** un fichier négatif : le test CLI doit passer
deux fois le même `decide.cir.pb` avec deux `--input`. Pour tous les négatifs,
le harnais d'intégration doit vérifier exit 3, code exact et rollback total
(y compris repositories/snapshots). `bad_source_hash_mismatch` doit être importé
avec `--source-root examples/go-branch-lab`.

## Grand flux

```sh
cargo run --release -p ske-cir --example gen_fixtures -- --big --out /tmp/ske-big
```

`big.cir.pb` n'est **jamais à committer**. Il contient 2 000 fonctions locales
indépendantes, 2 000 blocs et **200 000 instructions** (§17) : 99 NEG bien typés
puis un RETURN par fonction. Le générateur écrit record par record avec
`BufWriter`, sans construire toute l'unité en mémoire.
Les autres sorties positives sont aussi produites lorsque `--big` est employé.

## Vérifications effectuées et limites

Effectuées dans le fork T06 :

1. Comparaison indépendante Python des 23 fichiers Go aux blocs de §15.1,
   en convertissant chaque groupe de huit espaces d'indentation en tabulation :
   **23 correspondances exactes**, LF final compris.
2. `cargo run -p ske-cir --example gen_fixtures -- --negative --out tests/fixtures/cir` :
   succès, 40 fichiers produits.
3. `cargo test -p ske-cir --example gen_fixtures` : **4 tests passent**,
   aucun ignoré.
   Ils vérifient tous les types/valeurs/instructions/arêtes avec les validateurs
   locaux, les 22 compteurs de chaque fixture contre les golden manuels, le texte
   exact aux spans de décisions source, et l'égalité byte-à-byte avec les
   binaires versionnés, y compris les 33 négatifs.
4. Deux générations fraîches `--negative` sous `$DELTA_SCRATCH_DIR/fx1` et `fx2`,
   puis `diff -r` entre elles et avec `tests/fixtures/cir` : aucune différence.
   Même procédure qu'AC-0.1-17, avec répertoires scratch plutôt que `/tmp/fx1/2`.
5. Génération release `--big` dans scratch : succès, **36 621 518 octets**.
   Le test dédié décode chaque record avec son encadrement u32 BE et vérifie
   exactement 2 000 fonctions/blocs, 200 000 valeurs et 200 000 instructions.
6. `gofmt -l` des 23 fichiers : aucune sortie ; `rustfmt --check` des deux
   fichiers Rust : succès.

Le Clippy strict du crate était bloqué par trois lints de la lib CIR importée
(`large_enum_variant`, `should_implement_trait`, `manual_is_multiple_of`),
signalés à son propriétaire. Le contrôle de l'exemple avec `-D warnings` et ces
trois lints temporairement autorisés passe ; **ce n'est pas** AC-0.1-02.

**Non revendiqué ici :** tests SQLite/CLI, rollback des 33 négatifs, import du
grand flux et mesure RSS (AC-0.1-35), contrôle global de tous les AC v0.1.
Les tests de compteurs sur records ne remplacent pas les `SELECT COUNT(*)`
d'AC-0.1-19 ; ceux-ci restent à exécuter dans le harnais CLI intégré.

Les compteurs IR Go exacts de `DaySwitch`, `SumRange`, `Pipe` ne sont pas
inventés. Leurs golden n'ont pas de champ `ir_decisions` exact : le futur harnais
Go doit vérifier ≥1 ; `DaySwitch` exige ensuite une observation indépendante
de go/ssa épinglée et une mise à jour documentée. Les propriétés `P-*` et
diagnostics dans ces golden sont des attentes à tester en v0.2, pas des tests
d'adaptateur déjà exécutés.
