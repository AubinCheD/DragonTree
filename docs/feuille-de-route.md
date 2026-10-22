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

Synthèse au 2026-10-20. Chaque point est détaillé plus bas, sous son identifiant. Les options de Fork restent dans les tableaux ci-dessus et ne sont pas répétées. Terminés depuis le 2026-10-19 : T1, T2, N2, B1, B5, B6 (branche `options-fork`), B2, B3, B4 et B7 (branche `points-restants`).

### Bugs à corriger

| # | Point | Origine | Priorité |
|---|---|---|---|
| B1 | Ctrl+1 (« Modifications locales ») jamais atteint (corrigé : Affichage › Modifications locales) | revue finale, mineur n°19 | basse |
| B2 | Dossier `.git/dragontree-rebase` laissé après un rebase arrêté puis terminé (corrigé) | revue finale, mineur n°20 | basse |
| B3 | Décocher « Amend » trop vite écrase le message restauré (corrigé) | revue finale, mineur n°13 | basse |
| B4 | Compte enregistré avec le jeton du formulaire au lieu du jeton vérifié (corrigé) | revue finale, mineur n°10 | basse |
| B5 | La fenêtre Réglages écrase des changements faits ailleurs (corrigé) | revue finale, mineur n°8 | basse |
| B6 | `wt.exe -d` coupe un chemin contenant « ; » (corrigé) | revue finale, annexe de I8 | basse |
| B7 | Menu contextuel d'onglet lié à l'identifiant de l'onglet (corrigé) | arbitrage de la revue (M14) | basse |

### Fonctionnalités à terminer

| # | Point | Origine | Priorité |
|---|---|---|---|
| T1 | Worktrees : lister, ouvrir, créer, supprimer (fait, via R3) | demande du 2026-10-17 | haute |
| T2 | Barre de menus Fichier / Affichage / Dépôt / Configuration et raccourcis (fait) | demande du 2026-10-17 | haute |
| T3 | Stashs plus visibles (fait : section avant Remotes et Tags, stashs dans le graphe) | demande du 2026-10-17 | moyenne |
| T4 | Filtre sur la branche active : mémorisation par dépôt (fait ; raccourci fait, V2) | demande du 2026-10-17 | moyenne |
| T5 | Éléments du plan non livrés : glisser-déposer entre listes, colonne du graphe, colonne « mise à jour », aperçu des conflits (fait) | revue finale, mineur n°21 | basse |
| T6 | Associer un compte à un dépôt existant (fait) | arbitrage de la tâche 18, « plus tard » | moyenne |

### À implémenter

| # | Point | Origine | Priorité |
|---|---|---|---|
| N1 | Vue centralisée des pull requests de tous les comptes (fait) | demande initiale, « pour plus tard » | moyenne |
| N2 | Options de Fork : F2–F4, V3–V12, R2–R20, C2–C8, H1–H4 (fait, tout est ✅ dans les tableaux ci-dessus) | demande du 2026-10-17 | terminé |
| N3 | Icône intégrée à l'exécutable (fait) | limite connue | basse |

### Constats de revue

| # | Point | Origine | Priorité |
|---|---|---|---|
| RV1 | Fluidité de l'interface sur un gros dépôt : filtre, copies par image, coloration synchrone (corrigé) | revue finale, mineur n°6 | moyenne |
| RV2 | Gros diffs lus entièrement en mémoire avant la limite de 8 Mio (corrigé) | revue finale, mineur n°7 | moyenne |
| RV3 | Fetch automatique dans la file des écritures ; deux files pour un même dépôt (corrigé) | revue finale, mineur n°16 | basse |
| RV4 | Points non jugés : processus git à la fermeture, instances multiples, proxy, cas des forges (corrigé ; HTTP simple documenté comme limite) | revue finale et rapport du crate forges | basse |
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

#### T3 — Stashs plus visibles (fait)

- **Contexte :** l'utilisateur a demandé à « voir ses stashs ». La fonction existe. Claude a supposé que l'utilisateur ne l'avait pas vue ; ce n'est pas confirmé.
- **État :**
  - la section Stashes (`crates/dragontree-app/src/repo/sidebar.rs:170-202`) affiche le diff au clic et propose Appliquer, Pop et Supprimer ;
  - réalisé : elle vient maintenant juste après Branches, avant Remotes et Tags ; les stashs sont aussi dans le graphe (V7) ;
  - le bouton Pop de la barre d'outils n'agit que sur le dernier stash (`crates/dragontree-app/src/repo/toolbar.rs:74-82`).
- **À faire :**
  1. Confirmer le besoin avec l'utilisateur (à vérifier).
  2. Réaliser V7 (stashs dans le graphe) et R4 (stashs complets), s'ils sont retenus.
  3. Sinon, au minimum, placer la section Stashes avant Remotes et Tags.
- **Critère de fin (atteint) :** avec 400 branches distantes, le stash est visible sans défilement (`the_stash_entry_is_visible_in_the_sidebar_without_scrolling_despite_many_remote_branches`, `crates/dragontree-app/tests/ui_stashes.rs`), plus les tests propres à V7 et R4.
- **Priorité :** moyenne (demande explicite, mais la fonction existe).

#### T4 — Filtre sur la branche active : raccourci et mémorisation (fait)

- **Contexte :** l'utilisateur a demandé à filtrer l'historique sur la branche active. La fonction existe ; même réserve que pour T3.
- **État :**
  - le bouton « Toutes les branches / Branche courante » est dans `crates/dragontree-app/src/repo/history.rs:157-176` ;
  - le journal est alors limité à HEAD (`crates/dragontree-app/src/repo/scope.rs:102`) et il est rechargé après chaque checkout ;
  - fait (V2) : raccourci Ctrl+Maj+A et entrée Affichage › Branche courante seulement (cochée quand le filtre est actif) ;
  - fait (V3) : filtre sur plusieurs branches, dossiers ou remotes, exclusif avec « Branche courante seulement » ;
  - réalisé : le choix est mémorisé par dépôt (`Settings::current_branch_only_repos`) et restauré à l'ouverture de l'onglet ; le filtre V3 reste de session ;
- **À faire :**
  1. Raccourci Ctrl+Maj+A et entrée du menu Affichage (V2, fait).
  2. Mémoriser le choix par dépôt dans les réglages (Fork le garde par worktree), à confirmer avec l'utilisateur.
  3. Filtre sur plusieurs branches (V3, fait).
- **Critère de fin :**
  - test d'interface : Ctrl+Maj+A bascule le filtre et son libellé (atteint : `current_branch_shortcut_hides_and_restores_the_other_branch_commits`, `crates/dragontree-app/tests/ui_history_filters.rs`) ;
  - test : à la réouverture du dépôt, le choix est conservé (atteint : `current_branch_only_is_remembered_per_repository_across_sessions`, même fichier).
- **Priorité :** moyenne (pour la mémorisation).

#### T5 — Éléments du plan non livrés

