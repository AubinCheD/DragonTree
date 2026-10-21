# DragonTree

Client git graphique natif pour Windows, écrit entièrement en Rust (interface [egui](https://github.com/emilk/egui),
rendu OpenGL « glow »). Le moteur git est le `git.exe` installé sur la machine : vos credentials (Git Credential
Manager, clés SSH), hooks, signatures et configuration s'appliquent tels quels.

## Fonctionnalités

- **Barre de menus** : Fichier / Affichage / Dépôt / Configuration, avec les raccourcis dans les libellés ; une entrée
  grisée n'agit pas (pas de dépôt ouvert, git-lfs absent…). Les mêmes commandes sont dans la palette Ctrl+P.
  Configuration › Raccourcis clavier liste tous les raccourcis, Configuration › À propos donne la version.
- **Onglets** : un onglet par dépôt, créés à la demande (Ctrl+T, Ctrl+W, Ctrl+Tab, Ctrl+1…9, glisser pour
  réordonner, clic molette pour fermer, Ctrl+Maj+T pour rouvrir le dernier fermé). Les onglets ouverts sont restaurés au
  démarrage. **Couleurs d'onglet** et **espaces de travail** (groupes d'onglets nommés : Fichier › Espaces de travail).
- **Liste des dépôts à droite** : choisissez un ou plusieurs dossiers racines (ex. `DEV`). DragonTree y trouve
  tout seul tous les dépôts (scan parallèle, `node_modules`/`bin`/`obj`… ignorés) et les liste avec leur
  branche, le nombre de fichiers modifiés et l'avance/retard sur le remote. Recherche floue, dépôts épinglés,
  dépôts masqués, signalement des dépôts refusés par `safe.directory` (avec « Faire confiance à ce dépôt »).
- **Historique** : graphe des branches, étiquettes (branches locales/distantes, tags, HEAD), recherche par
  message/auteur/SHA, dans le contenu des diffs ou par chemin, détails du commit, fichiers modifiés,
  diff unifié ou côte à côte avec coloration syntaxique et surlignage des caractères modifiés, images en côte à côte,
  par balayage ou par superposition.
  - **Filtres** : toutes les branches, la branche courante seule (Ctrl+Maj+A, suit les checkouts, choix mémorisé par dépôt) ou des branches,
    dossiers et remotes choisis (entonnoir, bandeau « Filtré par »), masquer une branche, un dossier ou un remote
    (icône œil), masquer les tags ou les branches distantes.
  - **Graphe** : stashs affichés comme des commits (masquables), commits perdus du reflog (Ctrl+Maj+.), fusions
    repliables (← / →, Tout replier), révéler HEAD, tri topologique ou par date, disposition horizontale ou verticale.
  - **Colonne du graphe** redimensionnable à la poignée de l'en-tête (largeur mémorisée ; double-clic : automatique).
  - **Glisser une branche** sur une autre pour la fusionner ou la rebaser.
  - **Comparer** deux commits, deux branches, ou un commit avec les modifications locales.
- **Changements locaux** : fichiers non indexés / indexés / en conflit, indexation par fichier (aussi en glissant les fichiers, sélection comprise, entre « Non indexés » et « Indexés »), par **hunk** ou
  par **ligne** (y compris fichiers non suivis et CRLF), annulation de modifications, « Ignorer » dans le
  `.gitignore`, dialogue « Ignorer… » (motif libre ou proposé, `.gitignore` racine ou de dossier ou `.git/info/exclude`,
  aperçu des fichiers concernés), zone de commit (Amend, pousser après le commit, Ctrl+Entrée) avec indicateur de
  longueur 50/72, colonne guide, messages récents et Co-authored-by. Affichage des fichiers ignorés ou masquage des non
  suivis ; remise d'un fichier dans son état à un commit donné.
