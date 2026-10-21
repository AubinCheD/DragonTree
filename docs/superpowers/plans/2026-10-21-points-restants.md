# Plan — points restants B / T / N / RV / P

> **Pour les agents d'exécution :** ce plan s'exécute tâche par tâche (superpowers:subagent-driven-development).
> Chaque tâche produit **un commit** ; les corrections après revue sont des **commits ordinaires** (pas de
> `fixup!`, pas de réécriture d'historique : l'utilisateur veut conserver l'historique).

**Objectif :** traiter tous les points encore ouverts des sections B, T, N, RV et P de `docs/feuille-de-route.md`
(« Reste à faire » et « Détail des points restants ») : B2, B3, B4, B7, T3, T4, T5 (a à d), T6, N1, N3, RV1 à RV5,
P1 à P4, et les pistes L1 à L6 (ajoutées à la demande de l'utilisateur le 2026-10-21).

**Spécification :** `docs/feuille-de-route.md`, section **Détail des points restants** : pour chaque point, les
paragraphes « État », « À faire » et « Critère de fin » sont les exigences. En cas de conflit, la feuille de route
fait autorité, sauf décision notée ici.

**Architecture :** workspace Rust (edition 2024, egui/eframe 0.37, glow) : `crates/dragontree-git` (moteur),
`crates/dragontree-forges` (API GitHub/GitLab/Azure DevOps), `crates/dragontree-i18n` (traduction),
`crates/dragontree-app` (interface ; registre des commandes `commands.rs`, onglet de dépôt `repo/`).

---

## Contraintes globales

1. **Un commit par tâche**, message en français : `<ID> — <Verbe au présent> …` (ex. `B2 — Supprime le dossier
   de rebase après un rebase arrêté puis terminé`), corps facultatif, puis exactement ces deux lignes :
   ```
   Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
   Claude-Session: https://claude.ai/code/session_01Qp9mPXaCLhbdU9D2TH2bic
   ```
   (à recopier telles quelles, quel que soit le modèle). Identité git : celle du dépôt. Jamais de `push`, jamais de
   `--amend`, de `rebase` ni de `fixup!`. **Corrections après revue :** commit ordinaire
   `<ID> — Corrige … (revue)` avec les mêmes deux lignes.
2. **Avant chaque commit :** `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`,
   `cargo test --workspace` — tout vert, sortie propre. Pendant le travail, tests ciblés seulement.
3. **Traduction :** texte visible en français dans le code (`tr!`, `trn!`, `tr_mark!`), traduction anglaise dans
   `crates/dragontree-i18n/locales/en.json` à sa place triée (ordre insensible à la casse et aux accents, clés
   `{…}` en fin de fichier). Jamais de texte traduit dans la logique.
4. **Aucune commande git, aucun accès disque ou réseau notable sur le thread de l'interface** : `jobs.spawn` avec
   numéro de génération, écritures par la file de l'onglet.
5. **Tests :** moteur sur dépôts temporaires (`crates/dragontree-git/tests/`), interface avec egui_kittest dans des
   fichiers `crates/dragontree-app/tests/ui_<domaine>.rs` et les helpers de `tests/common/mod.rs` (`click_label`,
   `click_by`, `wait_until`… : attentes sur conditions, jamais de `sleep` de synchronisation, requêtes ciblées).
   Test vu rouge avant le code quand c'est possible.
6. **Jamais la configuration réelle de l'utilisateur dans les tests** (`~/.gitconfig` → helper
   `isolate_global_git_config` de `tests/common` : `GIT_CONFIG_GLOBAL` + `HOME` + `USERPROFILE` ; `~/.ssh` ;
   `%APPDATA%\DragonTree` → `profile_dir` temporaire ; Gestionnaire d'identifiants ; réseau → serveur local
   127.0.0.1 ou fixtures ; tests réels en `#[ignore]`).
7. **Confidentialité :** aucun nom réel d'entreprise ou de personne (noms fictifs : contoso, fabrikam, northwind),
   aucun chemin absolu de la machine dans les fichiers versionnés.
8. **Style :** code voisin, commentaires en français, `rustfmt.toml` (largeur 120). Pas de dépendance nouvelle sans
   nécessité (la déclarer dans `[workspace.dependencies]`).
9. **Réglages :** nouveau champ de `Settings` avec valeur par défaut ; ancien `settings.json` toujours lisible ; si la
   fenêtre Réglages l'édite, l'ajouter à `take_edits` (`settings_ui.rs`).
10. **Actions destructrices :** dialogue marqué destructeur (Entrée ne valide pas). Pas de `///` sur les variantes de
    l'enum `Dialog` (rustfmt l'étale) : documenter dans la doc de l'enum.
