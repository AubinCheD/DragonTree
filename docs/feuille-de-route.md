# DragonTree — feuille de route

Rédigé le 2026-10-18, mis à jour le 2026-10-19, le 2026-10-20 (branche `options-fork`) puis le 2026-10-22 (branche `points-restants`), voir plus bas. Ce document reprend les demandes du 2026-10-18 :

- renommer le projet (fait) ;
- choisir la langue (fait) ;
- voir et changer de worktree (fait : T1, via R3) ;
- voir ses stashs (fait : T3) ;
- filtrer l'historique sur la branche active (fait : T4) ;
- recenser les options de menu de Git Fork pour choisir celles à implémenter (recensement fait, toutes les options retenues et livrées : T2, N2).

La mise à jour du 2026-10-19 ajoutait l'état vérifié dans le code et la liste de tout ce qui restait à faire (tout est depuis terminé, ou documenté comme limite : voir les sections **Branche `options-fork` (2026-10-20)** et **Branche `points-restants` (2026-10-22)**) :

- la section **Reste à faire** donne la synthèse ;
- la section **Détail des points restants** donne le détail ;
- la section **État du plan d'implémentation** fait le point sur le plan.

Les références `crates/…:ligne` renvoient au code du 2026-10-19.

## Constats

- **Les stashs et le filtre de branche existent déjà, mais se voient mal.**
  - La section « Stashes » est dans la barre latérale gauche. Un clic sur un stash affiche son diff, et le clic droit propose Appliquer, Pop et Supprimer. Sur un gros dépôt, la section se retrouve sous des centaines de branches.
  - Le bouton « Toutes les branches / Branche courante » est à droite du champ de recherche, au-dessus de l'historique. Il est discret.
  - Les points V2, V3, V7 et R4 ci-dessous les rendent visibles comme dans Fork.
  - L'utilisateur n'a jamais confirmé que c'était bien le problème (voir T3 et T4) ; les deux sont néanmoins faits.
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
- Les autres points de la relecture sont traités dans RV5 (corrigé) : `--lang` et `--theme` sont temporaires, l'état « occupé » des boutons ne dépend plus de la langue, et les textes déjà affichés lors d'un changement de langue restent dans l'ancienne (limite, README).

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

Synthèse au 2026-10-22. Chaque point est détaillé plus bas, sous son identifiant. Les options de Fork restent dans les tableaux ci-dessus et ne sont pas répétées. Tout ce que les sections B, T, N, RV, P et L listaient est fait ou corrigé : T1, T2, N2, B1, B5 et B6 par la branche `options-fork`, tous les autres par la branche `points-restants` (voir la section du même nom plus bas). Les tableaux gardent chaque ligne avec son état, pour mémoire.

### Reste ouvert

Seul ce qui a été écarté, ou documenté comme limite, reste ouvert. Rien n'est planifié.