- **Opérations** : fetch, pull (merge ou rebase), push (upstream, force-with-lease, tags), branches (créer,
  renommer, supprimer localement et sur le remote, checkout, fast-forward), tags (annotés, push, suppression),
  merge (ff, no-ff, ff-only, squash), rebase, **rebase interactif** (réordonner, pick/reword/edit/squash/
  fixup/drop), cherry-pick, revert, reset (soft/mixed/hard), stash (avec message, non suivis, garder l'index,
  choix du stash au pop, renommage, fichiers sélectionnés seulement, snapshot ; les stashs sont aussi dans le graphe),
  mise à jour, push ou suppression de **plusieurs branches à la fois** (branches périmées sélectionnables),
  Fetch / Pull / Push rapides sans dialogue (Ctrl+clic), remotes (modifier l'URL, tester la connexion),
  sous-modules (ajouter, déplacer, changer l'URL, supprimer), **pull requests** (créer sur GitHub, GitLab, Azure DevOps),
  **worktrees** (section de la barre latérale : ouvrir dans un onglet, créer depuis une
  branche, une branche distante ou un commit, supprimer, nettoyer ; les branches sorties ailleurs sont signalées).
- **Conflits** : bandeau d'opération en cours (Continuer/Annuler/Passer), résolution « notre/leur version »,
  éditeur de conflits par région (avec aperçu du résultat), outil de merge externe (`git mergetool`).
- **Bisect** : démarrer depuis le menu d'un commit ou Dépôt › Bisect, bandeau d'état (révisions restantes, Bon, Mauvais,
  Passer, Terminer), commits marqués étiquetés dans le graphe, premier mauvais commit annoncé.
- **Git Flow** (Dépôt › Git Flow, sans l'extension git-flow) : initialiser (clés `gitflow.*` de git-flow, donc un dépôt déjà
  initialisé est reconnu), démarrer une feature, une release ou un hotfix, et terminer la branche courante : fusions
  `--no-ff`, tag annoté « Version x » pour une release ou un hotfix, suppression facultative (jamais forcée) ; sur
  conflit la suite s'arrête dans l'état de fusion de git et le message dit ce qui reste à faire.
- **Git LFS** (Dépôt › Git LFS, entrées grisées si `git lfs` est absent) : initialiser dans le dépôt (`git lfs install --local`,
  jamais global), suivre ou ne plus suivre un motif (liste lue dans `.gitattributes`), fetch, pull et prune avec progression et
  annulation, fenêtre des verrous (verrouiller, déverrouiller, forcer après confirmation) ; les fichiers gérés par LFS portent
  une icône dans la vue Changements.
- **Commandes personnalisées** (Réglages › Commandes) : une ligne de commande exécutée par `cmd.exe` dans le dossier du
  dépôt, ou une URL ouverte dans le navigateur (http, https, mailto), sur le dépôt (Dépôt › Commandes personnalisées), une
  branche, un commit ou un fichier (menus contextuels) ; variables `${repo}`, `${branch}`, `${sha}`, `${file}` et
  `${remote_url}`, citées pour cmd — une valeur n'est jamais lue comme une commande et reste un seul argument pour un
  programme qui lit sa ligne avec les règles de Windows ; une valeur avec un guillemet, un `%` ou un caractère que la
  page de code changerait en guillemet est refusée, comme un `%` collé à une variable ou un guillemet échappé dans la
  ligne — ou encodées dans l'URL (sans pouvoir sortir de leur place). Les valeurs acceptées sont aussi passées en
  variables d'environnement `DT_*`, à lire dans un script `.cmd` (`"%DT_FILE%"`, sans expansion différée) ou `.ps1`
  appelé par la ligne — jamais en `%DT_…%` dans la ligne elle-même (cmd y interpréterait la valeur : refusé).
  Confirmation facultative, exécution en arrière-plan annulable, sortie dans une fenêtre, actualisation du dépôt après.
- **Patchs** : appliquer un patch (fichier, glisser-déposer d'un `.patch`/`.diff`, texte collé) avec aperçu des fichiers
  touchés et vérification, vers la copie de travail, l'index ou en commits (`git am`, patchs `format-patch`) ; **enregistrer
  comme patch** (commit, commits jusqu'à HEAD en un fichier par commit, stash, fichiers sélectionnés) ou le copier.
- **Historique et blame d'un fichier.**
- **Réglages du dépôt** (Dépôt › Réglages du dépôt) : nom et e-mail propres au dépôt, liens vers les tickets
  (`#1234` → URL), modèle de message de commit. **Statistiques** du dépôt (commits, contributeurs, commits par mois, taille).
- **Ouvrir dans** : Explorateur, terminal au choix (Windows Terminal, PowerShell, Git Bash) et éditeurs détectés
  (VS Code, Visual Studio `.sln`, Rider, Cursor…).
- **Outils externes** (Réglages) : diff et merge configurables, avec préréglages VS Code, WinMerge, Beyond Compare,
  KDiff3 et P4Merge. **Identité git globale** (nom, e-mail) dans Configuration. **Clés SSH** (Fichier) : générer,
  choisir, gérer plusieurs clés.
- **Affichage du diff** : police, taille, largeur de tabulation, espaces visibles ou ignorés.
- **IA Claude Code** (facultatif, désactivée par défaut) : message de commit et revue de la branche courante.
  DragonTree lance la CLI `claude` en mode non interactif, sans aucun outil (`--tools ""`) : la consigne passe par stdin
  et la réponse revient sur stdout ; l'IA n'exécute ni ne modifie rien.
- **Comptes GitHub, GitLab et Azure DevOps**, plusieurs à la fois (GitHub Enterprise, GitLab auto-hébergé et
  Azure DevOps Server compris) ; **clonage** depuis la liste agrégée de vos dépôts (avec leur date de mise à jour, sauf Azure DevOps) ou depuis une URL.
- **Pull requests de tous les comptes** (Fichier › Pull requests…, aussi dans la palette) : celles qui vous concernent
  sur GitHub (auteur, assigné, mentionné ou relecteur demandé), GitLab (créées par vous, assignées, à relire) et
  Azure DevOps (créées par vous ou à relire, dans chaque organisation du compte), dans une seule liste de la plus
  récemment mise à jour à la plus ancienne, avec branches, brouillon et relecture demandée. Filtres : créées par moi,
  à relire, par compte et texte. Le titre ouvre la pull request dans le navigateur ; « Ouvrir le dépôt » ouvre le clone
  local qui a ce dépôt en remote (dépôts récents et liste de droite). Relecture à l'ouverture, toutes les 5 minutes tant
  que la fenêtre est ouverte et par Actualiser ; l'erreur d'un compte s'affiche à sa place sans bloquer les autres,
  comme une organisation Azure DevOps qui n'a pas répondu, et un compte Azure DevOps sans organisation est expliqué.
- Surveillance du dossier (rafraîchissement automatique), fetch automatique silencieux (sur une file à part : il ne
  fait pas attendre les écritures, et cède la place à un fetch, pull ou push lancé à la main), palette **Ctrl+P**
  (dépôts, branches, commandes), journal des commandes git, thèmes clair/sombre. Les opérations d'un dépôt ouvert
  dans plusieurs onglets passent par une seule file, dans l'ordre des clics.
- **Interface en français ou en anglais** (Réglages › Apparence › Langue), changement immédiat sans redémarrer.

## Performances (mesurées sur un dépôt de 86 524 commits et 3 600 références)

| Mesure | DragonTree |
| --- | --- |
| Premier affichage de l'historique (2 000 commits) | ≈ 0,5 s après ouverture |
| Historique complet + graphe | ≈ 1,3 s (en arrière-plan) |
| Mémoire privée, 3 onglets dont ce dépôt | ≈ 110 Mo |
| CPU au repos | ≈ 0,2 % |
| Scan d'un dossier `DEV` (13 dépôts) | ≈ 15 ms |

## Compilation

Prérequis : [Rust](https://rustup.rs) ≥ 1.95 et Git pour Windows ≥ 2.31.

### Toolchain GNU (celle installée sur ce poste)

La toolchain rustup `x86_64-pc-windows-gnu` fournit `dlltool` sans l'assembleur dont il a besoin. Un script
télécharge les binutils officiels de MSYS2 (empreintes SHA-256 vérifiées) dans `.tools\`, sans rien installer
ailleurs, et écrit `.cargo\config.toml` (non versionné : ses chemins sont propres au poste) :

```powershell
powershell -ExecutionPolicy Bypass -File scripts\setup-toolchain.ps1
cargo build --release
```

### Toolchain MSVC

Avec Visual Studio Build Tools (« Développement Desktop en C++ ») et `rustup default stable-msvc`, le script est
inutile : supprimez `.cargo\config.toml` puis `cargo build --release`.

L'exécutable est `target\release\dragontree.exe` (environ 16 Mo, aucune dépendance à installer). Il porte l'icône
`assets/dragontree.ico`, intégrée par `crates/dragontree-app/build.rs` sans compilateur de ressources (atteint avec
GNU ; MSVC : objet lié par lld-link, link.exe à confirmer). Elle est dessinée en code (`crates/dragontree-app/src/app_icon.rs`) ; après un changement
du dessin, `cargo run -p dragontree-app --example app_icon` régénère le fichier.

## Utilisation

```text
dragontree.exe [DÉPÔT…] [--open <dossier>] [--add-root <dossier>] [--theme light|dark] [--lang fr|en]
               [--view history|changes] [--profile <dossier>] [--screenshot <png> --after-ms <n>]
```

`--profile` isole réglages, cache et état de fenêtre (pratique pour tester sans toucher à votre profil).
Réglages : `%APPDATA%\DragonTree\settings.json` ; cache des dépôts : `%LOCALAPPDATA%\DragonTree\`.

**Fermeture** : si des opérations (fetch, pull, push, commit, clonage…) attendent ou tournent, fermer la fenêtre
demande d'attendre leur fin (la fenêtre se ferme ensuite d'elle-même) ou de quitter quand même (elles sont arrêtées) ;
le fetch automatique est arrêté sans question. Aucun processus git ne survit à DragonTree.

### Raccourcis

| Raccourci | Action |
| --- | --- |
| Ctrl+T / Ctrl+W | Nouvel onglet / fermer l'onglet |
| Ctrl+Maj+T | Rouvrir le dernier onglet fermé |
| Ctrl+Tab, Ctrl+Maj+Tab, Ctrl+1…9 | Changer d'onglet |
| Ctrl+P | Palette (dépôts, branches, commandes) |
| Ctrl+O | Ouvrir un dépôt |
| Ctrl+Maj+C | Cloner |
| Ctrl+, | Réglages |
| Ctrl+Maj+F / Ctrl+Maj+U / Ctrl+Maj+P | Fetch / Pull… / Push… |
| Ctrl+clic sur Pull / Push | Pull / Push rapides, sans dialogue (branche suivie) |
| Ctrl+Maj+B / Ctrl+Maj+H | Nouvelle branche… / Stash… |
| Ctrl+Maj+A | Historique : branche courante seulement (bascule) |
| Ctrl+Maj+. | Historique : afficher les commits perdus du reflog (bascule, par onglet) |
| Ctrl+B | Afficher/masquer la liste des dépôts |
| Ctrl+0 | Historique : révéler HEAD (le sélectionner) |
| Ctrl+Maj+D | Afficher/masquer le panneau de détails du commit |
| Ctrl+= (ou Ctrl++) / Ctrl+- | Zoom avant / arrière (taille de l'interface, enregistrée) |
| F5 | Actualiser |
| Ctrl+Maj+L | Journal des commandes git |
| Ctrl+Entrée | Commit (dans la zone de message) |
| Espace / Entrée, Suppr | Indexer/désindexer, annuler (liste des fichiers) |
| ↑ / ↓, Page préc./suiv., Début/Fin | Naviguer dans l'historique |
| ← / → | Historique : replier / déplier la fusion sélectionnée |

## Langue

Réglages › Apparence › Langue : **Système** (langue d'affichage de Windows si elle est gérée, anglais sinon),
**Français** ou **English**. Les messages de git lui-même restent dans la langue de git.

Le texte français écrit dans le code sert de clé : `tr!("Fermer l'onglet")`, `tr!("Supprimer « {name} » ?", name)`,
`trn!(n, "{n} fichier", "{n} fichiers")`. Chaque autre langue a un catalogue
`crates/dragontree-i18n/locales/<code>.json` (texte français → traduction). `cargo test -p dragontree-i18n`
vérifie qu'aucun texte n'est sans traduction, qu'aucune traduction n'est orpheline et que les paramètres `{nom}`
concordent.

Ajouter une langue : une variante de `Lang` (code et nom) dans `crates/dragontree-i18n/src/lib.rs`, son catalogue
(copie de `en.json` à traduire) et une entrée dans `CATALOGS`. Les règles de pluriel sont dans `plural_form`.

## Comptes

Fenêtre **Comptes** (page d'accueil, réglages ou Ctrl+P) : choisissez le service, collez un jeton personnel
(PAT) — le lien « Créer un jeton » ouvre la bonne page — puis « Vérifier et ajouter ».

| Service | Droits recommandés |
| --- | --- |
| GitHub | `repo`, `read:org`, `workflow` |
| GitLab | `api`, `read_repository`, `write_repository` |
| Azure DevOps | Code (lecture et écriture), Profil utilisateur (lecture) ; « toutes les organisations accessibles » |

Pour Azure DevOps, si le jeton est limité à une organisation, indiquez-la dans le champ « Organisation(s) »
(DragonTree propose celles trouvées dans les remotes de vos dépôts).

Sécurité : les jetons sont stockés dans le **Gestionnaire d'identifiants Windows** (service « DragonTree »), jamais
dans les réglages ni dans les URL. Pour un clone fait depuis un compte, puis pour les fetch/pull/push de ce
dépôt, DragonTree transmet l'authentification à git par variables d'environnement (`http.<origine>.extraHeader`) :
rien n'est écrit dans `.git/config`. Les autres dépôts utilisent votre configuration git habituelle, sauf si vous leur
associez un compte : **Dépôt › Utiliser le compte…** (ou les réglages du dépôt) propose d'abord les comptes qui
correspondent à un remote, **Dépôt › Ne plus utiliser de compte** retire l'association. Le compte associé est affiché
dans la barre d'outils de l'onglet ; un worktree lié sans compte propre utilise celui du dépôt principal.

## Architecture

```text
crates/dragontree-i18n    traduction de l'interface : langue courante, catalogues, macros tr!/trn!
crates/dragontree-git     moteur : exécution de git.exe (annulation par Job Object), parseurs (-z, porcelain v2),
                          graphe, patchs partiels, rebase interactif, conflits, blame, découverte des dépôts
crates/dragontree-forges  comptes GitHub/GitLab/Azure DevOps : API REST, jetons (keyring), auth git
crates/dragontree-app     interface egui : onglets, liste des dépôts, vues, dialogues, fenêtres
```

Aucune commande git ne tourne sur le thread de l'interface : lectures en parallèle, écritures sérialisées par
dépôt (pas de collision sur `index.lock`), résultats périmés ignorés.

## Tests

```powershell
cargo test --workspace
```

870 tests : parseurs, graphe, patchs appliqués par un vrai `git apply`, opérations sur des dépôts
temporaires (y compris remotes locaux), API des forges sur des réponses JSON représentatives, et parcours
d'interface de bout en bout (egui_kittest : indexer puis commiter, indexer un hunk, ouvrir un dépôt depuis la
liste, créer une branche, résoudre un conflit, rebase interactif, stash/pop, annulation, menus, worktrees, bisect,
Git Flow, LFS, commandes personnalisées, pull requests de tous les comptes).
Tests réels facultatifs (ignorés par défaut) : `cargo test -- --ignored` avec les variables décrites en tête
des fichiers `tests/live*.rs`.

## Limites connues et suite

- Vue des pull requests : Azure DevOps ne donne pas de date de mise à jour (la date affichée est celle de création) ;
  500 résultats au plus par recherche GitHub et 1 000 par liste GitLab ou Azure DevOps ; sur GitLab, « À relire » et
  « Relecture demandée » veulent dire « relecteur désigné » : GitLab garde les relecteurs après leur relecture et sa
  liste ne donne pas l'état de chaque relecture, donc une merge request déjà relue y reste. Les remotes des clones
  locaux sont lus dans `.git/config` sans suivre `include.path` ni `url.<base>.insteadOf`, ni les valeurs continuées
  sur plusieurs lignes ou un fichier qui n'est pas en UTF-8 ; un remote qui passe par un alias d'hôte SSH
  (`~/.ssh/config`) n'est jamais relié à sa pull request.
- Dépôts SHA-256 non pris en charge.
