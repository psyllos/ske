# Validation grandeur réelle — projets GitHub réels (Go, Java, Rust)

Demande de psyllos : (1) rien de ce qui est affirmé par la documentation sans
être prouvé par un test ; (2) tests sur des projets GitHub réels (idéalement Go,
Java, Rust) en confrontant ce que SKE produit avec l'analyse manuelle de la
source.

## Projet Go réel : github.com/mitchellh/go-homedir

Bibliothèque réelle, stdlib-only, logique réelle (détection de HOME, expansion
`~`). `git clone --depth 1` du dépôt public.

### Bug réel corrigé — identité des types byte/uint8 et rune/int32

L'ingestion échouait avec `E_TYPE_MISMATCH: CIR local contract violation` sur
`Expand/b2/i1/NE` (`path[1] != '/'`). Cause : l'adaptateur Go émettait deux
enregistrements de type distincts (`go:uint8` et `go:byte`) pour une
représentation identique, car `go/types` distingue `types.Byte` (un Basic
nommé) du singleton `types.Uint8`. La règle CIR `EQ`/`NE` exige l'égalité des
ids de types des opérandes → rejet.

Correction (`adapters/go/internal/lower/types.go`) : canonicaliser les types
`*types.Basic` par leur `Kind()` via `types.Typ[k].Name()`, ce qui fait de
`byte` → `uint8` et `rune` → `int32`. Test de régression
`TestByteRuneAliasTypeIdentity` (adapters/go/internal/lower/lower_test.go).

Résultat après correction : ingestion `status=CLOSED, inventory=COMPLETE, 0
erreur`. Analyse : `Expand` 8 cas de comportement, 12 issues IR toutes
`POSSIBLE SAT_ARRIVAL` — conforme à la source (aucune branche prouvablement
inaccessible) ; les gardes rendues utilisent `<expr:N>` car les chaînes sont
opaques (limite documentée §10.1).

### Confrontation manuelle (Expand, ~5 issues de la source)

| Source | SKE |
|---|---|
| `len(path)==0` → return (path, nil) | cas RETURN garde `== 0` ✓ |
| `path[0]!='~'` → return (path, nil) | cas RETURN garde `!= 126` ✓ |
| `len(path)>1 && path[1]!='/' && path[1]!='\\'` → erreur | 2 branches vraies/fausses POSSIBLE ✓ |
| `Dir() err != nil` → return ("", err) | cas retour avec `nil`/non-nil ✓ |

## Projet Java réel : jbloch/effective-java-3e-source-code (item34)

Sources réelles, JDK-only (Joshua Bloch, Effective Java 3e), package
`effectivejava.chapter6.item34` compilé avec `javac --release 17`.

### Bug réel corrigé — SSA orphelin sur stockages de tableaux

L'ingestion échouait avec `E_SSA_DOMINANCE: undefined ...Inverse.main/v15`.
Cause : le pré-traitement de l'adaptateur Java (Extractor) allouait un slot SSA
pour les lhs non-Local (`$a[i] = v`) ; la descente de ces affectations vers
`UNKNOWN_EFFECT` n'attribuait jamais de définition → valeur SSA orpheline
rejetée par le cœur. Correction : ne réserver un slot SSA que pour des lhs
`Local`.

### Bug réel corrigé — arithmétique IEEE_FLOAT émise illégalement

Après le point précédent, échec `E_TYPE_MISMATCH [instr fn=...Operation$1.apply
/b3/i0/ADD]`. Cause : l'adaptateur émettait `ADD`/`MUL` sur `double`
(IEEE_FLOAT) alors que le cœur ne modélise que BITVECTOR. Correction : rendre
opaques (OPAQUE_OP + diagnostic) l'arithmétique/ordre sur IEEE_FLOAT et toute
égalité d'opérandes de sortes différentes, conformément à la frontière
documentée.

### Confrontation manuelle (item34)

| Source | SKE |
|---|---|
| `Inverse.inverse(Operation op)` switch 4 cas + default | SWITCH CASE:0..3 + DEFAULT, tous POSSIBLE ✓ |
| `PayType.WEEKDAY.overtimePay` `minsWorked<=480 ? 0 : ...` | 1 branche (TRUE/FALSE POSSIBLE) + SUB/MUL/DIV ✓ |
| `PayType.WEEKEND.overtimePay` (pas de condition) | aucune branche ✓ |

