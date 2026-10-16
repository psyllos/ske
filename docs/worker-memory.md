# T-0.3-04 — mémoire M1

## Périmètre et état

Implémentation dans `crates/ske-symbolic/src/memory.rs`, tests unitaires inclus.
Lecture effectuée : `AGENTS.md`, état durable, journal, rapports v0.1/v0.2
et interruption historique résolue, **§6.9 intégral**, AC-0.3-10/26.
v0.1/v0.2 non réimplémentées. Aucun changement de Cargo, `lib.rs`, schéma,
migration, moteur ou arène ; aucun push ni merge partagé.

La crate symbolique n'existait pas dans ce fork. L'intégration Cargo et
l'adaptateur vers l'arène restent à faire par le coordinateur/moteur.
Ce rapport ne certifie pas les AC globaux ni la livraison v0.3.

## Contrat d'intégration

`MemoryState<B: MemoryBackend>` est indépendant de l'implémentation des
expressions, sans dupliquer l'arène :

- `B::Expr: Clone + Eq`, `B::Sort: Clone + Eq`.
- `sort(&Expr) -> Sort`, `fresh(&Sort, hint) -> Expr`,
  `fresh_bool(hint) -> Expr`, `ite(guard, yes, no) -> Expr`.
- Les symboles frais doivent être réellement nouveaux, pas internés uniquement
  par le hint. L'adaptateur peut employer les `ExprId`, `Type` et `Arena::havoc`
  de l'arène en cours d'implémentation. Les erreurs de typage de l'arène ne
  doivent pas être masquées.
- `Pointer::new(reference: u64, targets, Precision)` ;
  `Precision::{MustSingleton, MaySet, Unknown}`.
- `MemoryState::allocate(site, reference)` crée un `AllocInstance`.
- `load(&mut backend, &pointer, &sort) -> Expr` ;
  `store(&pointer, value) -> StoreEffect::{Strong, Weak, Havoc}`.
- `mark_escaped(&pointer)`, `havoc_all_memory()`, `fork() -> (left, right)`.
- `Pointer::field(reference, index)` conserve le parent abstrait.

**Responsabilités du moteur :**

1. Les IDs de référence nomment des adresses concrètes SSA, pas les seuls
   ensembles points-to. Réutiliser l'ID pour une copie ou adresse prouvée égale.
   Une allocation dynamique répétée reçoit un nouvel ID de référence, même
   si l'instruction SSA est identique. Interner les adresses de champ par
   `(identité de référence du parent, index)`.
2. `fork` doit créer les branches d'exploration ; `clone` n'est qu'un snapshot,
   et ne crée pas de namespace d'activations. Les objets antérieurs au fork
   sont préservés. Plusieurs forks du même parent restent distincts.
3. Appel potentiellement mutateur : `havoc_all_memory`. Une référence stockée
   qui peut s'échapper : marquer son **objet pointé** échappé. `store` reçoit
   une expression sans information points-to sur la valeur et ne peut pas
   déterminer cet échappement tout seul.
4. Fusion/perte d'identité : invalider la mémoire par havoc et remplacer les
   cibles perdues par `AllocSummary`/`Unknown`. Ne pas réutiliser l'ancien
   journal avec une nouvelle interprétation des IDs.
5. Cible inconnue/non bornée : `load` produit un symbole frais ; `store`
   retourne `Havoc` après invalidation. Produire les diagnostics et dégrader
   la soundness dans le moteur. Les agrégats partiels et accès chevauchants
   de types différents sont surapproximés (valeur libre du type demandé),
   jamais convertis silencieusement en égalité exacte.
6. Les hypothèses `CLOSED_ENTRY_CONTEXT` et `SEQUENTIAL_EXECUTION`, diagnostics
   et soundness de l'état sont à enregistrer par le moteur, pas par ce module.

## Règles mises en œuvre

- Compteur d'allocation et lignée déterministes ; chaque exécution du site
  produit une instance distincte, y compris après forks.
- Mise à jour forte seulement sur `MustSingleton`, un seul objet concret
  (`AllocInstance` ou globale) non échappé ; jamais sur résumé ni paramètre
  simplement représenté par un ensemble singleton.
- Les instances fraîches sont séparées de l'état initial et des autres
  instances fraîches ; jamais de séparation inventée avec un résumé.
- Journal ordonné de toutes les écritures. Une relecture de la même référence
  retrouve son dernier store ; les autres alias possibles utilisent des ITE.
- Gardes d'alias symétriques stables, **libres et sans exclusion mutuelle ni
  contraintes de transitivité inventées**. Toutes les affectations réelles
  d'égalité d'adresses restent admises ; des affectations supplémentaires
  peuvent réduire la précision, pas exclure un comportement réel.
- Valeurs initiales stables par référence/epoch ; lectures initiales liées
  conditionnellement sous alias possible. Havoc incrémente l'epoch et vide
  valeurs, lectures initiales, journal et gardes dépendants.
- Champs : l'identité du parent contrôle séparation et échappement ; les
  chevauchements non prouvés restent possibles, même entre champs différents.

## Validation réelle

Commande exacte demandée :

```sh
cargo test -p ske-symbolic memory_
```

**BLOCKED, exit 101** : `crates/ske-symbolic/Cargo.toml` absent du fork.
Pas de faux PASS Cargo ; aucune modification hors propriété pour contourner
le blocage. Le coordinateur doit exécuter cette commande après intégration,
ainsi que `cargo test -p ske-symbolic memory_ -- --list`.

Validation autonome de ce module :

```sh
rustc --edition 2021 -D warnings --test crates/ske-symbolic/src/memory.rs \
  -o "$DELTA_SCRATCH_DIR/memory-tests"
"$DELTA_SCRATCH_DIR/memory-tests" memory_
"$DELTA_SCRATCH_DIR/memory-tests" --list
rustfmt --edition 2021 --check crates/ske-symbolic/src/memory.rs
```

Dernière exécution complète : **exit 0**, 8 tests PASS, 0 ignoré ;
les huit noms normatifs sont présents. Une invocation antérieure avec budget
terminal 1 seconde a été interrompue après les tests ; la séquence complète
a ensuite été réexécutée avec budget 10 secondes, exit 0.

Le test `memory_alias_chain_soundness` énumère les 27 affectations de trois
adresses à trois paramètres. Après chaque store, chaque lecture d'une adresse
écrite est comparée à un heap concret indépendant, sous les gardes d'alias
correspondant exactement aux égalités concrètes. Cela couvre notamment
`p=q=r`, `p=r!=q`, `p=q!=r`, `q=r!=p` et trois adresses distinctes.
Les autres tests couvrent stabilité, store faible même adresse, non-échappement,
champs, havoc inconnu, répétition du même site, forks répétés et résumé faible.

## Limites / prochaine étape

Récupération d'intégration : le coordinateur a signalé que des fusions causales
avaient supprimé `memory.rs` dans son arbre. Le fichier existe toujours dans
ce fork (542 lignes avant l'ajout de la précision du contrat d'adressage).
Une nouvelle modification du module réaffirme explicitement la responsabilité
du moteur pour les IDs de référence dynamiques ; aucune réimplémentation.

Pas d'exécution SMT, intégration moteur/arène, Clippy workspace ou validation
globale v0.3 dans ce fork. Le backend des tests est un arbre d'expressions
évaluable minimal uniquement sous `cfg(test)`, pas une seconde arène livrée.
Le journal est rejoué linéairement et conservé jusqu'au havoc ; pas de
compaction ni de modèle d'octets/agrégats complet. Ces optimisations ne sont
pas nécessaires au contrat M1 ; le budget mémoire du moteur doit les borner.