- **Contexte :** la revue finale (mineur n°21) a relevé quatre éléments du plan absents. Ils ont été différés.
- **État et travail restant :**
  - **a. Glisser-déposer entre « Non indexés » et « Indexés »** (plan, tâche 16) (fait).
    - État : réalisé (`crates/dragontree-app/src/repo/file_drag.rs`). Un fichier se glisse d'une liste à l'autre ; toute la sélection de sa liste le suit ; conflits et fichiers ignorés ne se glissent pas. Double-clic, Espace, Entrée et menus sont inchangés.
    - Fin (atteinte) : `crates/dragontree-app/tests/ui_file_drag.rs` (glisser vers Indexés puis retour, sélection multiple, dépôt sur sa propre liste).
  - **b. Colonne du graphe redimensionnable** (tâche 15) (fait).
    - État : réalisé. Une poignée au bord droit de la colonne, dans l'en-tête, la redimensionne (22 à 600 px) ; un double-clic revient à la largeur calculée d'après les branches (22 à 220 px, `graph_paint::graph_width`). La largeur choisie est enregistrée dans les réglages (`graph_column_width`, globale, absente par défaut) et prime sur la largeur calculée : repli des fusions et filtres ne la changent pas. La fenêtre Réglages ne l'édite pas.
    - Fin (atteinte) : `crates/dragontree-app/tests/ui_graph_column.rs` (glisser la poignée, largeur retrouvée par une nouvelle session du même profil ; double-clic).
  - **c. Colonne « mise à jour » dans la liste de clonage** (tâche 18) (fait).
    - État : réalisé. Chaque ligne affiche la date de mise à jour à droite de la première ligne, à gauche du cadenas (relative si récente, absolue sinon : `widgets::format_time`), d'après `RemoteRepo::updated_timestamp` (GitHub : `pushed_at`, sinon `updated_at` ; GitLab : `last_activity_at`). Azure DevOps : colonne vide, décision documentée dans `crates/dragontree-forges/src/azure.rs` : l'API des dépôts ne donne aucune date par dépôt (le `project.lastUpdateTime` est celui du projet, donc trompeur) et le dernier push demanderait un appel par dépôt.
    - Fin (atteinte) : tests `updated_timestamp_reads_the_activity_date` (forges) et `the_update_date_is_relative_when_recent_and_absolute_otherwise` (formats GitHub et GitLab ; Azure sans date) ; `azure_repositories_fixture_parses` vérifie l'absence de date côté Azure.
  - **d. Aperçu du résultat dans l'éditeur de conflits** (tâche 22) (fait).
    - État : réalisé. Un panneau « Aperçu du résultat » (lecture seule, redimensionnable, sous les régions) montre le fichier tel qu'il sera enregistré, mis à jour à chaque choix ; une région sans choix garde ses marqueurs. Le texte vient de `resolve_partial` (`crates/dragontree-git/src/conflict.rs`), variante de `resolve` qui accepte des choix absents ; l'aperçu n'est recalculé que lorsque les choix changent. Pas de défilement synchronisé.
    - Fin (atteinte) : tests unitaires de l'aperçu pour chaque choix (notre, leur, les deux, les deux inversés, sans choix) dans `crates/dragontree-app/src/repo/conflicts.rs`, et test de `resolve_partial` dans le moteur.
- **Priorité :** basse (mineurs différés).

#### T6 — Associer un compte à un dépôt existant (fait)

- **Contexte :** arbitrage de la tâche 18. L'authentification d'un compte n'est injectée que pour les dépôts clonés depuis ce compte, pour ne pas casser un Git Credential Manager qui fonctionne. L'association manuelle était notée « à ajouter plus tard ».
- **État :** réalisé (`crates/dragontree-app/src/repo/repo_account.rs`).
  - Avant : `repo_accounts` (`crates/dragontree-app/src/settings.rs`) n'était rempli qu'à la fin d'un clonage (`crates/dragontree-app/src/clone.rs`), lu par `account_for_repo` pour fetch, pull et push (`RepoTab::net_account`, `crates/dragontree-app/src/repo/actions.rs`) et vidé à la suppression du compte (`crates/dragontree-app/src/accounts.rs`) ; aucune interface ne permettait d'associer ou de dissocier un compte.
  - Dépôt › **Utiliser le compte…** ouvre un dialogue : les comptes dont `account_matches_url` (`crates/dragontree-forges/src/auth.rs`) correspond à l'URL de fetch ou de push d'un remote d'abord, suivis du nom de ce remote, puis les autres sous un intertitre, avec un avertissement si l'on en coche un (son jeton ne serait envoyé à aucun remote). Le compte du dépôt est coché, sinon le premier qui correspond. Sans compte enregistré, la fenêtre Comptes s'ouvre.
  - Dépôt › **Ne plus utiliser de compte** retire l'association (grisée sans compte associé) : git reprend sa configuration habituelle.
  - Les réglages du dépôt (R8) ont la même liste, avec « Aucun » en tête ; seul un changement est enregistré.
  - Le compte associé est affiché dans la barre d'outils de l'onglet (un clic rouvre le choix) et dans l'infobulle de l'onglet.
  - L'association (`Settings::set_account_for_repo`) est rangée sous la racine de la copie de travail, comme après un clonage. La fenêtre Réglages n'édite pas `repo_accounts` : il reste hors de `take_edits`.
  - Un worktree lié sans association propre reprend le compte de la copie de travail principale (`Repo::main_workdir`, dérivé du répertoire commun) ; « Ne plus utiliser de compte » ne retire que l'association du worktree lui-même.
- **À faire :**
  1. Ajouter « Utiliser le compte… » et « Ne plus utiliser de compte » dans le menu du dépôt, ou dans les réglages du dépôt (R8) (fait : les deux).
  2. Proposer les comptes dont `account_matches_url` (`crates/dragontree-forges/src/auth.rs`) correspond à un remote (fait).
  3. Afficher le compte associé dans l'onglet (fait).
- **Critère de fin (atteint) :**
  - test unitaire de la proposition de comptes à partir des remotes : `accounts_matching_a_remote_are_proposed_first_with_that_remote` et `without_a_matching_remote_every_account_is_still_proposed_in_order` (`crates/dragontree-app/src/repo/repo_account.rs`) ;
  - test : après association, `account_for_repo` renvoie le compte, et la dissociation le retire (`an_associated_account_is_returned_until_it_is_dissociated`, `crates/dragontree-app/src/settings.rs`) ;
  - tests d'interface avec des comptes fictifs, sans jeton : `crates/dragontree-app/tests/ui_repo_account.rs` (menu Dépôt, réglages du dépôt, aucun compte enregistré, worktree lié).
