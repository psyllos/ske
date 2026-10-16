# Questions et choix conservateurs

## 2026-10-15 — §24 et progression des jalons

La demande vise une implémentation complète. §24 prescrit v0.1, puis v0.2,
puis v0.3, et §0.4 interdit de commencer un jalon avant validation du précédent.
Choix : terminer les vrais critères de chaque jalon, sans créer des interfaces
vides présentées comme des fonctionnalités ultérieures. Aucune dérogation
n'est accordée par le coordinateur.

## 2026-10-15 — §8.2 et sécurité des preuves

Le schéma doit être copié tel quel, même si des limites sont découvertes.
Les observations de l'agent stockage sont consignées dans `worker-storage.md`.
Elles ne valent ni correction du contrat ni dérogation approuvée. Avant tout
producteur de preuve v0.3, revoir les possibilités de transfert de faits entre
runs et les exigences de fermeture globale avec un vérificateur indépendant.

## 2026-10-15 — macOS et AC-0.1-35

La commande de référence utilise `/usr/bin/time -v` Linux. macOS exige `-l` ;
la mesure locale sera identifiée comme mesure macOS, pas comme exécution de
la commande Linux. La spécification rend cette mesure non bloquante sous macOS.

## 2026-10-15 — preuve SQL inter-snapshot (revue indépendante)

Le schéma exact autorise l'insertion d'une preuve `ACCEPTED` liée à un run
d'un snapshot et à une issue d'un autre snapshot. Les 22 contrôles ne détectent
pas ce cas tant qu'aucun `branch_analysis_results` ne référence cette preuve
(CHK-16 porte sur les résultats). Ce n'est pas un producteur de preuve valide.

Choix conservateur : ne pas altérer la migration copiée §8.2 ; le futur
vérificateur doit contrôler explicitement cette identité avant toute insertion.
Ajouter une régression au jalon v0.3, et proposer une nouvelle migration si une
protection SQL supplémentaire est retenue. Aucune preuve n'est produite en v0.1.

## 2026-10-15 — revue CIR : durcissement en cours

La revue a signalé : callee GLOBAL non-fonction, signature FUNCTION non
vérifiée, payload CallSemantics incohérent, NEG non WRAP, catch-all INVOKE
non final/multiple et construction publique BitVec non canonique. Ces points
sont assignés à l'agent CIR avec régressions. Ils ne sont pas dérogés ; attendre
intégration des correctifs et revalidation avant toute conclusion globale.

## 2026-10-15 — AC-0.1-38 et expansion de séquence sur macOS

La commande littérale `ls docs/adr/ADR-0{01..19}.md` échoue sous le Bash 3.2
fourni par Apple : ce shell retire le zéro de `01..09` et cherche `ADR-01.md`.
Les 19 fichiers normatifs `ADR-001.md` à `ADR-019.md` existent avec leurs cinq
sections. La même commande exacte réussit sous `/bin/zsh`, disponible sur cette
machine. Le harness exécute donc ce seul contrôle sous zsh sur macOS, en
enregistrant le shell ; `verify.sh` reste exécuté sous Bash comme exigé.
Aucun alias de document n'a été ajouté pour masquer la différence du shell.

## 2026-10-15 — Go nil et types CIR (§9.4 / §4.4)

Le lot SSA signale que `ssa.Const nil` sur SLICE/FUNCTION/map/chan doit
préserver le type réel, alors que le validateur de littéraux actuel n'accepte
`null_value` que sur POINTER/REFERENCE/INTERFACE. §9.4 exige `nil ⇒ null_value`.
Ce point reste à traiter avec régressions positives/négatives avant ingestion
réelle Go : ne pas convertir arbitrairement ces valeurs en POINTER pour passer
la validation. En attendant, préserver le rejet explicite plutôt qu'accepter
silencieusement une représentation sémantiquement fausse. Aucun schéma modifié.

### Résolution conservatrice — revue v0.2

ADR-020 retire l'élargissement par préfixe du descripteur. Le cœur reste strict ;
les nil Go SLICE/FUNCTION/map/chan sont des valeurs SSA explicitement opaques,
de leur vrai type, avec diagnostic. Leur nullité n'est pas présentée comme
précisément modélisée. Régressions unitaires et ingestion réelle exécutées.