| # | Ce qui reste | Pourquoi | Entrée |
|---|---|---|---|
| L1 | Identifiants SHA-256 de 32 octets (les dépôts SHA-256 sont refusés avec une erreur explicite) | `Oid` est un `[u8; 20]` utilisé partout ; coût disproportionné pour un format rare et expérimental | L1 |
| RV4d | Forges en HTTP simple (refusées à l'ajout du compte) | le jeton ne doit jamais circuler en clair | RV4 d |
| RV4c | Proxy SOCKS, proxy à authentification (NTLM, Kerberos), `http.proxy` de git | hors du plan ; `NO_PROXY` ne vaut que pour les variables d'environnement | RV4 c |
| RV4b | Instances : profil désigné par un nom 8.3, une jonction ou un lecteur `subst` ; instance élevée et non élevée | clé calculée sur le chemin normalisé, sans le résoudre ; droits d'écriture du tube | RV4 b |
| RV3, RV4a | SSH sans limite de débit du fetch automatique ; deux worktrees d'un même dépôt sur deux files ; outil de diff externe fermé avec l'application | git ne propose pas la limite pour SSH ; verrous de git ; ce n'est pas une opération du dépôt | RV3, RV4 a |
| RV5c | Textes déjà affichés non retraduits après un changement de langue | décision du plan | RV5 |
| P4 | Dans cmd ou PowerShell, l'invite revient avant le texte de `--help` | une application graphique n'est pas attendue par le shell | P4 |
| L3 | Fenêtre Réglages ouverte pendant la relecture : sa liste modifiée remplace celle du fichier ; réglages perdus à la déconnexion de Windows | brouillon de la fenêtre ; pas d'enregistrement à la fin de session | L3 |
| L4 | Refus d'authentification d'un fetch manuel non reconnu avec un git traduit | les opérations visibles gardent la langue de l'utilisateur | L4 |
| N1 | Vue des pull requests : dates Azure DevOps, plafonds de résultats, relecteur GitLab « désigné », remotes lus sans `include.path`, `insteadOf` ni alias SSH | limites des API et de la lecture de `.git/config` | N1 |
| N3 | Icône avec MSVC : `link.exe` à confirmer | toolchain MSVC non installée sur le poste | N3 |
| P1 | Marque : EUIPO/TMview et INPI à vérifier à la main | accès automatique impossible | P1 |
| T3 | Confirmer avec l'utilisateur que la visibilité des stashs répond à sa demande | jamais confirmé | T3 |

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
| RV5 | Résidus de la relecture de la traduction : `--lang` enregistré, bouton Push occupé, textes figés (corrigé ; textes figés documentés comme limite) | relecture de la traduction | basse |

### Pistes

| # | Point | Origine | Priorité |
|---|---|---|---|
| P1 | Publication open source : licence, vérification du nom, README (fait ; EUIPO et INPI à vérifier à la main) | objectif annoncé par l'utilisateur | moyenne |
| P2 | Cases du plan jamais cochées (fait : plans signalés comme historiques) | état du plan | basse |
| P3 | Stabilité de `ui_network` vérifiée sur 3 passages seulement (fait : 20 passages sur 20) | correction des tests instables | basse |
| P4 | `--help` en build release depuis une console Windows (corrigé ; l'invite revient avant le texte, limite documentée) | relevé en vérifiant le code | basse |
| L1 | Dépôts SHA-256 non pris en charge (limite documentée : erreur explicite à l'ouverture et au clonage) | limite connue (README) | basse |
| L2 | Éditeur de conflits limité aux fichiers UTF-8 (corrigé : fichiers Windows-1252 résolus sur leurs octets) | limite constatée dans le code | moyenne |
| L3 | Réglages illisibles au démarrage : rien n'est enregistré de la session (corrigé : fichier relu à chaque enregistrement puis fusionné, indicateur « Réglages non enregistrés ») | arbitrage de la revue (M9) | basse |
| L4 | Échec d'authentification du fetch automatique signalé une seule fois (corrigé : indicateur dans l'onglet jusqu'au prochain fetch réussi) | arbitrage de la revue (M2) | basse |
| L5 | Amend d'un commit déjà poussé avec « Pousser après » : pas de push (corrigé : push forcé proposé, protégé par `--force-with-lease`) | arbitrage de la revue (I7b) | basse |
| L6 | Option « Espaces » active : diff des changements en lecture seule (corrigé : patch construit sur le diff complet, hunk entier avec ses changements d'espaces) | arbitrage de la revue (M1) | basse |

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
  - Désormais, l'exécution des commandes git a un mode de sortie bornée (`GitCommand::stdout_limit`, `crates/dragontree-git/src/runner.rs`) : dès que la sortie standard dépasse la limite, la lecture s'arrête, git est arrêté avec ses sous-processus (Job Object, comme l'annulation) et la commande réussit avec `Output::stdout_truncated` ; le tampon ne grandit jamais au-delà de la limite (capacité comprise). Les morceaux lus dans les tubes passent par une file bornée (`runner::CHUNKS_IN_FLIGHT`, 2 morceaux de 64 Kio au plus en attente ; 8 avant la revue, sans gain de débit mesurable : environ 890 Mio/s dans les deux cas pour une sortie de 320 Mio, et autant pour `git log -p`) : un tube plus rapide que la lecture attend au lieu d'accumuler. Au-delà de la limite, la mémoire tenue est donc bornée par le code : les morceaux en file, un par thread de lecture bloqué, celui que la boucle traite et les deux tampons de lecture, 448 Kio en tout. Le journal git note une commande ainsi arrêtée comme une annulation, avec la limite (« sortie de plus de 8 Mio : lecture arrêtée et git arrêté »).
  - Les diffs de fichier (`diff_commit_file`, `diff_worktree_file`, `compare_file_diff`, `crates/dragontree-git/src/repo.rs`) lisent la sortie de git avec la limite `MAX_DIFF_BYTES` (8 Mio) : au-delà, le diff est `truncated`, sans hunks, comme avant. Le fichier non suivi trop gros reste écarté avant même de lancer git.
- **À faire :** arrêter la lecture, et le processus, dès que la limite est dépassée, ou contrôler la taille avant de lancer le diff (fait : lecture et processus arrêtés à la limite).
- **Critère de fin (atteint) :**
  - test d'intégration `the_diff_of_a_50_mib_file_is_truncated_without_holding_more_than_the_limit` (`crates/dragontree-git/tests/large_diff.rs`) : diff d'un commit qui ajoute un fichier de 50 Mio, de sa modification non indexée puis indexée et d'une comparaison de révisions ; chaque résultat est tronqué, et un allocateur qui compte les octets alloués mesure le pic de mémoire pendant le diff : environ 8,2 Mio (la limite de 8 Mio et quelques tampons de 64 Kio, 210 Kio en tout), contre près de 100 Mio avant (vu rouge). La marge du test est déduite des mêmes constantes (`CHUNKS_IN_FLIGHT`, `CHUNK_SIZE`) : la borne garantie (448 Kio) et 64 Kio pour les petites allocations, soit 512 Kio, vérifiés à la compilation ; jamais plus étroite que ce que le code garantit (suivi de revue : avec 8 morceaux en file, la borne dépassait la marge), et assez étroite pour voir une file sans borne (essayée : l'un des quatre diffs au moins dépassait, de 450 à 900 Kio au-delà ; trois essais sur trois en échec) ;
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
    - une telle opération, à son tour, annule le fetch automatique qui n'a pas encore commencé (git n'est pas lancé ; pas de notification) mais attend la fin de celui qui tourne, sans l'arrêter (`OpQueue::make_way`) : git tué pendant l'écriture des refs (transaction, réécriture de `packed-refs`) laisserait des fichiers `*.lock`, et le `git maintenance --auto` qu'il lance serait tué avec lui. Pendant cette attente, la barre de progression affiche « Fetch — en attente de la fin du fetch automatique… » ; « Annuler » y met fin sans lancer git. Coût : un fetch automatique lent retarde l'opération visible, comme avant la file à part ; en HTTP, un fetch automatique abandonne une connexion restée presque muette (`http.lowSpeedLimit=1000`, `http.lowSpeedTime=60`, options des opérations en arrière-plan, `with_net`, `crates/dragontree-git/src/net.rs`), si bien qu'un serveur qui ne répond plus ne bloque pas les opérations visibles au-delà d'une minute ; en SSH, il dure jusqu'à sa fin ou à la fermeture de l'application ;
    - seule la fermeture de l'application arrête un fetch automatique en cours (RV4a : « Quitter quand même », ou fermeture sans opération visible) : c'est le choix de l'utilisateur.
  - Limite : deux opérations réseau visibles lancées depuis deux worktrees du même dépôt sont sur deux files d'écriture et peuvent tourner ensemble ; elles écrivent alors les mêmes refs distantes. Les verrous de git (`*.lock`) font échouer l'une des deux (« cannot lock ref »), sans dommage ; l'utilisateur a lancé les deux.
  - Chaque file tient la liste de ses opérations en attente ou en cours, avec leur jeton d'annulation et leur caractère silencieux (`OpQueue::pending`, `OpQueues::pending`) : de quoi les retrouver et les annuler toutes à la fermeture (RV4a).
- **À faire :**
  1. Mettre les opérations réseau silencieuses dans une file à part (fait).
  2. Partager une seule file d'écriture par dépôt, indexée par chemin (fait : par dossier git, comparé avec `path_key`).
- **Critère de fin (atteint) :**
  - test d'interface `staging_completes_while_the_automatic_fetch_is_blocked` (`crates/dragontree-app/tests/ui_op_queues.rs`) : remote servi par un serveur HTTP local muet, fetch automatique dès l'ouverture (`StartOptions::auto_fetch_every`, réservé aux tests) ; une fois le fetch bloqué, « Tout indexer » se termine aussitôt (vu rouge : délai de 40 s dépassé avec la file unique, environ 1 s ensuite) ;
  - tests unitaires `two_tabs_on_the_same_repository_share_one_write_queue` (deux onglets sur le même dépôt : même file, et l'écriture du second attend celle, bloquée, du premier ; vu rouge avec une file par onglet : `["1 début", "2", "1 fin"]`), `the_automatic_fetch_gives_way_to_a_visible_network_operation_and_to_another_tab` (`crates/dragontree-app/src/repo/actions.rs`), et ceux du registre (`crates/dragontree-app/src/op_queue.rs` : même dépôt écrit autrement, worktrees, durée de vie, annulation) ;
  - suivi de revue (le fetch automatique en cours n'est plus arrêté) : `a_visible_network_operation_cancels_the_waiting_automatic_fetch_and_waits_for_the_running_one` (fetch en cours jamais annulé, pull parti après sa fin, attente affichée ; fetch en attente annulé sans lancer git ; vu rouge : le fetch en cours était annulé), `removing_a_remote_waits_for_the_running_automatic_fetch` (vu rouge sans l'option `remote_refs`) et `a_cancelled_automatic_fetch_shows_no_notification` (`actions.rs`) ; `making_way_cancels_what_has_not_started_and_waits_for_what_runs` et `making_way_stops_waiting_when_the_waiting_operation_is_cancelled` (`op_queue.rs`) ; test d'interface `a_visible_fetch_waits_for_the_running_automatic_fetch_and_says_so` (`ui_op_queues.rs` : la connexion du fetch automatique reste ouverte, le texte d'attente est affiché, le fetch visible part à sa fin ; vu rouge) ; deuxième suivi : `cancelling_a_visible_fetch_while_it_waits_stops_it_without_launching_git` (« Annuler » pendant l'attente : « Fetch : annulé », plus de texte d'attente, aucune connexion de plus, fetch automatique intact) et `a_background_fetch_gives_up_a_stalled_http_connection` (`net.rs` : options du fetch automatique, absentes du fetch visible ; vu rouge).
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
      - au démarrage, un mutex nommé `Local\DragonTree-<clé>` désigne la première instance du profil ; la clé est une empreinte stable (FNV-1a sur 64 bits) du dossier du profil normalisé, celui de `--profile` ou `%APPDATA%\DragonTree` ; une capture d'écran (`--screenshot`) reste une instance à part et n'écrit jamais les réglages (sans `--profile`, elle lit le profil d'une instance peut-être ouverte). Le chemin du profil n'est pas résolu sur le disque : nom court 8.3, jonction ou lecteur `subst` donnent une autre clé ;
      - la première instance écoute sur un tube nommé `\\.\pipe\DragonTree-<session>-<clé>` (thread de fond ; la session Windows dans le nom, comme pour le mutex `Local\` : un même utilisateur ouvert dans deux sessions a deux instances ; sécurité par défaut de Windows : seuls l'utilisateur, les administrateurs et le système y écrivent ; clients distants refusés ; un tube du même nom créé avant elle est refusé) ;
      - une instance suivante y envoie ses dépôts (dossiers en arguments et `--open`, en chemins absolus ; message JSON versionné, 1 Mio au plus), vérifie d'abord que le serveur du tube est un processus du même utilisateur (SID de son jeton : un autre utilisateur a pu créer ce tube au nom prévisible avant la première instance ; sinon rien n'est envoyé et l'instance démarre seule), permet à la première de passer au premier plan (`AllowSetForegroundWindow` avec le numéro de processus du serveur du tube), puis quitte avec le code 0 ; ses autres options sont ignorées. Une première instance qui se ferme arrête d'écouter (fin du thread attendue, 1 s au plus) avant de libérer son mutex, et dès `App::on_exit` : chaque tentative d'envoi dure 1 s au plus, puis la place est redemandée, et la création du tube est réessayée 2 s s'il existe encore ; au bout de 10 s sans réponse, l'instance démarre seule ;
      - la première instance garde les dossiers existants, les ouvre (`AppAction::OpenRepo` : onglet existant activé, onglet d'accueil remplacé, sinon nouvel onglet) et passe au premier plan (`ViewportCommand::Minimized(false)` si elle était réduite, puis `Focus`), depuis `App::logic`, qui tourne aussi fenêtre réduite.
    - Critère : atteint. Tests unitaires du message et de la clé (`single_instance.rs`) ; tests d'intégration `crates/dragontree-app/tests/single_instance.rs` (tube : aller-retour dans le processus, chemins reçus ; seconde instance sur le même profil : transmise et quitte, dossier absent écarté ; autre profil : instance distincte ; profil libéré à la fermeture de la première), vus rouges (module absent) ; test d'interface `crates/dragontree-app/tests/ui_single_instance.rs` (dépôt transmis ouvert dans un onglet, `Focus` envoyé). Suivi de revue : `the_profile_can_be_claimed_again_right_after_the_first_instance_closes` (50 fermetures et relances : toujours première instance), `a_closing_first_instance_gives_way_quickly` (mutex encore tenu sans tube : place reprise en moins de 5 s au lieu de 10 ; vu rouge), `the_pipe_name_carries_the_session` (vu rouge), `only_a_process_of_the_same_user_is_trusted` (ce processus oui, System et un processus absent non), `a_screenshot_run_never_writes_the_settings` (vu rouge).
    - Vérification manuelle (exécutable de débogage, 2026-10-21) : première instance `dragontree.exe --profile <tmp>\profil`, puis, depuis un autre dossier, `dragontree.exe --profile <tmp>\profil ..\depot` : le second processus quitte avec le code 0 en 150 ms environ, et le dépôt (chemin absolu) est ouvert dans la première (`open_tabs` du profil) ; relancer sans dépôt quitte aussi avec 0 ; avec `--profile <tmp>\profil2`, deux fenêtres tournent. À vérifier à la main : le passage au premier plan quand une autre application a le focus (Windows peut se contenter de faire clignoter le bouton de la barre des tâches).
  - **c. Proxy (corrigé).**
    - Avant : le client HTTP des forges ne lisait que les variables d'environnement (`crates/dragontree-forges/src/http.rs`), sans la configuration proxy de Windows. Derrière un proxy d'entreprise, la vérification d'un compte pouvait échouer alors que git fonctionne.
    - Décision (plan) : le client HTTP des forges utilise la configuration proxy de Windows quand aucune variable d'environnement de proxy n'est définie.
    - Réalisé (`crates/dragontree-forges/src/proxy.rs`, `http.rs`) :
      - une variable d'environnement (`ALL_PROXY`, `HTTPS_PROXY`, `HTTP_PROXY`) l'emporte, comme avant, avec les exclusions de `NO_PROXY` ; `NO_PROXY` ne s'applique pas au proxy de Windows, qui a sa propre liste d'exclusions ;
      - sinon, les réglages proxy de l'utilisateur (Paramètres › Réseau et Internet › Proxy, ceux d'Internet Explorer) sont lus par WinHTTP (`WinHttpGetIEProxyConfigForCurrentUser`) à chaque requête : un changement (VPN) s'applique sans redémarrer ;
      - détection automatique (WPAD) ou script de configuration (PAC) : WinHTTP donne le proxy de l'URL (`WinHttpGetProxyForUrl`, qui télécharge et exécute le script) ; sans réponse, les réglages manuels s'appliquent, comme pour Windows ; une configuration automatique sans réponse (détection sans résultat sur un réseau sans WPAD, script injoignable) n'est redemandée qu'après 5 minutes (`AutoFailures`, par réglage), et une session WinHTTP qui n'a pas pu s'ouvrir est retentée après le même délai (`OpenOnce`) : pas d'attente de 10 s à chaque requête ; les identifiants de la session Windows ne sont envoyés au serveur du script que s'il les demande (`with_auto_logon` : `fAutoLogonIfChallenged` seulement après `ERROR_WINHTTP_LOGIN_FAILURE`, recommandation de Microsoft) ;
      - proxy manuel : `hôte:port` pour tous les protocoles, ou un serveur par protocole (`http=hôte:port;https=hôte:port`, séparés par `;` ou des espaces) ; port 80 s'il manque ; le proxy est joint en HTTP (`CONNECT` pour HTTPS) ;
      - exclusions : la liste de Windows (`<local>` pour les noms sans point, motifs avec `*` comme `*.contoso.com` ou `10.*`, éventuellement précédés d'un protocole ou suivis d'un port, casse ignorée), et toujours le bouclage (`localhost`, `127.0.0.0/8`, `::1`) : les serveurs locaux des tests restent joints directement ;
      - un agent ureq par proxy, gardé pour les requêtes suivantes ; l'accès à Windows passe par le trait `ProxySource`, remplacé dans les tests.
    - Limites (README) : proxy SOCKS non pris en charge (connexion directe) ; proxy qui demande une authentification (NTLM, Kerberos) non pris en charge ; un proxy défini seulement dans la configuration de git (`http.proxy`) n'est pas lu.
    - Critère : atteint. Tests unitaires (`proxy.rs`) : formats `hôte:port`, `http=…;https=…` (séparateurs, casse, SOCKS, IPv6), exclusions (`<local>`, `*`, protocole, port, casse) et bouclage, configuration automatique puis repli sur les réglages manuels, absence de réglages ; `winhttp_runs_a_proxy_auto_config_script` : un script PAC servi par un serveur local est exécuté par WinHTTP (`PROXY` et `DIRECT` selon l'hôte), sans lire la configuration de la machine ; `http.rs` : `the_agent_takes_the_windows_proxy_of_the_url_unless_it_is_bypassed` et `a_request_goes_through_the_windows_proxy` (un serveur local qui joue le proxy reçoit la requête), vus rouges sans le proxy de Windows ; suivi de revue : `an_unanswered_automatic_configuration_is_not_asked_again_for_a_while` (détection et script, par réglage, délai), `a_session_that_could_not_be_opened_is_tried_again_later_and_an_open_one_is_kept`, `the_windows_credentials_are_sent_only_when_the_script_server_asks`.
  - **d. Forges (corrigé ; HTTP simple documenté comme limite).**
    - GitLab, visibilité avec `simple=true` (corrigé) :
      - avant : certaines versions de GitLab omettent la visibilité dans la liste « simple » des projets ; le dépôt s'affichait alors comme privé ;
      - désormais (`gitlab::list_repos_with`, `crates/dragontree-forges/src/gitlab.rs`) : si un projet n'a pas de visibilité, la liste des seuls projets publics du compte (`visibility=public`, toujours « simple ») la complète ; les autres restent privés (privés ou internes : cadenas dans les deux cas), comme si cette seconde liste échoue. Aucune requête de plus quand toutes les visibilités sont données ;
      - tests : `a_missing_visibility_is_completed_by_the_list_of_public_projects` (vu rouge sans le complément), `no_public_list_is_requested_when_every_visibility_is_known`, `a_failed_public_list_keeps_the_unknown_projects_private`.
    - Instances en HTTP simple (limite documentée) :
      - les forges ne sont jointes qu'en HTTPS : l'adresse de l'API, celles des pages web (pull requests, création d'un jeton) et l'origine de l'en-tête d'authentification de git sont construites en HTTPS, et le jeton ne doit jamais circuler en clair. Le prendre en charge demanderait de porter le schéma dans tous ces endroits (et dans `RepoLocation`) pour un cas rare et peu sûr ;
      - avant, une adresse `http://…` saisie à l'ajout d'un compte était appelée en HTTPS, avec une erreur TLS obscure ; elle est désormais refusée avec une explication à l'ajout du compte (`verify_new`, `reject_plain_http`, `crates/dragontree-forges/src/lib.rs`), pour toutes les forges : test `a_plain_http_server_is_rejected_without_network`. Un compte déjà enregistré ainsi n'est pas refusé par « Vérifier » (`verify`) : il reste appelé en HTTPS, comme avant (`a_saved_plain_http_account_is_still_checked_over_https`, vu rouge avec le refus dans `verify`). Un dépôt dont le remote est en `http://` garde les identifiants habituels de git (l'en-tête du compte ne vise que HTTPS) ; README, « Limites connues ».
    - Azure DevOps, « privé » déduit de la visibilité du projet (juste, vérifié) : Azure DevOps n'a pas de visibilité par dépôt, celle du projet vaut pour tous ses dépôts (projets publics ou privés). Seul un projet `public` rend ses dépôts publics ; `organization` (anciennes versions de l'API), une visibilité absente (Azure DevOps Server, qui n'a pas de projets publics) ou inconnue gardent le cadenas : test `a_repository_is_public_only_in_a_public_project` (`azure.rs`).
    - Azure DevOps, casse de l'organisation dans l'en-tête d'authentification (corrigé) :
      - avant : l'en-tête visait `https://dev.azure.com/<org>/` avec la casse de l'organisation du compte ; git compare le chemin des URL en respectant la casse, alors que l'association d'un compte à un remote l'ignore : un remote `https://dev.azure.com/Contoso/…` d'un compte qui connaît `contoso` ne recevait pas l'en-tête ;
      - vérifié avec git lui-même, sans réseau : test d'intégration `the_azure_header_reaches_a_remote_whose_organisation_is_written_with_another_case` (`crates/dragontree-forges/tests/git_auth.rs`, `git config --get-urlmatch http.extraheader <url>` avec les variables d'environnement de DragonTree et la configuration de la machine écartée) : avec l'origine du compte seule, git ne donne pas l'en-tête pour ce remote (vu rouge) ;
      - désormais, `git_auth_env` reçoit les URL des remotes du dépôt (celle du clone pour un clonage) : pour chaque remote HTTPS du compte Azure DevOps dont le chemin commence par une de ses organisations ou collections (casse ignorée), l'origine est ajoutée telle que le remote l'écrit (`https://dev.azure.com/Contoso/`, `https://tfs.contoso.com:8443/TFS/DefaultCollection/`) ; GitHub et GitLab n'en ont pas besoin (origine sans chemin, hôte comparé sans casse). Test unitaire `auth_env_azure_follows_the_case_of_the_organisation_in_the_remotes` (`auth.rs`) ;
      - suivi de revue : l'association d'un compte à une URL web (`account_matches_url`) compare aussi le port (443 par défaut, 80 pour `http://` ; sans port des deux côtés, les ports par défaut correspondent ; le port d'une URL SSH ne compte pas) : un autre port du même hôte, un autre serveur, ne reçoit plus le jeton par une origine tirée de son URL. Test `the_web_port_of_a_remote_must_be_the_one_of_the_account` (vu rouge sans la comparaison).
- **Critère de fin (atteint) :** chaque point est soit corrigé et testé, soit documenté comme limite dans le README (instances en HTTP simple).
- **Priorité :** basse.

#### RV5 — Résidus de la relecture de la traduction (corrigé)

- **Contexte :** relecture de la traduction du 2026-10-17. Trois points ont été corrigés (voir Langue) ; restaient ceux-ci.
- **État :** corrigé (deux points corrigés, le troisième documenté comme limite).
  - **`--lang` et `--theme` temporaires (corrigé).**
    - Avant : l'option était écrite dans les réglages, toujours sauvegardés. Une seule exécution de `dragontree --lang en` changeait donc la langue pour de bon, alors que l'aide disait « force la langue ». `--theme` faisait de même.
    - Décision (plan) : les deux options deviennent temporaires (non enregistrées) ; l'aide le dit.
    - Réalisé (`crates/dragontree-app/src/app.rs`) :
      - le thème et la langue de la ligne de commande sont gardés à part (`Shared::theme_override`, `Shared::lang_override`) ; le choix en vigueur est le leur, sinon celui des réglages (`Shared::theme`, `Shared::language`), lu par ces deux fonctions au démarrage (`DragonTreeApp::new`, suivi de revue) comme à chaque image. `Shared::settings` ne contient que les choix enregistrés : aucune écriture des réglages (`save_settings` après un changement, enregistrement automatique d'eframe toutes les 30 s et à la fermeture, « Enregistrer » de la fenêtre Réglages et son `take_edits`) ne peut les y mettre, ni écraser la valeur enregistrée ;
      - les menus Affichage › Thème et Configuration › Langue cochent le choix en vigueur, et la fenêtre Réglages le montre (`SettingsWindow::new`) ; un choix explicite y est enregistré et met fin au remplacement du champ concerné : entrée d'un menu (même celle déjà cochée), thème ou langue changés dans la fenêtre puis « Enregistrer ». « Enregistrer » sans y toucher les laisse temporaires. Cas limite : la valeur même de `--theme` ou de `--lang` est déjà sélectionnée dans la fenêtre ; pour l'enregistrer, passer par le menu (Affichage › Thème, Configuration › Langue) ;
      - l'aide (`crates/dragontree-app/src/main.rs`) : « thème pour cette exécution seulement (non enregistré) », « langue de l'interface pour cette exécution seulement (non enregistrée) » ; README, « Utilisation ».
    - Tests d'interface (`crates/dragontree-app/tests/ui_start_options.rs`) : `the_command_line_theme_and_language_are_in_effect_but_never_saved` (thème clair enregistré, `--theme dark --lang fr` : la fenêtre montre « Sombre » et « Français », le menu coche « Sombre » ; un réglage changé dans la fenêtre, puis un autre par un menu, sont enregistrés sans le thème ni la langue de la ligne de commande ; lancement suivant sans `--theme` : « Clair » coché ; vu rouge : `(Dark, Some(Fr))` enregistrés), `a_theme_chosen_in_the_menu_is_saved_even_if_it_is_the_command_line_one`, `a_theme_chosen_in_the_settings_window_replaces_the_command_line_one` ; pour la langue (suivi de revue), `a_language_chosen_in_the_menu_is_saved_even_if_it_is_the_command_line_one` (même fichier), et, dans des binaires à part puisque la langue est globale au processus, `language_menu_switches_the_interface_to_english` (`ui_menus_lang.rs` : `--lang fr`, English choisi dans le menu) et `language_can_be_switched_from_the_settings` (`ui_lang.rs` : `--lang en`, Français choisi dans la fenêtre) : l'interface ne change de langue que si le choix met fin au remplacement (vu rouge sans `lang_override = None` dans `run_app_command` et dans la fenêtre : délai dépassé, interface restée dans la langue de la ligne de commande).
  - **Bouton Push affiché occupé pendant le push d'un tag, en français seulement (corrigé).**
    - Avant : l'état occupé des boutons de la barre d'outils se testait sur le début du titre de l'opération, qui est traduit : « Push du tag » commence par « Push », pas « Tag push ». De même en français « Fetch LFS » et « Pull LFS » occupaient Fetch et Pull, en anglais « Stash rename » (renommage d'un stash) occupait Stash, et une commande personnalisée nommée « Commit… » ou « Push… » occupait le bouton, dans les deux langues. La zone de commit testait de même « Commit ».
    - Réalisé : chaque opération porte un type, `OpKind` (`crates/dragontree-app/src/repo/actions.rs`), dans ses options (`OpOptions::kind`) puis dans `RunningOp::kind` (`crates/dragontree-app/src/repo/mod.rs`) ; `RepoTab::op_running(kind)` (opération visible de ce type) donne l'état occupé des boutons Fetch, Pull, Push, Stash et Pop (`toolbar.rs`) et l'état « commit en cours » de la zone de commit (`changes.rs`). Plus aucune logique ne lit un titre d'opération (titres seulement affichés : barre de progression, notifications, dialogue de fermeture).
    - Règle : un bouton est occupé pendant sa propre opération, d'où qu'elle soit lancée. `Fetch` : fetch des remotes (le fetch automatique, silencieux, ne l'occupe pas, comme avant) ; `Pull` ; `Push` : dialogue Push, push rapide, push après un commit, push de plusieurs branches (Dépôt › Pousser plusieurs branches) ; `Commit` ; `Stash` : dialogue Stash (stash ou snapshot) et stash des fichiers choisis ; `StashPop` : pop d'un stash. Les autres (`Other`) n'occupent aucun bouton : push d'un tag (`op_push_tag`, sorti du menu du tag), fetch, pull et prune LFS, renommage, application et suppression d'un stash, commandes personnalisées. Changement de comportement assumé, identique dans les deux langues : un snapshot occupe désormais Stash. Le push de plusieurs branches occupe Push comme avant (« Push de plusieurs branches » / « Pushing several branches ») : d'abord passé en `Other`, il est typé `Push` depuis le suivi de revue, dans les deux langues.
    - Test unitaire `the_busy_buttons_follow_the_kind_of_operation_in_every_language` (`actions.rs`) : file des écritures bloquée, opérations lancées par le code réel ; push d'un tag, fetch et pull LFS, renommage d'un stash et fetch automatique n'occupent aucun bouton, le push de plusieurs branches occupe Push et lui seul (vu rouge en `Other` : aucun bouton occupé), fetch, pull, push, commit, stash et pop occupent le leur ; mêmes résultats avec les titres remplacés par leur traduction anglaise (« Tag push », « LFS fetch », « LFS pull », « Stash rename », « Pushing several branches », « Pop stash » : la langue de l'interface est globale au processus, les tests unitaires restent en français). Vu rouge avec l'ancienne règle (début du titre) : `[Fetch, Pull, Push]` occupés par les titres français.
  - **Textes figés dans l'ancienne langue après un changement de langue (documenté).**
    - Titres des opérations en cours, notifications déjà affichées, titres et textes des dialogues ouverts, messages d'erreur internes (qui peuvent alors mêler les deux langues) restent dans la langue du moment où ils ont été construits.
    - Décision (plan) : documenter la limite ; pas de retraduction des notifications déjà affichées. Documentée dans le README (« Langue ») et en commentaire là où la langue change (`App::process_actions`, `crates/dragontree-app/src/app.rs`) et sur `RunningOp::title`.
    - Message d'erreur des réglages illisibles, construit avant l'application de la langue choisie (vérifié) : il n'en résulte aucun mélange. Il n'existe que si les réglages n'ont pas pu être lus ; ce sont alors les réglages par défaut (langue du système), et le message est construit dans la langue du système ou dans celle de `--lang`, déjà appliquée par `main` : la même que la langue en vigueur ensuite.
- **Critère de fin (atteint) :** décision prise sur `--lang` (temporaire) et aide alignée ; test unitaire de l'état occupé indépendant de la langue.
- **Priorité :** basse.

### Pistes et limites connues

#### P1 — Publication open source (fait)

- **Contexte :** l'objectif annoncé par l'utilisateur est de publier l'outil en open source.
  - Les vérifications du nom proposées pendant le choix du nom n'avaient pas été faites : noms de domaine, registres de marques (INPI, EUIPO, USPTO), identifiants winget et Scoop.
  - Pour « Dragontree », seuls crates.io, GitHub (9 dépôts vides) et une recherche web avaient été consultés.
- **État :** réalisé.
  - Avant : le manifeste déclarait la licence MIT (`Cargo.toml:9`), mais le dépôt n'avait pas de fichier `LICENSE`.
  - `LICENSE` à la racine : texte MIT standard, « Copyright (c) 2026 DragonTree contributors ». La licence de la police Phosphor reste à part (`assets/fonts/LICENSE-Phosphor.txt`) ; le README les cite toutes deux (section « Licence »).
  - Le README annonce 1 077 tests (nombre réel au 2026-10-22, fin de la branche `points-restants` ; 870 le 2026-10-20, 277 le 2026-10-17).
  - Vérification du nom consignée ci-dessous ; aucune démarche d'achat, de dépôt ni d'inscription (décision du plan).
- **Vérification du nom (2026-10-22, recherche web uniquement) :**
  - **Domaines** (registres RDAP publics) :
    - `dragontree.com` : enregistré depuis le 1996-03-29, expire le 2029-03-30. Le site n'a pas répondu ; contenu non identifié.
    - `dragontree.io` : enregistré depuis le 2018-02-06, expire le 2027-02-06. Page d'accueil vide ; contenu non identifié.
    - `dragontree.app` et `dragontree.dev` : non enregistrés (le RDAP de Google Registry répond « introuvable » ; contrôle de la méthode : `web.dev` y est bien trouvé).
  - **Marques :**
    - USPTO : la recherche officielle (tmsearch.uspto.gov) ne s'affiche qu'en JavaScript, sans résultat lisible. Consulté à la place : Trademarkia, qui reprend les données de l'USPTO.
      - « dragontree » : trois marques vivantes, toutes d'un même titulaire américain, dans le bien-être : THE DRAGONTREE (n° 5838098, classes 35 et 41 : vente en ligne de produits de soin, formations au bien-être), THE DRAGONTREE (série 85763560, classe 44 : spa), DRAGONTREE LIFE COACHING (série 88019117, classe 41). Une marque figurative DRAGONTREE (classe 31, plantes vivantes, titulaire néerlandais) est radiée.
      - « dragon tree » : une seule marque vivante proche, DRAGON TREE LABS (n° 6546097, classe 42 : services de recherche et développement, « laboratoire de technologies de rupture »). DRAGON TREE pour des vins (33), un shampoing (3) et des produits horticoles (31) : radiées ou abandonnées.
      - Aucune marque « dragontree » ou « dragon tree » en classe 9 (logiciels).
    - EUIPO (eSearch) et TMview : interfaces JavaScript seules (eSearch : connexion coupée ; TMview : page vide). Non interrogés.
    - INPI (data.inpi.fr) : accès refusé aux requêtes automatiques (HTTP 403). Non interrogé.
    - Base mondiale des marques de l'OMPI : derrière un captcha. Non interrogée.
  - **Paquets :**
    - winget : aucune issue ni pull request de `microsoft/winget-pkgs` ne contient « dragontree » (recherche de l'API GitHub ; la même recherche trouve 69 résultats pour GitExtensions) ; pas de dossier `manifests/d/DragonTree`.
    - Scoop : pas de manifeste `dragontree.json` dans les buckets officiels Main et Extras (contrôle de la méthode : `sourcetree.json` est trouvé dans Extras) ; aucune issue ni pull request de l'organisation ScoopInstaller ne contient « dragontree ». L'index des buckets communautaires (scoop.sh) ne s'affiche qu'en JavaScript : non interrogé.
    - crates.io : pas de crate `dragontree` ; aucune crate pour « dragontree » ni « dragon-tree » (la recherche « dragon » en trouve 379).
  - **Autres :** GitHub compte 13 dépôts « dragontree » ; aucun n'est un client git ni un logiciel de bureau, hors celui du projet. La recherche web ne trouve aucun logiciel nommé DragonTree. Les homonymes sont l'arbre (*Dracaena draco*), un bâtiment du jeu Dragalia Lost, une société australienne de conseil aux entreprises et l'application Android DragTree (nom différent).
  - **Verdict :** aucun conflit trouvé pour un logiciel.
    - Les trois marques vivantes « dragontree » visent le bien-être (classes 35, 41, 44), pas les classes 9 ni 42.
    - La plus proche est DRAGON TREE LABS (classe 42, recherche et développement) : nom distinct (« Labs », en deux mots), services et non produit logiciel. Risque jugé faible, à faire confirmer avant tout dépôt de marque.
    - Restent à vérifier à la main, faute d'accès automatique : EUIPO/TMview et INPI, classes 9 et 42.
    - `.app` et `.dev` sont libres ; `.com` et `.io` sont pris.
- **À faire :**
  1. Ajouter `LICENSE` (fait).
  2. Vérifier le nom : domaines, marques, winget, Scoop (fait, ci-dessus ; EUIPO/TMview et INPI à vérifier à la main).
  3. Nombre de tests du README à jour (fait le 2026-10-20, puis le 2026-10-22).
  4. Ajouter l'icône (N3, fait).
- **Critère de fin (atteint) :** fichier `LICENSE` présent ; résultat des vérifications du nom consigné.
- **Priorité :** moyenne.

#### P2 — Cases du plan jamais cochées (fait)

- **Contexte :** le plan a été suivi grâce à un journal d'exécution non versionné, et ses cases n'ont pas été cochées.
- **État :** réalisé.
  - Avant : `docs/superpowers/plans/2026-10-16-dragontree.md` comptait 57 cases, toutes vides, alors que les tâches 0 à 24 sont terminées (voir État du plan). De même, `docs/superpowers/plans/2026-10-19-options-fork.md` comptait 90 cases vides, alors que ses 48 tâches sont terminées (branche `options-fork`).
  - Les deux plans portent en tête un avertissement : plan historique et réalisé, cases jamais cochées et laissées telles quelles, document plus tenu à jour, renvoi à ce document (sections « État du plan d'implémentation » ou « Branche `options-fork` », et « Reste à faire »). Les cases ne sont pas cochées.
  - Le plan en cours (`docs/superpowers/plans/2026-10-21-points-restants.md`) n'est pas concerné.
- **À faire :** cocher les étapes faites, ou signaler en tête du plan qu'il est historique et renvoyer à ce document (fait : avertissement en tête des deux plans).
- **Critère de fin (atteint) :** chaque plan terminé dit en tête qu'il est historique et renvoie à la feuille de route.
- **Priorité :** basse.

#### P3 — Stabilité de `ui_network` (fait)

- **Contexte :** le 2026-10-17, des tests d'interface échouaient de temps en temps (attentes qui paniquaient sur une erreur passagère de git, clic sur un bouton encore désactivé). Ils ont été corrigés : `ui_flows` passe 20 fois de suite, mais `ui_network` n'avait été relancé que 3 fois.
- **État :** vérifié le 2026-10-22, aucune correction nécessaire.
  - `cargo test -p dragontree-app --test ui_network` (deux tests : `push_fetch_pull_from_toolbar`, `amend_with_push_after_never_force_pushes`), compilé une fois puis lancé 20 fois de suite : **20 passages sur 20**, de 2,2 à 2,6 s chacun (git 2.38.1, toolchain GNU).
  - Sous charge, en plus du critère : l'exécutable de test lancé 4 fois en parallèle, 10 fois de suite (40 sur 40, 3,9 s au plus), puis 8 fois en parallèle, 5 fois de suite (40 sur 40, 6,2 s au plus).
  - Les attentes du fichier relisent l'état à chaque image : une commande git qui échoue un instant (`try_git`) ne fait pas paniquer l'attente.
- **À faire :** lancer `ui_network` en boucle (20 passages) (fait).
- **Critère de fin (atteint) :** 20 passages sur 20.
- **Priorité :** basse.

#### P4 — `--help` en build release depuis une console Windows (fait)

- **Contexte :** relevé en vérifiant le code, ce point n'a pas été discuté.
  - L'exécutable release utilise le sous-système Windows (attribut `windows_subsystem` en tête de `crates/dragontree-app/src/main.rs`), et l'aide est écrite sur la sortie d'erreur (`eprintln!` de `main`, quand `parse_args` renvoie l'aide ou une erreur d'option).
  - L'aide a été vérifiée à travers un tube, depuis Git Bash. Dans une console cmd ou PowerShell interactive, elle risque de ne pas s'afficher.
- **État :** corrigé.
  - Avant (vérifié le 2026-10-22 avec l'exécutable release) : `dragontree.exe --help` tapé dans cmd ou dans PowerShell, `cmd /c`, `Start-Process -NoNewWindow -Wait` : rien ne s'affichait. Une application graphique n'a ni console ni flux standard (handles nuls).
  - Réalisé (`crates/dragontree-app/src/console.rs`) : avant d'écrire l'aide ou une erreur d'argument, `main` appelle `console::attach_parent_console`.
    - Il ne fait rien si la sortie d'erreur est déjà utilisable : console, tube (Git Bash dans mintty), fichier, `NUL`.
    - Sinon il se rattache à la console du parent (`AttachConsole(ATTACH_PARENT_PROCESS)`), puis remet les flux qui étaient redirigés : constaté, depuis cmd, `--help > aide.txt` voyait sa sortie standard remplacée par la console, car cmd transmet les flux sans `STARTF_USESTDHANDLES` et `AttachConsole` les remplace alors tous.
    - Un flux de sortie encore inutilisable après l'attache est ouvert sur `CONOUT$` (un seul handle, partagé si les deux sorties en ont besoin) : c'est le cas de Git Bash lancé dans une console, qui transmet un handle de sortie d'erreur invalide chez l'application graphique, et `AttachConsole` ne le remplace pas (flux transmis avec `STARTF_USESTDHANDLES`).
    - La bibliothèque standard relit `GetStdHandle` à chaque écriture et écrit dans une console en UTF-16 : `eprintln!` suffit ensuite, accents compris.
    - Sans console à rejoindre (lancement depuis l'Explorateur), le message est perdu comme avant ; le code de sortie (2) ne change pas.
    - Le message « instance unique impossible », écrit au démarrage d'une application qui continue, ne se rattache pas : une application rattachée à une console en reçoit les Ctrl+C.
  - Vérifié à la main le 2026-10-22, exécutable release (texte lu dans la console) :
    - tapé dans cmd et dans PowerShell (interactifs) : aide affichée, accents compris ;
    - `cmd /c`, `start /wait`, script `.cmd`, `Start-Process -NoNewWindow -Wait`, Git Bash dans une console : aide affichée avant l'invite ou la commande suivante ; code de sortie 2 (`start /wait`, script, Git Bash) ;
    - option inconnue : message puis aide ;
    - `--help > aide.txt` dans cmd : aide dans la console, fichier vide (l'aide va sur la sortie d'erreur) ;
    - `--help 2> erreurs.txt` : aide dans le fichier, rien dans la console ;
    - Git Bash (tube) : inchangé, aide dans le tube, code 2.
  - Limite (documentée dans le README et le module) : tapé directement dans cmd ou PowerShell, le shell n'attend pas la fin d'une application graphique. L'invite revient aussitôt et le texte s'affiche après elle ; Entrée redonne une invite propre. `start /wait` (cmd), un script `.cmd` ou `Start-Process -NoNewWindow -Wait` (PowerShell) attendent la fin.
- **Tests** (`crates/dragontree-app/src/console.rs`) :
  - `only_a_missing_error_output_attaches_and_redirections_are_kept` : décision selon l'état des flux.
  - `handles_are_classified_by_what_windows_knows_of_them` : handles nul, invalide, inconnu, fichier, tube.
  - `without_a_console_the_call_does_not_panic_and_keeps_the_redirected_output` : le test se relance en processus sans console, sortie standard dans un tube, sans sortie d'erreur utilisable.
    - Lancé comme cmd lance un programme, avec une console parente : `Attached`, et la sortie standard garde son tube.
    - Lancé par la bibliothèque standard (`STARTF_USESTDHANDLES`, comme Git Bash) avec une console parente, sorties standard et d'erreur sur un handle jamais ouvert : `Attached`, les deux sorties sur un même handle `CONOUT$` utilisable.
    - Sans console parente : `NoConsole`, sans panique.
    - Vu rouge sans la remise des flux : sortie standard remplacée par la console, compte rendu de l'enfant perdu ; et avec un `CONOUT$` ouvert par sortie : deux handles différents.
    - Chaque attente d'un processus relancé est bornée (30 s pour l'enfant, 60 s pour le relais) : au-delà, il est tué et le test échoue au lieu de bloquer la suite ; une attente elle-même en échec (`WAIT_FAILED`) le dit, avec l'erreur de Windows. Le message d'échec reprend les sorties du relais et les paniques de l'enfant.
  - L'exécutable release n'est pas construit par la suite de tests (vérification manuelle ci-dessus).
- **À faire :** lancer `dragontree.exe --help` dans cmd et dans PowerShell. Si rien ne s'affiche, s'attacher à la console parente (`AttachConsole`) avant d'écrire (fait).
- **Critère de fin (atteint) :** l'aide s'affiche depuis cmd et PowerShell avec l'exécutable release ; tube de Git Bash inchangé.
- **Priorité :** basse.

#### L1 — Dépôts SHA-256 non pris en charge (limite documentée : erreur explicite)

- **État :** limite documentée (README, « Limites connues ») ; ouverture et clonage refusés avec un message explicite.
  - Avant (vérifié le 2026-10-22 avec git 2.38.1, par les tests ci-dessous lancés sans la correction) : rien ne détectait `extensions.objectFormat`. Un dépôt SHA-256 s'ouvrait sans erreur et son clonage réussissait ; ses branches disparaissaient (lignes de `for-each-ref` ignorées) et le journal échouait sur « commit illisible dans le log » (une HEAD détachée aurait donné « HEAD illisible », d'après le code). Aucune panique : `Oid::from_hex` refusait déjà les identifiants de 64 caractères, sans les tronquer.
  - Réalisé (`crates/dragontree-git`) :
    - erreur `GitError::UnsupportedObjectFormat { format, path }`, traduite : « Le dépôt {path} utilise des identifiants SHA-256 : DragonTree ne prend en charge que les dépôts SHA-1 » (« Identifiant SHA-256 : … » sans dépôt) ;
    - `Repo::open` lit le format avec le même `git rev-parse` (`--show-object-format`, présent dans git 2.31, le minimum pris en charge) et refuse tout format autre que `sha1`, aussi depuis un sous-dossier ;
    - `clone_repo` vérifie le format du clone terminé (`check_object_format`) et renvoie la même erreur ; l'interface supprime alors le dossier qu'elle avait créé, comme après un clonage échoué, et affiche le message dans la fenêtre de clonage. Suivi de revue : la vérification faisait échouer un clonage réussi pour une autre raison, et l'interface supprimait le clone. Cas principal, vérifié (git 2.38.1, `GIT_TEST_ASSUME_DIFFERENT_OWNER=1`) : un dossier d'un autre propriétaire (partage réseau, disque FAT ou exFAT, dossier d'un autre compte), où `git clone` réussit mais `git rev-parse` refuse le dépôt (`safe.directory`) ; avant L1, le clone restait et l'onglet proposait « Faire confiance à ce dépôt ». Désormais la vérification désigne le dossier git explicitement (`--git-dir=.git` : sans recherche du dépôt, pas de contrôle du propriétaire) dans l'environnement du clonage, et `clone_repo` ne renvoie que `UnsupportedObjectFormat` : tout autre échec de la vérification laisse le clone en place, et son ouverture le signale. Chacune des deux mesures suffit seule au cas testé (vérifié en retirant l'autre) ;
    - `Oid::parse` : identifiant SHA-1 ou erreur explicite (SHA-256 : `UnsupportedObjectFormat` ; autre : `Parse`). Il sert aux lectures qui renvoient une erreur : journal (`LogParser`), détail d'un commit (`commit_info`), HEAD détachée (`read_head`). Les lectures qui ignorent les lignes illisibles (refs, état, stashs, worktrees, bisect, blame) gardent `Oid::from_hex`, qui refuse les 64 caractères sans les tronquer ;
    - page d'erreur de l'onglet (`crates/dragontree-app/src/repo/mod.rs`) : en plus du message, « Utilisez git en ligne de commande pour ce dépôt : les identifiants SHA-256 sont une limite connue de DragonTree. »
  - Identifiants de 32 octets : non gérés, limite documentée. `Oid` est un `[u8; 20]` copiable utilisé partout (graphe, tables de hachage, caches de l'historique mesurés en RV1), et le code suppose aussi 40 caractères ailleurs : arbre vide SHA-1 (`EMPTY_TREE`), en-tête `From <sha>` des patchs mbox (`apply.rs`), SHA courts des dialogues, recherche par SHA. Le format reste expérimental dans git (pas d'échange avec un dépôt SHA-1) et rare chez les forges ; le coût (type à taille variable ou à 32 octets, mémoire du graphe, tous les analyseurs) n'est pas raisonnable pour ce point de priorité basse.
- **Tests :**
  - `crates/dragontree-git/tests/object_format.rs` (dépôt créé par `git init --object-format=sha256` avec un commit ; test ignoré avec un message si git a moins de 2.29) :
    - `opening_a_sha256_repository_gives_a_clear_error` : `Repo::open` à la racine et depuis un sous-dossier, erreur `UnsupportedObjectFormat` (`sha256`, chemin du dépôt), message avec « SHA-256 » et « SHA-1 » ; vu rouge avant : dépôt ouvert ;
    - `cloning_a_sha256_repository_gives_the_same_clear_error` : `clone_repo` en `file://`, le clone reste sur le disque (contrat du moteur) ; vu rouge avant : clonage réussi ;
    - `a_clone_in_a_folder_of_another_owner_is_kept` (suivi de revue) : clonage avec `GIT_TEST_ASSUME_DIFFERENT_OWNER=1` dans l'environnement du clonage (pas du processus) : réussi, dossier gardé, et l'ouverture du dépôt donne `DubiousOwnership` (l'erreur qui propose la confiance) ; vu rouge sans la correction : `Err(DubiousOwnership)` ;
    - `sha256_ids_are_refused_by_every_parser_without_panicking` : `Oid::from_hex` et `Oid::parse` sur un identifiant de 64 caractères ; sur le dépôt ouvert sans `Repo::open`, journal et HEAD détachée en erreur explicite, refs et état sans identifiant tronqué, stashs et worktrees sans panique.
  - `oid_refuses_sha256_ids_with_an_explicit_error` (`oid.rs`).
  - Interface : `a_sha256_repository_is_refused_with_an_explicit_error_at_open_and_clone` (`crates/dragontree-app/tests/ui_flows.rs`) : onglet ouvert sur le message et le renvoi vers git en ligne de commande, clonage par la fenêtre arrêté sur le même message, qui nomme le dossier du clone (preuve que git l'a créé), dossier du clone supprimé. Version minimale de git pour les dépôts SHA-256 : helper commun `sha256_supported` (`crates/dragontree-git/tests/common/git_version.rs`, partagé avec les tests de l'interface).
- **À faire :**
  1. Afficher au moins une erreur explicite à l'ouverture (fait, et au clonage).
  2. Plus tard, gérer des identifiants de 32 octets (non retenu : limite documentée ci-dessus).
- **Critère de fin (atteint) :** test d'intégration avec `git init --object-format=sha256` : message clair, sans panique.
- **Priorité :** basse.

#### L2 — Éditeur de conflits limité aux fichiers UTF-8 (corrigé)

- **Contexte :** l'indexation partielle a été corrigée pour les fichiers Windows-1252 (point critique C1), fréquents dans le vieux code .NET français. L'éditeur de conflits, lui, refuse ces fichiers.
- **État :** corrigé.
  - Avant : `open_conflict_editor` (`crates/dragontree-app/src/repo/conflicts.rs`) lisait le fichier en `String` ; un fichier qui n'était pas de l'UTF-8 donnait « Fichier binaire ou non UTF-8 : utilisez « notre / leur version » ou un outil externe. » et aucune région.
  - Réalisé, moteur (`crates/dragontree-git/src/conflict.rs`) : marqueurs reconnus sur les octets (ils sont en ASCII) ; `resolve_bytes` et `resolve_partial_bytes` recopient octet pour octet chaque morceau gardé (lignes hors conflit, côtés choisis, région sans choix), fins de ligne comprises ; `decode_conflict_text` décode pour l'affichage ligne à ligne, en UTF-8 quand la ligne en est, sinon en Windows-1252 (`decode_cp1252`, un octet par caractère) : lignes et marqueurs du texte décodé sont ceux des octets, donc `parse_conflicts` y trouve les mêmes régions. `resolve` et `resolve_partial` (texte) passent par la version sur les octets.
  - Réalisé, interface : l'éditeur garde les octets du fichier, montre les régions et l'aperçu décodés, et enregistre `resolve_bytes` : un fichier Windows-1252 le reste, sans réencodage ni perte. Une ligne le signale : « Fichier en Windows-1252 : affiché décodé, enregistré avec ses octets d'origine. » Un fichier qui contient un octet nul (binaire, UTF-16) reste refusé : « Fichier binaire : utilisez « notre / leur version » ou un outil externe. »
  - Texte saisi à la main : l'éditeur ne permet que de choisir des côtés, aucune saisie libre ; rien n'est donc réencodé. Décision pour une saisie future, notée dans le module : encoder en Windows-1252 et refuser explicitement un caractère qui n'y existe pas, jamais de perte silencieuse.
  - Encodages : tout fichier sans octet nul se résout sans perte, puisque seuls des octets d'origine sont écrits ; seul l'affichage suppose Windows-1252 (Latin-1 s'affiche pareil, hors 0x80 à 0x9F). Décodage ligne par ligne, comme la vue diff : un fichier qui mêle UTF-8 et Windows-1252 reste lisible ; en contrepartie (suivi de revue, noté dans `decode_line`), une ligne Windows-1252 dont les octets forment par hasard de l'UTF-8 valide (`Ã©`) s'affiche comme de l'UTF-8 (`é`), sans effet sur les octets écrits.
- **Tests :**
  - `crates/dragontree-git/tests/conflict_encoding.rs` : vraie fusion en conflit sur `form.cs` en Windows-1252 (é, ê, è, à, €, œ, ’, une ligne en CRLF), deux conflits. `a_cp1252_conflict_is_resolved_region_by_region_keeping_the_original_bytes` : texte et régions décodés ; première région seule choisie, la seconde garde ses marqueurs ; notre côté puis le leur, et les deux côtés dans les deux ordres, donnent exactement les octets attendus ; le fichier écrit se valide et `git show HEAD:form.cs` rend ces octets. `choosing_one_side_everywhere_gives_back_that_branch_file_byte_for_byte`.
  - `cp1252_bytes_are_decoded_for_display_and_copied_back_unchanged` (`conflict.rs` : CRLF, dernière ligne sans `\n`, libellé accentué, ligne UTF-8 dans un fichier qui ne l'est pas) ; `a_cp1252_file_is_shown_decoded` (`conflicts.rs`).
  - Interface : `a_cp1252_conflict_is_resolved_region_by_region_in_the_editor` (`crates/dragontree-app/tests/ui_flows.rs`) : éditeur ouvert sur le fichier, côtés décodés et encodage signalé, « Notre version » pour le premier conflit, « Leur version » pour le second, « Enregistrer et marquer résolu » : fichier indexé, octets exactement ceux attendus. Vu rouge avant : délai dépassé, l'éditeur affichait le refus. `ui_flows` passé 5 fois de suite.
- **À faire :** travailler sur les octets, décoder en Windows-1252 pour l'affichage (`decode_cp1252`, `crates/dragontree-git/src/diff.rs`) et réécrire les octets d'origine (fait).
- **Critère de fin (atteint) :** test d'intégration : un conflit dans un fichier Latin-1 est résolu région par région, et les octets hors conflit sont préservés.
- **Priorité :** moyenne.

#### L3 — Réglages illisibles au démarrage (corrigé)

- **Contexte :** arbitrage de la revue (M9) au coût assumé : un fichier de réglages illisible au démarrage n'est jamais écrasé, mais rien de la session n'était alors enregistré. Piste : retenter la lecture plus tard et prévenir à chaque modification non enregistrée.
- **État :** corrigé.
  - Avant : `SettingsFile::load` (`crates/dragontree-app/src/settings.rs`) rendait le fichier non inscriptible pour toute la session s'il était illisible (verrouillé, droits) ou invalide sans pouvoir être mis de côté ; `save` ne faisait plus rien, sans le redire après la notification du démarrage.
  - Réalisé, lecture (`settings.rs`) : un fichier non lu est relu à chaque enregistrement (`SettingsFile::save`) : à chaque réglage modifié, à l'enregistrement périodique d'eframe (toutes les 30 s) et à la fermeture.
    - Toujours illisible : rien n'est écrit (`Saved::NotWritten`). Un fichier existant n'est jamais écrit sans avoir été lu.
    - Relu : fusion (`merge_session`), écrite puis reprise par la session (`Saved::Recovered`).
    - Relu mais impossible à écrire (fichier tenu ouvert sans partage d'écriture, lecture seule) : rien n'est écrit ; la session reprend la fusion, le fichier tel que lu devient le départ de la fusion suivante, l'erreur d'écriture est retenue (`Pending::WriteFailed`, montrée par l'indicateur) et l'enregistrement suivant réessaie, sans nouvelle notification dans la même série (`Saved::ReadNotWritten`).
    - Disparu : réglages de la session écrits. Toujours invalide mais renommable : mis de côté comme au démarrage, réglages de la session écrits (`Saved::SetAside`, notification).
  - Règle de fusion, sur trois versions : départ de la session (réglages par défaut), session, fichier relu.
    - Un réglage que la session n'a pas changé prend la valeur du fichier ; un réglage changé garde celle de la session.
    - Tables (réglages par dépôt, espaces de travail…) : la même règle, entrée par entrée.
    - Listes (dépôts récents, favoris, comptes, dossiers scannés, commandes…) : ajouts de la session en tête, puis la liste du fichier sans ce que la session a retiré ; plafonds des dépôts récents et des messages récents respectés (`RECENT_REPOS_MAX`, `RECENT_MESSAGES_MAX`).
    - Sans doublon : chemins comparés comme les dépôts (`path_key` : casse, séparateurs, `/` final) dans les dossiers scannés, favoris, dépôts masqués, dépôts récents et onglets des espaces ; comptes comparés par connexion (`Account::same_login` : forge, hôte, identifiant), celui de la session gardé et les dépôts associés au compte du fichier reportés sur lui. Le jeton du compte écarté reste dans le Gestionnaire d'identifiants.
    - Onglets ouverts, onglet actif et espace de travail actif décrivent la fenêtre : ceux de la session. Si le fichier avait un autre espace actif, ses onglets ouverts sont rangés dans cet espace (après ceux que la session y aurait rangés) ; l'espace que la fenêtre affiche prend les onglets de la fenêtre, comme dans toute session (des onglets que le fichier y avait rangés ne sont pas gardés).
    - Repli, si une valeur fusionnée ne se relit pas (énumération à données fusionnée entrée par entrée ; aucune aujourd'hui) : la règle au premier niveau seulement (un réglage changé par la session y prend sa valeur entière, table ou liste comprise, sans les entrées du fichier), puis, en dernier recours, le fichier tel quel (modifications de la session perdues).
  - Limite : la fenêtre Réglages ouverte pendant la relecture garde son brouillon d'avant ; si l'on y a modifié une liste (dossiers scannés, commandes…), « Enregistrer » la remplace entière par ce brouillon, sans les entrées que le fichier relu y avait ajoutées (`take_edits`). Les autres réglages ne sont pas touchés.
  - Réalisé, avertissement (`app.rs`, `menu_bar.rs`) :
    - indicateur « Réglages non enregistrés » à droite de la barre de menus, tant que la session a des modifications que le fichier n'a pas reçues ; infobulle avec le chemin et l'erreur (lecture, ou écriture après relecture), un clic relance l'enregistrement (donc la relecture) ;
    - notification « Modifications des réglages non enregistrées » à la première modification refusée, une seule par série (l'indicateur, lui, reste visible à chaque modification) ;
    - à la relecture : notification « Réglages relus… », indicateur effacé ; l'application reprend les réglages fusionnés (`Shared::adopt_settings`, partagé avec la fenêtre Réglages : thème, taille, langue, police, comptes… ; git imposé et nouveau scan des dossiers si leurs réglages changent).
  - Capture d'écran (`--screenshot`, `Shared::settings_read_only`) : ni relecture, ni écriture, ni indicateur. `--theme` et `--lang` restent hors des réglages : la fusion ne peut pas les écrire.
  - README, « Utilisation ».
- **Tests :**
  - `crates/dragontree-app/src/settings.rs` :
    - `a_locked_settings_file_is_never_overwritten_and_is_merged_once_it_can_be_read` : fichier verrouillé (aucun partage) au chargement ; deux modifications, `NotWritten`, fichier intact ; verrou levé : `Recovered`, valeurs du fichier gardées (thème, tags), modifications de la session (branches distantes, favori ajouté en tête du favori du fichier), fusion écrite ; ensuite `Written`.
    - `an_invalid_settings_file_that_cannot_be_set_aside_is_never_overwritten` : fichier invalide tenu ouvert en lecture partagée (renommage impossible) : `NotWritten`, fichier intact ; relâché : mis de côté, réglages de la session écrits.
    - `the_merge_keeps_the_file_values_the_session_did_not_change` : réglages simples, listes (ajouts, retraits, ordre), tables par dépôt, énumération à données, session sans modification, plafond des dépôts récents.
    - `the_merge_keeps_the_window_tabs_and_stores_the_file_tabs_in_their_workspace` (revue) : fichier dans l'espace « Travail », fenêtre dans « Par défaut » : onglets et espace de la fenêtre, onglets du fichier rangés dans « Travail » (rouverts en y passant), plus d'onglets rangés pour l'espace affiché ; avec des onglets rangés aussi par la session : ceux de la session puis ceux du fichier, sans doublon.
    - `the_merge_has_no_duplicate_path_or_account` (revue) : même compte ajouté des deux côtés (casse, hôte écrit autrement) : celui de la session, association du dépôt reportée ; dossier scanné et dépôt récent écrits autrement : une fois.
    - `a_settings_file_read_again_but_not_writable_is_retried_with_the_session_on_its_values` (revue) : fichier relu mais tenu ouvert sans partage d'écriture : `ReadNotWritten`, fichier intact, `WriteFailed`, seules les modifications de la session restent en attente ; fichier changé entre-temps : l'essai suivant prend sa nouvelle valeur et garde la modification de la session.
    - Vus rouges (revue) : sans le traitement des onglets et des doublons, les deux tests de fusion échouent (onglets du fichier ajoutés à ceux de la fenêtre ; compte en double).
  - Interface (`crates/dragontree-app/tests/ui_settings_file.rs`), avec `--lang fr` :
    - `a_locked_settings_file_is_never_overwritten_then_receives_the_session_changes` : deux modifications par le menu Affichage, indicateur et une seule notification, fichier intact ; verrou levé, clic sur l'indicateur : fichier fusionné (valeurs du fichier, modifications de la session, langue du fichier et non celle de `--lang`), indicateur effacé, menu Affichage avec les tags masqués du fichier.
    - `an_invalid_settings_file_held_open_is_never_overwritten_then_merged_once_repaired` : même chose avec un fichier invalide tenu ouvert, réparé à la main, relu à la modification suivante.
  - Vus rouges avec la relecture désactivée (`save` qui n'écrit jamais, comme avant) : les deux tests du fichier en échec, et le test d'interface du fichier verrouillé arrêté sur « délai dépassé en attendant : fichier relu et enregistré ».
- **À faire :** retenter la lecture plus tard et prévenir à chaque modification non enregistrée (fait).
- **Critère de fin (atteint) :** fichier verrouillé ou invalide au démarrage : une modification affiche l'avertissement et le fichier reste intact ; redevenu lisible, l'enregistrement suivant l'écrit en gardant ses autres valeurs.
- **Priorité :** basse.

#### L4 — Échec d'authentification du fetch automatique visible en permanence (corrigé)

- **Contexte :** arbitrage de la revue (M2) au coût assumé : en arrière-plan, seule une authentification refusée est signalée, et une seule fois par série d'échecs ; la notification fermée, plus rien ne le montrait. Piste : un indicateur persistant dans l'onglet.
- **État :** corrigé.
  - Avant : `RepoTab` (`crates/dragontree-app/src/repo/mod.rs`) retenait seulement qu'un refus avait été notifié (`auth_warned`), remis à zéro au fetch automatique réussi suivant.
  - Réalisé : `RepoTab::fetch_auth_error` garde l'erreur du dernier fetch refusé (`is_auth_error`), automatique ou lancé à la main, de tous les remotes ou d'un seul, et l'efface au prochain fetch de tous les remotes réussi (fetch automatique, bouton Fetch : `fetch --all`, retenus dans `fetches_of_all_remotes`). Le fetch réussi d'un seul remote (« Fetch <remote> » de la barre latérale) ne dit rien des autres et ne l'efface pas ; un pull réussi ou un échec réseau ne la changent pas non plus.
    - Barre d'outils (`repo/toolbar.rs`) : avertissement « Authentification refusée » à côté du bouton Fetch ; infobulle avec l'erreur de git et la marche à suivre (compte associé, identifiants git) ; un clic relance le fetch de tous les remotes. Quand Fetch est grisé, l'infobulle ne le propose pas et le clic ne fait rien.
    - Barre d'onglets (`tabbar.rs`, `TabInfo::auth_failed`) : l'icône de l'onglet devient un avertissement, son infobulle ajoute l'erreur ; visible aussi pour un onglet en arrière-plan, dont le fetch automatique tourne.
    - Notification : inchangée pour le fetch automatique, au premier refus d'une série seulement (« Fetch automatique : authentification refusée ») ; un fetch manuel refusé garde sa notification d'erreur habituelle.
  - Réalisé, aucune invite en arrière-plan (`crates/dragontree-git/src/runner.rs`, `GitCommand::non_interactive`) : `GIT_TERMINAL_PROMPT=0` (toujours posé) n'empêchait pas un programme « askpass » (`GIT_ASKPASS` hérité, celui du terminal de VS Code par exemple ; `core.askPass` ; `SSH_ASKPASS`) d'être lancé à chaque fetch automatique. Désormais `GIT_ASKPASS` vide (git consulte `GIT_ASKPASS`, puis `core.askPass`, puis `SSH_ASKPASS`, et ignore une valeur vide : la recherche s'arrête), `SSH_ASKPASS` vide, `SSH_ASKPASS_REQUIRE=never` pour ssh, en plus de `GCM_INTERACTIVE=never`. DragonTree n'utilise pas de programme askpass à lui (l'authentification par compte passe par des en-têtes HTTP). Les tests posent la même neutralisation pour tout git qu'ils lancent (`NO_CREDENTIAL_PROMPT`, `crates/dragontree-git/tests/common/global_config.rs`).
  - Réalisé, langue (`crates/dragontree-git/src/net.rs`, `with_net`) : les opérations réseau en arrière-plan lancent git avec `LC_ALL=C`. `is_auth_error` reconnaît les messages anglais de git ; avec une installation de git traduite, un refus n'aurait pas été reconnu. Rien d'autre de visible : en arrière-plan, seule l'erreur notifiée s'affiche (désormais en anglais). Les opérations visibles gardent la langue de l'utilisateur ; un refus d'un fetch manuel traduit n'y est pas reconnu (limite ; le Git pour Windows de la machine de test n'a pas de traductions).
  - Textes traduits (`en.json`).
- **Tests :**
  - `a_fetch_authentication_failure_stays_shown_until_the_next_successful_fetch` (`crates/dragontree-app/src/repo/actions.rs`) : fins d'opérations simulées ; deux fetchs automatiques refusés : indicateur, une notification ; échec réseau, pull réussi et fetch réussi d'un seul remote : indicateur gardé ; fetch manuel de tous les remotes réussi : effacé ; fetch manuel d'un remote refusé puis automatique refusé : indicateur, pas de seconde notification ; fetch automatique réussi : effacé ; nouvelle série : nouvelle notification. Vu rouge (revue) avec l'ancien effacement par tout fetch réussi.
  - `only_a_fetch_of_every_remote_is_marked_as_such` (revue) : fetch automatique (`--all`) retenu, « Fetch <remote> » non.
  - `a_non_interactive_command_never_runs_an_askpass_program` (`crates/dragontree-git/tests/runner.rs`, revue) : `git credential fill` sans assistant ; témoin : avec `GIT_ASKPASS`, le programme est lancé deux fois ; sans lui : `core.askPass` avec l'environnement des tests, `core.askPass` puis `SSH_ASKPASS` avec `GIT_ASKPASS` vide, et commande non interactive malgré `GIT_ASKPASS`, `SSH_ASKPASS` et `core.askPass` hérités : « terminal prompts disabled », aucun lancement. Vu rouge sans la neutralisation (programme lancé), et sans `NO_CREDENTIAL_PROMPT` dans les tests.
  - `a_background_fetch_runs_git_in_english_without_any_credential_prompt` (`crates/dragontree-git/tests/net_ops.rs`, revue) : transport ssh simulé (`core.sshCommand`) qui note son environnement ; langue de l'utilisateur simulée (`LC_ALL=fr_FR.UTF-8`) : en arrière-plan `LC_ALL=C` et aucune invite possible, fetch visible inchangé. Vu rouge sans `LC_ALL=C`.
  - Interface (`crates/dragontree-app/tests/ui_fetch_auth.rs`, `a_refused_automatic_fetch_shows_a_lasting_indicator_until_a_fetch_succeeds`) : serveur HTTP local qui répond 401 à tout ; dépôt sans assistant d'identifiants (`credential.helper` vide : le Gestionnaire d'identifiants de la configuration système n'est pas lancé, vérifié avec `GIT_TRACE`), sans invite (`GIT_TERMINAL_PROMPT=0`, `GCM_INTERACTIVE=never`). Fetchs automatiques refusés plusieurs fois : indicateur et une seule notification ; infobulles de l'indicateur (avec l'erreur de git) et de l'onglet ; remote remplacé par un dépôt nu local : le fetch suivant réussit, indicateur effacé, et l'infobulle de l'onglet réduite à son chemin (attendue telle quelle, puis ligne d'erreur absente ; vu rouge, revue, avec une ligne d'erreur restée). Vu rouge sans la correction : « délai dépassé en attendant : indicateur d'authentification refusée ».
- **À faire :** un indicateur persistant dans l'onglet (fait).
- **Critère de fin (atteint) :** un fetch automatique refusé (serveur local 401) affiche l'indicateur ; un fetch de tous les remotes réussi l'efface.
- **Priorité :** basse.

#### L5 — Amend d'un commit déjà poussé avec « Pousser après » (corrigé)

- **Contexte :** arbitrage de la revue (I7b) au coût assumé : le push qui suit un commit n'est jamais forcé d'office. Piste : proposer explicitement un push forcé dans un dialogue de confirmation.
- **État :** corrigé.
  - Avant : `push_after_commit` (`crates/dragontree-app/src/repo/actions.rs`) écartait le push quand le commit modifié par « Amend » était déjà poussé (branche sans commit en avance) : rien n'était poussé, notification « Le commit n'a pas été poussé » (Push › Forcer à faire à la main).
  - Réalisé, moteur (`crates/dragontree-git/src/net.rs`) :
    - `Repo::head_if_pushed` : HEAD s'il est aussi la référence de suivi de la branche courante (`git rev-parse HEAD @{upstream}`), sinon `None` (HEAD détachée, pas de branche suivie, HEAD en avance, en retard ou divergente) ;
    - `Repo::push_with_lease` : `git push --force-with-lease=refs/heads/<branche distante>:<commit remplacé>`. Bail fixé à une valeur : jamais de `--force` nu, ni de bail sans valeur, qui prendrait pour valeur attendue la référence de suivi, qu'un fetch (automatique) a pu avancer entre-temps.
  - Réalisé, interface (`repo/actions.rs`, `repo/dialogs.rs`) :
    - le commit (« Amend » et « Pousser après le commit », branche sans commit en avance) lit `head_if_pushed` dans la même opération, juste avant l'amend (`AfterCommit::OfferForcePush`) : le bail porte sur le commit que l'amend remplace ;
    - commit réussi, commit remplacé au sommet de la branche distante : dialogue « Pousser le commit modifié ? », marqué destructeur (Entrée ne le valide pas), qui nomme le commit remplacé et la protection. « Pousser en forçant » lance le push forcé (`OpKind::Push` : le bouton Push est occupé, RV5b) ; « Annuler », Échap ou un clic hors du dialogue gardent le commit modifié en local et affichent la notification d'avant ;
    - remote qui a bougé entre-temps (un autre clone a poussé, même suivi d'un fetch) : git refuse (« stale info »), la notification « Push a échoué » le dit : « Push forcé refusé : origin/main n'est plus sur <sha>, le commit remplacé (quelqu'un a poussé entre-temps). Rien n'a été écrasé ; le commit modifié reste en local. » ;
    - commit remplacé qui n'était pas le sommet de la branche distante (branche en retard après un fetch : un push forcé écraserait des commits jamais intégrés), ou autre dialogue ouvert pendant le commit : pas de dialogue, la notification d'avant ;
    - textes traduits (`en.json`).
- **Tests :**
  - Moteur (`crates/dragontree-git/tests/net_ops.rs`) :
    - `an_amended_pushed_commit_is_replaced_by_a_push_with_a_pinned_lease` : commit remplacé lu avant l'amend, push forcé, remote sur le commit modifié ;
    - `a_push_with_a_pinned_lease_is_refused_when_the_remote_moved_even_after_a_fetch` : un autre clone pousse, fetch : refus « stale info », commit de l'autre clone gardé. Vu rouge avec un bail sans valeur (`--force-with-lease` seul) : push accepté ;
    - `head_if_pushed_is_none_unless_head_is_its_upstream` : en avance, en retard, sans branche suivie, HEAD détachée.
  - Interface (`crates/dragontree-app/tests/ui_network.rs`, remote nu local en `file://`, sans réseau), à la place de `amend_with_push_after_never_force_pushes` :
    - `amending_a_pushed_commit_offers_a_force_push_that_replaces_it` : dialogue qui nomme le commit remplacé, remote inchangé avant la confirmation ; « Pousser en forçant » : le remote a le commit modifié ;
    - `cancelling_the_force_push_keeps_the_amended_commit_local` : « Annuler » : notification, remote inchangé, commit modifié en local ;
    - `the_force_push_is_refused_when_someone_pushed_meanwhile` : un collègue pousse pendant le dialogue, fetch, confirmation : refus signalé, commit du collègue gardé sur le remote ;
    - `no_force_push_is_offered_when_the_branch_is_behind_its_remote` : branche en retard : pas de dialogue, notification, remote inchangé.
    - Vus rouges sans la correction (interface d'avant) : « délai dépassé en attendant : dialogue du push forcé » pour les trois premiers.
- **À faire :** proposer explicitement un push forcé dans un dialogue de confirmation (fait).
- **Critère de fin (atteint) :** amend d'un commit poussé avec « Pousser après » : dialogue ; confirmé, le remote reçoit le commit modifié ; annulé, rien n'est poussé ; remote qui a bougé entre-temps : push refusé et signalé, sans perte.
- **Priorité :** basse.

#### L6 — Indexation partielle avec l'option « Espaces » (corrigé)

- **Contexte :** arbitrage de la revue (M1) au coût assumé : un diff calculé sans les espaces (`-w`) ne correspond pas au fichier (ses lignes de contexte viennent de la nouvelle version), on ne peut pas en tirer un patch. Piste : recalculer le diff sans `-w` pour construire le patch, comme la revue le suggérait.
- **État :** corrigé.
  - Avant : avec « Espaces », le diff des changements passait en lecture seule (`change_detail`, `crates/dragontree-app/src/repo/changes.rs`) : « Lecture seule : désactivez « Espaces »… », ni boutons de hunk ni sélection de lignes.
  - Réalisé, moteur (`crates/dragontree-git/src/patch.rs`, `build_patch_ignoring_whitespace`) : la sélection faite dans le diff affiché (`-w`) est reportée sur le diff complet (sans `-w`) du même fichier, puis passe par `build_patch` comme avant.
    - Le patch est construit sur les lignes du diff complet (octets exacts, `\r` et fin de fichier sans saut de ligne compris), dans l'ordre du hunk affiché : chaque ligne de contexte affichée dont les deux versions diffèrent (par des espaces seulement, puisque `-w` les a égalées) y devient une suppression suivie d'un ajout. Une ligne choisie va donc à la place où elle est affichée, même là où le diff complet range les lignes autrement (bloc réindenté sous un `if` : le diff complet supprime puis ajoute tout le bloc, le diff affiché ne montre que les lignes ajoutées autour).
    - Règle de correspondance, hunk entier (« Indexer le hunk », « Désindexer le hunk », « Annuler le hunk ») : toutes les modifications du hunk affiché, y compris les changements d'espaces de ses lignes de contexte, c'est-à-dire des lignes des deux versions qu'il couvre ; jamais ceux d'en dehors des hunks affichés.
    - Règle de correspondance, lignes choisies : ces lignes seulement, à leur place ; les changements d'espaces des lignes de contexte ne sont pas pris (ces lignes gardent leur version de départ). Rien d'autre que ce qui est choisi n'est donc indexé, hormis, pour un hunk entier, les changements d'espaces de ses propres lignes.
    - Refus (`GitError::Refused`, rien n'est appliqué, message explicite) plutôt qu'un patch douteux : ligne de contexte affichée dont une version ne se retrouve pas dans le diff complet (les deux diffs n'alignent pas les lignes de la même façon : « désactivez « Espaces » pour choisir ces lignes ») ; ligne dont le texte diffère d'un diff à l'autre, ou deux versions qui diffèrent autrement que par des espaces (fichier modifié entre les deux lectures : « recommencez une fois le diff rechargé ») ; diff complet tronqué ou binaire.
    - En-tête : simple modification (comme une sélection partielle), sauf création ou suppression de fichier ; un changement de mode, non affiché, n'est pas indexé par ce chemin.
  - Réalisé, interface :
    - avec « Espaces », le diff complet est lu dans la même tâche que le diff affiché (`load_work_diff`, `ChangesState::diff_full`) : les deux décrivent le même état du fichier, et l'on indexe ce qui a été vu ; un fichier non suivi garde son chemin (`git add -N`, diff relu sans `-w` : toutes ses lignes sont ajoutées) ;
    - `op_patch` passe par `build_patch_ignoring_whitespace` quand le diff affiché a un diff complet ; indexer, désindexer et annuler suivent la même règle ;
    - au-dessus du diff, la règle est rappelée : « Espaces ignorés à l'affichage : un hunk indexé, désindexé ou annulé emporte aussi ses changements d'espaces ; une sélection de lignes, non. » ; le dialogue « Annuler des lignes » d'un hunk entier ajoute « Ses changements d'espaces, masqués par « Espaces », seront aussi annulés. » ;
    - textes traduits (`en.json`), texte de lecture seule retiré.
- **Tests :**
  - `crates/dragontree-git/src/patch.rs` : `whitespace_patch_keeps_the_shown_order_with_the_full_diff_bytes` (CRLF perdu sur une ligne de contexte : paire `-b\r` / `+b` dans le hunk entier, ligne gardée en CRLF pour une sélection de lignes, contexte sélectionné sans effet, sens inverse) ; `whitespace_patch_refuses_a_context_line_the_full_diff_aligns_elsewhere` ; `whitespace_patch_refuses_lines_that_differ_between_the_two_diffs` (texte différent, versions qui diffèrent au-delà des espaces, diff complet tronqué).
  - `crates/dragontree-git/tests/whitespace_patch.rs` (vrais diffs de git, index et copie de travail comparés octet pour octet, fichiers en CRLF) :
    - `a_whole_hunk_takes_the_whitespace_changes_inside_it_and_only_those` : ligne changée, ligne passée de CRLF à LF et ligne réindentée dans le hunk, changements d'espaces hors du hunk (un seul hunk affiché, deux dans le diff complet) : l'index reçoit ceux du hunk seulement ;
    - `selected_lines_take_only_those_lines` : lignes `-`/`+` choisies, puis l'ajout seul ;
    - `a_selected_line_goes_where_it_is_shown_even_when_the_full_diff_orders_lines_otherwise` : bloc entouré d'un `if` ;
    - `a_newline_added_at_the_end_of_the_file_follows_the_whole_hunk_only` : saut de ligne final que `-w` ne voit pas ;
    - `unstaging_and_discarding_a_whole_hunk_follow_the_same_rule`.
    - Vus rouges en traitant les contextes affichés comme des contextes (sans les paires d'espaces) : 4 tests sur 5 en échec.
  - Interface (`crates/dragontree-app/tests/ui_flows.rs`, `a_hunk_is_staged_with_whitespace_ignored`, à la place de `ignore_whitespace_makes_the_diff_read_only`) : « Espaces » actif, fichier en CRLF ; un seul « Indexer le hunk », règle rappelée ; après le clic, l'index est octet pour octet l'ancienne version avec la ligne changée et les changements d'espaces du hunk, sans ceux d'en dehors ; copie de travail intacte. Vu rouge avant : « délai dépassé en attendant : bouton « Indexer le hunk » ».
- **À faire :** construire le patch sur le diff complet (fait).
- **Critère de fin (atteint) :** avec « Espaces », un hunk ou des lignes s'indexent, se désindexent et s'annulent ; l'index reçoit exactement la sélection, plus, pour un hunk entier, les changements d'espaces de ses lignes ; une correspondance ambiguë est refusée avec un message.
- **Priorité :** basse.

## Branche `options-fork` (2026-10-20)

La branche `options-fork` livre toutes les options des tableaux ci-dessus (F2–F4, V2–V13, R1–R20, C2–C8, H1–H4) ainsi que la barre de menus Fichier / Affichage / Dépôt / Configuration (T2), un commit par option (id en tête du sujet), corrections de relecture en `fixup!`. Elle résout T1 (worktrees, via R3), T2, N2, B1, B5 et B6 (via C3), et tranche les raccourcis (Ctrl+Maj+L reste le journal, Pull = Ctrl+Maj+U, Ctrl+Maj+C reste Cloner). Tout passe par un registre de commandes unique (`crates/dragontree-app/src/commands.rs`) : menus, raccourcis, palette Ctrl+P et fenêtre des raccourcis en dérivent. Les commandes personnalisées (R15) refusent les valeurs dangereuses pour `cmd.exe` et passent aussi les valeurs acceptées en variables `DT_*` ; l'IA (C7) lance Claude Code sans aucun outil, consigne sur stdin, et reste désactivée par défaut. La suite compte 870 tests, clippy et rustfmt sont verts. Restaient ouverts alors : B2 à B4 et B7, T3 à T6, N1, N3, RV1 à RV5, P1 à P4 (tous traités depuis par la branche `points-restants`, ci-dessous).

## Branche `points-restants` (2026-10-22)

La branche `points-restants` traite tous les points qui restaient ouverts après `options-fork` : B2, B3, B4 et B7 (bugs), T3 à T6, N1 et N3, RV1 à RV5, P1 à P4, puis les pistes L1 à L6, plus l'isolation des tests de la configuration git globale. Un commit par tâche (id en tête du sujet) ; les corrections de revue sont des commits ordinaires `<id> — Corrige … (revue)`, sans `fixup!` ni réécriture de l'historique.

- **Livré :** stashs avant remotes et tags dans la barre latérale (T3), choix « branche courante seulement » mémorisé par dépôt (T4), glisser-déposer entre listes, colonne du graphe redimensionnable, date de mise à jour dans la liste de clonage et aperçu du résultat des conflits (T5), compte associé à un dépôt existant (T6), fenêtre Fichier › Pull requests… (N1), icône intégrée à l'exécutable (N3) ; filtre et coloration en arrière-plan avec cache (RV1), lecture des diffs bornée à 8 Mio (RV2), fetch automatique sur une file à part et une file par dépôt (RV3), confirmation à la fermeture, instance unique par profil, proxy de Windows, cas des forges (RV4), `--lang` et `--theme` temporaires (RV5) ; licence MIT et vérification du nom (P1), plans historiques signalés (P2), stabilité de `ui_network` (P3), `--help` dans cmd et PowerShell (P4) ; dépôts SHA-256 refusés avec une erreur explicite (L1), conflits Windows-1252 (L2), réglages illisibles relus à l'enregistrement (L3), indicateur d'authentification refusée du fetch automatique (L4), push forcé protégé après un amend (L5), indexation partielle avec « Espaces » (L6).
- **Décisions à retenir :**
  - un fetch ou un pull lancé à la main n'arrête jamais un fetch automatique qui tourne (tuer git pendant l'écriture des références laisse des fichiers `*.lock`) : il annule celui qui n'a pas commencé, sinon attend et le dit ; en HTTP, le fetch automatique abandonne de lui-même une connexion muette (`http.lowSpeedLimit`, `http.lowSpeedTime`), pas en SSH ;
  - `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` ne vaut que pendant qu'une commande tourne : ce que git laisse tourner après une commande terminée (démon fsmonitor, éditeur de `git mergetool`) survit, car le tuer commande par commande casserait ces cas ; à la fermeture, aucune opération git en cours ne survit ;
  - instance unique : le client vérifie que le serveur du tube est du même utilisateur avant d'y envoyer quoi que ce soit ;
  - un fetch automatique ne lance jamais d'invite d'identifiants (`GIT_ASKPASS` et `SSH_ASKPASS` neutralisés) et tourne avec `LC_ALL=C` pour reconnaître un refus d'authentification ;
  - T6 liste aussi les comptes qui ne correspondent pas au remote, après ceux qui correspondent et avec un avertissement (une organisation Azure DevOps ou un serveur auto-hébergé peut ne pas correspondre textuellement) ;
  - `--lang` et `--theme` montrent leur valeur dans les menus et dans Réglages ; l'enregistrer demande de la choisir dans un menu ;
  - L1 : l'erreur explicite suffit, les identifiants de 32 octets sont écartés (coût d'un `Oid` de taille variable) ; L3 : la fusion des réglages relus se fait sur trois versions (départ de la session, session, fichier) et les onglets ouverts, l'onglet actif et l'espace actif restent ceux de la fenêtre ;
  - les tests ne lisent jamais la configuration git globale : l'isolation est la valeur par défaut des helpers communs (déterminisme, aucune écriture dans la configuration de l'utilisateur) ;
  - les noms de marques tiers cités dans la vérification du nom (P1) sont gardés : ce sont des constats, sans démarche d'achat ni de dépôt.
- **Qualité :** 1 077 tests (10 autres sont ignorés par défaut), clippy (`-D warnings`) et rustfmt verts.
- **Reste ouvert :** seulement ce qui a été écarté ou documenté comme limite, listé dans « Reste ouvert » plus haut avec un renvoi vers chaque entrée : identifiants SHA-256 de 32 octets, forges en HTTP simple, proxy SOCKS et à authentification, instances (profil 8.3 ou `subst`, élevée et non élevée), SSH sans limite de débit du fetch automatique, textes figés après un changement de langue, invite de `--help` dans cmd et PowerShell, fenêtre Réglages et déconnexion de Windows, fetch manuel avec un git traduit, limites de la vue des pull requests, `link.exe` pour l'icône MSVC, recherche de marque EUIPO et INPI, confirmation de T3.

## État du plan d'implémentation

Plan : `docs/superpowers/plans/2026-10-16-dragontree.md` (non réécrit ici).

- **Tâches 0 à 24 :** toutes faites selon le journal d'exécution ; aucune case n'est cochée, le plan dit en tête qu'il est historique (P2).
- **Mesures de la tâche 24 :**
  - 110 Mo de mémoire privée avec 3 onglets, dont le gros dépôt ;
  - historique complet en 1,24 s ;
  - CPU au repos quasi nul.
- **Revue finale :** elle a été faite. Ses corrections sont listées pour mémoire, à ne pas refaire ; chacune a un test vu en échec avant la correction :
  - critiques : C1 (indexation partielle des fichiers non UTF-8), C2 (Maj+clic qui plantait) ;
  - importants : I3 à I9 ;
  - mineurs reclassés en importants : M1 à M5, M9, M11, M12, M14, M15, M17, M18.
- **Écarts qui restaient (tous résolus depuis, branche `points-restants`) :**
  - tâche 15 : colonne du graphe → T5 (fait) ;
  - tâche 16 : glisser-déposer entre listes → T5 (fait) ;
  - tâche 18 : colonne « mise à jour » → T5 (fait) ;
  - tâche 19 : cache LRU et seuil de coloration → RV1 (fait) ;
  - tâche 22 : aperçu des conflits → T5 (fait).
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