- **Priorité :** moyenne (reportée explicitement ; utile dès qu'on a plusieurs comptes).

### À implémenter

#### N1 — Vue centralisée des pull requests (fait)

- **Contexte :** cette vue fait partie de la demande initiale (« à noter pour plus tard ») : centraliser les PR de tous les comptes au même endroit. Le README l'annonçait « à venir ». Décision du plan : vue en lecture, la création existant déjà (R18).
- **État :** réalisé.
  - Avant : le crate des forges savait vérifier un compte, lister les dépôts et créer une pull request (R18) ; aucune liste de pull requests n'était lue et aucune vue n'existait.
  - Modèle `PullRequest` (`crates/dragontree-forges/src/open_pulls.rs`) : compte, dépôt (`RepoLocation`, d'où la forge, le nom affiché et la page web), numéro, titre, auteur, brouillon, relecteurs, « créée par moi », « relecture demandée », URL, date de mise à jour, branches source et cible. Seules les pull requests ouvertes sont lues : leur état se réduit au brouillon. `list_pull_requests(compte, jeton)` fusionne les recherches (une entrée par URL, indicateurs cumulés) et trie par date de mise à jour décroissante.
  - GitHub : recherche GraphQL (`/graphql`, `/api/graphql` pour GitHub Enterprise Server, `api.<hôte>/graphql` pour `*.ghe.com`) `is:pr is:open archived:false involves:<login>` puis `review-requested:<login>` (que `involves:` ne couvre pas), triée par mise à jour, curseur suivi sur 10 pages de 50 : branches, brouillon et relecteurs viennent avec la recherche (la recherche REST demanderait un appel de plus par pull request). Des erreurs sans résultat (limite de débit) sont renvoyées avec leur message ; avec des résultats (organisation protégée par SSO), ignorées.
  - GitLab : `merge_requests?scope=created_by_me`, `scope=assigned_to_me` et `scope=all&reviewer_username=<login>` (`state=opened`, triées par mise à jour). `scope=all` seul listerait toutes les merge requests visibles de l'instance : il n'est jamais demandé sans filtre, et les réponses du filtre de relecteur ne sont gardées que si l'utilisateur est relecteur (une instance qui ignorerait le paramètre ne fait donc rien apparaître de plus). Brouillon : `draft`, ou `work_in_progress` avant GitLab 14 ; projet lu dans `references.full`, sinon dans l'adresse de la page. « À relire » et « Relecture demandée » veulent dire « relecteur désigné » : GitLab garde les relecteurs après leur relecture et sa liste ne donne pas l'état de chaque relecture (limite notée dans le README et sur `PullRequest::review_requested`).
  - Azure DevOps : dans chaque organisation du compte, identité de l'utilisateur par `connectionData`, puis `_apis/git/pullrequests?searchCriteria.status=active` avec `creatorId` et avec `reviewerId` (API 5.0 pour Azure DevOps Server). À relire : l'utilisateur n'a pas encore voté, ou une de ses équipes est relectrice. Une organisation qui échoue alors qu'une autre répond est renvoyée à part (`PullRequestList::failed_orgs`) et signalée par une ligne d'avertissement sous le compte ; si toutes échouent, la première erreur devient celle du compte. Un compte sans organisation n'est pas lu : une explication s'affiche à sa place (« Vérifier » ou l'ajouter de nouveau avec ses organisations). L'API ne donne pas de date de mise à jour : la date de création en tient lieu.
  - Fenêtre Fichier › **Pull requests…** (registre `Command::PullRequests`, aussi dans la palette ; `--window pullrequests`) : `crates/dragontree-app/src/pull_requests.rs`. Liste unique de tous les comptes, de la plus récemment mise à jour à la plus ancienne : titre (clic : la pull request dans le navigateur), marques « Brouillon » et « Relecture demandée », date ; dépôt (clic : sa page), numéro, auteur, branches. Filtres « Toutes / Créées par moi / À relire », par compte et texte (titre, dépôt, auteur, branche, numéro ; sans casse ni accents).
  - Lien vers le dépôt local : les remotes des dépôts récents puis de ceux de la liste de droite sont lus sur disque en tâche de fond (`discovery::remote_urls` : `.git/config`, celui du dépôt principal pour un worktree), à chaque rafraîchissement et à la fin d'un scan ; « Ouvrir le dépôt » apparaît quand l'un d'eux désigne le dépôt de la pull request (`RepoLocation::same_repo` : même forge, hôte et noms sans la casse, formes HTTPS, SSH et `visualstudio.com` comprises). Limites de cette lecture simplifiée (documentées sur `parse_remote_urls` et dans le README) : valeurs continuées sur plusieurs lignes, clé écrite sur la ligne de la section, échappements autres que `\"` et `\\`, fichier qui n'est pas en UTF-8 ; un alias d'hôte SSH (`~/.ssh/config`) n'est pas résolu, un tel remote n'est donc jamais relié.
  - Rafraîchissement en arrière-plan : à l'ouverture, toutes les 5 minutes tant que la fenêtre est ouverte, pour un compte ajouté entre-temps, et par **Actualiser** (grisé pendant une lecture). Une tâche par compte, numéro de génération (une réponse d'un rafraîchissement précédent est ignorée) ; l'erreur d'un compte s'affiche à sa place sans empêcher les autres, une panique devient son erreur. Les jetons ne sont lus que dans ces tâches et ne figurent dans aucune erreur. La fenêtre demande l'image suivante pour l'heure du prochain rafraîchissement, et au moins une seconde plus tard (jamais à chaque image) quand une lecture dure plus de 5 minutes (`repaint_delay`). « Aucune pull request ouverte. » ne s'affiche que si au moins un des comptes affichés a répondu : quand tous sont en erreur, leurs erreurs suffisent.
- **À faire :**
  1. Modèle `PullRequest` : forge, dépôt, titre, auteur, état, relecteurs, brouillon, URL, date (fait ; état : ouvertes seulement, brouillon ou non).
  2. Appels par forge (fait) :
     - GitHub : recherche des PR ouvertes qui concernent l'utilisateur (GraphQL, `involves:` et `review-requested:`) ;
     - GitLab : `merge_requests?state=opened`, créées par l'utilisateur, assignées et à relire (`scope=all` filtré par relecteur) ;
     - Azure DevOps : `_apis/git/pullrequests?searchCriteria.status=active` par organisation, créateur et relecteur.
  3. Vue agrégée avec des filtres (créées par moi, à relire, par compte), l'ouverture dans le navigateur et le lien vers le dépôt local s'il est cloné (fait).
  4. Rafraîchissement en arrière-plan (fait).
  5. La création de PR (R18) vient ensuite (déjà faite).
- **Critère de fin (atteint) :**
  - tests d'analyse sur des réponses JSON représentatives de chaque forge, dans `crates/dragontree-forges/tests/fixtures/` (`github_pull_requests_search.json`, `gitlab_merge_requests.json`, `azure_pull_requests.json`) : tests de `crates/dragontree-forges/src/open_pulls.rs` (analyse, adresses des requêtes, pagination et curseur, fusion des recherches, filtre du relecteur GitLab, votes et équipes Azure DevOps, organisations en échec) ; `same_repo` et `web_url` dans `location.rs` ; `remote_urls` dans le moteur (`discovery.rs` et `crates/dragontree-git/tests/remote_urls.rs`, comparé à `git remote -v` pour un dépôt et son worktree) ;
  - tests réels ignorés par défaut dans `crates/dragontree-forges/tests/live.rs` : `live_pull_requests_with_an_invalid_token_are_unauthorized` et `live_list_open_pull_requests` (variables `DRAGONTREE_LIVE_PRS_*`, n'affiche que des nombres) ;
  - test d'interface de la liste : `crates/dragontree-app/tests/ui_pull_requests.rs` (comptes fictifs, réponses injectées par `StartOptions::pull_request_source`, sans réseau ni Gestionnaire d'identifiants : tri, erreur d'un compte, filtres, ouverture dans le navigateur, ouverture du clone local ; sans compte, la palette ouvre la fenêtre qui propose d'en ajouter).