Ingestion finale : `status=CLOSED, inventory=PARTIAL, 0 erreur, 699
WARNING` (limites documentées : réflexion, exceptions, open-world, switch maps).

## Projet Rust : sous-ensemble réel (bubble-shell, minipratt, moonwalk)

L'adaptateur Rust est explicitement un sous-ensemble source-only (pas de
macros, attributs, closures, génériques, modules, FFI, extern crates). Les
projets réels testés :

- `JoshMcguigan/bubble-shell` (vrai shell mono-fichier, stdlib only) :
  **rejet propre** `RUST_SOURCE_UNSUPPORTED: macros (...)` — comportement
  documenté, pas de silencieux.
- `matklad/minipratt` (`binding_power`, vraie table de précédence d'opérateurs)
  : dépend de `char` → `MIR_UNSUPPORTED: unsupported MIR constant` (limite
  réelle du modèle, constants char non modélisées).
- Fonctions entières réelles (calendrier/classification) : ingestion
  `CLOSED/PARTIAL`, analyse conforme — `classify` 4 décisions/8 issues (4
  branches source), `bound` 2 décisions/4 issues.

Le `%` et `/` (division par zéro) émettent des asserts MIR au runtime ; ils sont
désormais abaissés en BRANCH réels vers un bloc PANIC par fonction (panique
Rust réelle modélisée, pas un rejet silencieux) — voir
`rust_division_and_remainder_representable` (adapters/rust/tests/adapter.rs).

## Vérifications exécutées

- `go-homedir` : `ske ingest go` → CLOSED/COMPLETE, `branches analyze` sur
  Expand/Dir/dirWindows, `rules list`, `behaviors reconstruct` (8 cas).
- effective-java item34 : `ske ingest java` → CLOSED/PARTIAL, analyse de
  `Inverse.inverse`, `overtimePay` WEEKDAY/WEEKEND.
- Rust : `ske ingest rust` sur bubble-shell (rejet) et fonctions entières
  (succès), analyse de `classify`/`bound`.

## Liens vers les tests ajoutés

- `adapters/go/internal/lower/lower_test.go::TestByteRuneAliasTypeIdentity`
- `adapters/go/internal/lower/lower_test.go::TestAnyInterfaceTypeIdentity`
- `adapters/rust/tests/adapter.rs::rust_loops_and_arithmetic_representable`
- `adapters/java/src/test/java/org/ske/javaadapter/AdapterTest.java::
  subtype_to_supertype_invoke_is_modeled`
- `adapters/java/src/test/java/org/ske/javaadapter/AdapterTest.java::
  array_store_and_float_arithmetic_are_valid_or_opaque`
- Ces tests sont épinglés par `scripts/check-doc-claims.py`.

## Lot 2 — autres projets GitHub au hasard

### joho/godotenv (Go, ~10.7k⭐) — bug corrigé : noms de paramètres dans les signatures

`git clone --depth 1` de github.com/joho/godotenv (stdlib-only, go.mod présent).

**Bug réel corrigé** : l'ingestion échouait sur
`github.com/joho/godotenv.expandEscapes/b0/i1/CALL` avec
`E_TYPE_MISMATCH: [instr .../CALL]`. Cause : `expandEscapes` appelle
`re.ReplaceAllStringFunc(str, func(match string) string {...})` — l'adaptateur
émettait un id de type `go:func(match string) string` pour la closure et
`go:func(string) string` pour le paramètre (interface) du même type de
fonction, car `go/types.TypeString` fait fuiter les noms de paramètres.
La vérification CIR des paramètres d'appel exige l'égalité → rejet.

Correction (`adapters/go/internal/lower/types.go`) : nouvel
`signatureID()` qui construit l'id d'une `*types.Signature` à partir des seuls
types (receiver/paramètres/résultats) récursivement, sans aucun nom. Test de
régression `TestSignatureTypeNamesDoNotLeak` qui reproduit le motif godotenv et
échoue sans le correctif (vérifié par inversion temporaire).

Résultat : godotenv ingère `CLOSED/PARTIAL` (0 erreur), autoload/cmd COMPLETE.
Confrontation : `filenamesOrDefault` 1 décision/2 issues (source : 1 `if`) ✓ ;
`getStatementStart` 3 décisions/6 issues ✓ ; `isSpace` 1 décision source
(SWITCH) → 7 décisions IR (les 7 constantes `\t\v\f\r\n espace 0x85 0xA0`
abaissées en comparaisons) ✓ ; `locateKeyName` 12 décisions/24 issues ✓.

### spf13/cast (Go, ~2.6k⭐)

`git clone --depth 1`. Ingestion `CLOSED/PARTIAL` (0 erreur), 100 fonctions,
unités main PARTIAL + internal COMPLETE. `ToBoolE` (type-switch ~20 cas)
analyse : 19 décisions IR / 38 issues, tous POSSIBLE — cohérent avec le
type-switch abaissé. Le `switch i.(type)` source n'est pas attesté en source
(mapping partiel documenté pour les asserts de type).

### patrickmn/go-cache (Go, ~8.8k⭐) — projet pré-modules, désormais supporté

Projet **sans `go.mod`** (ère pré-modules). Jusqu'ici `packages.Load` en mode
module échouait (`pattern ./...: directory prefix . does not contain main
module`) et le snapshot était ABORTED — frontière documentée, pas de support
GO111MODULE=off. Depuis le support pré-module de l'adaptateur (champ de profil
`gopath_import`), un tel projet se charge explicitement en mode GOPATH :
copie temporaire sous `$GOPATH/src/<import>`, `GO111MODULE=off`, typecheck
source sur les fichiers réels, unités `go:<import>@<goos>/<goarch>`, aucune
métadonnée de module. Voir `docs/soundness-and-limits.md` (section Go —
pré-modules). Sans le flag, l'échec reste explicite et actionnable
(`PROJECT_MODULE_REQUIRED`).