11. **Menus :** toute action utilisable sans élément cliqué passe par le registre `commands.rs` (menu, palette,
    raccourci éventuel) ; les actions liées à un élément restent dans les menus contextuels.
12. **Git ≥ 2.31** (README) : pas d'option plus récente sans repli.
13. **Documentation dans le même commit :** dans `docs/feuille-de-route.md`, le point traité passe à « (fait) » ou
    « corrigé » dans le tableau « Reste à faire » et dans son détail (État → réalisé, Critère de fin → atteint) ;
    README si un comportement visible ou un raccourci change.

## Décisions (rulings du contrôleur)

- **Historique :** pas de squash ni de fixup ; les corrections de revue sont des commits ordinaires.
- **T3 :** la demande « voir ses stashs » est couverte par V7 (stashs dans le graphe) et R4 ; reste à placer la
  section Stashes avant Remotes et Tags dans la barre latérale et à vérifier le critère (stash visible sans
  défilement avec beaucoup de branches distantes : test d'interface avec quelques centaines de branches).
- **T4 :** mémoriser par dépôt le choix « branche courante seulement » (pas le filtre V3, qui reste de session).
- **RV4a (processus git à la fermeture) :** à la fermeture, si des opérations non silencieuses tournent, demander
  confirmation (« Attendre / Quitter quand même ») ; en quittant, annuler les jetons et activer
  `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` pour qu'aucun processus git ne survive ; les opérations silencieuses (fetch
  automatique) sont arrêtées sans demander.
- **RV4b (instances multiples) :** instance unique par profil, avec transmission des arguments (`--open`, dossiers)
  à l'instance déjà ouverte, qui les ouvre et passe au premier plan ; `--profile <dossier>` donne une instance
  distincte (tests).
- **RV4c (proxy) :** le client HTTP des forges utilise la configuration proxy de Windows quand aucune variable
  d'environnement de proxy n'est définie.
- **RV5a (`--lang`, `--theme`) :** deviennent temporaires (non enregistrés) ; l'aide le dit.
- **RV5c (textes figés) :** documenter la limite (README ou commentaire) ; pas de retraduction des notifications déjà
  affichées.
- **P1 :** ajouter `LICENSE` (MIT, « Copyright (c) 2026 DragonTree contributors ») ; vérifier le nom par recherche
  web (domaines, registres de marques INPI/EUIPO/USPTO consultables en ligne, winget, Scoop, crates.io) et consigner
  les résultats dans la feuille de route — aucune démarche d'achat ou de dépôt.
- **N1 :** vue en lecture (liste agrégée, filtres, ouverture dans le navigateur, lien vers le dépôt local, rafraîchis-
  sement en arrière-plan) ; la création de PR existe déjà (R18).
- **N3 :** icône générée à partir du dessin actuel (`main.rs` `window_icon`), fichier `.ico` versionné dans
  `assets/`, intégré à l'exécutable pour les toolchains GNU et MSVC.

---

## Tâches

### Task 1: Test du catalogue insensible aux fins de ligne

Le test `every_text_has_a_translation_and_every_translation_is_used` (`crates/dragontree-i18n/tests/catalog.rs`)
lit les sources ; avec `core.autocrlf=true`, un checkout écrit les `.rs` en CRLF et les clés multilignes (aide de
`main.rs`) ne correspondent plus (rustc, lui, normalise CRLF en LF). Normaliser `\r\n` en `\n` à la lecture des
sources dans le test (et partout où le test extrait des clés). Test unitaire du extracteur avec une source CRLF.
Vérifier que `cargo test --workspace` passe avec les fichiers de travail actuels (CRLF).
**Commit :** `Rend le test du catalogue insensible aux fins de ligne CRLF`