- **Priorité :** moyenne (reportée par l'utilisateur lui-même).

#### N2 — Options de Fork à développer (fait)

- **Contexte et état :** les options sont dans les tableaux ci-dessus, avec leur taille ; toutes sont ✅ depuis la branche `options-fork`. R3 relève de T1. Les fonctions que le README listait comme non gérées sont faites : Git LFS (R14), git-flow (R13), reflog (V6), commandes personnalisées (R15).
- **Réalisé :** toutes les options ont été implémentées. Pour chacune :
  - un test d'intégration du moteur si une commande git est ajoutée ;
  - un test d'interface ;
  - les traductions.
- **Critère de fin :** les tests de l'option passent, et l'option passe à ✅ dans les tableaux.
- **Priorité :** terminé.

#### N3 — Icône intégrée à l'exécutable (fait)

- **Contexte :** limite citée dans le README et dans le bilan du 2026-10-16.
- **État :** réalisé.
  - Avant : seule l'icône de fenêtre existait, dessinée en code dans `main.rs`, et `dragontree.exe` n'avait pas de ressource d'icône.
  - Le dessin est dans `crates/dragontree-app/src/app_icon.rs` : à toute taille (grille de 64 unités, bords lissés sur un pixel, composition sans frange sombre), pour l'icône de fenêtre (64 px) et pour `assets/dragontree.ico` (16, 24, 32, 48 et 64 px en bitmaps 32 bits, 256 px en PNG), versionné. `cargo run -p dragontree-app --example app_icon` le régénère ; un test vérifie qu'il correspond au dessin.
  - `crates/dragontree-app/build.rs` intègre le `.ico` sans outil externe ni nouvelle dépendance : il écrit l'objet COFF qu'aurait produit `cvtres` (`.rsrc$01` : répertoire des ressources RT_ICON et RT_GROUP_ICON, `.rsrc$02` : les images, adresses relogées en ADDR32NB) et le passe à l'éditeur de liens du binaire (`cargo:rustc-link-arg-bins`). Le même objet est destiné à ld (GNU) et à link.exe (MSVC) ; `scripts/setup-toolchain.ps1` reste inchangé (pas de `windres.exe`).
- **À faire :**
  1. Ajouter un `.ico` dans `assets/` (fait).
  2. Ajouter un `build.rs` qui intègre la ressource. Avec la toolchain GNU, il faut aussi extraire `windres.exe`, ou utiliser une crate qui n'a besoin d'aucun outil externe (fait : objet écrit par `build.rs`, sans outil).
- **Critère de fin (atteint avec GNU ; MSVC : objet lié par lld-link, link.exe à confirmer) :** l'Explorateur affiche l'icône de `target\release\dragontree.exe`, avec les toolchains GNU et MSVC.
  - Test `the_executable_carries_the_application_icon_at_every_size` (`crates/dragontree-app/tests/exe_icon.rs`) : il lit les ressources PE de l'exécutable construit (groupe d'icônes, six images identiques à celles du `.ico`), quelle que soit la toolchain qui lance les tests.
  - Vérifié le 2026-10-21 avec la toolchain GNU (ld 2.44) : icône extraite par le shell Windows (`ExtractAssociatedIcon`) en debug et en release. Côté MSVC, l'objet a été lié par `rust-lld -flavor link` (éditeur de liens compatible link.exe) : ressources identiques ; la toolchain MSVC elle-même n'est pas installée sur le poste de développement, l'édition des liens par link.exe reste donc à confirmer.
- **Priorité :** basse.

### Bugs à corriger

#### B1 — Ctrl+1 (« Modifications locales ») jamais atteint (fait)

- **Contexte :** mineur n°19 de la revue finale.
- **État :** corrigé en T2 : le code mort a été supprimé et la vue s'atteint par Affichage › Modifications locales. Avant : le raccourci d'onglet (`crates/dragontree-app/src/app.rs:479-485`) consommait Ctrl+1 avant celui de l'onglet de dépôt (`crates/dragontree-app/src/repo/mod.rs:895-899`). Ce second code ne s'exécute donc jamais.
- **Réalisé :** Ctrl+1…9 reste au changement d'onglet (section Raccourcis) ; le code mort de Ctrl+1 a été supprimé en T2 et la vue s'atteint par Affichage › Modifications locales.
- **Critère de fin :** atteint (accès par le menu testé, code mort supprimé).
- **Priorité :** basse.

#### B2 — Dossier `.git/dragontree-rebase` laissé après un rebase arrêté (fait)

