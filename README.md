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
  aperçu des fichiers concernés), zone de commit (Amend, pousser après le commit, avec un push forcé `--force-with-lease`
  proposé quand Amend modifie un commit déjà poussé, Ctrl+Entrée) avec indicateur de
  longueur 50/72, colonne guide, messages récents et Co-authored-by. Affichage des fichiers ignorés ou masquage des non
  suivis ; remise d'un fichier dans son état à un commit donné.
- **Opérations** : fetch, pull (merge ou rebase), push (upstream, force-with-lease, tags), branches (créer,
  renommer, supprimer localement et sur le remote, checkout, fast-forward), tags (annotés, push, suppression),
  merge (ff, no-ff, ff-only, squash), rebase, **rebase interactif** (réordonner, pick/reword/edit/squash/
  fixup/drop), cherry-pick, revert, reset (soft/mixed/hard), stash (avec message, non suivis, garder l'index,
  choix du stash au pop, renommage, fichiers sélectionnés seulement, snapshot ; la section Stashes de la barre latérale
  suit les branches, avant les remotes et les tags, et les stashs sont aussi dans le graphe),
  mise à jour, push ou suppression de **plusieurs branches à la fois** (branches périmées sélectionnables),
  Fetch / Pull / Push rapides sans dialogue (Ctrl+clic), remotes (modifier l'URL, tester la connexion),
  sous-modules (ajouter, déplacer, changer l'URL, supprimer), **pull requests** (créer sur GitHub, GitLab, Azure DevOps),
  **worktrees** (section de la barre latérale : ouvrir dans un onglet, créer depuis une
  branche, une branche distante ou un commit, supprimer, nettoyer ; les branches sorties ailleurs sont signalées).
- **Conflits** : bandeau d'opération en cours (Continuer/Annuler/Passer), résolution « notre/leur version »,
  éditeur de conflits par région (avec aperçu du résultat ; fichiers UTF-8 ou Windows-1252, affichés décodés et
  enregistrés avec leurs octets d'origine), outil de merge externe (`git mergetool`).
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
- **Gros dépôts et gros diffs** : le filtre de l'historique et la coloration syntaxique tournent en arrière-plan (la
  coloration est mise en cache et s'arrête au-delà de 20 000 lignes) ; un diff de plus de 8 Mio est tronqué et git est
  arrêté à cette limite, sans que la mémoire monte avec la taille du fichier.
- **Affichage du diff** : police, taille, largeur de tabulation, espaces visibles ou ignorés (espaces ignorés :
  l'indexation par hunk ou par ligne reste possible ; un hunk entier emporte les changements d'espaces de ses lignes,
  pas un changement de mode, signalé ; diff en lecture seule si le diff complet du fichier ne peut pas être lu en
  entier).
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
  fait pas attendre les écritures ; un fetch, pull ou push lancé à la main attend la fin de celui qui tourne, sans
  l'interrompre, et le dit ; en HTTP, il abandonne une connexion restée presque muette pendant une minute ; une
  authentification refusée est notifiée une fois, puis signalée par un avertissement à côté du bouton Fetch et sur
  l'onglet, avec l'erreur en infobulle, jusqu'au prochain fetch de tous les remotes réussi ; en arrière-plan, git
  ne demande jamais d'identifiants : ni Gestionnaire d'identifiants interactif, ni programme « askpass »), palette
  **Ctrl+P** (dépôts, branches, commandes), journal des commandes git, thèmes clair/sombre. Les opérations d'un dépôt
  ouvert dans plusieurs onglets passent par une seule file, dans l'ordre des clics.
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

`--help` affiche l'aide sur la sortie d'erreur (code de sortie 2), aussi dans une console cmd ou PowerShell :
l'exécutable release est une application graphique, qui se rattache alors à la console pour écrire l'aide ou une
erreur d'option. Limite : tapé directement, cmd et PowerShell n'attendent pas la fin d'une application graphique ;
leur invite revient avant le texte (Entrée en redonne une). `start /wait dragontree --help` (cmd), un script `.cmd`
ou `Start-Process -NoNewWindow -Wait` (PowerShell) attendent. Dans un tube (Git Bash), rien ne change.

`--profile` isole réglages, cache et état de fenêtre (pratique pour tester sans toucher à votre profil).
Réglages : `%APPDATA%\DragonTree\settings.json` ; cache des dépôts : `%LOCALAPPDATA%\DragonTree\`.
Un fichier de réglages invalide est mis de côté (`settings.illisible-<horodatage>.json`). Illisible au démarrage
(verrouillé par un autre programme, ou invalide et impossible à mettre de côté), il n'est jamais écrasé : DragonTree
part des réglages par défaut, affiche « Réglages non enregistrés » en haut à droite dès qu'un réglage change (un clic
réessaie) et relit le fichier à chaque enregistrement. Relu, il reçoit les modifications de la session et garde ses
autres valeurs ; dans ses listes et ses réglages par dépôt, les ajouts et retraits de la session s'ajoutent aux siens.
`--theme` et `--lang` valent pour ce lancement seulement et ne sont pas enregistrés ; un thème ou une langue choisis
ensuite dans les menus ou dans Réglages › Apparence sont enregistrés et les remplacent. Leur valeur même est déjà
sélectionnée dans Réglages : pour l'enregistrer, la choisir dans le menu (Affichage › Thème, Configuration › Langue).

**Instance unique** : une seule fenêtre par profil. Lancer `dragontree.exe C:\dépôt` alors que DragonTree est ouvert
ouvre le dépôt dans la fenêtre existante, qui passe au premier plan (sans dépôt : elle passe seulement au premier
plan) ; les autres options de ce second lancement sont ignorées. `--profile <dossier>` donne une instance distincte
(le même dossier désigné par un nom court 8.3, une jonction ou un lecteur `subst` aussi), et une capture d'écran
(`--screenshot`) est toujours une instance à part, qui n'écrit jamais les réglages.

**Fermeture** : si des opérations (fetch, pull, push, commit, clonage, création d'une pull request…) attendent ou
tournent, fermer la fenêtre demande d'attendre leur fin (la fenêtre se ferme ensuite d'elle-même) ou de quitter quand
même (elles sont arrêtées, un clonage supprime son dossier partiel) ; le fetch automatique est arrêté sans question.
Attendre attend aussi ce qu'une opération enchaîne (le push de « Pousser après le commit ») ; si sa fin pose une
question (push forcé proposé après un amend), ou si un dépôt arrive d'un second lancement, la fermeture est
abandonnée et la fenêtre reste ouverte. Aucune opération git en cours ne survit à DragonTree ; ce que git
laisse tourner après une commande terminée (démon fsmonitor, éditeur que `git mergetool` a démarré) n'est pas
touché. Limite : un outil de diff encore ouvert par « Ouvrir dans l'outil de diff externe » (qui
n'est pas une opération du dépôt) est fermé avec DragonTree, sans question.

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
Limite : un changement de langue vaut pour les textes affichés ensuite ; ceux qui l'étaient déjà ne sont pas retraduits
(titre d'une opération en cours, notifications affichées, titre et texte d'un dialogue ouvert, certains messages
d'erreur internes, qui peuvent alors mêler les deux langues).

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

Les comptes sont **communs à toute la suite EyrieSuite** : ceux ajoutés ici valent aussi dans Eyrie et Owlery, et
réciproquement (liste dans `%APPDATA%\EyrieSuite\accounts.json`, voir `../SuperApp/docs/comptes.md`). Au premier
lancement, les comptes d'avant la suite y sont repris, avec leurs dépôts associés. Dans Eyrie, « Comptes… » ouvre la
page Comptes d'Eyrie.

Sécurité : les jetons sont stockés dans le **Gestionnaire d'identifiants Windows** (service « EyrieSuite » ; l'ancien
service « DragonTree » est encore relu), jamais dans les réglages ni dans les URL. Pour un clone fait depuis un compte, puis pour les fetch/pull/push de ce
dépôt, DragonTree transmet l'authentification à git par variables d'environnement (`http.<origine>.extraHeader`) :
rien n'est écrit dans `.git/config`. Les autres dépôts utilisent votre configuration git habituelle, sauf si vous leur
associez un compte : **Dépôt › Utiliser le compte…** (ou les réglages du dépôt) propose d'abord les comptes qui
correspondent à un remote, **Dépôt › Ne plus utiliser de compte** retire l'association. Le compte associé est affiché
dans la barre d'outils de l'onglet ; un worktree lié sans compte propre utilise celui du dépôt principal.

Proxy : les appels aux API des forges (vérification d'un compte, listes de dépôts et de pull requests) passent par le
proxy des variables d'environnement (`HTTPS_PROXY`, `HTTP_PROXY`, `ALL_PROXY`) s'il y en a un, avec les exclusions de
`NO_PROXY` ; sinon par celui de Windows (Paramètres › Réseau et Internet › Proxy) : détection automatique, script de
configuration (PAC) ou proxy manuel, avec la liste d'exclusions de Windows (`NO_PROXY` ne s'y applique pas). Ni les
proxys SOCKS ni ceux qui demandent une authentification (NTLM, Kerberos) ne sont pris en charge, et le `http.proxy` de
git n'est pas lu. git lui-même suit sa propre configuration.

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

1 091 tests (et 10 tests ignorés par défaut) : parseurs, graphe, patchs appliqués par un vrai `git apply`, opérations sur des dépôts
temporaires (y compris remotes locaux), API des forges sur des réponses JSON représentatives, et parcours
d'interface de bout en bout (egui_kittest : indexer puis commiter, indexer un hunk, ouvrir un dépôt depuis la
liste, créer une branche, résoudre un conflit, rebase interactif, stash/pop, annulation, menus, worktrees, bisect,
Git Flow, LFS, commandes personnalisées, pull requests de tous les comptes).
Les tests ne lisent jamais votre configuration git globale : les helpers communs font désigner à `GIT_CONFIG_GLOBAL`,
`HOME`, `USERPROFILE` et `XDG_CONFIG_HOME` un dossier temporaire avant toute commande git (la configuration système
de Git pour Windows reste lue).
Tests facultatifs (ignorés par défaut) : `cargo test -- --ignored` ; ceux qui joignent un vrai compte prennent les
variables décrites en tête des fichiers `tests/live*.rs`.

## Limites connues et suite

Ces limites sont des choix assumés ou des cas non traités ; chacune a son entrée dans `docs/feuille-de-route.md`, qui dit
aussi ce qui reste ouvert. Celles qui touchent une fonction sont aussi dans sa section (Utilisation, Langue, Comptes).

**Dépôts et git**

- Dépôts SHA-256 (`git init --object-format=sha256`) non pris en charge : leur ouverture, et leur clonage (le dossier
  cloné est alors supprimé), s'arrêtent sur un message qui le dit. DragonTree ne gère que des identifiants SHA-1 de
  20 octets, pas ceux de 32 octets de SHA-256 ; pour un tel dépôt, utilisez git en ligne de commande.
- Un refus d'authentification n'est reconnu que dans les messages anglais de git : le fetch automatique lance git en
  anglais pour cela, mais pour un fetch lancé à la main avec un git traduit, l'avertissement « Authentification
  refusée » peut manquer.
- Le fetch automatique abandonne en HTTP une connexion restée presque muette pendant une minute ; en SSH, cette limite
  de débit ne s'applique pas : un serveur muet l'occupe jusqu'à sa fin ou à la fermeture de l'application, et un fetch,
  pull ou push lancé à la main attend alors (« Annuler » y met fin).
- Deux opérations réseau lancées à la main depuis deux worktrees d'un même dépôt peuvent tourner ensemble ; git en fait
  échouer une (verrou de référence), sans dommage.
- Un outil de diff encore ouvert par « Ouvrir dans l'outil de diff externe » est fermé avec DragonTree, sans question.

**Comptes et forges**

- Forges en HTTP simple (GitLab, GitHub Enterprise ou Azure DevOps Server sans HTTPS) : refusées à l'ajout du compte,
  car DragonTree ne joint les forges qu'en HTTPS (le jeton ne circule jamais en clair) ; un compte déjà enregistré
  ainsi reste appelé en HTTPS. Un dépôt dont le remote est en `http://` garde les identifiants habituels de git :
  l'en-tête du compte ne vise que HTTPS.
- Proxy des forges : ni SOCKS, ni proxy à authentification (NTLM, Kerberos), ni `http.proxy` de git ; `NO_PROXY` ne
  vaut que pour les variables d'environnement, pas pour le proxy de Windows (voir Comptes).
- Vue des pull requests : Azure DevOps ne donne pas de date de mise à jour (la date affichée est celle de création) ;
  500 résultats au plus par recherche GitHub et 1 000 par liste GitLab ou Azure DevOps ; sur GitLab, « À relire » et
  « Relecture demandée » veulent dire « relecteur désigné » : GitLab garde les relecteurs après leur relecture et sa
  liste ne donne pas l'état de chaque relecture, donc une merge request déjà relue y reste. Les remotes des clones
  locaux sont lus dans `.git/config` sans suivre `include.path` ni `url.<base>.insteadOf`, ni les valeurs continuées
  sur plusieurs lignes ou un fichier qui n'est pas en UTF-8 ; un remote qui passe par un alias d'hôte SSH
  (`~/.ssh/config`) n'est jamais relié à sa pull request.

**Application**

- Après un changement de langue, les textes déjà affichés restent dans l'ancienne : titre d'une opération en cours,
  notifications, dialogue ouvert, certains messages d'erreur internes (voir Langue).
- Dans cmd ou PowerShell, l'invite revient avant le texte de `--help` (voir Utilisation).
- Instance unique par profil : un dossier de profil désigné par un nom court 8.3, une jonction ou un lecteur `subst`
  donne une autre instance, et une instance lancée avec les droits d'administrateur ne reconnaît pas celle qui est
  lancée sans (deux fenêtres).
- Réglages : la fenêtre Réglages ouverte pendant la relecture d'un fichier d'abord illisible remplace, à
  « Enregistrer », une liste qu'elle a modifiée (dossiers scannés, commandes…) par son brouillon, sans les entrées que
  le fichier relu y avait ajoutées.
- Déconnexion de Windows : DragonTree n'en est pas prévenu (winit 0.30 ne traite pas `WM_QUERYENDSESSION` ni
  `WM_ENDSESSION`), sa fermeture habituelle n'a donc pas lieu. Les réglages, écrits à chaque modification, ne sont pas
  perdus ; ce qui n'est écrit qu'à la fermeture ou à l'enregistrement périodique peut l'être : taille et position de
  la fenêtre, onglet actif.
- Icône de l'exécutable : vérifiée avec GNU ; avec MSVC, objet lié par lld-link, `link.exe` à confirmer.
- Nom du projet : aucun conflit trouvé pour un logiciel, mais l'EUIPO (TMview) et l'INPI n'ont pas pu être interrogés
  automatiquement : à vérifier à la main avant tout dépôt de marque.

## Licence

MIT, voir [`LICENSE`](LICENSE). La police d'icônes Phosphor (`assets/fonts/Phosphor.ttf`) a sa propre licence MIT :
`assets/fonts/LICENSE-Phosphor.txt`.