### Task 2: B2 — Dossier de rebase laissé après un rebase arrêté
Spécification : feuille de route, B2. **Commit :** `B2 — …`

### Task 3: B3 — Décocher « Amend » trop vite écrase le message
Spécification : feuille de route, B3. **Commit :** `B3 — …`

### Task 4: B4 — Compte enregistré avec le jeton vérifié
Spécification : feuille de route, B4. **Commit :** `B4 — …`

### Task 5: B7 — Menu contextuel d'onglet lié à l'identifiant de l'onglet
Spécification : feuille de route, B7 (H2 a déjà fait passer `SetColor` par l'identifiant ; généraliser).
**Commit :** `B7 — …`

### Task 6: T3 — Stashs plus visibles
Spécification : feuille de route, T3 + décision ci-dessus. **Commit :** `T3 — …`

### Task 7: T4 — Mémoriser le filtre « branche courante » par dépôt
Spécification : feuille de route, T4 + décision ci-dessus. **Commit :** `T4 — …`

### Task 8: T5a — Glisser-déposer entre « Non indexés » et « Indexés »
Spécification : feuille de route, T5 a. Plusieurs fichiers sélectionnés glissent ensemble ; ne pas casser clic,
Maj+clic, Ctrl+clic, double-clic, menus contextuels ; fichiers en conflit exclus. **Commit :** `T5 — Permet de
glisser des fichiers entre les listes Non indexés et Indexés`

### Task 9: T5b — Colonne du graphe redimensionnable
Spécification : feuille de route, T5 b (poignée, largeur mémorisée ; repli V8 et filtres pris en compte).
**Commit :** `T5 — Rend la colonne du graphe redimensionnable`

### Task 10: T5c — Colonne « mise à jour » dans la liste de clonage
Spécification : feuille de route, T5 c (date relative ; Azure DevOps : date du dernier push si l'API la fournit,
sinon colonne vide). **Commit :** `T5 — Affiche la date de mise à jour dans la liste de clonage`

### Task 11: T5d — Aperçu du résultat dans l'éditeur de conflits
Spécification : feuille de route, T5 d. **Commit :** `T5 — Affiche l'aperçu du résultat dans l'éditeur de conflits`

### Task 12: T6 — Associer un compte à un dépôt existant
Spécification : feuille de route, T6 (entrée « Utiliser le compte… » / « Ne plus utiliser de compte » dans le menu
Dépôt et dans les réglages du dépôt R8 ; compte affiché dans l'onglet). **Commit :** `T6 — …`

### Task 13: N3 — Icône intégrée à l'exécutable
Spécification : feuille de route, N3 + décision ci-dessus. Test : l'exécutable construit contient une ressource
d'icône (lecture des ressources PE dans un test, ou vérification dans `build.rs`). Le script
`scripts/setup-toolchain.ps1` est mis à jour si un outil supplémentaire est nécessaire (empreintes vérifiées).
**Commit :** `N3 — Intègre l'icône à l'exécutable`

### Task 14: N1 — Vue centralisée des pull requests
Spécification : feuille de route, N1 + décision ci-dessus. Fixtures JSON par forge, test réel `#[ignore]`, test
d'interface de la liste (comptes factices, serveur local ou injection des résultats). **Commit :** `N1 — …`

### Task 15: RV1a — Filtre de l'historique hors du thread de l'interface, sans copies par image
Spécification : feuille de route, RV1 (points 1 et 2). **Commit :** `RV1 — …`

### Task 16: RV1b — Coloration syntaxique en arrière-plan avec cache
Spécification : feuille de route, RV1 (points 3 et 4 ; cache LRU indexé par le contenu ; seuil revu). **Commit :**
`RV1 — …`

### Task 17: RV2 — Gros diffs : arrêter la lecture à la limite
Spécification : feuille de route, RV2. **Commit :** `RV2 — …`

### Task 18: RV3 — Files d'opérations : réseau silencieux à part, une file par dépôt
Spécification : feuille de route, RV3. **Commit :** `RV3 — …`

### Task 19: RV4a — Processus git à la fermeture
Spécification : feuille de route, RV4 a + décision ci-dessus. **Commit :** `RV4 — …`

### Task 20: RV4b — Instance unique
Spécification : feuille de route, RV4 b + décision ci-dessus. **Commit :** `RV4 — …`

### Task 21: RV4c — Proxy de Windows pour les forges
Spécification : feuille de route, RV4 c + décision ci-dessus. **Commit :** `RV4 — …`

### Task 22: RV4d — Cas des forges
Spécification : feuille de route, RV4 d (visibilité GitLab avec `simple=true`, instances GitLab en HTTP, « privé »
Azure DevOps, casse de l'organisation Azure dans l'en-tête d'authentification). Chaque point corrigé et testé, ou
documenté comme limite. **Commit :** `RV4 — …`

### Task 23: RV5a — `--lang` et `--theme` temporaires
Spécification : feuille de route, RV5 + décision ci-dessus. **Commit :** `RV5 — …`

### Task 24: RV5b — État « occupé » par type d'opération, textes figés documentés
Spécification : feuille de route, RV5 (bouton Push occupé, identification des opérations par un type) + décision
RV5c. **Commit :** `RV5 — …`

### Task 25: P1 — Publication open source
Spécification : feuille de route, P1 + décision ci-dessus (LICENSE, vérification du nom consignée, nombre de tests
du README à jour ; N3 est la tâche 13). **Commit :** `P1 — …`

### Task 26: P2 — Cases des plans
Spécification : feuille de route, P2 : signaler en tête de `docs/superpowers/plans/2026-10-16-dragontree.md` et de
`2026-10-19-options-fork.md` qu'ils sont historiques et réalisés, et renvoyer à la feuille de route (ou cocher les
cases). **Commit :** `P2 — …`

### Task 27: P3 — Stabilité de `ui_network`
Spécification : feuille de route, P3 (20 passages sur 20 ; corriger les attentes si un passage échoue).
**Commit :** `P3 — …` (si rien n'est à corriger : mettre à jour la feuille de route avec le résultat).

### Task 28: P4 — `--help` depuis une console Windows
Spécification : feuille de route, P4 (`AttachConsole(ATTACH_PARENT_PROCESS)` en build release avant d'écrire ;
tester ce qui est testable : construction, appel sans console). **Commit :** `P4 — …`

### Task 29: L1 — Dépôts SHA-256
Spécification : feuille de route, L1. Au minimum le critère de fin (erreur explicite à l'ouverture d'un dépôt
`--object-format=sha256`, sans panique, test d'intégration) ; la prise en charge des identifiants de 32 octets
seulement si elle reste raisonnable (sinon la documenter comme limite). **Commit :** `L1 — …`

### Task 30: L2 — Éditeur de conflits pour les fichiers Windows-1252
Spécification : feuille de route, L2. **Commit :** `L2 — …`

### Task 31: L3 — Réglages illisibles au démarrage
Spécification : feuille de route, L3 (retenter la lecture plus tard ; prévenir à chaque modification non
enregistrée ; ne jamais écraser un fichier non relu). **Commit :** `L3 — …`

### Task 32: L4 — Échec d'authentification du fetch automatique visible en permanence
Spécification : feuille de route, L4 (indicateur persistant dans l'onglet, effacé au prochain succès).
**Commit :** `L4 — …`

### Task 33: L5 — Amend d'un commit déjà poussé avec « Pousser après »
Spécification : feuille de route, L5 (dialogue de confirmation proposant un push forcé `--force-with-lease`).
**Commit :** `L5 — …`

### Task 34: L6 — Indexation partielle avec l'option « Espaces »
Spécification : feuille de route, L6 (le diff affiché ignore les espaces, le patch d'indexation est construit sur le
diff complet). **Commit :** `L6 — …`

### Task 35: Synthèse de la documentation
README (fonctionnalités, limites, nombre de tests) et feuille de route (« Reste à faire » : seul ce qui aurait été
écarté ou documenté comme limite reste ouvert ; paragraphe daté de la branche `points-restants`). **Commit :** `Met à jour le
README et la feuille de route après les points restants`
