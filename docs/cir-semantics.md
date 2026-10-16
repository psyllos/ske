# Sémantique CIR v1

Les types wire proviennent exclusivement des schémas §5. `BOOL` n'est pas un
entier; les valeurs machine sont des motifs big-endian, sans normalisation NaN.
Les types Go dépendants de l'architecture gardent la largeur du profil.
Les identifiants sont locaux au flux. Les validateurs ne conservent aucune unité.
L'importeur fournit les tables de types et valeurs; dominance, appartenance
aux fonctions, signature RETURN et unicité globale restent à sa charge.

Les modes non WRAP ne deviennent jamais implicitement WRAP au moteur:
ils exigent une approximation et un diagnostic. Un effet optionnel absent
signifie « possible ». Aucune preuve de pureté n'est déduite d'une absence.

### COPY
Copie T vers le même T. Exemple: `r = COPY x`.
### PHI
Sélection par prédécesseur, zéro opérande ordinaire. Exemple: `[b0:x,b1:y]`.
### SELECT
Choix BOOL entre deux valeurs du même type. Exemple: `SELECT c,x,y`.
### UNDEF
Symbole libre typé indéfini. Exemple: `r:i8 = UNDEF`.
### HAVOC
Valeur arbitraire typée. Exemple: `r:i8 = HAVOC`.
### ADD
Addition BV avec mode explicite. Exemple i8 WRAP: `127+1=-128`.
### SUB
Soustraction BV. Exemple u8 WRAP: `0-1=255`.
### MUL
Multiplication BV. Exemple u8 WRAP: `16*16=0`.
### DIV
Division vers zéro; zéro déclenche PANIC. Exemple: `-7/2=-3`.
### REM
Reste du signe du dividende. Exemple: `-7%2=-1`.
### NEG
Opposé BV avec sémantique binaire. Exemple i8 WRAP: `-(-128)=-128`.
### EQ
Égalité de types identiques, hors float. Exemple: `EQ x,y -> BOOL`.
### NE
Inégalité de types identiques, hors float. Exemple: `NE x,y`.
### LT
Ordre BV signé selon le type. Exemple i8: `-1<1`.
### LE
Ordre inférieur ou égal BV. Exemple: `LE x,y`.
### GT
Ordre supérieur BV. Exemple: `GT x,y`.
### GE
Ordre supérieur ou égal BV. Exemple: `GE x,y`.
### BIT_AND
Conjonction bit à bit. Exemple: `3 & 1 = 1`.
### BIT_OR
Disjonction bit à bit. Exemple: `2 | 1 = 3`.
### XOR
Disjonction exclusive. Exemple: `3 ^ 1 = 2`.
### BIT_NOT
Complément limité à la largeur. Exemple u8: `~0=255`.
### SHL
Décalage sans troncature du compte. Exemple u8: `1<<256=0`.
### SHR
Décalage logique ou arithmétique selon le type. Exemple i8: `-128>>100=-1`.
### LOGICAL_NOT
Négation BOOL. Exemple: `!true=false`.
### SIGN_EXTEND
Élargissement avec signe. Exemple i8 -1 vers i16: `ffff`.
### ZERO_EXTEND
Élargissement avec zéros. Exemple u8 255 vers u16: `00ff`.
### TRUNCATE
Conserve les bits faibles. Exemple i16 300 vers i8: `44`.
### BITCAST
Réinterprète un BV de même largeur. Exemple i8 -1 vers u8: `255`.
### EXTRACT_FIELD
Lit un champ STRUCT/TUPLE par FieldAccess. Exemple: `r = s.field[0]`.
### INSERT_FIELD
Remplace un champ et rend le même agrégat typé. Exemple: `s2 = INSERT_FIELD s,x`.
### INDEX
Index ARRAY/SLICE/STRING par BV; hors bornes PANIC. Exemple: `a[i]`.
### LENGTH
Longueur vers BV. Exemple: `LENGTH a`.
### GET_TAG
Discriminant ENUM/INTERFACE vers BV. Exemple: `GET_TAG e`.
### ALLOC
Alloue un objet abstrait, résultat POINTER. Exemple: `p = ALLOC`.
### LOAD
Lit le pointee d'un POINTER/REFERENCE. Exemple: `x = LOAD p`.
### STORE
Écrit le pointee sans résultat. Exemple: `STORE p,x`.
### FIELD_ADDR
Adresse d'un champ de STRUCT pointée. Exemple: `p = FIELD_ADDR s,field[0]`.
### INDEX_ADDR
Adresse d'un élément POINTER/SLICE. Exemple: `p = INDEX_ADDR a,i`.
### ADDRESS_OF
Rend une valeur adressable. Exemple: `p = ADDRESS_OF x`.
### MEMORY_BARRIER
Barrière, sans opérande ni résultat. Exemple: `MEMORY_BARRIER`.
### CALL
Appel callee puis arguments; résultat facultatif, tuple si multiple.
Exemple: `r = CALL f,x`; effets absents conservés possibles.
### NEW_OBJECT
Objet typé par TypeTest, résultat POINTER/REFERENCE. Exemple: `NEW_OBJECT T`.
### TYPE_TEST
Test INTERFACE, BOOL ou TUPLE(T,BOOL) comma_ok. Exemple: `TYPE_TEST i,T`.
### DOWNCAST
Conversion vers type cible, échec PANIC possible. Exemple: `DOWNCAST i,T`.
### MAKE_INTERFACE
Encapsulation dans INTERFACE. Exemple: `MAKE_INTERFACE x`.
### MAKE_CLOSURE
Fonction suivie des captures, résultat FUNCTION. Exemple: `MAKE_CLOSURE f,x`.
### DEFER_PUSH
Empile callee et arguments sans résultat. Exemple: `DEFER_PUSH close,f`.
### RUN_DEFERS
Exécute les appels différés. Exemple: `RUN_DEFERS`.
### RECOVER
Résultat INTERFACE, modélisation partielle. Exemple: `r = RECOVER`.
### EMIT_EFFECT
Effet observable identifié par extension. Exemple: `EMIT_EFFECT http.send,x`.
### UNKNOWN_EFFECT
Effet OpaqueSemantics, mémoire conservatrice. Exemple: `UNKNOWN_EFFECT x`.
### OPAQUE_OP
Opération non modélisée, résultat facultatif et effets explicites conservateurs.
Exemple: `r = OPAQUE_OP x`.
### SPAWN
Lance callee et arguments; concurrence partielle. Exemple: `SPAWN f,x`.
### JOIN
Synchronisation à arité libre. Exemple: `JOIN worker`.
### CHANNEL_SEND
Envoi à deux opérandes, peut bloquer. Exemple: `CHANNEL_SEND ch,x`.
### CHANNEL_RECV
Réception, résultat T ou TUPLE(T,BOOL). Exemple: `r = CHANNEL_RECV ch`.
### LOCK
Verrouille un objet, sans résultat. Exemple: `LOCK mutex`.
### UNLOCK
Déverrouille un objet, sans résultat. Exemple: `UNLOCK mutex`.
### RETURN
Retour ordonné conforme à FunctionDef, zéro arête. Exemple: `RETURN x,y`.
### JUMP
Une arête UNCONDITIONAL ordinal 0 étiquetée NEXT. Exemple: `JUMP b1`.
### BRANCH
BOOL, TRUE ordinal 0 puis FALSE ordinal 1, garde inversée sur FALSE.
Exemple: `BRANCH c -> b1,b2`.
### SWITCH
Tag BV, CASE ordonnés distincts puis DEFAULT final. Exemple: `{0,1}:b1; default:b2`.
### INVOKE
Appel avec NORMAL ordinal 0 puis EXCEPTION; dispatch inconnu surapproximé.
Exemple: `INVOKE risky -> normal,catch`.
### THROW
Sortie exceptionnelle à un opérande, zéro arête. Exemple: `THROW exception`.
### PANIC
Sortie anormale à un opérande, zéro arête. Exemple: `PANIC value`.
### UNREACHABLE
Chemin interdit par le compilateur, zéro opérande/arête. Exemple: `UNREACHABLE`.