## Lot 3 — campagne systématique (sous-agents scout, en parallèle)

6 scouts (2 Go, 2 Java, 2 Rust) testent ~3 projets réels chacun. Rapports
individuels dans `target/realworld3/<lang>-?/{report.md}`. Les 6 rapports sont
arrivés ; cette section consolide les constats et les corrections qui en ont
découlé.

### go-b (rapport reçu) — 3 projets, 0 bug, preuve formelle

- **go-playground/colors** ✓ : ingest CLOSED/COMPLETE, 34 fns ; `Parse` 4/8,
  `IsLightAlpha` 2/4 — tous POSSIBLE, confrontations conformes.
- **xtaci/smux** ✓ : ingest CLOSED/PARTIAL ; `VerifyConfig` avec **1
  PROVEN_UNREACHABLE formel** (MODEL_UNREACHABLE, ALL_ARRIVALS_UNSAT, 4 arrivées
  éliminées) — correct (contraintes antérieures bornent MaxStreamBuffer). `select
  + default`/goroutine/defer : partiels documentés.
- **spf13/pflag** ✓ : CLOSED/PARTIAL ; `AddFlag` 8/16, `Set` 5/10 EXACT ;
  type-switch (`switch f.Value.(type)`) inventorié mais sans IR (UNKNOWN,
  conservateur).
- Frontière : `matryer/way`, `bmizerany/pat` (sans go.mod, pré-modules) →
  ABORTED propre à l'époque ; désormais chargeables via le champ de profil
  `gopath_import` (même chemin que go-cache ci-dessus).

### java-b (rapport reçu) — vrai bug majeur, corrigé

Voir `docs/work-log.md` (Lot 3) et les 3 correctifs `Extractor.java` :
garde BRANCH BOOL, invoke booléen, élargissement char/byte. Repros et
effective-java items 37/50 ingèrent désormais 0 erreur ; json-simple/commons-
codec étaient bloqués sur E_SSA_DOMINANCE de code généré.

