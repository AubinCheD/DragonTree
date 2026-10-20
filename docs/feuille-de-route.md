# DragonTree — feuille de route

Rédigé le 2026-10-18, mis à jour le 2026-10-19 puis le 2026-10-20 (branche `options-fork`, voir plus bas). Ce document reprend les demandes du 2026-10-18 :

- renommer le projet (fait) ;
- choisir la langue (fait) ;
- voir et changer de worktree (fait : T1, via R3) ;
- voir les stashs (la fonction existe : T3) ;
- filtrer l'historique sur la branche active (la fonction existe : T4) ;
- recenser les options de menu de Git Fork pour choisir celles à implémenter (recensement fait, toutes les options retenues et livrées : T2, N2).

La mise à jour du 2026-10-19 ajoute l'état vérifié dans le code et la liste de tout ce qui reste à faire (T1, T2, N2 et B1 sont depuis terminés : voir la section **Branche `options-fork` (2026-10-20)**) :

- la section **Reste à faire** donne la synthèse ;
- la section **Détail des points restants** donne le détail ;
- la section **État du plan d'implémentation** fait le point sur le plan.

Les références `crates/…:ligne` renvoient au code du 2026-10-19.

## Constats

- **Les stashs et le filtre de branche existent déjà, mais se voient mal.**
  - La section « Stashes » est dans la barre latérale gauche. Un clic sur un stash affiche son diff, et le clic droit propose Appliquer, Pop et Supprimer. Sur un gros dépôt, la section se retrouve sous des centaines de branches.
  - Le bouton « Toutes les branches / Branche courante » est à droite du champ de recherche, au-dessus de l'historique. Il est discret.
  - Les points V2, V3, V7 et R4 ci-dessous les rendent visibles comme dans Fork.
  - L'utilisateur n'a pas encore confirmé que c'était bien le problème (voir T3 et T4).
