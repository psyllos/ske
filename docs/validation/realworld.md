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

Le `%` (division par zéro) déclenche un assert MIR non modélisé → rejet
explicite (fail-closed), conforme au contrat.

## Vérifications exécutées

- `go-homedir` : `ske ingest go` → CLOSED/COMPLETE, `branches analyze` sur
  Expand/Dir/dirWindows, `rules list`, `behaviors reconstruct` (8 cas).
- effective-java item34 : `ske ingest java` → CLOSED/PARTIAL, analyse de
  `Inverse.inverse`, `overtimePay` WEEKDAY/WEEKEND.
- Rust : `ske ingest rust` sur bubble-shell (rejet) et fonctions entières
  (succès), analyse de `classify`/`bound`.

## Liens vers les tests ajoutés

- `adapters/go/internal/lower/lower_test.go::TestByteRuneAliasTypeIdentity`
- `adapters/java/src/test/java/org/ske/javaadapter/AdapterTest.java::
  array_store_and_float_arithmetic_are_valid_or_opaque`
- Ces deux tests sont épinglés par `scripts/check-doc-claims.py`.

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

### patrickmn/go-cache (Go, ~8.8k⭐) — limite réelle de l'adaptateur

Projet **sans `go.mod`** (ère pré-modules) : `packages.Load` échoue avec
`pattern ./...: directory prefix . does not contain main module`. L'adaptateur
Go nécessite un module Go (go.mod). Comportement honnête (échec clair, pas
d'analyse incorrecte), à documenter comme frontière : pas de support GO111MODULE=off.