- **Contexte :** mineur n°20 de la revue finale.
- **État :** corrigé. Avant : le dossier n'était supprimé que si le rebase se terminait d'un seul coup ; continuer, abandonner ou passer un commit depuis le bandeau le laissait en place.
- **Réalisé :** `Repo::cleanup_rebase_dir` (`crates/dragontree-git/src/rebase.rs`) supprime le dossier quand `operation_state()` vaut `None` ; elle est appelée après `interactive_rebase`, `continue_op`, `abort_op` et `skip_rebase_commit` (`crates/dragontree-git/src/ops.rs`), même si git renvoie une erreur.
- **Critère de fin :** atteint (tests d'intégration `continuing_a_stopped_rebase_removes_the_rebase_folder`, `aborting_…`, `skipping_…` et `a_rebase_still_in_progress_keeps_its_folder`, `crates/dragontree-git/tests/advanced_ops.rs`).
- **Priorité :** basse.

#### B3 — Décocher « Amend » trop vite écrase le message restauré (fait)

- **Contexte :** mineur n°13 de la revue finale.
- **État :** corrigé. Avant : cocher Amend lançait la lecture du dernier message dans un thread et le résultat remplaçait le résumé et la description sans vérifier qu'Amend était encore coché.
- **Réalisé :** la lecture porte un numéro (`ChangesState::last_message_generation`, `RepoEvent::LastMessage(generation, message)`) ; cocher et décocher l'incrémentent, et `apply_last_message` (`crates/dragontree-app/src/repo/changes.rs`) ignore un message dont le numéro n'est plus le bon ou arrivé quand Amend n'est plus coché. Le message restauré ou saisi est conservé ; le modèle de message (R8) et les suggestions (C5, C7) ne sont pas touchés.
- **Critère de fin :** atteint (tests unitaires `a_last_message_arriving_after_amend_was_unchecked_keeps_the_typed_message` et `a_last_message_from_an_earlier_check_is_ignored_after_checking_again`, `crates/dragontree-app/src/repo/changes.rs`).
- **Priorité :** basse.

#### B4 — Compte enregistré avec le jeton du formulaire au lieu du jeton vérifié (fait)

- **Contexte :** mineur n°10 de la revue finale.
- **État :** corrigé. Avant : la vérification partait avec le type, l'hôte et le jeton lus au clic, mais l'enregistrement relisait le formulaire à son retour ; un champ modifié entre-temps faisait stocker un jeton non vérifié.
- **Réalisé :** `ForgeEvent::Verified` transporte les valeurs envoyées (`Submitted` : type, hôte, jeton) et `AccountsWindow::on_verified` (`crates/dragontree-app/src/accounts.rs`) crée le compte et enregistre le jeton avec elles, sans relire le formulaire.
- **Critère de fin :** atteint (test unitaire `the_account_is_saved_with_the_verified_values_not_the_edited_form` : jeton et hôte modifiés pendant la vérification, le jeton enregistré et l'hôte du compte sont ceux qui ont été vérifiés ; le stockage est injecté, aucun accès au Gestionnaire d'identifiants).
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

#### B7 — Menu contextuel d'onglet lié à la position de l'onglet (fait)

- **Contexte :** arbitrage de la correction M14. Les lignes des listes ont reçu des identifiants stables, mais pas les onglets.
- **État :** corrigé. Avant : chaque onglet prenait un identifiant positionnel et ses actions portaient un index ; si un onglet se fermait pendant qu'un menu était ouvert, l'action pouvait viser l'onglet voisin.
- **Réalisé :** le widget d'un onglet a un identifiant dérivé de son `TabId` (`crates/dragontree-app/src/tabbar.rs`) ; toutes les actions (`Select`, `Close`, `CloseOthers`, `Move`, `OpenExplorer`, `CopyPath`, comme `SetColor`) portent des `TabId`, et `apply_tab_actions` (`crates/dragontree-app/src/app.rs`) en retrouve la position à l'application (action ignorée si l'onglet n'existe plus).
- **Critère de fin :** atteint (test d'interface `the_tab_menu_closes_the_tab_it_was_opened_on_even_if_an_earlier_tab_closes_meanwhile`, `crates/dragontree-app/tests/ui_tab_menu.rs` : vu rouge avec les index, vert avec les identifiants).
- **Priorité :** basse.

### Constats de revue

#### RV1 — Fluidité de l'interface sur un gros dépôt (corrigé)

- **Contexte :** mineur n°6 de la revue finale. Il touche l'objectif de performance de la demande initiale.
- **État :** corrigé.
  - **filtre (corrigé) :**
    - avant : à chaque frappe, il repliait tous les commits sur le thread de l'interface (`apply_filter`, `crates/dragontree-app/src/repo/history.rs`), estimé par la revue à 50–150 ms sur 86 k commits (194 ms mesurés sur 100 000 commits synthétiques) ;
    - désormais (`crates/dragontree-app/src/repo/history_filter.rs`) : une frappe ne fait que lancer le calcul en arrière-plan, avec un numéro de génération ; le calcul précédent est annulé et un résultat périmé ignoré ; le résultat précédent reste affiché en attendant (compteur et indicateur d'activité) ;
    - les lignes désignent toujours la liste affichée : quand elle est remplacée (fusions repliées V8, journal relu), l'ancien résultat est aussitôt ramené aux commits qu'elle contient encore, puis recalculé ; la recherche git (H3) passe par le même calcul (lignes dont l'oid est trouvé) ;
    - le repli des textes ne fait plus d'allocation par commit (`widgets::fold_into`, chemin rapide ASCII) : environ 24 ms en arrière-plan pour 100 000 commits ;
    - un commit défilé pendant le calcul (sélection par la recherche, « Aller à HEAD », repli des fusions) l'est sur les lignes provisoires, puis de nouveau, sur le résultat, à son arrivée, s'il est toujours sélectionné (`refollow`) ;
    - la tâche du filtre répond toujours : si elle panique ou ne démarre pas, le calcul n'est plus attendu, le résultat précédent reste affiché et une notification le signale (`Jobs::spawn_guarded`, `crates/dragontree-app/src/jobs.rs`) ;
  - **copies à chaque image (corrigé) :** la liste filtrée est partagée par `Arc` (`HistoryState::filtered`) et les étiquettes de refs sont lues dans l'instantané `Arc<RefsSnapshot>` au lieu d'une copie de leur table ; la position de la sélection parmi les résultats se trouve par recherche dichotomique ; le diff affiché est partagé par `Arc` (`LoadedDiff`) au lieu d'être copié à chaque image ;
  - **vue de diff (corrigé) :** son cache (lignes, largeur, caractères modifiés surlignés dans une ligne) avait pour clé une empreinte des longueurs de lignes, sans leur texte (`crates/dragontree-app/src/repo/diff_view.rs`) : après une modification de même longueur, le rafraîchissement de l'état, qui relit le diff en gardant la vue, laissait le surlignage périmé, et la largeur aussi quand une tabulation devenait une espace. Sa clé est désormais l'empreinte du contenu, texte compris (`LoadedDiff::content_hash`, `crates/dragontree-app/src/diff_style.rs`), calculée une fois au chargement du diff, hors du thread de l'interface, et partagée avec la coloration ;
  - **coloration syntaxique (corrigé) :**
    - avant : elle était calculée de façon synchrone, chargement de syntect compris (`crates/dragontree-app/src/highlight.rs`), et son cache ne gardait qu'un fichier, sous la clé (chemin, nombre de lignes, thème) : les couleurs restaient périmées après une modification qui ne change pas le nombre de lignes ; le plan demandait un cache LRU de 64 fichiers et un seuil de 20 000 lignes, le seuil était de 3 000 lignes (arbitrage de la tâche 19) ;
    - désormais : `Highlighter::prepare` ne fait que comparer l'empreinte du contenu (chemin, nature et texte de chaque ligne), calculée au chargement du diff, et, si elle change, reprendre les couleurs du cache ou lancer leur calcul dans une tâche de fond (lecture des lignes, développement des tabulations et syntect y compris) ; le calcul devenu inutile est interrompu ; le texte s'affiche sans couleur jusqu'au résultat, qui demande une nouvelle image ; aucun calcul n'est relancé tant que le contenu, le thème et la largeur de tabulation restent les mêmes ;
    - cache LRU partagé par les vues de diff : 64 diffs, indexés par l'empreinte du contenu, le thème et la largeur de tabulation (C6 développe les tabulations avant la coloration : les plages portent sur le texte affiché), borné aussi à 100 000 lignes colorées pour la mémoire ; plages voisines de même couleur réunies ;
    - seuil revu : 20 000 lignes, la valeur du plan. Synchrone, 3 000 lignes de code dense figeaient déjà l'interface environ une demi-seconde ; en tâche de fond, 20 000 lignes demandent environ 3,5 s, pendant lesquelles le texte reste lisible et l'interface fluide ;
    - seuils en octets : 1 Mio de texte en tout, 10 000 octets par ligne (au-delà, pas de coloration ni de tâche). syntect colore une ligne d'un seul tenant, sans voir l'annulation : environ 5 ms par Kio mesurés, soit 0,2 à 0,6 s pour une ligne minifiée de 100 Kio ; une extension sans syntaxe ne lance plus de tâche, une fois les syntaxes chargées.
- **À faire :**
  1. Filtrer en arrière-plan avec un numéro de génération, ou après un court délai de frappe (fait : en arrière-plan, avec génération et annulation).
  2. Partager les étiquettes et la liste filtrée par `Arc` au lieu de les copier (fait).
  3. Colorer dans une tâche de fond, avec un cache LRU indexé par le contenu (fait).
  4. Revoir ensuite le seuil de 3 000 lignes (fait : 20 000 lignes).
- **Critère de fin (atteint) :**
  - mesure sur le gros dépôt : une frappe dans le filtre coûte moins de 16 ms au thread d'interface (atteint : test `a_keystroke_in_the_filter_costs_less_than_a_frame_on_the_interface_thread`, `crates/dragontree-app/src/repo/history_filter.rs`, sur 100 000 commits synthétiques en mémoire : environ 0,1 ms, contre 194 ms avant ; les tests voisins vérifient que le résultat est appliqué au retour du calcul, qu'un résultat périmé est ignoré et que le filtre suit la liste affichée) ;
  - test unitaire : une ligne modifiée sans changer le nombre de lignes est recolorée (atteint : `a_line_edited_without_changing_the_line_count_is_recolored`, `crates/dragontree-app/src/highlight.rs`, vu rouge avec l'ancienne clé ; aussi `the_cache_evicts_the_least_recently_used_diff` et `no_highlighting_work_runs_on_the_interface_thread`, qui vérifie que la coloration ne tourne jamais sur le thread qui affiche) ;
  - suivi de revue : `a_line_edited_without_changing_its_length_is_highlighted_again` (`crates/dragontree-app/src/repo/diff_view.rs` : surlignage et largeur recalculés, vu rouge avec l'ancienne empreinte), `a_very_long_line_or_a_very_large_diff_is_not_colored` et `an_unknown_extension_is_not_colored_and_starts_no_thread` (`highlight.rs`, vus rouges), `the_scroll_follows_the_selected_commit_when_the_filter_result_arrives` (vu rouge) et `a_failed_filter_job_does_not_leave_the_filter_pending` (`history_filter.rs`), `a_guarded_job_that_panics_sends_its_fallback` (`jobs.rs`).
- **Priorité :** moyenne.

#### RV2 — Gros diffs lus entièrement en mémoire (corrigé)

- **Contexte :** mineur n°7 de la revue finale. Il touche l'objectif de moins de 150 Mo de mémoire.
- **État :** corrigé.
  - Avant : `diff_commit_file` et `diff_worktree_file` lisaient toute la sortie de git, et la limite de 8 Mio n'était appliquée qu'ensuite ; seul le fichier non suivi était contrôlé avant la lecture. Un diff de 50 Mio faisait monter la mémoire de près de 100 Mio.
  - Désormais, l'exécution des commandes git a un mode de sortie bornée (`GitCommand::stdout_limit`, `crates/dragontree-git/src/runner.rs`) : dès que la sortie standard dépasse la limite, la lecture s'arrête, git est arrêté avec ses sous-processus (Job Object, comme l'annulation) et la commande réussit avec `Output::stdout_truncated` ; le tampon ne grandit jamais au-delà de la limite (capacité comprise). Les morceaux lus dans les tubes passent par une file bornée (8 morceaux de 64 Kio au plus en attente) : un tube plus rapide que la lecture attend au lieu d'accumuler. Le journal git note une commande ainsi arrêtée comme une annulation, avec la limite (« sortie de plus de 8 Mio : lecture arrêtée et git arrêté »).
  - Les diffs de fichier (`diff_commit_file`, `diff_worktree_file`, `compare_file_diff`, `crates/dragontree-git/src/repo.rs`) lisent la sortie de git avec la limite `MAX_DIFF_BYTES` (8 Mio) : au-delà, le diff est `truncated`, sans hunks, comme avant. Le fichier non suivi trop gros reste écarté avant même de lancer git.
- **À faire :** arrêter la lecture, et le processus, dès que la limite est dépassée, ou contrôler la taille avant de lancer le diff (fait : lecture et processus arrêtés à la limite).
- **Critère de fin (atteint) :**
  - test d'intégration `the_diff_of_a_50_mib_file_is_truncated_without_holding_more_than_the_limit` (`crates/dragontree-git/tests/large_diff.rs`) : diff d'un commit qui ajoute un fichier de 50 Mio, de sa modification non indexée puis indexée et d'une comparaison de révisions ; chaque résultat est tronqué, et un allocateur qui compte les octets alloués mesure le pic de mémoire pendant le diff : environ 8,2 Mio (la limite de 8 Mio et quelques tampons de 64 Kio, 210 Kio en tout), contre près de 100 Mio avant (vu rouge). Le test accepte 512 Kio au-delà de la limite : assez peu pour voir une file de morceaux sans borne (essayée : l'un des quatre diffs au moins dépassait, de 450 à 900 Kio au-delà) ;
  - test `a_bounded_output_stops_reading_and_stops_git_at_the_limit` (`crates/dragontree-git/tests/runner.rs`) : une sortie sans fin (`yes`, lancé par `sh`) est coupée exactement à la limite et la commande se termine au lieu de lire sans fin ; une sortie de la taille exacte de la limite n'est pas coupée. Ce test ne vérifie pas lui-même que les processus ont disparu : l'arrêt de git et de ses sous-processus par le Job Object est celui de l'annulation, vérifié par `runner_cancel_kills_process_tree` (même fichier) et `a_process_dies_when_its_job_handle_closes` ;
  - suivi de revue : le contrôle de taille devenu inutile dans `pick_diff` (une sortie lue avec la limite n'est jamais plus longue) est retiré ; le journal donne la limite en Mio ou Kio (`the_journal_gives_a_readable_size`, `runner.rs`).
- **Priorité :** moyenne.

#### RV3 — Fetch automatique dans la file des écritures (corrigé)

- **Contexte :** mineur n°16 de la revue finale.
- **État :** corrigé.
  - Avant : le fetch automatique passait par la file de l'onglet, et une indexation lancée pendant un fetch lent attendait la fin du fetch ; chaque onglet avait sa propre file, si bien qu'un dépôt ouvert dans deux onglets (« Ouvrir dans un nouvel onglet ») avait deux files qui écrivaient en même temps, avec des collisions possibles sur `index.lock`.
  - Désormais, les files d'opérations sont celles du dépôt, tenues dans un registre partagé (`Shared::queues`, `crates/dragontree-app/src/op_queue.rs`) et prises par l'onglet à sa première opération :
    - écritures : une file par dossier git (`git_dir`, propre à chaque worktree comme son index et son `index.lock`), commune à tous les onglets du dépôt : leurs opérations s'exécutent dans l'ordre des clics ;
    - réseau silencieux (fetch automatique) : une file à part par dépôt (`common_dir`, commun aux worktrees, qui partagent les refs) : un fetch lent ne fait plus attendre une indexation, un commit ou un checkout ;
    - une file vit tant qu'un onglet la tient ou qu'une opération y attend : un dépôt rouvert entre-temps retrouve la même.
  - Le fetch automatique ne tourne jamais en même temps qu'une opération du dépôt qui écrit ses refs distantes : opération réseau visible (fetch, pull, push, test de connexion…) ou suppression d'un remote, qui réécrit `refs/remotes` et compte ici comme une opération réseau (décision du plan, précisée après revue) :
    - le fetch automatique passe son tour (jusqu'à l'échéance suivante) quand une telle opération attend ou tourne sur une file d'écriture du dépôt, ou quand la file silencieuse n'est pas vide (un autre onglet du même dépôt fetche déjà) ; une écriture locale ordinaire (indexation, commit, checkout…) ne l'empêche plus ;
    - une telle opération, à son tour, annule le fetch automatique qui n'a pas encore commencé (git n'est pas lancé ; pas de notification) mais attend la fin de celui qui tourne, sans l'arrêter (`OpQueue::make_way`) : git tué pendant l'écriture des refs (transaction, réécriture de `packed-refs`) laisserait des fichiers `*.lock`, et le `git maintenance --auto` qu'il lance serait tué avec lui. Pendant cette attente, la barre de progression affiche « Fetch — en attente de la fin du fetch automatique… » ; « Annuler » y met fin. Coût : un fetch automatique lent retarde l'opération visible, comme avant la file à part ;
    - seule la fermeture de l'application arrête un fetch automatique en cours (RV4a : « Quitter quand même », ou fermeture sans opération visible) : c'est le choix de l'utilisateur.
  - Limite : deux opérations réseau visibles lancées depuis deux worktrees du même dépôt sont sur deux files d'écriture et peuvent tourner ensemble ; elles écrivent alors les mêmes refs distantes. Les verrous de git (`*.lock`) font échouer l'une des deux (« cannot lock ref »), sans dommage ; l'utilisateur a lancé les deux.
  - Chaque file tient la liste de ses opérations en attente ou en cours, avec leur jeton d'annulation et leur caractère silencieux (`OpQueue::pending`, `OpQueues::pending`) : de quoi les retrouver et les annuler toutes à la fermeture (RV4a).
- **À faire :**
  1. Mettre les opérations réseau silencieuses dans une file à part (fait).
  2. Partager une seule file d'écriture par dépôt, indexée par chemin (fait : par dossier git, comparé avec `path_key`).
- **Critère de fin (atteint) :**
  - test d'interface `staging_completes_while_the_automatic_fetch_is_blocked` (`crates/dragontree-app/tests/ui_op_queues.rs`) : remote servi par un serveur HTTP local muet, fetch automatique dès l'ouverture (`StartOptions::auto_fetch_every`, réservé aux tests) ; une fois le fetch bloqué, « Tout indexer » se termine aussitôt (vu rouge : délai de 40 s dépassé avec la file unique, environ 1 s ensuite) ;
  - tests unitaires `two_tabs_on_the_same_repository_share_one_write_queue` (deux onglets sur le même dépôt : même file, et l'écriture du second attend celle, bloquée, du premier ; vu rouge avec une file par onglet : `["1 début", "2", "1 fin"]`), `the_automatic_fetch_gives_way_to_a_visible_network_operation_and_to_another_tab` (`crates/dragontree-app/src/repo/actions.rs`), et ceux du registre (`crates/dragontree-app/src/op_queue.rs` : même dépôt écrit autrement, worktrees, durée de vie, annulation) ;
  - suivi de revue (le fetch automatique en cours n'est plus arrêté) : `a_visible_network_operation_cancels_the_waiting_automatic_fetch_and_waits_for_the_running_one` (fetch en cours jamais annulé, pull parti après sa fin, attente affichée ; fetch en attente annulé sans lancer git ; vu rouge : le fetch en cours était annulé), `removing_a_remote_waits_for_the_running_automatic_fetch` (vu rouge sans l'option `remote_refs`) et `a_cancelled_automatic_fetch_shows_no_notification` (`actions.rs`) ; `making_way_cancels_what_has_not_started_and_waits_for_what_runs` et `making_way_stops_waiting_when_the_waiting_operation_is_cancelled` (`op_queue.rs`) ; test d'interface `a_visible_fetch_waits_for_the_running_automatic_fetch_and_says_so` (`ui_op_queues.rs` : la connexion du fetch automatique reste ouverte, le texte d'attente est affiché, le fetch visible part à sa fin ; vu rouge).
- **Priorité :** basse.

#### RV4 — Points non jugés par la revue et cas des forges (corrigé)

- **Contexte :** la revue finale a laissé ces points sans jugement. Le rapport du crate des forges a signalé les autres.
- **Points (tous traités) :**
  - **a. Processus git à la fermeture de l'application (corrigé).**
    - Avant : aucun arrêt n'était prévu. Le job Windows était créé sans `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`, donc un push ou un fetch en cours continuait après la fermeture.
    - Décision (plan) : demander confirmation (« Attendre / Quitter quand même ») ; en quittant, annuler les opérations, et aucune opération git en cours ne survit ; les opérations silencieuses (fetch automatique) sont arrêtées sans demander.
    - Réalisé :
      - le job de chaque commande est créé avec `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` (`job::JobGuard`, `crates/dragontree-git/src/runner.rs`) : tant que la commande tourne, ses processus meurent à la fermeture du handle, à la fin du processus de DragonTree, dont Windows ferme alors les handles. Toute commande terminée lève la limite (`let_survive`) : ce que git laisse tourner en partant lui survit (démon fsmonitor de `core.fsmonitor`, éditeur démarré par `git mergetool`, navigateur ouvert par le Gestionnaire d'identifiants, programme lancé par `start` dans une commande personnalisée) ; seuls l'annulation, la limite de sortie et la fin de DragonTree arrêtent une commande en cours. Limite (README) : un outil de diff encore ouvert par « Ouvrir dans l'outil de diff externe », qui n'est pas une opération du dépôt, est fermé avec DragonTree sans question ;
      - la demande de fermeture (croix, Alt+F4, Fichier › Quitter, barre des tâches) passe par `Closing::intercept` (`crates/dragontree-app/src/closing.rs`, appelée par `App::logic`, qui tourne aussi fenêtre réduite) : si des opérations visibles attendent ou tournent (files des dépôts, `Shared::queues.pending()`, et clonage), la fermeture est suspendue (`ViewportCommand::CancelClose`) et un dialogue les nomme. « Attendre » ferme la fenêtre d'elle-même à la fin des opérations ; « Quitter quand même » annule tous les jetons (chaque opération arrête son git) et ferme ; Échap renonce à fermer. Sans opération visible, les opérations silencieuses sont annulées et la fenêtre se ferme sans question ; une fois la fermeture décidée, plus aucun fetch automatique ne part ;
      - à la sortie (`App::on_exit`), les onglets ne font plus rien attendre : la surveillance du disque n'est pas arrêtée une à une (jusqu'à 1 s par onglet), Windows la ferme avec le processus.
    - Critère : atteint. Tests `a_process_dies_when_its_job_handle_closes` et `a_released_job_lets_its_processes_live` (`runner.rs`, le premier vu rouge sans la limite), `a_program_started_in_the_background_outlives_the_command` (`crates/dragontree-git/tests/shell.rs`, vu rouge sans la levée de la limite), `a_program_git_leaves_running_outlives_the_command` (`crates/dragontree-git/tests/runner.rs` : un sous-shell laissé en arrière-plan par un alias git écrit son fichier après la fin de la commande ; vu rouge quand seule une commande personnalisée levait la limite) ; tests d'interface (`crates/dragontree-app/tests/ui_quit.rs`, remote servi par un serveur HTTP local muet, vus rouges) : `closing_during_an_operation_asks_and_quitting_anyway_stops_it` (fetch visible bloqué : fermeture suspendue, dialogue, « Quitter quand même » ferme et git est arrêté, sa connexion se fermant), `waiting_closes_the_window_when_the_operations_end`, `closing_while_only_the_automatic_fetch_runs_does_not_ask`.
  - **b. Plusieurs instances (corrigé).**
    - Avant : rien n'empêchait deux instances, qui écrivaient alors le même `settings.json` ; la dernière écriture l'emportait.
    - Décision (plan) : instance unique par profil, avec transmission des arguments (`--open`, dossiers) à l'instance déjà ouverte, qui les ouvre et passe au premier plan ; `--profile <dossier>` donne une instance distincte.
    - Réalisé (`crates/dragontree-app/src/single_instance.rs`, appelé par `main.rs`, jamais par `DragonTreeApp::new` : les tests d'interface créent plusieurs applications) :
      - au démarrage, un mutex nommé `Local\DragonTree-<clé>` désigne la première instance du profil ; la clé est une empreinte stable (FNV-1a sur 64 bits) du dossier du profil normalisé, celui de `--profile` ou `%APPDATA%\DragonTree` ; une capture d'écran (`--screenshot`) reste une instance à part ;
      - la première instance écoute sur un tube nommé `\\.\pipe\DragonTree-<clé>` (thread de fond ; sécurité par défaut de Windows : seuls l'utilisateur, les administrateurs et le système y écrivent ; clients distants refusés ; un tube du même nom créé avant elle est refusé) ;
      - une instance suivante y envoie ses dépôts (dossiers en arguments et `--open`, en chemins absolus ; message JSON versionné, 1 Mio au plus), permet à la première de passer au premier plan (`AllowSetForegroundWindow` avec le numéro de processus du serveur du tube), puis quitte avec le code 0 ; ses autres options sont ignorées. Si la première ne répond pas (elle se ferme), la place est redemandée pendant 10 s, puis l'instance démarre seule ;
      - la première instance garde les dossiers existants, les ouvre (`AppAction::OpenRepo` : onglet existant activé, onglet d'accueil remplacé, sinon nouvel onglet) et passe au premier plan (`ViewportCommand::Minimized(false)` si elle était réduite, puis `Focus`), depuis `App::logic`, qui tourne aussi fenêtre réduite.
    - Critère : atteint. Tests unitaires du message et de la clé (`single_instance.rs`) ; tests d'intégration `crates/dragontree-app/tests/single_instance.rs` (tube : aller-retour dans le processus, chemins reçus ; seconde instance sur le même profil : transmise et quitte, dossier absent écarté ; autre profil : instance distincte ; profil libéré à la fermeture de la première), vus rouges (module absent) ; test d'interface `crates/dragontree-app/tests/ui_single_instance.rs` (dépôt transmis ouvert dans un onglet, `Focus` envoyé).
    - Vérification manuelle (exécutable de débogage, 2026-10-21) : première instance `dragontree.exe --profile <tmp>\profil`, puis, depuis un autre dossier, `dragontree.exe --profile <tmp>\profil ..\depot` : le second processus quitte avec le code 0 en 150 ms environ, et le dépôt (chemin absolu) est ouvert dans la première (`open_tabs` du profil) ; relancer sans dépôt quitte aussi avec 0 ; avec `--profile <tmp>\profil2`, deux fenêtres tournent. À vérifier à la main : le passage au premier plan quand une autre application a le focus (Windows peut se contenter de faire clignoter le bouton de la barre des tâches).
  - **c. Proxy (corrigé).**
    - Avant : le client HTTP des forges ne lisait que les variables d'environnement (`crates/dragontree-forges/src/http.rs`), sans la configuration proxy de Windows. Derrière un proxy d'entreprise, la vérification d'un compte pouvait échouer alors que git fonctionne.
    - Décision (plan) : le client HTTP des forges utilise la configuration proxy de Windows quand aucune variable d'environnement de proxy n'est définie.
    - Réalisé (`crates/dragontree-forges/src/proxy.rs`, `http.rs`) :
      - une variable d'environnement (`ALL_PROXY`, `HTTPS_PROXY`, `HTTP_PROXY`, avec `NO_PROXY`) l'emporte, comme avant ;
      - sinon, les réglages proxy de l'utilisateur (Paramètres › Réseau et Internet › Proxy, ceux d'Internet Explorer) sont lus par WinHTTP (`WinHttpGetIEProxyConfigForCurrentUser`) à chaque requête : un changement (VPN) s'applique sans redémarrer ;
      - détection automatique (WPAD) ou script de configuration (PAC) : WinHTTP donne le proxy de l'URL (`WinHttpGetProxyForUrl`, qui télécharge et exécute le script) ; sans réponse, les réglages manuels s'appliquent, comme pour Windows ; une détection automatique sans résultat n'est retentée qu'après 5 minutes (pas d'attente à chaque requête sur un réseau sans WPAD) ;
      - proxy manuel : `hôte:port` pour tous les protocoles, ou un serveur par protocole (`http=hôte:port;https=hôte:port`, séparés par `;` ou des espaces) ; port 80 s'il manque ; le proxy est joint en HTTP (`CONNECT` pour HTTPS) ;
      - exclusions : la liste de Windows (`<local>` pour les noms sans point, motifs avec `*` comme `*.contoso.com` ou `10.*`, éventuellement précédés d'un protocole ou suivis d'un port, casse ignorée), et toujours le bouclage (`localhost`, `127.0.0.0/8`, `::1`) : les serveurs locaux des tests restent joints directement ;
      - un agent ureq par proxy, gardé pour les requêtes suivantes ; l'accès à Windows passe par le trait `ProxySource`, remplacé dans les tests.
    - Limites (README) : proxy SOCKS non pris en charge (connexion directe) ; proxy qui demande une authentification (NTLM, Kerberos) non pris en charge ; un proxy défini seulement dans la configuration de git (`http.proxy`) n'est pas lu.
    - Critère : atteint. Tests unitaires (`proxy.rs`) : formats `hôte:port`, `http=…;https=…` (séparateurs, casse, SOCKS, IPv6), exclusions (`<local>`, `*`, protocole, port, casse) et bouclage, configuration automatique puis repli sur les réglages manuels, absence de réglages ; `winhttp_runs_a_proxy_auto_config_script` : un script PAC servi par un serveur local est exécuté par WinHTTP (`PROXY` et `DIRECT` selon l'hôte), sans lire la configuration de la machine ; `http.rs` : `the_agent_takes_the_windows_proxy_of_the_url_unless_it_is_bypassed` et `a_request_goes_through_the_windows_proxy` (un serveur local qui joue le proxy reçoit la requête), vus rouges sans le proxy de Windows.
  - **d. Forges (corrigé ; HTTP simple documenté comme limite).**
    - GitLab, visibilité avec `simple=true` (corrigé) :
      - avant : certaines versions de GitLab omettent la visibilité dans la liste « simple » des projets ; le dépôt s'affichait alors comme privé ;
      - désormais (`gitlab::list_repos_with`, `crates/dragontree-forges/src/gitlab.rs`) : si un projet n'a pas de visibilité, la liste des seuls projets publics du compte (`visibility=public`, toujours « simple ») la complète ; les autres restent privés (privés ou internes : cadenas dans les deux cas), comme si cette seconde liste échoue. Aucune requête de plus quand toutes les visibilités sont données ;
      - tests : `a_missing_visibility_is_completed_by_the_list_of_public_projects` (vu rouge sans le complément), `no_public_list_is_requested_when_every_visibility_is_known`, `a_failed_public_list_keeps_the_unknown_projects_private`.
    - Instances en HTTP simple (limite documentée) :
      - les forges ne sont jointes qu'en HTTPS : l'adresse de l'API, celles des pages web (pull requests, création d'un jeton) et l'origine de l'en-tête d'authentification de git sont construites en HTTPS, et le jeton ne doit jamais circuler en clair. Le prendre en charge demanderait de porter le schéma dans tous ces endroits (et dans `RepoLocation`) pour un cas rare et peu sûr ;
      - avant, une adresse `http://…` saisie à l'ajout d'un compte était appelée en HTTPS, avec une erreur TLS obscure ; elle est désormais refusée avec une explication (`reject_plain_http`, `crates/dragontree-forges/src/lib.rs`), pour toutes les forges : test `a_plain_http_server_is_rejected_without_network`. Un dépôt dont le remote est en `http://` garde les identifiants habituels de git (l'en-tête du compte ne vise que HTTPS) ; README, « Limites connues ».
    - Azure DevOps, « privé » déduit de la visibilité du projet (juste, vérifié) : Azure DevOps n'a pas de visibilité par dépôt, celle du projet vaut pour tous ses dépôts (projets publics ou privés). Seul un projet `public` rend ses dépôts publics ; `organization` (anciennes versions de l'API), une visibilité absente (Azure DevOps Server, qui n'a pas de projets publics) ou inconnue gardent le cadenas : test `a_repository_is_public_only_in_a_public_project` (`azure.rs`).
    - Azure DevOps, casse de l'organisation dans l'en-tête d'authentification (corrigé) :
      - avant : l'en-tête visait `https://dev.azure.com/<org>/` avec la casse de l'organisation du compte ; git compare le chemin des URL en respectant la casse, alors que l'association d'un compte à un remote l'ignore : un remote `https://dev.azure.com/Contoso/…` d'un compte qui connaît `contoso` ne recevait pas l'en-tête ;
      - vérifié avec git lui-même, sans réseau : test d'intégration `the_azure_header_reaches_a_remote_whose_organisation_is_written_with_another_case` (`crates/dragontree-forges/tests/git_auth.rs`, `git config --get-urlmatch http.extraheader <url>` avec les variables d'environnement de DragonTree et la configuration de la machine écartée) : avec l'origine du compte seule, git ne donne pas l'en-tête pour ce remote (vu rouge) ;
      - désormais, `git_auth_env` reçoit les URL des remotes du dépôt (celle du clone pour un clonage) : pour chaque remote HTTPS du compte Azure DevOps dont le chemin commence par une de ses organisations ou collections (casse ignorée), l'origine est ajoutée telle que le remote l'écrit (`https://dev.azure.com/Contoso/`, `https://tfs.contoso.com:8443/TFS/DefaultCollection/`) ; GitHub et GitLab n'en ont pas besoin (origine sans chemin, hôte comparé sans casse). Test unitaire `auth_env_azure_follows_the_case_of_the_organisation_in_the_remotes` (`auth.rs`).
- **Critère de fin (atteint) :** chaque point est soit corrigé et testé, soit documenté comme limite dans le README (instances en HTTP simple).
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
  4. Ajouter l'icône (N3, fait).
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