- **Fork n'existe qu'en anglais.** Son développeur a refusé la traduction ([TrackerWin #392](https://github.com/fork-dev/TrackerWin/issues/392)). Le choix de la langue est donc un plus de DragonTree.
- **Les deux décisions en attente depuis le 2026-10-17 sont levées :** la proposition Worktrees est réalisée (R3) et toutes les options de menu sont implémentées (branche `options-fork`).

## Décisions

### Renommage en DragonTree — fait

- Crates `dragontree-git`, `dragontree-forges`, `dragontree-app` et `dragontree-i18n`.
- Exécutable `dragontree.exe` et titre de la fenêtre.
- Dossiers `%APPDATA%\DragonTree` et `%LOCALAPPDATA%\DragonTree`.
- Service « DragonTree » dans le Gestionnaire d'identifiants Windows.
- Dossier de travail du rebase interactif : `.git/dragontree-rebase`.
- Les comparaisons avec Fork sont retirées du code et du README (« comme Fork », chiffres de mémoire de Fork).
- Le mot technique *fork* reste là où il désigne autre chose que le produit : le champ `fork` de l'API GitHub, une divergence dans le graphe, le nom d'une icône Phosphor.
- **Pas de code de migration** (décision de l'utilisateur). Les réglages de `%APPDATA%\Fourche` et le cache de `%LOCALAPPDATA%\Fourche` ne sont pas repris. Les jetons enregistrés sous le service « Fourche » non plus : il faut ajouter les comptes à nouveau, ou copier ces dossiers à la main.
- Le dossier de travail d'origine (`GitFork`) n'a pas été renommé ; le code vit désormais dans le dépôt DragonTree.
- Vérifié le 2026-10-19 : l'ancien nom n'apparaît plus dans le code, les manifestes ni le script de toolchain.

### Langue français / anglais — fait

- Le choix se fait dans **Réglages › Apparence › Langue** : Système, Français ou English. Il s'applique tout de suite, sans redémarrer.
- « Système » suit la langue d'affichage de Windows : français si Windows est en français, anglais sinon.
- **Principe technique :** le texte français reste dans le code et sert de clé. Chaque autre langue a un catalogue `crates/dragontree-i18n/locales/<code>.json` qui associe le texte français à sa traduction.
- **Ajouter une langue :** créer un nouveau catalogue, puis ajouter la variante de langue.
- Un test vérifie trois points :
  - chaque texte de l'interface a sa traduction ;
  - aucune traduction n'est orpheline ;
  - les paramètres `{nom}` sont les mêmes dans les deux langues.
- Les messages renvoyés par git lui-même restent dans la langue de git.
- Une relecture indépendante (2026-10-17) n'a trouvé aucun texte traduit utilisé dans la logique. Elle a relevé trois points, corrigés :
  - `--help` ignorait `--lang` ;
  - un format de date traduit invalide pouvait faire planter l'application ;
  - « Journal git » était traduit par « Git log » ; il est maintenant traduit par « Git command log ».
- Les autres points de la relecture restent à traiter : voir RV5.

### Worktrees — fait (R3)

Le fonctionnement s'inspire de Fork et a été réalisé ainsi :

- **Section « Worktrees »** en haut de la barre latérale. Chaque ligne affiche la branche et le chemin.
- **Ouverture :** un clic ouvre le worktree dans un onglet, ou bascule sur cet onglet s'il est déjà ouvert.
- **Création :** depuis une branche locale, une branche distante ou un commit.
- **Gestion :** supprimer (avec confirmation), copier le chemin.
- **Branches déjà sorties ailleurs :** une icône signale les branches sorties dans un autre worktree. Un checkout d'une de ces branches propose d'ouvrir son worktree.
- **État au 2026-10-20 :** réalisé par R3 (section de la barre latérale, dialogue de création, branches sorties ailleurs signalées). L'état détaillé est en T1.

### Raccourcis

Quand un raccourci actuel entre en conflit avec ceux de Fork, l'actuel est gardé :

- Ctrl+1…9 change d'onglet (Fork l'utilise pour changer de vue) ;
- Ctrl+B affiche la liste des dépôts (Fork l'utilise pour un checkout rapide).

**Points tranchés** (relevés en vérifiant le code le 2026-10-19, résolus sur la branche `options-fork`). Tous les raccourcis sont déclarés dans le registre `crates/dragontree-app/src/commands.rs` (`Command::shortcut`), sauf Ctrl+Tab et Ctrl+1…9 :

- **Ctrl+Maj+L** reste le journal git. Pull prend **Ctrl+Maj+U** (Fetch Ctrl+Maj+F, Push Ctrl+Maj+P, Nouvelle branche Ctrl+Maj+B, Stash Ctrl+Maj+H).
- **Ctrl+Maj+C** reste Cloner (dans Fork, Cloner est sur Ctrl+N et Ctrl+Maj+C copie les informations du commit).
- **« Modifications locales »** s'atteint par Affichage › Modifications locales ; le code mort de Ctrl+1 a été supprimé (voir B1).

## Options de Fork pour les menus File / View / Repo / Config

**Légende**
- ✅ : existe déjà, il manque seulement l'entrée de menu ou le raccourci.
- 🆕 : à développer.
- Taille : P petit, M moyen, G gros.

Toutes les options ci-dessous ont été implémentées sur la branche `options-fork` (voir « Branche options-fork » plus bas).

La barre de menus existe (T2). Les états ci-dessous ont été vérifiés dans le code le 2026-10-20 ; les options ont été précisées le 2026-10-19 (V11, R10, C5).

### File

| # | Option | État |
|---|---|---|
| F1 | Nouvel onglet, Ouvrir, Cloner, Initialiser un dépôt, Fermer l'onglet, Palette Ctrl+P, Comptes, Réglages (Ctrl+,) | ✅ |
| F2 | Rouvrir le dernier onglet fermé (Ctrl+Maj+T) | ✅ |
| F3 | Gestion des clés SSH (générer, choisir, plusieurs clés) | ✅ |
| F4 | Quitter | ✅ |

### View

| # | Option | État |
|---|---|---|
| V1 | Modifications locales / Tous les commits, liste des dépôts, journal git | ✅ |
| V2 | Filtrer sur la branche active (Ctrl+Maj+A), en suivant les checkouts | ✅ |
| V3 | Filtrer sur des branches choisies : entonnoir sur chaque branche, dossier ou remote, plusieurs à la fois, bandeau « Filtré par : … » | ✅ |
| V4 | Masquer une branche, un dossier ou un remote du graphe (icône œil) | ✅ |
| V5 | Masquer les tags, masquer les branches distantes | ✅ |
| V6 | Afficher les commits perdus (reflog) dans le graphe (Ctrl+Maj+.) | ✅ |
| V7 | Afficher les stashs dans le graphe, avec une option pour les masquer | ✅ |
| V8 | Replier et déplier les merges (← / →, Tout replier) | ✅ |
| V9 | Révéler HEAD (Ctrl+0), afficher ou masquer le panneau de détails (Ctrl+Maj+D) | ✅ |
| V10 | Disposition verticale ou horizontale de l'historique | ✅ |
| V11 | Tri des commits : topologique ou par date | ✅ (le moteur sait déjà faire les deux : `crates/dragontree-git/src/log.rs:60-61`) |
| V12 | Vue Changements : afficher les fichiers ignorés, masquer les non suivis | ✅ |
| V13 | Zoom Ctrl+= / Ctrl+- (le réglage de taille existe) et thème | ✅ |

### Repo

| # | Option | État |
|---|---|---|
| R1 | Actualiser, Fetch/Pull/Push (Ctrl+Maj+F/U/P), Nouvelle branche (Ctrl+Maj+B), Nouveau tag, Stash (Ctrl+Maj+H), ouvrir dans l'Explorateur, le Terminal ou VS Code | ✅ |
| R2 | Fetch, Pull et Push rapides, sans dialogue (Ctrl+clic) | ✅ |
| R3 | Créer un worktree… (voir Worktrees) | ✅ |
| R4 | Stashs complets : liste déroulante sur Pop, renommer, stash des seuls fichiers sélectionnés, snapshot (garde les modifications), fichiers non suivis visibles dans le diff | ✅ |
| R5 | Appliquer un patch (fichier, glisser-déposer, presse-papiers) | ✅ |
| R6 | Enregistrer comme patch (commit, plage de commits, stash, fichiers) | ✅ |
| R7 | Comparer deux commits, deux branches, ou un commit avec les modifications locales | ✅ |
| R8 | Réglages du dépôt : nom et email propres au dépôt, liens vers les tickets (`#1234` → Azure DevOps), modèle de message de commit | ✅ |
| R9 | Remotes : modifier l'URL, tester la connexion (ajouter et supprimer existent déjà) | ✅ |
| R10 | Sous-modules : ajouter, supprimer, déplacer, changer l'URL (la liste, la mise à jour et l'ouverture en onglet existent déjà : `crates/dragontree-app/src/repo/sidebar.rs:204-259`) | ✅ |
| R11 | .gitignore : ajouter un motif avec un aperçu des fichiers concernés (« Ignorer » existe déjà) | ✅ |
| R12 | Bisect (bon, mauvais, passer, bandeau d'état) | ✅ |
| R13 | Git Flow (initialiser, start/finish feature, release, hotfix) | ✅ |
| R14 | Git LFS (init, track, fetch/pull/prune, verrous) | ✅ |
| R15 | Commandes personnalisées (sur le dépôt, une branche, un commit ou un fichier ; script ou URL) | ✅ |
| R16 | Fast-forward ou push de plusieurs branches à la fois ; sélection des branches périmées à supprimer | ✅ |
| R17 | Remettre un fichier dans son état à un commit donné | ✅ |
| R18 | Créer une pull request (GitHub, GitLab, Azure DevOps) | ✅ |
| R19 | « Ouvrir dans ▾ » avec d'autres éditeurs (Visual Studio .sln, Rider, Cursor…) | ✅ |
| R20 | Statistiques du dépôt | ✅ |

### Config

| # | Option | État |
|---|---|---|
| C1 | Langue français / anglais | ✅ (dans Réglages) |
| C2 | Outils externes de diff et de merge configurables, avec préréglages VS Code, WinMerge, Beyond Compare, KDiff3, P4Merge (`git mergetool` existe déjà) | ✅ |
| C3 | Terminal à utiliser (Windows Terminal, PowerShell, Git Bash) | ✅ |
| C4 | Identité git globale (nom, email) | ✅ |
| C5 | Message de commit : indicateur de longueur 50/72, colonne guide, messages récents, Co-authored-by | ✅ |
| C6 | Diff : police, taille, largeur de tabulation, espaces visibles | ✅ |
| C7 | IA : message de commit et revue de branche par Claude Code (dans Fork depuis la 2.14) | ✅ |
| C8 | Fenêtre des raccourcis clavier, À propos | ✅ |

### Hors menus, présents dans Fork

| # | Option | État |
|---|---|---|
| H1 | Glisser une branche sur une autre pour un merge ou un rebase | ✅ |
| H2 | Couleurs d'onglets et espaces de travail (groupes d'onglets) | ✅ |
| H3 | Recherche de commits dans le contenu des diffs et par chemin (aujourd'hui : SHA, sujet, auteur et e-mail seulement, `crates/dragontree-app/src/repo/history.rs:31-56`) | ✅ |
| H4 | Diff d'images par balayage ou superposition (aujourd'hui : côte à côte seulement) | ✅ |

### Écartés par défaut

Activation et licence, mises à jour automatiques, modèle « Lean Branching » propre à Fork, Treemap, Benchmark, avatars Gravatar.

## Reste à faire

Synthèse au 2026-10-20. Chaque point est détaillé plus bas, sous son identifiant. Les options de Fork restent dans les tableaux ci-dessus et ne sont pas répétées. Terminés depuis le 2026-10-19 : T1, T2, N2, B1, B5, B6 (branche `options-fork`).

### Bugs à corriger

| # | Point | Origine | Priorité |
|---|---|---|---|
| B1 | Ctrl+1 (« Modifications locales ») jamais atteint (corrigé : Affichage › Modifications locales) | revue finale, mineur n°19 | basse |
| B2 | Dossier `.git/dragontree-rebase` laissé après un rebase arrêté puis terminé | revue finale, mineur n°20 | basse |
| B3 | Décocher « Amend » trop vite écrase le message restauré | revue finale, mineur n°13 | basse |
| B4 | Compte enregistré avec le jeton du formulaire au lieu du jeton vérifié | revue finale, mineur n°10 | basse |
| B5 | La fenêtre Réglages écrase des changements faits ailleurs (corrigé) | revue finale, mineur n°8 | basse |
| B6 | `wt.exe -d` coupe un chemin contenant « ; » (corrigé) | revue finale, annexe de I8 | basse |
| B7 | Menu contextuel d'onglet lié à la position de l'onglet | arbitrage de la revue (M14) | basse |

### Fonctionnalités à terminer

| # | Point | Origine | Priorité |
|---|---|---|---|
| T1 | Worktrees : lister, ouvrir, créer, supprimer (fait, via R3) | demande du 2026-10-17 | haute |
| T2 | Barre de menus Fichier / Affichage / Dépôt / Configuration et raccourcis (fait) | demande du 2026-10-17 | haute |
| T3 | Stashs plus visibles (à vérifier avec l'utilisateur) | demande du 2026-10-17 | moyenne |
| T4 | Filtre sur la branche active : mémorisation par dépôt (à vérifier avec l'utilisateur ; raccourci fait, V2) | demande du 2026-10-17 | moyenne |
| T5 | Éléments du plan non livrés : glisser-déposer entre listes, colonne du graphe, colonne « mise à jour », aperçu des conflits | revue finale, mineur n°21 | basse |
| T6 | Associer un compte à un dépôt existant | arbitrage de la tâche 18, « plus tard » | moyenne |

### À implémenter

| # | Point | Origine | Priorité |
|---|---|---|---|
| N1 | Vue centralisée des pull requests de tous les comptes | demande initiale, « pour plus tard » | moyenne |
| N2 | Options de Fork : F2–F4, V3–V12, R2–R20, C2–C8, H1–H4 (fait, tout est ✅ dans les tableaux ci-dessus) | demande du 2026-10-17 | terminé |
| N3 | Icône intégrée à l'exécutable | limite connue | basse |

### Constats de revue

| # | Point | Origine | Priorité |
|---|---|---|---|
| RV1 | Fluidité de l'interface sur un gros dépôt : filtre, copies par image, coloration synchrone | revue finale, mineur n°6 | moyenne |
| RV2 | Gros diffs lus entièrement en mémoire avant la limite de 8 Mio | revue finale, mineur n°7 | moyenne |
| RV3 | Fetch automatique dans la file des écritures ; deux files pour un même dépôt | revue finale, mineur n°16 | basse |
| RV4 | Points non jugés : processus git à la fermeture, instances multiples, proxy, cas des forges (à vérifier) | revue finale et rapport du crate forges | basse |
| RV5 | Résidus de la relecture de la traduction : `--lang` enregistré, bouton Push occupé, textes figés | relecture de la traduction | basse |

### Pistes

| # | Point | Origine | Priorité |
|---|---|---|---|
| P1 | Publication open source : licence, vérification du nom, README | objectif annoncé par l'utilisateur | moyenne |
| P2 | Cases du plan jamais cochées | état du plan | basse |
| P3 | Stabilité de `ui_network` vérifiée sur 3 passages seulement | correction des tests instables | basse |
| P4 | `--help` en build release depuis une console Windows (à vérifier) | relevé en vérifiant le code | basse |
| L1 | Dépôts SHA-256 non pris en charge | limite connue (README) | basse |
| L2 | Éditeur de conflits limité aux fichiers UTF-8 | limite constatée dans le code | moyenne |
| L3 | Réglages illisibles au démarrage : rien n'est enregistré de la session | arbitrage de la revue (M9) | basse |
| L4 | Échec d'authentification du fetch automatique signalé une seule fois | arbitrage de la revue (M2) | basse |
| L5 | Amend d'un commit déjà poussé avec « Pousser après » : pas de push | arbitrage de la revue (I7b) | basse |
| L6 | Option « Espaces » active : diff des changements en lecture seule | arbitrage de la revue (M1) | basse |

## Détail des points restants

### Fonctionnalités à terminer

#### T1 — Worktrees : lister, ouvrir, créer, supprimer (fait)

- **Contexte :** l'utilisateur a demandé, le 2026-10-17, de « voir les différents worktrees facilement et basculer entre eux ». La proposition de la section Worktrees a été réalisée avec l'option R3 (branche `options-fork`).
- **État :**
  - fait (R3) : moteur `crates/dragontree-git/src/worktree.rs` (liste sans `-z`, création, suppression, prune), section « Worktrees » de la barre latérale et dialogue « Créer un worktree… » (`crates/dragontree-app/src/repo/worktrees.rs`), branches sorties ailleurs signalées, surveillance de `<common>/worktrees` ; les points ci-dessous décrivent l'état d'avant ;
  - le scan reconnaît un worktree (`crates/dragontree-git/src/discovery.rs:16-17`, `:159-160`) ;
  - la liste de droite l'affiche avec une icône propre (`crates/dragontree-app/src/repo_list.rs:336`) et l'ouvre comme un dépôt ordinaire ;
  - un worktree hors des dossiers scannés n'apparaît nulle part ;
  - le moteur n'appelle jamais `git worktree`, et la barre latérale n'a pas de section Worktrees (`crates/dragontree-app/src/repo/sidebar.rs:69-261`) ;
  - la surveillance du disque ignore `.git/worktrees` (`crates/dragontree-app/src/repo/watcher.rs:30`) : un ajout ou une suppression de worktree ne déclencherait aucun rafraîchissement.
- **Plan d'origine (réalisé, étapes 1 à 6 faites) :**
  1. Moteur :
     - `Repo::worktrees()`, à partir de `git worktree list --porcelain`, sans `-z` (chemin, HEAD, branche, détaché, verrouillé, à nettoyer) ;
     - `add_worktree`, depuis une branche locale, une branche distante (avec création d'une branche de suivi) ou un commit (HEAD détachée) ;
     - `remove_worktree`, avec `--force` seulement sur confirmation explicite ;
     - `prune`.
  2. Section « Worktrees » en tête de la barre latérale :
     - un clic lance `AppAction::OpenRepo`, qui active l'onglet existant s'il y en a un (`crates/dragontree-app/src/app.rs:268-288`) ;
     - menu contextuel : Supprimer (dialogue destructeur, sans validation par Entrée), Copier le chemin, Ouvrir dans l'Explorateur.
  3. Branches sorties dans un autre worktree :
     - les signaler par une icône dans la liste des branches ;
     - quand git refuse leur checkout, proposer d'ouvrir leur worktree.
  4. Dialogue « Créer un worktree… » (R3) et commande dans la palette Ctrl+P.
  5. Rafraîchir la section quand `.git/worktrees` change.
  6. Ajouter les traductions dans `crates/dragontree-i18n/locales/en.json`.
- **Critère de fin (atteint) :**
  - tests d'intégration du moteur sur un dépôt temporaire : création depuis une branche, une branche distante et un commit, liste, suppression, refus d'une branche déjà sortie ;
  - test d'interface : un clic sur une ligne ouvre l'onglet, un second clic le réactive ;
  - test d'interface : le checkout d'une branche sortie ailleurs propose le worktree ;
  - `cargo test -p dragontree-i18n` vert.
- **Priorité :** terminé (demande explicite).

#### T2 — Barre de menus Fichier / Affichage / Dépôt / Configuration (fait)

- **Contexte :** l'utilisateur a demandé, le 2026-10-17, un menu File/View/Repo/Config « avec une majorité d'options qui existent sur Fork ». Toutes les options des tableaux ci-dessus ont été retenues et implémentées (branche `options-fork`).
- **État :**
  - fait : barre de menus Fichier / Affichage / Dépôt / Configuration au-dessus des onglets (`crates/dragontree-app/src/menu_bar.rs`), alimentée par le registre `crates/dragontree-app/src/commands.rs` (entrées, raccourcis, palette Ctrl+P) ; les raccourcis sont affichés dans les libellés et les entrées propres à un dépôt sont grisées sans dépôt ouvert ;
  - les points ci-dessous décrivent l'état d'avant :
  - il n'y avait aucune barre de menus : le seul panneau du haut était la barre d'onglets (`crates/dragontree-app/src/app.rs:611-617`) ;
  - les options ✅ passaient par la barre d'outils (`crates/dragontree-app/src/repo/toolbar.rs`), les menus contextuels, la palette Ctrl+P (`crates/dragontree-app/src/quick_open.rs:46-59`) et la fenêtre Réglages ;
  - raccourcis présents : `crates/dragontree-app/src/app.rs:451-486` et `crates/dragontree-app/src/repo/mod.rs:886-900`. Il manque Ctrl+, (F1), Ctrl+Maj+A (V2), Ctrl+= et Ctrl+- (V13), ainsi que Ctrl+Maj+F/L/P, Ctrl+Maj+B et Ctrl+Maj+H (R1).
- **Plan d'origine (réalisé) :**
  1. Choix de l'utilisateur : toutes les options (fait).
  2. Ajouter la barre au-dessus des onglets.
  3. Brancher les entrées ✅ sur l'existant : `AppAction` (`crates/dragontree-app/src/app.rs:373-435`) et `RepoTab::quick_command` (`crates/dragontree-app/src/repo/toolbar.rs:133-154`).
  4. Griser les entrées propres à un dépôt quand l'onglet actif n'est pas un dépôt.
  5. Afficher les raccourcis dans les libellés.
  6. Ajouter les raccourcis manquants une fois les conflits tranchés (section Raccourcis).
  7. Implémenter ensuite les options retenues (N2, fait).
- **Critère de fin (atteint) :**
  - test d'interface qui ouvre chaque menu et déclenche une entrée de chacun ;
  - test d'interface pour chaque nouveau raccourci ;
  - test de traduction vert.
- **Priorité :** terminé.

#### T3 — Stashs plus visibles

- **Contexte :** l'utilisateur a demandé à « voir ses stashs ». La fonction existe. Claude a supposé que l'utilisateur ne l'avait pas vue ; ce n'est pas confirmé.
- **État :**
  - la section Stashes (`crates/dragontree-app/src/repo/sidebar.rs:170-202`) affiche le diff au clic et propose Appliquer, Pop et Supprimer ;
  - elle vient après Branches, Remotes et Tags (`:101`, `:116`, `:164`) ;
  - le bouton Pop de la barre d'outils n'agit que sur le dernier stash (`crates/dragontree-app/src/repo/toolbar.rs:74-82`).
- **À faire :**
  1. Confirmer le besoin avec l'utilisateur (à vérifier).
  2. Réaliser V7 (stashs dans le graphe) et R4 (stashs complets), s'ils sont retenus.
  3. Sinon, au minimum, placer la section Stashes avant Remotes et Tags.
- **Critère de fin :** sur un dépôt avec plus de 1 500 branches distantes, un stash créé se voit sans défilement (capture d'écran), plus les tests propres à V7 et R4.
- **Priorité :** moyenne (demande explicite, mais la fonction existe).

#### T4 — Filtre sur la branche active : raccourci et mémorisation

- **Contexte :** l'utilisateur a demandé à filtrer l'historique sur la branche active. La fonction existe ; même réserve que pour T3.
- **État :**
  - le bouton « Toutes les branches / Branche courante » est dans `crates/dragontree-app/src/repo/history.rs:157-176` ;
  - le journal est alors limité à HEAD (`crates/dragontree-app/src/repo/scope.rs:102`) et il est rechargé après chaque checkout ;
  - fait (V2) : raccourci Ctrl+Maj+A et entrée Affichage › Branche courante seulement (cochée quand le filtre est actif) ;
  - fait (V3) : filtre sur plusieurs branches, dossiers ou remotes, exclusif avec « Branche courante seulement » ;
  - reste : le choix est propre à l'onglet et repart sur « Toutes les branches » à chaque ouverture (`crates/dragontree-app/src/repo/mod.rs:215`).
- **À faire :**
  1. Raccourci Ctrl+Maj+A et entrée du menu Affichage (V2, fait).
  2. Mémoriser le choix par dépôt dans les réglages (Fork le garde par worktree), à confirmer avec l'utilisateur.
  3. Filtre sur plusieurs branches (V3, fait).
- **Critère de fin :**
  - test d'interface : Ctrl+Maj+A bascule le filtre et son libellé (atteint : `current_branch_shortcut_hides_and_restores_the_other_branch_commits`, `crates/dragontree-app/tests/ui_history_filters.rs`) ;
  - test : à la réouverture du dépôt, le choix est conservé (reste à faire).
- **Priorité :** moyenne (pour la mémorisation).

#### T5 — Éléments du plan non livrés

- **Contexte :** la revue finale (mineur n°21) a relevé quatre éléments du plan absents. Ils ont été différés.
- **État et travail restant :**
  - **a. Glisser-déposer entre « Non indexés » et « Indexés »** (plan, tâche 16).
    - État : absent. Un fichier change de liste par double-clic, Espace, Entrée ou par le menu (`crates/dragontree-app/src/repo/changes.rs:134-145`, `:261`).
    - Fin : test d'interface où un glisser d'une liste à l'autre indexe le fichier.
  - **b. Colonne du graphe redimensionnable** (tâche 15).
    - État : la largeur est calculée et bornée entre 22 et 220 px (`crates/dragontree-app/src/repo/graph_paint.rs:40-42`, utilisée en `crates/dragontree-app/src/repo/history.rs:130`). Il n'y a pas de poignée.
    - À faire : ajouter une poignée et mémoriser la largeur.
    - Fin : la largeur choisie est conservée après redémarrage.
  - **c. Colonne « mise à jour » dans la liste de clonage** (tâche 18).
    - État : la liste montre le nom, l'espace, la description et un cadenas (`crates/dragontree-app/src/clone.rs:373-427`). `RemoteRepo::updated_at` existe et sert déjà au tri (`crates/dragontree-forges/src/model.rs:91`, `:265`), mais Azure DevOps ne le renseigne pas (`crates/dragontree-forges/src/azure.rs:252`).
    - À faire : afficher la date relative, et la chercher côté Azure DevOps (date du dernier push) si l'API la fournit.
    - Fin : la date s'affiche sur les fixtures GitHub et GitLab.
  - **d. Aperçu du résultat dans l'éditeur de conflits** (tâche 22).
    - État : l'éditeur montre les deux côtés de chaque région, sans aperçu (`crates/dragontree-app/src/repo/conflicts.rs:91-164`). `resolve` (`crates/dragontree-git/src/conflict.rs:118`) sait déjà produire le texte final.
    - À faire : afficher le résultat au fil des choix.
    - Fin : test unitaire de l'aperçu pour chaque choix (notre, leur, les deux).
- **Priorité :** basse (mineurs différés).

#### T6 — Associer un compte à un dépôt existant

- **Contexte :** arbitrage de la tâche 18. L'authentification d'un compte n'est injectée que pour les dépôts clonés depuis ce compte, pour ne pas casser un Git Credential Manager qui fonctionne. L'association manuelle était notée « à ajouter plus tard ».
- **État :**
  - `repo_accounts` (`crates/dragontree-app/src/settings.rs:46`) n'est rempli qu'à la fin d'un clonage (`crates/dragontree-app/src/clone.rs:57`) ;
  - il est lu par `account_for_repo` (`crates/dragontree-app/src/settings.rs:215`) pour fetch, pull et push (`crates/dragontree-app/src/repo/actions.rs:37-40`) ;
  - il est vidé à la suppression du compte (`crates/dragontree-app/src/accounts.rs:241`) ;
  - aucune interface ne permet d'associer ou de dissocier un compte.
- **À faire :**
  1. Ajouter « Utiliser le compte… » et « Ne plus utiliser de compte » dans le menu du dépôt, ou dans les réglages du dépôt (R8).
  2. Proposer les comptes dont `account_matches_url` (`crates/dragontree-forges/src/auth.rs`) correspond à un remote.
  3. Afficher le compte associé dans l'onglet.
- **Critère de fin :**
  - test unitaire de la proposition de comptes à partir des remotes ;
  - test : après association, `account_for_repo` renvoie le compte, et la dissociation le retire.
- **Priorité :** moyenne (reportée explicitement ; utile dès qu'on a plusieurs comptes).

### À implémenter

#### N1 — Vue centralisée des pull requests

- **Contexte :** cette vue fait partie de la demande initiale (« à noter pour plus tard ») : centraliser les PR de tous les comptes au même endroit. Le README l'annonce « à venir ».
- **État :**
  - le crate des forges sait vérifier un compte et lister les dépôts (`crates/dragontree-forges/src/lib.rs:36`), rien de plus ;
  - aucune API de pull request n'est appelée et aucune vue n'existe ;
  - les comptes multiples, les jetons et l'agent HTTP sont prêts.
- **À faire :**
  1. Modèle `PullRequest` : forge, dépôt, titre, auteur, état, relecteurs, brouillon, URL, date.
  2. Appels par forge :
     - GitHub : recherche des PR ouvertes qui concernent l'utilisateur ;
     - GitLab : `merge_requests?scope=all&state=opened` ;
     - Azure DevOps : `_apis/git/pullrequests?searchCriteria.status=active` par organisation.
  3. Vue agrégée avec des filtres (créées par moi, à relire, par compte), l'ouverture dans le navigateur et le lien vers le dépôt local s'il est cloné.
  4. Rafraîchissement en arrière-plan.
  5. La création de PR (R18) vient ensuite.
- **Critère de fin :**
  - tests d'analyse sur des réponses JSON représentatives de chaque forge, dans `crates/dragontree-forges/tests/fixtures/` ;
  - test réel ignoré par défaut, comme `tests/live.rs` ;
  - test d'interface de la liste.
- **Priorité :** moyenne (reportée par l'utilisateur lui-même).

#### N2 — Options de Fork à développer (fait)

- **Contexte et état :** les options sont dans les tableaux ci-dessus, avec leur taille ; toutes sont ✅ depuis la branche `options-fork`. R3 relève de T1. Les fonctions que le README listait comme non gérées sont faites : Git LFS (R14), git-flow (R13), reflog (V6), commandes personnalisées (R15).
- **Réalisé :** toutes les options ont été implémentées. Pour chacune :
  - un test d'intégration du moteur si une commande git est ajoutée ;
  - un test d'interface ;
  - les traductions.
- **Critère de fin :** les tests de l'option passent, et l'option passe à ✅ dans les tableaux.
- **Priorité :** terminé.

#### N3 — Icône intégrée à l'exécutable

- **Contexte :** limite citée dans le README et dans le bilan du 2026-10-16.
- **État :**
  - seule l'icône de fenêtre existe, dessinée en code (`crates/dragontree-app/src/main.rs:69-102`) ;
  - `dragontree.exe` n'a pas de ressource d'icône ;
  - `scripts/setup-toolchain.ps1` n'extrait que `as.exe` et `dlltool.exe` des binutils MSYS2.
- **À faire :**
  1. Ajouter un `.ico` dans `assets/`.
  2. Ajouter un `build.rs` qui intègre la ressource. Avec la toolchain GNU, il faut aussi extraire `windres.exe`, ou utiliser une crate qui n'a besoin d'aucun outil externe.
- **Critère de fin :** l'Explorateur affiche l'icône de `target\release\dragontree.exe`, avec les toolchains GNU et MSVC.
- **Priorité :** basse.

### Bugs à corriger

#### B1 — Ctrl+1 (« Modifications locales ») jamais atteint (fait)

- **Contexte :** mineur n°19 de la revue finale.
- **État :** corrigé en T2 : le code mort a été supprimé et la vue s'atteint par Affichage › Modifications locales. Avant : le raccourci d'onglet (`crates/dragontree-app/src/app.rs:479-485`) consommait Ctrl+1 avant celui de l'onglet de dépôt (`crates/dragontree-app/src/repo/mod.rs:895-899`). Ce second code ne s'exécute donc jamais.
- **Réalisé :** Ctrl+1…9 reste au changement d'onglet (section Raccourcis) ; le code mort de Ctrl+1 a été supprimé en T2 et la vue s'atteint par Affichage › Modifications locales.
- **Critère de fin :** atteint (accès par le menu testé, code mort supprimé).
- **Priorité :** basse.

#### B2 — Dossier `.git/dragontree-rebase` laissé après un rebase arrêté

- **Contexte :** mineur n°20 de la revue finale.
- **État :** le dossier n'est supprimé que si le rebase se termine d'un seul coup (`crates/dragontree-git/src/rebase.rs:84-86`). Continuer, abandonner ou passer un commit depuis le bandeau (`crates/dragontree-git/src/ops.rs:193-216`) le laisse en place.
- **À faire :** après chacune de ces trois opérations, supprimer le dossier si `operation_state()` vaut `None`.
- **Critère de fin :** test d'intégration avec un rebase interactif contenant `edit` (il s'arrête), puis `continue_op` : le dossier a disparu. Même test avec `abort_op`.
- **Priorité :** basse.

#### B3 — Décocher « Amend » trop vite écrase le message restauré

- **Contexte :** mineur n°13 de la revue finale.
- **État :**
  - cocher Amend lance la lecture du dernier message dans un thread (`crates/dragontree-app/src/repo/changes.rs:586-596`, `crates/dragontree-app/src/repo/actions.rs:265-271`) ;
  - le résultat remplace le résumé et la description sans vérifier qu'Amend est encore coché (`crates/dragontree-app/src/repo/mod.rs:674-678`).
- **À faire :** ignorer le message reçu si Amend a été décoché entre-temps, avec un numéro de génération comme pour les autres lectures.
- **Critère de fin :** test : cocher puis décocher avant l'arrivée de l'événement ; le message saisi est conservé.
- **Priorité :** basse.

#### B4 — Compte enregistré avec le jeton du formulaire au lieu du jeton vérifié

- **Contexte :** mineur n°10 de la revue finale.
- **État :**
  - la vérification part avec le type, l'hôte et le jeton lus au clic (`crates/dragontree-app/src/accounts.rs:156-170`) ;
  - à son retour, l'enregistrement relit le formulaire (`crates/dragontree-app/src/accounts.rs:113-131`) ;
  - si le champ a été modifié entre-temps, un jeton non vérifié est stocké.
- **À faire :** transporter les valeurs vérifiées dans `ForgeEvent::Verified`, ou figer le formulaire pendant la vérification.
- **Critère de fin :** test : modifier le jeton pendant la vérification ; le jeton enregistré est celui qui a été vérifié.
- **Priorité :** basse.

#### B5 — La fenêtre Réglages écrase des changements faits ailleurs (fait)

- **Contexte :** mineur n°8 de la revue finale, corrigé en partie, puis relevé de nouveau par la revue de la branche `options-fork`.
- **État :** corrigé. Avant : l'enregistrement remplaçait tous les réglages par le brouillon pris à l'ouverture de la fenêtre, sauf une liste de champs conservés (onglets, dépôts récents, comptes, favoris, espaces de travail, messages récents) ; tout autre réglage changé ailleurs pendant que la fenêtre était ouverte reprenait sa valeur d'ouverture : liens vers les tickets et références masquées d'un dépôt, options du menu Affichage, mode du diff d'images, dernier éditeur, taille et thème, diff côte à côte et espaces, « Pousser après le commit », dépôts masqués, largeur et visibilité de la liste de droite, dossiers racines…
- **Réalisé :** la fenêtre garde l'état d'ouverture (`SettingsWindow::opened`) ; à l'enregistrement, elle part des réglages actuels et n'y reporte que les champs qu'elle modifie, et seulement ceux qui diffèrent de l'état d'ouverture (`take_edits`, `crates/dragontree-app/src/settings_ui.rs`). La liste de champs conservés est supprimée ; le nouveau scan et le changement d'exécutable git ne se déclenchent que si leurs réglages ont changé.
- **Critère de fin :** atteint — test unitaire `take_edits_keeps_what_changed_elsewhere` (dont « Espaces » changé dans la vue diff pendant qu'on enregistre un autre réglage) et test d'interface `saving_the_settings_window_keeps_what_changed_elsewhere_while_it_was_open` (`crates/dragontree-app/tests/ui_settings_window.rs`).
- **Priorité :** terminé.

#### B6 — `wt.exe -d` coupe un chemin contenant « ; » (fait)

- **Contexte :** annexe du point important n°8 de la revue finale.
- **État :** corrigé avec C3. Avant : Windows Terminal lisait « ; » comme un séparateur de commandes, et `wt.exe -d` coupait le chemin du dépôt à cet endroit.
- **Réalisé :** `wt_directory_arg` (`crates/dragontree-app/src/terminal.rs`) écrit « ; » en `\;` pour `wt.exe -d` ; les autres terminaux reçoivent le dossier par `current_dir`.
- **Critère de fin :** atteint (test unitaire `windows_terminal_escapes_semicolons_and_keeps_other_characters`, sur la commande construite pour un chemin contenant « ; »).
- **Priorité :** terminé.

#### B7 — Menu contextuel d'onglet lié à la position de l'onglet

- **Contexte :** arbitrage de la correction M14. Les lignes des listes ont reçu des identifiants stables, mais pas les onglets.
- **État :**
  - chaque onglet prend un identifiant positionnel (`crates/dragontree-app/src/tabbar.rs:92`) ;
  - ses actions portent un index (`crates/dragontree-app/src/tabbar.rs:168-190`, `crates/dragontree-app/src/app.rs:510-552`) ;
  - si un onglet se ferme pendant qu'un menu est ouvert, l'action peut viser l'onglet voisin.
- **À faire :** dériver l'identifiant de `TabId`, et faire porter `TabId` aux actions au lieu de l'index.
- **Critère de fin :** test d'interface : menu ouvert sur l'onglet 2, l'onglet 1 se ferme, puis « Fermer l'onglet » ferme bien l'onglet visé.
- **Priorité :** basse.

### Constats de revue

#### RV1 — Fluidité de l'interface sur un gros dépôt

- **Contexte :** mineur n°6 de la revue finale. Il touche l'objectif de performance de la demande initiale.
- **État :**
  - **filtre :** à chaque frappe, il replie tous les commits sur le thread de l'interface (`crates/dragontree-app/src/repo/history.rs:31-56`). La revue l'estime à 50–150 ms sur 86 k commits ;
  - **copies à chaque image :** la liste filtrée (`crates/dragontree-app/src/repo/history.rs:113`) et la table des étiquettes de refs (`:196`) sont copiées à chaque image ;
  - **coloration syntaxique :**
    - elle est calculée de façon synchrone (`crates/dragontree-app/src/highlight.rs:52-111`) ;
    - son cache ne garde qu'un fichier, sous la clé (chemin, nombre de lignes, thème) (`:37`, `:54`) : les couleurs restent périmées après une modification qui ne change pas le nombre de lignes ;
    - le plan demandait un cache LRU de 64 fichiers et un seuil de 20 000 lignes ; le seuil est de 3 000 lignes (`:19`, arbitrage de la tâche 19).
- **À faire :**
  1. Filtrer en arrière-plan avec un numéro de génération, ou après un court délai de frappe.
  2. Partager les étiquettes et la liste filtrée par `Arc` au lieu de les copier.
  3. Colorer dans une tâche de fond, avec un cache LRU indexé par le contenu.
  4. Revoir ensuite le seuil de 3 000 lignes.
- **Critère de fin :**
  - mesure sur le gros dépôt : une frappe dans le filtre coûte moins de 16 ms au thread d'interface ;
  - test unitaire : une ligne modifiée sans changer le nombre de lignes est recolorée.
- **Priorité :** moyenne.

#### RV2 — Gros diffs lus entièrement en mémoire

- **Contexte :** mineur n°7 de la revue finale. Il touche l'objectif de moins de 150 Mo de mémoire.
- **État :**
  - `diff_commit_file` et `diff_worktree_file` lisent toute la sortie de git (`crates/dragontree-git/src/repo.rs:256`, `:285`) ;
  - la limite de 8 Mio n'est appliquée qu'ensuite (`crates/dragontree-git/src/repo.rs:399-407`) ;
  - seul le fichier non suivi est contrôlé avant la lecture (`:269-278`).
- **À faire :** arrêter la lecture, et le processus, dès que la limite est dépassée, ou contrôler la taille avant de lancer le diff.
- **Critère de fin :** test d'intégration : le diff d'un fichier de 50 Mio rend un résultat tronqué sans jamais garder plus de 8 Mio en mémoire.
- **Priorité :** moyenne.

#### RV3 — Fetch automatique dans la file des écritures

- **Contexte :** mineur n°16 de la revue finale.
- **État :**
  - le fetch automatique (`crates/dragontree-app/src/repo/mod.rs:779-788`) passe par la file de l'onglet (`crates/dragontree-app/src/repo/actions.rs:44`, `:91-96`) : une indexation lancée pendant un fetch lent attend la fin du fetch ;
  - chaque onglet a sa propre file (`crates/dragontree-app/src/repo/mod.rs:293`) ;
  - un même dépôt peut être ouvert dans deux onglets (« Ouvrir dans un nouvel onglet », `crates/dragontree-app/src/repo_list.rs:446-447`) : deux files écrivent alors sur le même dépôt, et des collisions sur `index.lock` sont possibles.
- **À faire :**
  1. Mettre les opérations réseau silencieuses dans une file à part.
  2. Partager une seule file d'écriture par dépôt, indexée par chemin.
- **Critère de fin :**
  - test : pendant un fetch automatique bloqué (serveur HTTP muet, comme le test de clonage annulé), une indexation se termine tout de suite ;
  - test : deux onglets sur le même dépôt partagent la même file.
- **Priorité :** basse.

#### RV4 — Points non jugés par la revue et cas des forges

- **Contexte :** la revue finale a laissé ces points sans jugement. Le rapport du crate des forges a signalé les autres.
- **Points (tous à vérifier) :**
  - **a. Processus git à la fermeture de l'application.**
    - État : aucun arrêt n'est prévu. Le job Windows est créé sans `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` (`crates/dragontree-git/src/runner.rs:420-433`), donc un push ou un fetch en cours continue probablement après la fermeture.
    - Pour trancher : fermer la fenêtre pendant un push lent et observer.
    - Décision à prendre : attendre la fin des écritures, ou les arrêter proprement.
  - **b. Plusieurs instances.**
    - État : rien n'empêche deux instances, qui écrivent alors le même `settings.json` ; la dernière écriture l'emporte.
    - Décision à prendre : instance unique avec transmission des arguments à l'instance ouverte (comme Fork), ou fusion à l'écriture.
  - **c. Proxy.**
    - État : le client HTTP des forges ne lit que les variables d'environnement (`crates/dragontree-forges/src/http.rs:103`), sans la configuration proxy de Windows. Derrière un proxy d'entreprise, la vérification d'un compte peut échouer alors que git fonctionne.
  - **d. Forges.**
    - GitLab : la liste `simple=true` peut omettre la visibilité ; le dépôt s'affiche alors comme privé (`crates/dragontree-forges/src/gitlab.rs:42`, `:98`).
    - GitLab : les instances en HTTP simple ne sont pas prises en charge (`crates/dragontree-forges/src/gitlab.rs:10`).
    - Azure DevOps : « privé » est déduit de la visibilité du projet (`crates/dragontree-forges/src/azure.rs:250`).
    - Azure DevOps : l'en-tête d'authentification vise `https://dev.azure.com/<org>/` (`crates/dragontree-forges/src/auth.rs:45-50`) et git compare ce chemin en respectant la casse, alors que l'association d'un compte à une URL l'ignore. Un remote écrit avec une autre casse d'organisation ne recevrait pas l'en-tête.
    - Pour trancher : test réel avec un remote dont la casse diffère.
- **Critère de fin :** chaque point est soit corrigé et testé, soit documenté comme limite dans le README.
- **Priorité :** basse.

#### RV5 — Résidus de la relecture de la traduction

- **Contexte :** relecture de la traduction du 2026-10-17. Trois points ont été corrigés (voir Langue) ; restent ceux-ci.
- **État et travail restant :**
  - **`--lang` est enregistré définitivement.**
    - État : l'option est écrite dans les réglages (`crates/dragontree-app/src/app.rs:170-172`), qui sont toujours sauvegardés (`:703-706`). Une seule exécution de `dragontree --lang en` change donc la langue pour de bon, alors que l'aide dit « force la langue ». `--theme` fait de même (`:167-169`). Ce comportement a été annoncé à l'utilisateur, mais l'aide ne le dit pas.
    - À trancher : garder ce comportement et corriger l'aide, ou en faire un réglage temporaire, non enregistré.
  - **Bouton Push affiché occupé pendant le push d'un tag, en français seulement.**
    - État : l'état occupé se teste sur le début du titre de l'opération (`crates/dragontree-app/src/repo/toolbar.rs:16`) ; « Push du tag » commence par « Push », pas « Tag push » (`crates/dragontree-app/src/repo/sidebar.rs:587`). Le même procédé sert pour « Commit » (`crates/dragontree-app/src/repo/changes.rs:553`).
    - À faire : identifier les opérations par un type plutôt que par leur titre.
  - **Textes figés dans l'ancienne langue après un changement de langue :** titres d'opérations en cours, notifications déjà affichées, titres de dialogues ouverts, messages d'erreur internes (qui peuvent alors mêler les deux langues). Le message d'erreur des réglages est construit avant l'application de la langue choisie (`crates/dragontree-app/src/app.rs:166` puis `:174`). Acceptable ; à documenter.
- **Critère de fin :** décision prise sur `--lang` et aide alignée ; test unitaire de l'état occupé indépendant de la langue.
- **Priorité :** basse.

### Pistes et limites connues

#### P1 — Publication open source

- **Contexte :** l'objectif annoncé par l'utilisateur est de publier l'outil en open source.
  - Les vérifications du nom proposées pendant le choix du nom n'ont pas été faites : noms de domaine, registres de marques (INPI, EUIPO, USPTO), identifiants winget et Scoop.
  - Pour « Dragontree », seuls crates.io, GitHub (9 dépôts vides) et une recherche web ont été consultés.
- **État :**
  - le manifeste déclare la licence MIT (`Cargo.toml:9`), mais le dépôt n'a pas de fichier `LICENSE` ;
  - la licence de la police Phosphor est présente (`assets/fonts/LICENSE-Phosphor.txt`) ;
  - le README annonce 870 tests (nombre réel au 2026-10-20 ; 277 le 2026-10-17).
- **À faire :**
  1. Ajouter `LICENSE`.
  2. Vérifier le nom (domaines, marques, winget, Scoop).
  3. (fait le 2026-10-20) Nombre de tests du README à jour.
  4. Ajouter l'icône (N3).
- **Critère de fin :** fichier `LICENSE` présent ; résultat des vérifications du nom consigné.
- **Priorité :** moyenne.

#### P2 — Cases du plan jamais cochées

- **Contexte :** le plan a été suivi grâce à un journal d'exécution non versionné, et ses cases n'ont pas été cochées.
- **État :** `docs/superpowers/plans/2026-10-16-dragontree.md` compte 58 cases, toutes vides, alors que les tâches 0 à 24 sont terminées (voir État du plan).
- **À faire :** cocher les étapes faites, ou signaler en tête du plan qu'il est historique et renvoyer à ce document.
- **Priorité :** basse.

#### P3 — Stabilité de `ui_network`

- **Contexte :** le 2026-10-17, des tests d'interface échouaient de temps en temps (attentes qui paniquaient sur une erreur passagère de git, clic sur un bouton encore désactivé). Ils ont été corrigés : `ui_flows` passe 20 fois de suite, mais `ui_network` n'a été relancé que 3 fois.
- **À faire :** lancer `ui_network` en boucle (20 passages).
- **Critère de fin :** 20 passages sur 20.
- **Priorité :** basse.

#### P4 — `--help` en build release depuis une console Windows (à vérifier)

- **Contexte :** relevé en vérifiant le code, ce point n'a pas été discuté.
  - L'exécutable release utilise le sous-système Windows (`crates/dragontree-app/src/main.rs:1`), et l'aide est écrite sur la sortie d'erreur (`:110`).
  - L'aide a été vérifiée à travers un tube, depuis Git Bash. Dans une console cmd ou PowerShell interactive, elle risque de ne pas s'afficher.
- **À faire :** lancer `dragontree.exe --help` dans cmd et dans PowerShell. Si rien ne s'affiche, s'attacher à la console parente (`AttachConsole`) avant d'écrire.
- **Priorité :** basse.

#### L1 — Dépôts SHA-256 non pris en charge

- **État :** l'identifiant de commit fait 20 octets (`crates/dragontree-git/src/oid.rs:6`), et rien ne détecte `extensions.objectFormat`. Le comportement sur un dépôt SHA-256 n'a pas été testé (à vérifier).
- **À faire :**
  1. Afficher au moins une erreur explicite à l'ouverture.
  2. Plus tard, gérer des identifiants de 32 octets.
- **Critère de fin :** test d'intégration avec `git init --object-format=sha256` : message clair, sans panique.
- **Priorité :** basse.

#### L2 — Éditeur de conflits limité aux fichiers UTF-8

- **Contexte :** l'indexation partielle a été corrigée pour les fichiers Windows-1252 (point critique C1), fréquents dans le vieux code .NET français. L'éditeur de conflits, lui, refuse ces fichiers.
- **État :** `crates/dragontree-app/src/repo/conflicts.rs:24-36` renvoie vers « notre / leur version » ou vers l'outil externe.
- **À faire :** travailler sur les octets, décoder en Windows-1252 pour l'affichage (`decode_cp1252`, `crates/dragontree-git/src/diff.rs:30`) et réécrire les octets d'origine.
- **Critère de fin :** test d'intégration : un conflit dans un fichier Latin-1 est résolu région par région, et les octets hors conflit sont préservés.
- **Priorité :** moyenne.

#### L3 à L6 — Arbitrages de la revue au coût assumé

Chacun de ces arbitrages a un coût annoncé. Ce sont des pistes d'amélioration, de priorité basse.

- **L3. Réglages illisibles au démarrage :** si le fichier est verrouillé, rien n'est enregistré de la session (`crates/dragontree-app/src/settings.rs:118-131`, `:163-169`). Piste : retenter la lecture plus tard et prévenir à chaque modification non enregistrée.
- **L4. Échec d'authentification du fetch automatique :** il n'est signalé qu'une fois par série d'échecs (`crates/dragontree-app/src/repo/mod.rs:638-655`). Piste : un indicateur persistant dans l'onglet.
- **L5. Amend d'un commit déjà poussé avec « Pousser après » :** rien n'est poussé, une notification s'affiche (`crates/dragontree-app/src/repo/actions.rs:224-243`). Piste : proposer explicitement un push forcé dans un dialogue de confirmation.
- **L6. Option « Espaces » active :** le diff des changements passe en lecture seule (`crates/dragontree-app/src/repo/changes.rs:510-524`). Piste : recalculer le diff sans `-w` pour construire le patch, comme la revue le suggérait.

## Branche `options-fork` (2026-10-20)

La branche `options-fork` livre toutes les options des tableaux ci-dessus (F2–F4, V2–V13, R1–R20, C2–C8, H1–H4) ainsi que la barre de menus Fichier / Affichage / Dépôt / Configuration (T2), un commit par option (id en tête du sujet), corrections de relecture en `fixup!`. Elle résout T1 (worktrees, via R3), T2, N2, B1, B5 et B6 (via C3), et tranche les raccourcis (Ctrl+Maj+L reste le journal, Pull = Ctrl+Maj+U, Ctrl+Maj+C reste Cloner). Tout passe par un registre de commandes unique (`crates/dragontree-app/src/commands.rs`) : menus, raccourcis, palette Ctrl+P et fenêtre des raccourcis en dérivent. Les commandes personnalisées (R15) refusent les valeurs dangereuses pour `cmd.exe` et passent aussi les valeurs acceptées en variables `DT_*` ; l'IA (C7) lance Claude Code sans aucun outil, consigne sur stdin, et reste désactivée par défaut. La suite compte 870 tests, clippy et rustfmt sont verts. Restent ouverts : B2 à B4 et B7, T3 à T6, N1, N3, RV1 à RV5, P1 à P4, L1 à L6.

## État du plan d'implémentation

Plan : `docs/superpowers/plans/2026-10-16-dragontree.md` (non réécrit ici).

- **Tâches 0 à 24 :** toutes faites selon le journal d'exécution, mais aucune case n'est cochée (P2).
- **Mesures de la tâche 24 :**
  - 110 Mo de mémoire privée avec 3 onglets, dont le gros dépôt ;
  - historique complet en 1,24 s ;
  - CPU au repos quasi nul.
- **Revue finale :** elle a été faite. Ses corrections sont listées pour mémoire, à ne pas refaire ; chacune a un test vu en échec avant la correction :
  - critiques : C1 (indexation partielle des fichiers non UTF-8), C2 (Maj+clic qui plantait) ;
  - importants : I3 à I9 ;
  - mineurs reclassés en importants : M1 à M5, M9, M11, M12, M14, M15, M17, M18.
- **Écarts restants :**
  - tâche 15 : colonne du graphe → T5 ;
  - tâche 16 : glisser-déposer entre listes → T5 ;
  - tâche 18 : colonne « mise à jour » → T5 ;
  - tâche 19 : cache LRU et seuil de coloration → RV1 ;
  - tâche 22 : aperçu des conflits → T5.
- **Écarts assumés (arbitrages, rien à faire) :**
  - éditeur de séquence `cp` au lieu du mode `--write-todo` (tâches 10 et 12) ;
  - PowerShell comme terminal de repli au lieu de `cmd /K` (tâche 14) ;
  - journal limité à 600 entrées au lieu de 500 (tâche 17) ;
  - interface en français et en anglais au lieu du français seul (contraintes globales).
- **Tests au 2026-10-17 :** 277 passent, clippy ne signale rien.

## Sources

- Notes de version de Fork pour Windows : <https://git-fork.com/releasenoteswin>. Version 2.23.2 au 2026-10-18.
- Liste officielle des raccourcis : <https://github.com/fork-dev/TrackerWin/issues/333>.
- Captures d'écran et réponses du développeur dans les tickets <https://github.com/fork-dev/TrackerWin/issues>.
- Barre de menus de Fork pour Windows : File, View, Repository, Window, Help. Il n'y a ni menu Edit ni menu Tools.
- Échanges de développement des 16 et 17 octobre 2026 :
  - demande initiale ;
  - revue de code finale (2 critiques, 7 importants, 21 mineurs) ;
  - relecture de la traduction ;
  - rapport du crate des forges ;
  - recensement des menus de Fork ;
  - choix du nom.
- Journal d'exécution du plan (non versionné), avec les arbitrages et les mineurs différés.
- Code du dépôt au 2026-10-19 : chaque point ci-dessus y a été vérifié.