**Évolution (E_SSA_DOMINANCE réduit)** : la cause racine était un bug de
l'adaptateur sur du bytecode ordinaire — un try/catch dont la plage protégée
contient une instruction non-INVOKE qui peut lever (accès tableau, arithmétique,
cast) laissait le bloc handler déconnecté (aucune arête entrante), donc sa valeur
SSA d'exception capturée n'avait pas de définition dominante et le cœur rejetait
l'unité en `E_SSA_DOMINANCE`. Correctif : les blocs concernés se terminent par un
`SWITCH` de dispatch conservateur sur un tag opaque (CASE par handler dans l'ordre
de la table d'exceptions, DEFAULT vers le successeur normal), gardant les handlers
atteignables et dominés sans affaiblir la règle SSA ni changer le schéma. Repro réel :
json-simple 1.1.1 (`Yylex.zzScanError`) passait de `ABORTED`/`E_SSA_DOMINANCE` à
`CLOSED/PARTIAL` 0 erreur ; commons-codec Hex-style `CLOSED/PARTIAL` 0 erreur. Chaque
occurrence porte un diagnostic `EXCEPTION_FLOW_PARTIAL`/`SYMBOLIC` (sur-approximation
assumée).

### go-a (rapport reçu) — bug `any`/`interface{}` réel, corrigé

- **mitchellh/mapstructure** — ABORTED/FAILED : `E_TYPE_MISMATCH` au CALL
  `reflect.ValueOf` (`(*Decoder).Decode/b0/i4`). Cause racine : le paramètre
  déclaré de `reflect.ValueOf` est l'alias prédéclaré `any` (`go:any`) alors que
  l'argument réel (`d.config.Result`, champ `interface{}`) est `go:interface{}`
  — deux ids pour la même représentation → le cœur (`E_TYPE_MISMATCH`, égalité
  exacte, §CIR CALL) rejette. **Corrigé** : `typ()` résout désormais les
  `*types.Alias` (`any`, et `byte`/`rune` en go/types ≥1.22) via
  `types.Unalias`, faisant tomber `any` sur `go:interface{}`. Test
  `TestAnyInterfaceTypeIdentity` (échoue avant, passe après).
- **hashicorp/golang-lru** (génériques) — CLOSED, pkg racine PARTIAL (52
  WARNING documentés : DEFER_UNMODELED 24, SOURCE_MISMATCH 22,
  CONCURRENCY_PARTIAL 5, LOOP_BOUND_REACHED 1), sous-paquets COMPLETE ~110 fns.
  3 confrontations ✓ vs lecture manuelle : `(*Cache).Add` IR 4/8 source 6/12 ;
  `ContainsOrAdd` IR 5/10 source 7/14 ; `Purge` IR 4/8 source 5/10 (mapping
  SHORT_CIRCUIT EXACT, run PARTIAL loop-cut documenté). 0 erreur.
- **inconshreveable/mousetrap** — CLOSED/COMPLETE. `StartedByExplorer` IR 0/0 ✓
  (corps non-Windows = `return false`) ; `trap_windows.go` exclu par build-tag
  (INFO BUILD_TAG_EXCLUDED_FILE, seule limite réelle) ; `init` synthétique non
  confrontable.
- Écartés (discipline) : ryanuber/columnize (sans go.mod, pré-modules ;
  chargeable désormais via `gopath_import`, voir go-cache ci-dessus),
  golang/groupcache (require protobuf réel).

### rust-b (rapport reçu) — sous-ensemble réel étroit, 2 fonctions réelles ✓

Rejet propre au portail source sur rayon-rs/rayon et rust-lang/regex :

- **rayon-rs/rayon** `src/math.rs` → `RUST_SOURCE_UNSUPPORTED: macros (...`
  (déclencheur : `panic!` dans `simplify_range`).
- **rust-lang/regex** `regex-syntax/src/utf8.rs` → `RUST_SOURCE_UNSUPPORTED:
  modules require multi-file provenance` (`#[cfg(test)] mod tests`).
- Frontière réelle : aucune doc/attribut dans un fichier réel (tout `///` est un
  attribut `#[doc]`, `#[test]`/`mod` omniprésents) → aucun fichier entier ne
  passe le portail ; seule l'extraction de fonctions (doc commentée, adaptée)
  marche.

Fait clé (sonde) : la caméra MIR debug rejetait `+ - * << >> -x` (asserts de
DÉBORDEMENT du producer non optimisé) → les compteurs de boucle ne pouvaient
plus progresser, **les boucles étaient non représentables**. **Levé** : la
caméra passe désormais `-Coverflow-checks=off` (sémantique
release : l'arithmétique déborde en wrappant, pas de panic) ; les boucles et
l'arithmétique `+ - * << >> -x` rentrent (OPAQUE_OP conservateur). Frontière
restante fail-closed réelle : `/` et `%` (division par zéro, panic réelle en
Rust) déclenchent toujours un assert MIR → `MIR_UNSUPPORTED`. Voir
`rust_loops_and_arithmetic_representable` (adapters/rust/tests/adapter.rs).

Fonctions réelles qui rentrent (verdicts ✓) :

- **`is_valid_cap_letter`** (regex-lite/src/interpolate.rs:276) : CLOSED/PARTIAL,
  match 4 plages + `_` → 1 décision source (2 issues), 7 décisions IR / 14
  issues, **14/14 POSSIBLE**, exploration fermée — conforme à la lecture
  manuelle (borne basse/haut ×3 + égalité `_`).
- **`is_leading_or_invalid_byte`** (regex-automata/src/util/utf8.rs:138) :
  CLOSED/COMPLETE, expression plate `(b & 0b1100_0000) != 0b1000_0000`, 0/0
  issue — trivialement exact.

Frontières fail-closed réelles : `max_scalar_value` (`unreachable!` → rejet
macros) ; corps de `ByteSet::contains` (`/128 %128`) → rejet MIR
`assert "attempt to divide by zero"` (le `1<<bit` qui plantait avant est
entré grâce à `-Coverflow-checks=off`).
Détails dans `target/realworld3/rust-b/report.md`.

### rust-a (rapport reçu) — 2 rejets macro propres, 3 fonctions réelles fit

- **ripgrep** (`crates/core/main.rs`) et **serde** (`serde/src/lib.rs`) →
  ABORTED/FAILED propre : `RUST_SOURCE_UNSUPPORTED: macros (including include!)
  require expansion provenance; unsupported` (frontière attendue, sous-ensemble
  mono-fichier sans macros).
- Frontière MIR complémentaire (producer non optimisé, checks de débordement
  actifs) : `%`, `-`, `<<` (même constant), `assert!` → `MIR_UNSUPPORTED`
  (terminator assert/drop) ; `match` avec alternatives `|` → PARTIAL.
- **3 fonctions réelles de ripgrep ingérées COMPLETE, confrontations ✓** :
  `byte_range_contains` (ban.rs, 1 décision IR / 2 issues POSSIBLE) ;
  `literal_seq_is_good` (literal.rs ::is_good, 2 décisions / 4 issues dont 1
  PROVEN_UNREACHABLE correct par raisonnement manuel) ; `Match::len` de
  matcher — corps verbatim `end - start` **désormais représentable** après
  `-Coverflow-checks=off` (auparavant rejeté par l'assert de débordement ;
  forme adaptée `saturating_sub` toujours COMPLETE, issues NORMAL+EXCEPTION :
  artefact de modélisation d'appel externe documenté).
- Extraction/adaptation mécanique uniquement (aucun changement de sémantique) ;
  aucun binaire étudié exécuté. Détails et commandes : `target/realworld3/rust-a/report.md`.

### java-a (rapport reçu) — 3e bug Java réel, corrigé (INVOKE retour sous-typage)

- **item30 `RecursiveTypeBound`** et tout appel non-void passant un argument de
  type **sous-type** à un paramètre supertype (`Number←Integer`,
  `Collection←List`) ou `throw <résultat d'appel>` échouait en `E_TYPE_MISMATCH`
  au INVOKE (le cœur exige l'égalité exacte des type-ids, résultat d'appel =
  type de retour déclaré). **Cause racine (dump CIR)** : un local JVM typé large
  (ex. `java:java.lang.Object`) recevant le résultat d'un invoke déclaré plus
  précisément (ex. `java:java.util.List` de `Arrays.asList`) → le slot SSA du
  local était pré-alloué avec le type large, l'INVOKE réutilisait ce slot, et le
  résultat différait du type de retour déclaré du callee.
  **Corrigé** : à la pré-allocation des slots, un local à définition unique dont
  la définition est un `JAssignStmt` à RHS invoke est typé par le **type de
  retour déclaré** de l'invoke (pas le type JVM du local). Les 33 repros iso-*
  du scout (params/retours/varargs/throw/fresh) passent CLOSED 0 erreur ;
  effective-java items 30/37/46 CLOSED/PARTIAL 0 erreur. Test
  `subtype_to_supertype_invoke_is_modeled` (échoue avant le fix en vérifiant
  l'assertion `Arrays.asList result must be typed java.util.List`, passe
  après).
- **item37/item46 (Phase/Plant/Freq)** : CLOSED/PARTIAL 0 erreur, confrontations
  ✓ (SRC 3, 5, 1 décisions correspondant à la lecture manuelle ; switch 16 cas
  sur `Conversion.intToHexDigitMsb0` mappé exactement).
- **commons-lang/commons-collections scopes** : CLOSED/PARTIAL 0 erreur,
  confrontations ✓. Le full-repo >~100 classes avortait en `E_ADAPTER_LIMIT` par
  OOM sous le cap launcher fixe `-Xmx512m` (limite documentée) ; ce cap est
  désormais un défaut `-Xmx2g` configurable via `SKE_JAVA_XMX` et transmis par
  `ske ingest java` (voir `adapters/java/README.md`).
