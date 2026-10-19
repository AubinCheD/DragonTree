# DragonTree — feuille de route

Rédigé le 2026-10-18. Ce document reprend les demandes du 2026-10-18 :

- renommer le projet ;
- choisir la langue ;
- voir et changer de worktree ;
- voir les stashs ;
- filtrer l'historique sur la branche active ;
- recenser les options de menu de Git Fork pour choisir celles à implémenter.

## Constats

- **Les stashs et le filtre de branche existent déjà, mais se voient mal.**
  - La section « Stashes » est dans la barre latérale gauche. Un clic sur un stash affiche son diff, et le clic droit propose Appliquer, Pop et Supprimer. Sur un gros dépôt, la section se retrouve sous des centaines de branches.
  - Le bouton « Toutes les branches / Branche courante » est à droite du champ de recherche, au-dessus de l'historique. Il est discret.
  - Les points V2, V3, V7 et R4 ci-dessous les rendent visibles comme dans Fork.
- **Fork n'existe qu'en anglais.** Son développeur a refusé la traduction ([TrackerWin #392](https://github.com/fork-dev/TrackerWin/issues/392)). Le choix de la langue est donc un plus de DragonTree.

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
- Le dossier du projet (`GitFork`) n'est pas renommé.

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

### Worktrees — à faire, proposition

Le fonctionnement s'inspire de Fork :

- **Section « Worktrees »** en haut de la barre latérale. Chaque ligne affiche la branche et le chemin.
- **Ouverture :** un clic ouvre le worktree dans un onglet, ou bascule sur cet onglet s'il est déjà ouvert.
- **Création :** depuis une branche locale, une branche distante ou un commit.
- **Gestion :** supprimer (avec confirmation), copier le chemin.
- **Branches déjà sorties ailleurs :** une icône signale les branches sorties dans un autre worktree. Un checkout d'une de ces branches propose d'ouvrir son worktree.

### Raccourcis

Quand un raccourci actuel entre en conflit avec ceux de Fork, l'actuel est gardé :

- Ctrl+1…9 change d'onglet (Fork l'utilise pour changer de vue) ;
- Ctrl+B affiche la liste des dépôts (Fork l'utilise pour un checkout rapide).

## Options de Fork pour les menus File / View / Repo / Config

**Légende**
- ✅ : existe déjà, il manque seulement l'entrée de menu ou le raccourci.
- 🆕 : à développer.
- Taille : P petit, M moyen, G gros.

L'utilisateur choisira les numéros à implémenter.

### File

| # | Option | État |
|---|---|---|
| F1 | Nouvel onglet, Ouvrir, Cloner, Initialiser un dépôt, Fermer l'onglet, Palette Ctrl+P, Comptes, Réglages (Ctrl+,) | ✅ |
| F2 | Rouvrir le dernier onglet fermé (Ctrl+Maj+T) | 🆕 P |
| F3 | Gestion des clés SSH (générer, choisir, plusieurs clés) | 🆕 M |
| F4 | Quitter | 🆕 P |

### View

| # | Option | État |
|---|---|---|
| V1 | Modifications locales / Tous les commits, liste des dépôts, journal git | ✅ |
| V2 | Filtrer sur la branche active (Ctrl+Maj+A), en suivant les checkouts | ✅ + raccourci |
| V3 | Filtrer sur des branches choisies : entonnoir sur chaque branche, dossier ou remote, plusieurs à la fois, bandeau « Filtré par : … » | 🆕 M |
| V4 | Masquer une branche, un dossier ou un remote du graphe (icône œil) | 🆕 M |
| V5 | Masquer les tags, masquer les branches distantes | 🆕 P |
| V6 | Afficher les commits perdus (reflog) dans le graphe (Ctrl+Maj+.) | 🆕 M |
| V7 | Afficher les stashs dans le graphe, avec une option pour les masquer | 🆕 M |
| V8 | Replier et déplier les merges (← / →, Tout replier) | 🆕 G |
| V9 | Révéler HEAD (Ctrl+0), afficher ou masquer le panneau de détails (Ctrl+Maj+D) | 🆕 P |
| V10 | Disposition verticale ou horizontale de l'historique | 🆕 M |
| V11 | Tri des commits : topologique ou par date | 🆕 P |
| V12 | Vue Changements : afficher les fichiers ignorés, masquer les non suivis | 🆕 P |
| V13 | Zoom Ctrl+= / Ctrl+- (le réglage de taille existe) et thème | ✅ + raccourcis |

### Repo

| # | Option | État |
|---|---|---|
| R1 | Actualiser, Fetch/Pull/Push (Ctrl+Maj+F/L/P), Nouvelle branche (Ctrl+Maj+B), Nouveau tag, Stash (Ctrl+Maj+H), ouvrir dans l'Explorateur, le Terminal ou VS Code | ✅ + raccourcis |
| R2 | Fetch, Pull et Push rapides, sans dialogue (Ctrl+clic) | 🆕 P |
| R3 | Créer un worktree… (voir Worktrees) | 🆕 |
| R4 | Stashs complets : liste déroulante sur Pop, renommer, stash des seuls fichiers sélectionnés, snapshot (garde les modifications), fichiers non suivis visibles dans le diff | 🆕 M |
| R5 | Appliquer un patch (fichier, glisser-déposer, presse-papiers) | 🆕 M |
| R6 | Enregistrer comme patch (commit, plage de commits, stash, fichiers) | 🆕 M |
| R7 | Comparer deux commits, deux branches, ou un commit avec les modifications locales | 🆕 M |
| R8 | Réglages du dépôt : nom et email propres au dépôt, liens vers les tickets (`#1234` → Azure DevOps), modèle de message de commit | 🆕 M |
| R9 | Remotes : modifier l'URL, tester la connexion (ajouter et supprimer existent déjà) | 🆕 P |
| R10 | Sous-modules : ajouter, supprimer, mettre à jour, ouvrir en onglet (la liste existe déjà) | 🆕 M |
| R11 | .gitignore : ajouter un motif avec un aperçu des fichiers concernés (« Ignorer » existe déjà) | 🆕 P |
| R12 | Bisect (bon, mauvais, passer, bandeau d'état) | 🆕 M |
| R13 | Git Flow (initialiser, start/finish feature, release, hotfix) | 🆕 M |
| R14 | Git LFS (init, track, fetch/pull/prune, verrous) | 🆕 G |
| R15 | Commandes personnalisées (sur le dépôt, une branche, un commit ou un fichier ; script ou URL) | 🆕 G |
| R16 | Fast-forward ou push de plusieurs branches à la fois ; sélection des branches périmées à supprimer | 🆕 P |
| R17 | Remettre un fichier dans son état à un commit donné | 🆕 P |
| R18 | Créer une pull request (GitHub, GitLab, Azure DevOps) | 🆕 M |
| R19 | « Ouvrir dans ▾ » avec d'autres éditeurs (Visual Studio .sln, Rider, Cursor…) | 🆕 P |
| R20 | Statistiques du dépôt | 🆕 M |

### Config

| # | Option | État |
|---|---|---|
| C1 | Langue français / anglais | ✅ (dans Réglages) |
| C2 | Outils externes de diff et de merge configurables, avec préréglages VS Code, WinMerge, Beyond Compare, KDiff3, P4Merge (`git mergetool` existe déjà) | 🆕 M |
| C3 | Terminal à utiliser (Windows Terminal, PowerShell, Git Bash) | 🆕 P |
| C4 | Identité git globale (nom, email) | 🆕 P |
| C5 | Message de commit : indicateur de longueur 50/72, colonne guide, messages récents, Co-authored-by | 🆕 M |
| C6 | Diff : police, taille, largeur de tabulation, espaces visibles | 🆕 P |
| C7 | IA : message de commit et revue de branche par Claude Code (dans Fork depuis la 2.14) | 🆕 M |
| C8 | Fenêtre des raccourcis clavier, À propos | 🆕 P |

### Hors menus, présents dans Fork

| # | Option |
|---|---|
| H1 | Glisser une branche sur une autre pour un merge ou un rebase |
| H2 | Couleurs d'onglets et espaces de travail (groupes d'onglets) |
| H3 | Recherche de commits dans le contenu des diffs et par chemin |
| H4 | Diff d'images par balayage ou superposition |

### Écartés par défaut

Activation et licence, mises à jour automatiques, modèle « Lean Branching » propre à Fork, Treemap, Benchmark, avatars Gravatar.

## Sources

- Notes de version de Fork pour Windows : <https://git-fork.com/releasenoteswin>. Version 2.23.2 au 2026-10-18.
- Liste officielle des raccourcis : <https://github.com/fork-dev/TrackerWin/issues/333>.
- Captures d'écran et réponses du développeur dans les tickets <https://github.com/fork-dev/TrackerWin/issues>.
- Barre de menus de Fork pour Windows : File, View, Repository, Window, Help. Il n'y a ni menu Edit ni menu Tools.
