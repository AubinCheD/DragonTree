# DragonTree

Client git graphique natif pour Windows, écrit entièrement en Rust (interface [egui](https://github.com/emilk/egui),
rendu OpenGL « glow »). Le moteur git est le `git.exe` installé sur la machine : vos credentials (Git Credential
Manager, clés SSH), hooks, signatures et configuration s'appliquent tels quels.

## Fonctionnalités

- **Onglets** : un onglet par dépôt, créés à la demande (Ctrl+T, Ctrl+W, Ctrl+Tab, Ctrl+1…9, glisser pour
  réordonner, clic molette pour fermer). Les onglets ouverts sont restaurés au démarrage.
- **Liste des dépôts à droite** : choisissez un ou plusieurs dossiers racines (ex. `DEV`). DragonTree y trouve
  tout seul tous les dépôts (scan parallèle, `node_modules`/`bin`/`obj`… ignorés) et les liste avec leur
  branche, le nombre de fichiers modifiés et l'avance/retard sur le remote. Recherche floue, dépôts épinglés,
  dépôts masqués, signalement des dépôts refusés par `safe.directory` (avec « Faire confiance à ce dépôt »).
- **Historique** : graphe des branches, étiquettes (branches locales/distantes, tags, HEAD), recherche par
  message/auteur/SHA, toutes les branches ou la branche courante seule, détails du commit, fichiers modifiés,
  diff unifié ou côte à côte avec coloration syntaxique et surlignage des caractères modifiés, aperçu
  avant/après des images.
- **Changements locaux** : fichiers non indexés / indexés / en conflit, indexation par fichier, par **hunk** ou
  par **ligne** (y compris fichiers non suivis et CRLF), annulation de modifications, « Ignorer » dans le
  `.gitignore`, zone de commit (Amend, pousser après le commit, Ctrl+Entrée).
- **Opérations** : fetch, pull (merge ou rebase), push (upstream, force-with-lease, tags), branches (créer,
  renommer, supprimer localement et sur le remote, checkout, fast-forward), tags (annotés, push, suppression),
  merge (ff, no-ff, ff-only, squash), rebase, **rebase interactif** (réordonner, pick/reword/edit/squash/
  fixup/drop), cherry-pick, revert, reset (soft/mixed/hard), stash (avec message, non suivis, garder l'index,
  choix du stash au pop, renommage, fichiers sélectionnés seulement, snapshot),
  remotes, sous-modules, **worktrees** (section de la barre latérale : ouvrir dans un onglet, créer depuis une
  branche, une branche distante ou un commit, supprimer, nettoyer ; les branches sorties ailleurs sont signalées).
- **Conflits** : bandeau d'opération en cours (Continuer/Annuler/Passer), résolution « notre/leur version »,
  éditeur de conflits par région, outil de merge externe (`git mergetool`).
- **Historique et blame d'un fichier.**
- **Comptes GitHub, GitLab et Azure DevOps**, plusieurs à la fois (GitHub Enterprise, GitLab auto-hébergé et
  Azure DevOps Server compris) ; **clonage** depuis la liste agrégée de vos dépôts ou depuis une URL.
- Surveillance du dossier (rafraîchissement automatique), fetch automatique silencieux, palette **Ctrl+P**
  (dépôts, branches, commandes), journal des commandes git, thèmes clair/sombre.
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

L'exécutable est `target\release\dragontree.exe` (environ 16 Mo, aucune dépendance à installer).

## Utilisation

```text
dragontree.exe [DÉPÔT…] [--open <dossier>] [--add-root <dossier>] [--theme light|dark] [--lang fr|en]
               [--view history|changes] [--profile <dossier>] [--screenshot <png> --after-ms <n>]
```

`--profile` isole réglages, cache et état de fenêtre (pratique pour tester sans toucher à votre profil).
Réglages : `%APPDATA%\DragonTree\settings.json` ; cache des dépôts : `%LOCALAPPDATA%\DragonTree\`.

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
| Ctrl+= / Ctrl+- | Zoom avant / arrière (taille de l'interface, enregistrée) |
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
rien n'est écrit dans `.git/config`. Les autres dépôts utilisent votre configuration git habituelle.

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

Environ 240 tests : parseurs, graphe, patchs appliqués par un vrai `git apply`, opérations sur des dépôts
temporaires (y compris remotes locaux), API des forges sur des réponses JSON représentatives, et parcours
d'interface de bout en bout (egui_kittest : indexer puis commiter, indexer un hunk, ouvrir un dépôt depuis la
liste, créer une branche, résoudre un conflit, rebase interactif, stash/pop, annulation).
Tests réels facultatifs (ignorés par défaut) : `cargo test -- --ignored` avec les variables décrites en tête
des fichiers `tests/live*.rs`.

## Limites connues et suite

- **À venir** : vue centralisée des pull requests de tous les comptes (l'architecture des forges est prête).
- Non gérés pour l'instant : Git LFS (au-delà de ce que fait git lui-même), git-flow, commandes personnalisées,
  icône intégrée à l'exécutable.
- Dépôts SHA-256 non pris en charge.
