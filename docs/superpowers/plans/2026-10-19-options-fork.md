# Plan — barre de menus et options de Fork (F / V / R / C / H)

> **Pour les agents d'exécution :** ce plan s'exécute tâche par tâche (superpowers:subagent-driven-development).
> Chaque tâche produit **exactement un commit**. Les étapes utilisent des cases à cocher (`- [ ]`).

**Objectif :** implémenter toutes les options de `docs/feuille-de-route.md` (section « Options de Fork pour les
menus File / View / Repo / Config ») qui ne sont pas encore terminées : la barre de menus elle-même, les options
« ✅ + raccourci » (V2, V13, R1) et toutes les options 🆕 (F2–F4, V3–V12, R2–R20, C2–C8, H1–H4).

**Spécification :** `docs/feuille-de-route.md` — tableaux des options et sections T1 (worktrees), T2 (barre de
menus), T3/T4 (stashs, filtre), Raccourcis. En cas de conflit, la feuille de route fait autorité, sauf décision
contraire notée ici (section « Décisions »).

**Architecture :** workspace Rust (edition 2024, egui/eframe 0.37 en rendu glow) :
`crates/dragontree-git` (moteur : `git.exe` + parseurs), `crates/dragontree-forges` (API GitHub/GitLab/Azure),
`crates/dragontree-i18n` (traduction, macros `tr!`, `trn!`, `tr_mark!`), `crates/dragontree-app` (interface).

---

## Contraintes globales

Elles s'appliquent à **toutes** les tâches.

1. **Un commit par tâche.** Message en français : `<ID> — <Verbe au présent> …` (ex. `F2 — Rouvre le dernier
   onglet fermé (Ctrl+Maj+T)`), corps facultatif, puis exactement ces deux lignes finales :
   ```
   Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
   Claude-Session: https://claude.ai/code/session_01Qp9mPXaCLhbdU9D2TH2bic
   ```
   Identité git : celle déjà configurée dans le dépôt (ne pas la changer). Ne jamais `push`, ne jamais réécrire un
   commit existant (`--amend`, `rebase`). **Corrections après revue :** `git commit --fixup=<sha du commit de la
   tâche>` (le contrôleur les fusionnera à la fin).
2. **Vérifications avant chaque commit** (toutes vertes, sortie sans avertissement) :
   `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`.
   Pendant le travail, lancer seulement les tests ciblés.
3. **Traduction.** Tout texte visible est écrit en français dans le code (`tr!`, `trn!`, `tr_mark!`/`tr_str`)
   et sa traduction anglaise est ajoutée dans `crates/dragontree-i18n/locales/en.json` (clés triées comme le
   fichier existant). `cargo test -p dragontree-i18n` doit passer (aucun texte manquant ni orphelin, mêmes
   paramètres `{nom}`). Ne jamais utiliser un texte traduit dans la logique (comparaison, identifiant).
4. **Aucune commande git sur le thread de l'interface.** Lectures : `shared.jobs.spawn(...)` avec un numéro de
   génération pour ignorer les résultats périmés (voir `Gens` dans `repo/mod.rs`). Écritures : la file série de
   l'onglet (`run_simple`, `op_*` dans `repo/actions.rs`).
5. **Tests.**
   - Moteur : tests d'intégration sur des dépôts temporaires dans `crates/dragontree-git/tests/` (helpers de
     `tests/common/mod.rs`).
   - Interface : tests egui_kittest dans des **nouveaux** fichiers `crates/dragontree-app/tests/ui_<domaine>.rs`
     (pas dans `ui_flows.rs`), avec les helpers partagés `crates/dragontree-app/tests/common/mod.rs` (créés par
     la tâche 1). Libellés cherchés en français (`lang: Some(Lang::Fr)`), toujours un `profile_dir` temporaire.
   - Logique pure (construction de commande, analyse, calcul) : tests unitaires `#[cfg(test)]` dans le module.
   - Un test doit échouer sans la fonctionnalité (TDD : le voir rouge, puis vert).
6. **Ne jamais toucher à la configuration réelle de l'utilisateur dans les tests** : pas d'écriture dans
   `~/.gitconfig` (utiliser `GIT_CONFIG_GLOBAL` vers un fichier temporaire), `~/.ssh`, `%APPDATA%\DragonTree`,
   le Gestionnaire d'identifiants, ni d'appel réseau réel (les tests réels éventuels sont `#[ignore]`).
7. **Confidentialité.** Aucun nom réel d'entreprise ou de personne dans le code, les tests, les fixtures ou les
   messages : noms fictifs (contoso, fabrikam, northwind). Aucun chemin absolu de la machine dans les fichiers
   versionnés.
8. **Style.** Suivre le code voisin : commentaires en français, même densité, noms explicites, `rustfmt.toml`
   (largeur 120). Pas de nouvelle dépendance sans nécessité ; si besoin, la déclarer dans
   `[workspace.dependencies]`.
9. **Réglages.** Tout nouveau réglage persistant est un champ de `Settings` (`crates/dragontree-app/src/settings.rs`)
   avec une valeur par défaut dans `impl Default` ; un ancien `settings.json` doit toujours se charger. Un réglage
   propre à un dépôt est indexé par `path_key(...)`.
10. **Actions destructrices** : dialogue de confirmation marqué destructeur (Entrée ne valide pas, voir
    `Dialog::title_and_ok` dans `repo/dialogs.rs`).
11. **Menus et raccourcis.** Chaque nouvelle action est déclarée dans le registre de commandes (tâche 1) : elle
    apparaît dans la barre de menus (et la palette Ctrl+P si elle a un sens hors contexte), avec son raccourci
    affiché. Un raccourci n'est déclaré **que** dans le registre. Les menus contextuels existants reçoivent
    l'action quand elle porte sur l'élément cliqué.
12. **Compatibilité git ≥ 2.31** (README) : pas d'option git plus récente sans repli (ex. `worktree list -z`
    exige 2.36 : ne pas l'utiliser).
13. **Documentation dans le même commit.**
    - `docs/feuille-de-route.md` : dans le tableau de l'option, la colonne État passe à `✅` (pour V2, V13 et R1 :
      `✅` sans « + raccourci »). Ne pas réécrire le reste du document.
    - `README.md` : tableau « Raccourcis » si la tâche ajoute ou change un raccourci.

## Décisions (rulings du contrôleur)

- **Raccourcis en conflit** : on applique la règle de la feuille de route (« l'actuel est gardé »).
  Ctrl+Maj+L reste le journal git ; Ctrl+Maj+C reste Cloner ; Ctrl+1…9 change d'onglet. **Pull prend
  Ctrl+Maj+U.** Fetch Ctrl+Maj+F, Push Ctrl+Maj+P, Nouvelle branche Ctrl+Maj+B, Stash Ctrl+Maj+H, filtre branche
  active Ctrl+Maj+A, Révéler HEAD Ctrl+0, panneau de détails Ctrl+Maj+D, zoom Ctrl+= / Ctrl+-, réglages Ctrl+,,
  rouvrir l'onglet fermé Ctrl+Maj+T, commits perdus Ctrl+Maj+. (point).
- **Menus** : « Fichier », « Affichage », « Dépôt », « Configuration » (anglais : File, View, Repository,
  Configuration). La barre est au-dessus de la barre d'onglets.
- **R3 couvre tout T1** (lister, ouvrir, créer, supprimer les worktrees, branches sorties ailleurs) : créer un
  worktree sans pouvoir le voir n'aurait pas de sens.
- **Git Flow (R13)** est implémenté avec des commandes git ordinaires (pas de dépendance à l'extension git-flow),
  en lisant/écrivant les clés de configuration de git-flow (`gitflow.branch.master`, `gitflow.branch.develop`,
  `gitflow.prefix.*`) pour rester compatible.
- **Git LFS (R14)** : les tests s'arrêtent proprement (avec un message) si `git lfs version` échoue.
- **IA (C7)** : appel de la CLI Claude Code (`claude -p`), chemin réglable ; testée avec un faux exécutable.

## Architecture commune

### Registre de commandes (créé par la tâche 1)

Module `crates/dragontree-app/src/commands.rs` — la seule source des entrées de menu, des raccourcis et des
commandes de la palette :

```rust
/// Commande de l'interface : entrée de menu, raccourci éventuel, palette Ctrl+P.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Command { NewTab, OpenRepo, Clone, InitRepo, CloseTab, QuickOpen, Accounts, Settings, /* … */ }

pub enum Scope { App, Repo }          // Repo : grisée si l'onglet actif n'est pas un dépôt ouvert

impl Command {
    pub fn label(self) -> &'static str;                 // traduit (tr!)
    pub fn shortcut(self) -> Option<egui::KeyboardShortcut>;
    pub fn scope(self) -> Scope;
}

/// Contenu de la barre de menus : (titre du menu, entrées).
pub enum MenuItem { Cmd(Command), Separator, Submenu(&'static str, &'static [MenuItem]) }
```

- `DragonTreeApp` affiche la barre (`egui::MenuBar`) au-dessus des onglets ; un clic ou un raccourci produit la
  commande, traitée par `DragonTreeApp::run_command` (portée App) ou transmise à l'onglet actif
  (`RepoTab::run_command(cmd, shared)`, portée Repo). L'état coché (entrées à bascule) et l'état grisé sont
  calculés à l'affichage (`fn checked(...)`, `fn enabled(...)` ou équivalent).
- Les raccourcis sont consommés **les plus spécifiques d'abord** (egui fait correspondre Ctrl+T à Ctrl+Maj+T :
  `matches_logically` ignore un Maj en trop) ; un test unitaire vérifie l'ordre et l'absence de doublon.
- La palette Ctrl+P liste les commandes du registre au lieu de sa liste propre (`quick_open.rs`).
- Ajouter une option = ajouter une variante, son libellé, son raccourci éventuel, sa place dans `MENUS`, et son
  traitement.

### Portée de l'historique (à partir de la tâche 2)

Un seul point construit les options du journal : `RepoTab::log_options(&self, shared) -> LogOptions` (extrait
de `load_log`, `repo/mod.rs`). Les tâches qui changent ce qu'affiche le graphe (V2, V3, V4, V5, V6, V7, V11)
l'enrichissent ; le moteur (`dragontree-git/src/log.rs`) gagne les sélecteurs nécessaires avec leurs tests.

---

## Tâches

### Task 1: Barre de menus, registre de commandes et entrées existantes (T2, F1, V1, C1)

**Options :** T2 (barre), F1 (entrées + Ctrl+,), V1, C1 — les fonctions existent, il manque menus et raccourci.

**Comportement :**
- Barre de menus au-dessus de la barre d'onglets : **Fichier** (Nouvel onglet Ctrl+T, Ouvrir un dépôt… Ctrl+O,
  Cloner un dépôt… Ctrl+Maj+C, Initialiser un dépôt…, Fermer l'onglet Ctrl+W, Palette de commandes Ctrl+P,
  Comptes…), **Affichage** (Modifications locales, Tous les commits, Liste des dépôts Ctrl+B, Journal git
  Ctrl+Maj+L), **Dépôt** (Actualiser F5, Fetch, Pull…, Push…, Nouvelle branche…, Nouveau tag…, Stash…, Ouvrir
  dans l'Explorateur, Ouvrir un terminal, Ouvrir dans VS Code si disponible), **Configuration** (Réglages…
  Ctrl+,, sous-menu Langue : Système / Français / English avec la langue courante cochée, Comptes…).
- Entrées de portée Dépôt grisées quand l'onglet actif est l'accueil ou un dépôt pas encore ouvert.
- Libellés avec leur raccourci (`Button::shortcut_text` ou équivalent).
- La palette Ctrl+P liste les commandes depuis le registre (mêmes libellés qu'aujourd'hui ou ceux du registre).
- Raccourcis existants déplacés dans le registre (Ctrl+T, Ctrl+W, Ctrl+B, Ctrl+O, Ctrl+P, Ctrl+Maj+C, F5,
  Ctrl+Maj+L) ; Ctrl+Tab / Ctrl+Maj+Tab / Ctrl+1…9 restent dans `handle_shortcuts`. Nouveau : Ctrl+, (Réglages).
- `RepoTab::quick_command(&str)` est remplacé par `RepoTab::run_command(Command, &mut Shared)`.

**Fichiers :** créer `crates/dragontree-app/src/commands.rs` (registre, `MENUS`, tests unitaires) et
`crates/dragontree-app/src/menu_bar.rs` (affichage) ; modifier `lib.rs`, `app.rs`, `repo/mod.rs`,
`repo/toolbar.rs`, `quick_open.rs`, `locales/en.json`. Créer `crates/dragontree-app/tests/common/mod.rs`
(helpers partagés repris de `ui_flows.rs` : `git`, `try_git`, `init_repo`, `commit_file`, `demo_repo`,
`harness`, `start`, `wait_until`, `becomes_true`, `copied_text`, `has_text_input`) sans modifier les fichiers de
test existants, et `crates/dragontree-app/tests/ui_menus.rs`.

**Tests :**
- [ ] Unitaires : chaque commande a un libellé non vide ; aucun raccourci en double ; l'ordre de consommation met
  Ctrl+Maj+X avant Ctrl+X ; chaque commande du registre apparaît au moins une fois dans `MENUS` ou est
  explicitement hors menu.
- [ ] UI : ouvrir chaque menu (Fichier, Affichage, Dépôt, Configuration) et vérifier une entrée de chacun ;
  « Affichage › Modifications locales » bascule la vue d'un dépôt ouvert ; « Dépôt › Nouvelle branche… » ouvre le
  dialogue ; Ctrl+, ouvre les Réglages ; sur l'onglet d'accueil, « Dépôt › Fetch » est grisée ; « Configuration ›
  Langue › English » passe l'interface en anglais.
- [ ] Les tests existants (palette comprise) restent verts.

**Doc :** README : ligne Ctrl+, dans « Raccourcis ». Feuille de route : F1, V1 et C1 sont déjà ✅ ; dans la
section T2, ajouter une ligne d'état « barre de menus faite (registre `commands.rs`) ».

**Commit :** `T2 — Ajoute la barre de menus Fichier / Affichage / Dépôt / Configuration`

### Task 2: V2 — Filtre sur la branche active (Ctrl+Maj+A)

**Comportement :** entrée à bascule « Affichage › Branche courante seulement » (cochée selon l'état de l'onglet),
raccourci Ctrl+Maj+A ; même effet que le bouton « Toutes les branches / Branche courante » de l'historique
(`repo/history.rs:79-93`), qui reste. Le filtre suit les checkouts (déjà le cas : le journal est rechargé).

**Technique :** extraire `RepoTab::log_options(&self, shared: &Shared) -> LogOptions` depuis `load_log`
(`repo/mod.rs:400-449`) ; les deux appels de `repo.log` l'utilisent (avec `max_count` pour le premier lot).

**Tests :**
- [ ] UI (`ui_history_filters.rs`) : dépôt avec deux branches divergentes ; Ctrl+Maj+A masque le commit de
  l'autre branche et coche l'entrée ; un second Ctrl+Maj+A le réaffiche.

**Doc :** V2 → ✅ ; README : Ctrl+Maj+A.

**Commit :** `V2 — Filtre l'historique sur la branche active (Ctrl+Maj+A)`

### Task 3: V13 — Zoom (Ctrl+= / Ctrl+-) et thème dans le menu Affichage

**Comportement :** « Affichage › Zoom avant » (Ctrl+=, accepter aussi Ctrl+Plus du pavé), « Zoom arrière »
(Ctrl+-), « Taille par défaut » (sans raccourci : Ctrl+0 est réservé à V9) ; ils changent
`settings.ui_scale` (bornes et pas de la fenêtre Réglages) et l'enregistrent. Sous-menu « Thème » : Système /
Clair / Sombre (coché). Le zoom clavier intégré d'egui est désactivé (`zoom_with_keyboard = false`) pour que
seul le réglage persistant agisse.

**Tests :**
- [ ] Unitaires : calcul du pas et des bornes.
- [ ] UI : Ctrl+= augmente `ui_scale` enregistré dans le `settings.json` du profil ; « Thème › Sombre » applique
  le thème sombre (`ctx.style().visuals.dark_mode`).

**Doc :** V13 → ✅ ; README : Ctrl+= / Ctrl+-.

**Commit :** `V13 — Ajoute le zoom au clavier et le choix du thème dans le menu Affichage`

### Task 4: R1 — Raccourcis Fetch / Pull / Push / Nouvelle branche / Stash

**Comportement :** raccourcis du registre : Fetch Ctrl+Maj+F, Pull… Ctrl+Maj+U, Push… Ctrl+Maj+P, Nouvelle
branche… Ctrl+Maj+B, Stash… Ctrl+Maj+H ; actifs seulement sur un onglet de dépôt ouvert, avec les mêmes
conditions que les boutons de la barre d'outils (remote présent, sur une branche, modifications pour Stash). Les
infobulles de la barre d'outils mentionnent le raccourci.

**Tests :**
- [ ] UI (`ui_menus.rs` ou `ui_shortcuts.rs`) : chacun des cinq raccourcis ouvre le bon dialogue ou lance le
  fetch (dépôt avec remote local bare, comme `ui_network.rs`).

**Doc :** R1 → ✅ (retirer la mention du conflit), section Raccourcis de la feuille de route : noter que Pull a
pris Ctrl+Maj+U ; README.

**Commit :** `R1 — Ajoute les raccourcis Fetch, Pull, Push, Nouvelle branche et Stash`

### Task 5: F2 — Rouvrir le dernier onglet fermé (Ctrl+Maj+T)

**Comportement :** chaque fermeture d'un onglet de dépôt (Ctrl+W, croix, clic molette, « Fermer les autres »,
« Tout fermer ») empile son chemin (pile bornée, ~20, sans doublon consécutif). « Fichier › Rouvrir l'onglet
fermé » (Ctrl+Maj+T) rouvre le plus récent (à sa position d'origine si possible, sinon à la fin) ; grisé si la
pile est vide. Mémoire de session seulement.

**Tests :**
- [ ] Unitaires sur la pile.
- [ ] UI : ouvrir deux dépôts, fermer le premier, Ctrl+Maj+T le rouvre et l'active ; Ctrl+T ouvre toujours un
  nouvel onglet (pas de confusion entre les deux raccourcis).

**Commit :** `F2 — Rouvre le dernier onglet fermé (Ctrl+Maj+T)`

### Task 6: F4 — Quitter

**Comportement :** « Fichier › Quitter » ferme l'application proprement (`ViewportCommand::Close`, ce qui passe
par `save`). Pas de raccourci propre (Alt+F4 reste celui du système).

**Tests :**
- [ ] UI : cliquer « Quitter » émet `ViewportCommand::Close` (sortie de la harness).

**Commit :** `F4 — Ajoute « Quitter » au menu Fichier`

### Task 7: F3 — Gestion des clés SSH

**Comportement :** fenêtre « Clés SSH » (Fichier › Clés SSH…) :
- liste des clés du dossier `~/.ssh` (paires `x` / `x.pub`) : type, commentaire, empreinte (`ssh-keygen -lf`) ;
- « Générer une clé… » : type ed25519 (RSA 4096 en option), nom de fichier, commentaire (proposé : e-mail git
  global), phrase secrète facultative ; refuse d'écraser un fichier existant ;
- « Copier la clé publique » ;
- « Utiliser pour git » : écrit `core.sshCommand` (global, ou local au dépôt actif au choix) =
  `ssh -i "<clé>" -o IdentitiesOnly=yes` ; « Revenir à la configuration par défaut » retire la clé ; la clé
  utilisée est signalée dans la liste.
- `ssh-keygen` : celui de Git pour Windows (déduit du chemin de `git.exe` : `…\usr\bin\ssh-keygen.exe`), sinon le
  PATH.

**Technique :** moteur dans `dragontree-git` (nouveau module `ssh.rs`) avec le dossier `.ssh` en paramètre (testable) ;
écriture de la config via `git config` (respecte `GIT_CONFIG_GLOBAL`).

**Tests :**
- [ ] Moteur : générer une clé dans un dossier temporaire, la lister, lire son empreinte ; refus d'écraser.
- [ ] Moteur : `core.sshCommand` écrit puis retiré avec `GIT_CONFIG_GLOBAL` temporaire (jamais le vrai).
- [ ] Unitaire : citation correcte d'un chemin avec espaces dans `core.sshCommand`.
- [ ] UI : la fenêtre s'ouvre depuis le menu et liste une clé d'un dossier `.ssh` de test (dossier injecté via
  le profil ou une option de `StartOptions` réservée aux tests — ne jamais lire le vrai `~/.ssh` en test).

**Commit :** `F3 — Ajoute la gestion des clés SSH`

### Task 8: V9 — Révéler HEAD (Ctrl+0) et panneau de détails (Ctrl+Maj+D)

**Comportement :** « Affichage › Révéler HEAD » (Ctrl+0) : passe à l'historique, sélectionne le commit de HEAD
et le fait défiler dans la vue. « Affichage › Panneau de détails » (Ctrl+Maj+D, à bascule, réglage persistant) :
masque ou affiche le panneau de détails du commit ; la liste occupe alors toute la place.

**Tests :**
- [ ] UI : sélectionner un autre commit, Ctrl+0 resélectionne HEAD ; Ctrl+Maj+D masque le panneau (un libellé du
  panneau disparaît) et l'état est enregistré.

**Doc :** README.

**Commit :** `V9 — Ajoute « Révéler HEAD » et l'affichage du panneau de détails`

### Task 9: V11 — Tri des commits : topologique ou par date

**Comportement :** « Affichage › Tri des commits » : Par date / Topologique (coché), réglage persistant global ;
le changement recharge le journal des onglets ouverts. Le moteur sait déjà faire (`LogOrder`).

**Tests :**
- [ ] UI : dépôt où les deux ordres diffèrent (commits de dates croisées sur deux branches fusionnées) ; l'ordre
  des lignes change après bascule.
- [ ] Moteur si nécessaire (l'option `--topo-order` est déjà couverte par `log_args`).

**Commit :** `V11 — Permet de trier l'historique par date ou topologiquement`

### Task 10: V5 — Masquer les tags et les branches distantes

**Comportement :** « Affichage › Afficher les tags » et « Afficher les branches distantes » (cochées par
défaut, réglages persistants). Décochées : le graphe ne parcourt plus ces références et leurs étiquettes ne
s'affichent plus dans l'historique ; la barre latérale ne change pas.

**Technique :** introduire dans `dragontree-git/src/log.rs` un sélecteur structuré des références (par exemple
`LogScope::Refs(RefSelection { tags: bool, remotes: bool, exclude: Vec<String>, extra: Vec<String> })` ou
équivalent) qui produit les bons arguments (`--exclude=<motif>` avant `--branches`/`--remotes`/`--tags`) ; il
servira à V4, V3, V6, V7. `LogScope::All` reste valide.

**Tests :**
- [ ] Unitaires de `log_args` (ordre des `--exclude`, absence de `--tags`/`--remotes`).
- [ ] Moteur : un commit atteignable seulement par un tag disparaît quand les tags sont masqués.
- [ ] UI : décocher « Afficher les tags » retire l'étiquette du tag de l'historique.

**Commit :** `V5 — Permet de masquer les tags et les branches distantes du graphe`

### Task 11: V4 — Masquer une branche, un dossier ou un remote du graphe (œil)

**Comportement :** dans la barre latérale, une icône œil au survol de chaque branche locale, dossier de branches
(`feature/`), remote et branche distante ; un clic masque l'élément du graphe (icône œil barré permanente, ligne
atténuée) ; un nouveau clic le réaffiche. Entrée « Masquer du graphe / Afficher dans le graphe » dans les menus
contextuels. « Affichage › Réafficher toutes les branches masquées ». Liste persistante **par dépôt**
(motifs de références : `refs/heads/x`, `refs/heads/feature/*`, `refs/remotes/origin/*`).

**Technique :** `--exclude=<motif>` via le sélecteur de la tâche 10. Masquer la branche courante n'empêche pas
HEAD d'être affichée (HEAD reste parcourue).

**Tests :**
- [ ] Unitaires : motifs produits pour une branche, un dossier, un remote.
- [ ] Moteur : un commit atteignable seulement par la branche exclue disparaît.
- [ ] UI : masquer une branche par l'œil retire son commit propre de l'historique ; réglage conservé à la
  réouverture du dépôt (nouvelle harness, même profil).

**Commit :** `V4 — Permet de masquer une branche, un dossier ou un remote du graphe`

### Task 12: V3 — Filtrer l'historique sur des branches choisies

**Comportement :** icône entonnoir au survol (à côté de l'œil) sur chaque branche, dossier, remote et tag ; un
clic ajoute l'élément au filtre (plusieurs à la fois) ; l'historique n'affiche que les commits atteignables par
les éléments filtrés. Bandeau au-dessus de l'historique : « Filtré par : main ✕  feature/* ✕ » + « Tout
afficher ». Le filtre « Branche courante » (V2) devient un cas de ce mécanisme ou s'exclut avec lui (activer
l'un retire l'autre, le bandeau reflète l'état). Mémoire de session par onglet.

**Technique :** révisions explicites (`refs/heads/x`, `--glob=refs/heads/feature/*`, `--remotes=origin/*`).

**Tests :**
- [ ] Unitaires : arguments produits pour un mélange branche/dossier/remote.
- [ ] UI : filtrer sur deux branches parmi trois ; le commit propre à la troisième disparaît ; « ✕ » sur un
  élément du bandeau le retire ; « Tout afficher » rétablit tout.

**Doc :** feuille de route T4 : rien ; V3 → ✅.

**Commit :** `V3 — Filtre l'historique sur des branches, dossiers ou remotes choisis`

### Task 13: V6 — Afficher les commits perdus (reflog) (Ctrl+Maj+.)

**Comportement :** « Affichage › Commits perdus » (Ctrl+Maj+., à bascule, mémoire par onglet) : ajoute au graphe
les commits atteignables seulement par le reflog (ex. après un `reset --hard`) ; ils sont dessinés atténués avec
une étiquette « perdu » ; leur menu contextuel propose déjà « Nouvelle branche ici » / « Nouveau tag » (existant)
pour les récupérer.

**Technique :** `--reflog` dans le sélecteur ; ensemble des commits perdus =
`git rev-list --reflog --not --branches --remotes --tags HEAD` (ou calcul équivalent) chargé en arrière-plan.

**Tests :**
- [ ] Moteur : après `reset --hard HEAD~1`, le commit abandonné est listé comme perdu.
- [ ] UI : activer l'option fait apparaître le commit abandonné (résumé visible) avec l'étiquette « perdu ».

**Commit :** `V6 — Affiche les commits perdus du reflog dans le graphe`

### Task 14: V7 — Afficher les stashs dans le graphe

**Comportement :** « Affichage › Afficher les stashs » (coché par défaut, réglage persistant) : chaque stash est
une ligne du graphe rattachée à son commit de base (premier parent seulement), avec une étiquette `stash@{n}` et
son message ; les commits techniques (index, non suivis) n'apparaissent pas. Clic : détails et diff du stash
(par rapport à sa base). Menu contextuel d'une ligne de stash : Appliquer, Pop, Supprimer (actions existantes de
la section Stashes).

**Technique :** ajouter les oid des stashs au sélecteur (tâche 10) ; post-traitement : pour un stash, ne garder
que le premier parent et retirer ses parents 2 et 3 de la liste ; étiquette dédiée dans le rendu des refs.

**Tests :**
- [ ] Unitaire du post-traitement (liste de commits synthétique).
- [ ] UI : créer un stash avec un fichier non suivi ; la ligne « stash@{0} » apparaît une seule fois, sans ligne
  « index on … » ni « untracked files on … » ; décocher l'option la retire.

**Commit :** `V7 — Affiche les stashs dans le graphe`

### Task 15: V8 — Replier et déplier les merges

**Comportement :**
- sur la ligne d'un commit de fusion, un petit triangle replie/déplie la branche fusionnée : les commits
  introduits seulement par les parents 2+ (atteignables par eux mais pas par le premier parent, parmi les commits
  chargés) disparaissent ; la ligne de fusion indique le nombre de commits repliés ;
- clavier, commit de fusion sélectionné : ← replie, → déplie ;
- « Affichage › Replier toutes les fusions » / « Déplier toutes les fusions » ;
- le graphe est recalculé sans les commits masqués (en arrière-plan pour un gros dépôt, avec génération) ;
  la sélection et le filtre texte restent cohérents.

**Technique :** calcul en mémoire sur les commits chargés (`HistoryState.commits`), ensemble des fusions repliées
par onglet ; reconstruire `GraphLayout` à partir de la liste filtrée. Mesurer sur un dépôt de ~10 000 commits
généré en test (le repli total doit rester en dessous de la seconde).

**Tests :**
- [ ] Unitaires : ensemble des commits masqués pour une fusion simple, des fusions imbriquées, une fusion dont la
  branche a été refusionnée.
- [ ] UI : replier une fusion masque les commits de la branche fusionnée ; ← / → sur la sélection.

**Commit :** `V8 — Permet de replier et déplier les fusions dans le graphe`

### Task 16: V10 — Disposition verticale ou horizontale de l'historique

**Comportement :** « Affichage › Disposition » : Horizontale (détails à droite, actuel) / Verticale (détails sous
la liste) ; réglage persistant ; tailles des panneaux redimensionnables dans les deux cas.

**Tests :**
- [ ] UI : en disposition verticale, le panneau de détails est sous la liste (comparaison des rectangles d'un
  libellé de la liste et d'un libellé du panneau).

**Commit :** `V10 — Permet une disposition verticale ou horizontale de l'historique`

### Task 17: V12 — Vue Changements : fichiers ignorés et non suivis

**Comportement :** dans « Affichage » (et un menu « … » de la liste « Non indexés ») : « Afficher les fichiers
ignorés » (décoché par défaut) et « Masquer les fichiers non suivis » (décoché par défaut) ; réglages persistants.
Les fichiers ignorés s'affichent atténués avec une icône propre, sans action d'indexation par défaut (ou
`add -f` explicite via le menu contextuel « Indexer quand même »).

**Technique :** le parseur connaît déjà `FileState::Ignored` (`status.rs:176`) ; demander `--ignored` seulement
si l'option est active.

**Tests :**
- [ ] Moteur : statut avec et sans fichiers ignorés.
- [ ] UI : un fichier ignoré n'apparaît qu'avec l'option ; un non suivi disparaît avec « Masquer… ».

**Commit :** `V12 — Affiche les fichiers ignorés et masque les non suivis à la demande`

### Task 18: R2 — Fetch, Pull et Push rapides (Ctrl+clic)

**Comportement :** Ctrl+clic sur Fetch / Pull / Push de la barre d'outils lance l'opération sans dialogue avec
les options par défaut : Pull = branche suivie, mode merge (ou dernier mode utilisé dans la session) ; Push =
branche courante vers son upstream (création de l'upstream sur le remote par défaut s'il n'y en a pas), sans
force ni tags ; Fetch est déjà sans dialogue. Entrées « Dépôt › Pull rapide » et « Push rapide ». Les infobulles
mentionnent le Ctrl+clic.

**Tests :**
- [ ] UI (remote bare local) : Ctrl+clic sur Push pousse sans dialogue (le commit arrive dans le bare) ;
  Ctrl+clic sur Pull intègre un commit poussé par un second clone.

**Commit :** `R2 — Ajoute le fetch, le pull et le push rapides (Ctrl+clic)`

### Task 19: R3 — Worktrees : lister, ouvrir, créer, supprimer (T1)

**Spécification :** feuille de route, section **T1** (détail complet : moteur, section de la barre latérale,
branches sorties ailleurs, dialogue, rafraîchissement, traductions, critères de fin). Résumé :
1. Moteur : `Repo::worktrees()` (`git worktree list --porcelain`, sans `-z`), `add_worktree` (branche locale,
   branche distante avec branche de suivi, commit en HEAD détachée), `remove_worktree` (`--force` seulement sur
   confirmation), `prune_worktrees`.
2. Section « Worktrees » en tête de la barre latérale (branche + chemin ; clic : `AppAction::OpenRepo`, qui
   active l'onglet existant) ; menu contextuel : Ouvrir, Supprimer… (destructeur), Copier le chemin, Ouvrir dans
   l'Explorateur.
3. Icône sur les branches sorties dans un autre worktree ; un checkout refusé pour cette raison propose d'ouvrir
   ce worktree.
4. Dialogue « Créer un worktree… » (Dépôt › Créer un worktree…, palette, menus contextuels branche/branche
   distante/commit) : dossier proposé `<parent>/<dépôt>-<branche>`, ouverture du nouveau worktree dans un onglet.
5. Rafraîchissement quand `.git/worktrees` change (`repo/watcher.rs:30`).

**Tests :** ceux du critère de fin de T1 (moteur : création depuis branche / distante / commit, liste,
suppression, refus d'une branche déjà sortie ; UI : clic qui ouvre puis réactive l'onglet ; checkout d'une
branche sortie ailleurs qui propose le worktree).

**Doc :** R3 → ✅ ; section T1 : état « fait » ; README (worktrees gérés).

**Commit :** `R3 — Gère les worktrees : liste, ouverture, création et suppression`

### Task 20: R4 — Stashs complets

**Comportement :**
- bouton Pop de la barre d'outils avec une flèche ▾ : liste déroulante des stashs (Pop / Appliquer sur celui
  choisi) ; clic principal inchangé (dernier stash) ;
- « Renommer… » dans le menu d'un stash (git n'a pas de renommage : `stash drop` puis `stash store -m` du même
  oid, en conservant sa position si possible, sinon en tête — documenté dans l'infobulle) ;
- « Stash des fichiers sélectionnés… » depuis la vue Changements (`git stash push -m … -- <chemins>`) ;
- « Snapshot » : enregistre un stash sans toucher aux modifications (`git stash create` + `git stash store`) ;
  entrée du menu Dépôt et option du dialogue Stash ;
- le diff d'un stash montre aussi ses fichiers non suivis (troisième parent).

**Tests :**
- [ ] Moteur : stash de fichiers choisis (les autres restent modifiés) ; snapshot (worktree inchangé, stash
  créé) ; renommage (message changé, contenu identique) ; fichiers non suivis d'un stash listés.
- [ ] UI : liste déroulante de Pop qui applique le deuxième stash ; renommage depuis la section Stashes.

**Commit :** `R4 — Complète les stashs : choix au pop, renommage, fichiers choisis, snapshot`

### Task 21: R5 — Appliquer un patch

**Comportement :** « Dépôt › Appliquer un patch… » : depuis un fichier (sélecteur), un glisser-déposer d'un
fichier `.patch`/`.diff` sur la fenêtre, ou le presse-papiers (« Coller un patch »). Dialogue d'aperçu : fichiers
touchés (`git apply --numstat`), vérification (`--check`) avec l'erreur affichée, cible : copie de travail ou
index ; un patch au format mail (`From <sha> …` de `format-patch`) propose aussi `git am` (crée les commits).

**Tests :**
- [ ] Moteur : vérification, application à la copie de travail, à l'index, `am` d'un patch `format-patch`,
  refus propre d'un patch qui ne s'applique pas.
- [ ] UI : un fichier déposé (`dropped_files` de l'entrée brute) ouvre l'aperçu ; « Appliquer » modifie le
  fichier.

**Commit :** `R5 — Permet d'appliquer un patch depuis un fichier, un dépôt ou le presse-papiers`

### Task 22: R6 — Enregistrer comme patch

**Comportement :** « Enregistrer comme patch… » dans le menu d'un commit (`format-patch -1 --stdout`), d'une
sélection de plusieurs commits ou « de ce commit jusqu'à HEAD » (plage, un fichier par commit dans un dossier
choisi), d'un stash (`stash show -p --include-untracked` si disponible, sinon sans), et des fichiers sélectionnés
de la vue Changements (diff indexé ou non). Aussi « Copier comme patch » (presse-papiers).

**Tests :**
- [ ] Moteur : contenu produit pour un commit, une plage, un stash, des fichiers ; aller-retour : le patch
  produit s'applique (`git apply --check`) sur la base.
- [ ] UI : « Copier comme patch » sur un commit met un patch valide dans le presse-papiers.

**Commit :** `R6 — Permet d'enregistrer des commits, un stash ou des fichiers comme patch`

### Task 23: R7 — Comparer deux commits, deux branches, ou un commit avec les modifications locales

**Comportement :** menu d'un commit : « Comparer avec HEAD », « Comparer avec les modifications locales »,
« Marquer pour comparaison » puis « Comparer avec <commit marqué> » ; menu d'une branche : « Comparer avec la
branche courante ». Fenêtre ou vue « Comparaison A ↔ B » : liste des fichiers modifiés (`diff --name-status`
A B, ou A et la copie de travail) et diff du fichier choisi avec la vue de diff existante (unifiée / côte à côte,
coloration). Bouton d'inversion A/B.

**Tests :**
- [ ] Moteur : fichiers et diff entre deux révisions, et entre une révision et la copie de travail.
- [ ] UI : comparer deux commits affiche le fichier modifié entre eux et son diff.

**Commit :** `R7 — Permet de comparer deux commits, deux branches ou un commit avec la copie de travail`

### Task 24: R8 — Réglages du dépôt

**Comportement :** « Dépôt › Réglages du dépôt… » (fenêtre) :
- identité propre au dépôt : `user.name` / `user.email` en `--local` (vide = retirer la clé, on hérite du global ;
  affichage de la valeur globale en indication) ;
- liens vers les tickets : liste de règles « motif → URL » (ex. `#(\d+)` →
  `https://dev.azure.com/contoso/projet/_workitems/edit/$1`) ; dans les détails du commit, les occurrences
  deviennent des liens cliquables ; stockage par dépôt dans les réglages de l'application ;
- modèle de message de commit : clé standard `commit.template` (fichier) ; le modèle préremplit la zone de commit
  quand elle est vide.
- **Compte associé (T6) n'est pas dans cette tâche.**

**Tests :**
- [ ] Moteur : lecture/écriture/suppression d'une clé `--local` ; lecture du modèle.
- [ ] Unitaires : découpage d'un message en segments texte/lien selon les règles (motif invalide ignoré sans
  plantage).
- [ ] UI : l'identité locale enregistrée est utilisée par un commit fait dans l'application (auteur vérifié avec
  `git log -1 --format=%an`).

**Commit :** `R8 — Ajoute les réglages du dépôt : identité, liens vers les tickets, modèle de message`

### Task 25: R9 — Remotes : modifier l'URL, tester la connexion

**Comportement :** menu d'un remote : « Modifier l'URL… » (dialogue prérempli, `git remote set-url`, URL de push
distincte si présente : champ séparé) ; « Tester la connexion » (`git ls-remote --heads <remote>` en arrière-plan,
avec l'authentification du compte associé comme pour fetch ; notification succès / échec avec le message de git).

**Tests :**
- [ ] Moteur : `set-url` puis lecture ; `ls-remote` réussi sur un bare local, échec sur un chemin absent.
- [ ] UI : modifier l'URL depuis le menu du remote change `git remote get-url`.

**Commit :** `R9 — Permet de modifier l'URL d'un remote et de tester la connexion`

### Task 26: R10 — Sous-modules : ajouter, supprimer, déplacer, changer l'URL

**Comportement :** section Sous-modules (existante, `repo/sidebar.rs:204-259`) : bouton « + » / « Dépôt › Ajouter
un sous-module… » (URL, chemin, branche facultative) ; menu d'un sous-module : « Supprimer… » (destructeur :
`submodule deinit -f`, `git rm -f`, suppression de `.git/modules/<nom>`), « Déplacer… » (`git mv`), « Modifier
l'URL… » (`git submodule set-url` si git ≥ 2.25, sinon `git config -f .gitmodules` + `submodule sync`).

**Tests :**
- [ ] Moteur (sous-module depuis un dépôt local ; `-c protocol.file.allow=always` pour les tests) : ajout,
  déplacement, changement d'URL, suppression complète (plus de trace dans `.gitmodules`, l'index et
  `.git/modules`).
- [ ] UI : ajout depuis le dialogue puis le sous-module apparaît dans la section.

**Commit :** `R10 — Gère l'ajout, la suppression, le déplacement et l'URL des sous-modules`

### Task 27: R11 — .gitignore : ajouter un motif avec aperçu

**Comportement :** « Ignorer… » (menu d'un fichier de la vue Changements et « Dépôt › Ignorer des fichiers… ») :
dialogue avec motifs proposés (le fichier, `*.ext`, le dossier `dir/`), champ libre, choix du fichier cible
(`.gitignore` racine, `.gitignore` du dossier, `.git/info/exclude`), et aperçu en direct des fichiers concernés
(non suivis qui seraient ignorés : `git ls-files --others --ignored --exclude=<motif>` ; avertissement si des
fichiers déjà suivis correspondent). « Ignorer » existant reste.

**Tests :**
- [ ] Moteur : aperçu d'un motif ; ajout dans chacune des trois cibles.
- [ ] UI : saisir `*.log` montre les deux fichiers `.log` non suivis dans l'aperçu ; valider les retire des non
  suivis.

**Commit :** `R11 — Ajoute un dialogue pour ignorer un motif avec aperçu des fichiers concernés`

### Task 28: R12 — Bisect

**Comportement :** menu d'un commit : « Bisect : marquer comme mauvais / bon » (démarre la session si besoin,
avec `HEAD` mauvais par défaut) ; « Dépôt › Bisect › Démarrer / Terminer ». Bandeau d'état pendant une session
(comme le bandeau d'opération) : nombre de révisions restantes (sortie de git), boutons Bon, Mauvais, Passer,
Terminer (`bisect reset`) ; à la fin, le premier mauvais commit est annoncé et sélectionnable. Étiquettes
`bisect bon` / `bisect mauvais` sur les commits marqués.

**Technique :** état détecté par `.git/BISECT_START` / `BISECT_LOG` (pas de nouvelle commande sur le thread UI).

**Tests :**
- [ ] Moteur : session complète sur un historique linéaire de 8 commits avec un fichier « cassé » à partir du
  5e : start/bad/good puis réponses jusqu'au résultat ; skip ; reset ; détection d'état.
- [ ] UI : démarrer depuis le menu d'un commit, le bandeau apparaît ; « Terminer » le retire.

**Commit :** `R12 — Ajoute git bisect avec bandeau d'état`

### Task 29: R13 — Git Flow

**Comportement :** « Dépôt › Git Flow » : « Initialiser… » (branches production/développement, préfixes
feature/, release/, hotfix/, support/, tag de version ; écrits dans `gitflow.*` de la config locale ; crée
`develop` si absente) ; « Démarrer une feature / release / hotfix… » (nom → branche depuis develop, ou depuis
master pour hotfix) ; « Terminer… » sur la branche courante (feature : merge `--no-ff` dans develop, suppression
facultative ; release/hotfix : merge dans master + tag + merge dans develop). Entrées grisées si non initialisé.

**Tests :**
- [ ] Moteur : init, start/finish feature, release (tag créé, les deux branches contiennent la release), hotfix ;
  lecture d'une config git-flow existante.
- [ ] UI : initialiser puis démarrer une feature crée et sort `feature/x`.

**Commit :** `R13 — Ajoute Git Flow (feature, release, hotfix) sans dépendance externe`

### Task 30: R14 — Git LFS

**Comportement :** « Dépôt › Git LFS » : « Initialiser » (`git lfs install --local`), « Suivre un motif… »
(`git lfs track "<motif>"`, liste des motifs suivis depuis `.gitattributes`), « Ne plus suivre… », Fetch / Pull /
Prune (`git lfs fetch|pull|prune`, en file d'écriture avec progression si possible), « Verrous » (fenêtre :
`git lfs locks --json`, Verrouiller / Déverrouiller un fichier, `--force` sur confirmation). Les fichiers LFS
sont signalés dans la vue Changements (pointeur ⇒ icône). Menu grisé si `git lfs` est absent (détection une fois).

**Tests :** (ignorés proprement si LFS absent)
- [ ] Moteur : install local, track/untrack (lecture de `.gitattributes`), commit d'un fichier suivi (pointeur
  dans l'objet), détection de `git lfs`.
- [ ] Unitaires : analyse de `git lfs locks --json`.
- [ ] UI : « Suivre un motif » ajoute la ligne à `.gitattributes`.

**Commit :** `R14 — Ajoute la prise en charge de Git LFS`

### Task 31: R15 — Commandes personnalisées

**Comportement :**
- Réglages › nouvel onglet « Commandes » : liste éditable de commandes : nom, cible (dépôt, branche, commit,
  fichier), type **script** (ligne de commande exécutée dans le dossier du dépôt, via `cmd /C`) ou **URL** (ouverte
  dans le navigateur), variables `${repo}`, `${branch}`, `${sha}`, `${file}`, `${remote_url}` ; options :
  demander confirmation, afficher la sortie (fenêtre), rafraîchir après.
- Les commandes apparaissent dans « Dépôt › Commandes personnalisées » (cible dépôt) et dans les menus
  contextuels des branches, commits et fichiers selon leur cible.
- Exécution en arrière-plan, annulable, sortie dans une fenêtre si demandé.

**Tests :**
- [ ] Unitaires : substitution des variables (citation des valeurs avec espaces/guillemets pour `cmd`), filtrage
  par cible.
- [ ] UI : une commande « script » de cible commit (`echo ${sha} > out.txt`) écrit le SHA dans le dépôt ; une
  commande URL produit `OpenUrl` avec la variable remplacée.

**Commit :** `R15 — Ajoute les commandes personnalisées (script ou URL)`

### Task 32: R16 — Plusieurs branches à la fois ; branches périmées

**Comportement :**
- « Dépôt › Mettre à jour les branches locales… » : branches locales en retard sur leur upstream et
  fast-forwardables (cases cochées par défaut) → fast-forward de toutes (sans checkout ; la branche courante via
  `merge --ff-only`) ;
- « Dépôt › Pousser plusieurs branches… » : branches en avance → push des cochées ;
- « Dépôt › Supprimer les branches périmées… » : branches dont l'upstream a disparu (`[gone]`) ou fusionnées dans
  la branche par défaut (hors branche courante et branche par défaut), cochables, suppression locale (destructeur).
- Rapport final : réussites et échecs par branche.

**Tests :**
- [ ] Moteur : détection ff-able / en avance / gone / fusionnée ; ff de plusieurs branches ; suppression.
- [ ] UI : le dialogue des branches périmées liste une branche dont l'upstream a été supprimé et la supprime.

**Commit :** `R16 — Met à jour, pousse ou supprime plusieurs branches à la fois`

### Task 33: R17 — Remettre un fichier dans son état à un commit donné

**Comportement :** menu d'un fichier dans les détails d'un commit et dans la fenêtre d'historique d'un fichier :
« Remettre ce fichier à cette version… » (confirmation destructrice si le fichier a des modifications locales) :
`git checkout <sha> -- <chemin>` (le résultat est indexé, comme git) ; variante « à la version d'avant ce commit »
(`<sha>^`). Fichier supprimé dans ce commit : la variante « d'avant » le restaure.

**Tests :**
- [ ] Moteur : restauration à un commit, au parent, d'un fichier supprimé.
- [ ] UI : depuis les détails d'un ancien commit, le fichier reprend son contenu d'alors.

**Commit :** `R17 — Permet de remettre un fichier dans son état à un commit donné`

### Task 34: R18 — Créer une pull request

**Comportement :** « Dépôt › Créer une pull request… » et menu d'une branche : dialogue (branche source = courante
ou cliquée, cible = branche par défaut du remote, titre = sujet du dernier commit, description, brouillon,
relecteurs facultatifs non requis). Si un compte DragonTree correspond au remote (`account_for_repo` ou
`account_matches_url`) : création par l'API (GitHub `POST /repos/{o}/{r}/pulls`, GitLab
`POST /projects/:id/merge_requests`, Azure DevOps `POST …/_apis/git/repositories/{repo}/pullrequests`), puis lien
vers la PR créée. Sinon (ou en cas d'échec) : « Ouvrir dans le navigateur » avec l'URL de création de la forge.
Proposer de pousser la branche si elle n'a pas d'upstream.

**Technique :** `dragontree-forges` : analyse des URL de remote (HTTPS, SSH, Azure `dev.azure.com` et
`*.visualstudio.com`), construction des requêtes, analyse des réponses ; fixtures JSON dans
`crates/dragontree-forges/tests/fixtures/` ; test réel `#[ignore]` comme `tests/live.rs`.

**Tests :**
- [ ] Unitaires : analyse des URL de remote des trois forges ; URL web de création ; corps JSON des requêtes ;
  analyse des réponses (fixtures).
- [ ] UI : sans compte, le dialogue propose l'ouverture dans le navigateur avec la bonne URL (`OpenUrl`).

**Commit :** `R18 — Permet de créer une pull request sur GitHub, GitLab ou Azure DevOps`

### Task 35: R19 — « Ouvrir dans ▾ » avec d'autres éditeurs

**Comportement :** le bouton VS Code de la barre d'outils devient « Ouvrir dans ▾ » : VS Code, Cursor, Visual
Studio (s'il y a une `.sln` à la racine ou un niveau en dessous : l'ouvrir ; choix si plusieurs), Rider
(Toolbox / Program Files), Sublime Text, Notepad++, et « Éditeur par défaut » ; seuls les outils détectés sont
listés ; le dernier utilisé devient l'action du clic principal (réglage persistant). Même liste dans « Dépôt ›
Ouvrir dans ».

**Technique :** détection à partir du PATH et de chemins connus, testable avec des dossiers injectés (comme
`vscode_command_in`, `util.rs:84`) ; détection faite une fois, hors du thread de l'interface si elle touche le
disque de façon notable.

**Tests :**
- [ ] Unitaires : détection avec un faux PATH et de faux dossiers d'installation ; recherche des `.sln` ; commande
  construite pour chaque éditeur.

**Commit :** `R19 — Ajoute « Ouvrir dans » avec Visual Studio, Rider, Cursor et d'autres éditeurs`

### Task 36: R20 — Statistiques du dépôt

**Comportement :** « Dépôt › Statistiques… » : fenêtre calculée en arrière-plan (annulable) : nombre de commits
(branche courante et toutes branches), contributeurs classés (`shortlog -sne`, fusion des alias par e-mail),
commits par mois (histogramme dessiné), premier et dernier commit, nombre de fichiers suivis, taille du dépôt
(`count-objects -vH`), branches et tags.

**Tests :**
- [ ] Moteur : statistiques d'un dépôt de test connu (comptes exacts).
- [ ] Unitaires : agrégation par mois et fusion des contributeurs.
- [ ] UI : la fenêtre affiche le nombre de commits attendu.

**Commit :** `R20 — Ajoute les statistiques du dépôt`

### Task 37: C2 — Outils externes de diff et de merge

**Comportement :** Réglages › « Outils externes » : outil de diff et outil de merge, chacun parmi des préréglages
(VS Code, WinMerge, Beyond Compare, KDiff3, P4Merge) détectés aux emplacements usuels, « Configuration de git »
(défaut actuel : `git difftool`/`git mergetool` tels que configurés), ou commande personnalisée avec `$LOCAL`,
`$REMOTE`, `$BASE`, `$MERGED`. Actions : « Ouvrir dans l'outil de diff externe » (fichier de la vue Changements
ou des détails d'un commit : les deux versions sont extraites dans des fichiers temporaires) ; « Outil de merge
externe » existant utilise le préréglage (`git mergetool --tool=<nom>` avec `-c mergetool.<nom>.cmd=…` et
`trustExitCode`).

**Tests :**
- [ ] Unitaires : commande produite pour chaque préréglage et pour une commande personnalisée (citations).
- [ ] Moteur : `mergetool` avec un faux outil (`.cmd` de test qui écrit la résolution dans `$MERGED`) résout le
  conflit.

**Commit :** `C2 — Rend les outils externes de diff et de merge configurables`

### Task 38: C3 — Terminal à utiliser

**Comportement :** Réglages › « Terminal » : Automatique (comportement actuel), Windows Terminal, PowerShell
(pwsh si présent, sinon Windows PowerShell), Git Bash (déduit du chemin de git), Invite de commandes. « Ouvrir
un terminal » utilise ce choix (repli automatique si l'exécutable manque, avec notification).

**Tests :**
- [ ] Unitaires : commande construite pour chaque choix (dossier de travail, arguments), détection de Git Bash à
  partir d'un chemin de git donné.

**Commit :** `C3 — Permet de choisir le terminal à ouvrir`

### Task 39: C4 — Identité git globale

**Comportement :** « Configuration › Identité git… » : nom et e-mail globaux (`git config --global`), valeurs
actuelles préremplies ; avertissement au premier commit si aucune identité n'est configurée (message de git
reconnu) avec un bouton qui ouvre ce dialogue.

**Tests :**
- [ ] Moteur : lecture/écriture avec `GIT_CONFIG_GLOBAL` temporaire.
- [ ] UI : le dialogue enregistre l'identité dans le fichier global de test (variable d'environnement posée pour
  le processus de test seulement ; vérifier qu'elle est propagée aux commandes git de l'application).

**Commit :** `C4 — Permet de régler l'identité git globale`

### Task 40: C5 — Message de commit : 50/72, colonne guide, messages récents, Co-authored-by

**Comportement :** dans la zone de commit :
- compteur du résumé coloré : normal ≤ 50, avertissement 51–72, erreur > 72 (le compteur actuel « n/72 » de
  `repo/changes.rs:562-567` est remplacé) ;
- colonne guide verticale à 72 caractères dans la description ;
- bouton « Messages récents ▾ » : derniers messages utilisés (mémoire par dépôt, ~20) et derniers sujets de
  l'historique ; un choix remplit résumé et description ;
- bouton « Co-auteur ▾ » : auteurs connus de l'historique (recherche) ; ajoute une ligne
  `Co-authored-by: Nom <email>` en fin de description (sans doublon) ;
- réglages : longueurs 50/72 modifiables (Réglages › Commit).

**Tests :**
- [ ] Unitaires : niveau du compteur selon la longueur ; ajout de la ligne Co-authored-by (ligne vide de
  séparation, pas de doublon).
- [ ] UI : choisir un message récent remplit le résumé ; ajouter un co-auteur.

**Commit :** `C5 — Améliore la saisie du message de commit (50/72, récents, co-auteurs)`

### Task 41: C6 — Diff : police, taille, tabulations, espaces visibles

**Comportement :** Réglages › « Diff » : taille de la police du diff, police (chasse fixe intégrée ou une police
système monospace choisie parmi celles trouvées dans `C:\Windows\Fonts` : Consolas, Cascadia Mono, Courier New),
largeur de tabulation (2, 4, 8), afficher les espaces (· et →). Appliqué aux vues de diff (unifiée, côte à côte),
au blame et à l'éditeur de conflits ; le copier-coller du diff ne contient pas les marqueurs.

**Tests :**
- [ ] Unitaires : expansion des tabulations selon la colonne ; marqueurs d'espaces ; texte copié sans marqueurs.

**Commit :** `C6 — Rend la police, les tabulations et les espaces du diff configurables`

### Task 42: C7 — IA : message de commit et revue de branche par Claude Code

**Comportement :**
- Réglages › « IA » : activer, chemin de la CLI (`claude` par défaut, détection dans le PATH), modèle facultatif ;
- zone de commit : bouton « Générer avec Claude » (actif s'il y a des fichiers indexés) : envoie le diff indexé
  (tronqué à une taille raisonnable) à `claude -p` avec une consigne (résumé ≤ 72 caractères, ligne vide,
  description ; langue de l'interface) ; remplit résumé et description ; annulable ; erreur affichée si la CLI
  manque ;
- menu d'une branche et « Dépôt › Revue de la branche par Claude… » : diff de la branche par rapport à sa base
  (merge-base avec la branche par défaut) → fenêtre affichant la revue (texte défilant, copiable).
- Aucun envoi sans action explicite de l'utilisateur.

**Tests :**
- [ ] Unitaires : construction de la consigne et troncature du diff ; découpage de la réponse en résumé/corps.
- [ ] UI : avec un faux `claude` (`.cmd` de test qui renvoie un message fixe), « Générer » remplit le résumé.

**Commit :** `C7 — Génère le message de commit et la revue de branche avec Claude Code`

### Task 43: C8 — Fenêtre des raccourcis clavier, À propos

**Comportement :** « Configuration › Raccourcis clavier » : fenêtre listant tous les raccourcis du registre (et
les raccourcis fixes : Ctrl+Tab, Ctrl+1…9, Ctrl+Entrée, Espace/Entrée/Suppr dans les listes, flèches de
l'historique), groupés par menu, avec recherche. « Configuration › À propos de DragonTree » : version
(`CARGO_PKG_VERSION`), version de git détectée, licence MIT, crédits (egui, police Phosphor et sa licence).

**Tests :**
- [ ] UI : la fenêtre des raccourcis liste « Ctrl+Maj+T » et la recherche filtre ; « À propos » affiche la
  version du paquet.

**Commit :** `C8 — Ajoute la fenêtre des raccourcis clavier et « À propos »`

### Task 44: H1 — Glisser une branche sur une autre pour un merge ou un rebase

**Comportement :** dans la barre latérale (et sur les étiquettes de branche de l'historique si simple), glisser
une branche S sur une branche T ouvre un menu : si T est la branche courante : « Fusionner S dans T… », « Rebaser
T sur S… » ; si S est la branche courante : « Rebaser S sur T… », « Fusionner T dans S… » ; sinon : « Checkout de T
puis fusionner S… ». Les entrées ouvrent les dialogues existants (Merge, Rebase). Indication visuelle de la cible
pendant le glisser.

**Tests :**
- [ ] Unitaires : actions proposées selon (S, T, branche courante).
- [ ] UI : glisser (événements pointeur simulés) une branche sur la branche courante puis « Fusionner » ouvre le
  dialogue de fusion avec la bonne révision.

**Commit :** `H1 — Permet de glisser une branche sur une autre pour fusionner ou rebaser`

### Task 45: H2 — Couleurs d'onglets et espaces de travail

**Comportement :**
- menu contextuel d'un onglet : « Couleur ▸ » (aucune + 7 couleurs lisibles dans les deux thèmes) ; liseré coloré
  sur l'onglet ; couleur mémorisée par dépôt ;
- espaces de travail : « Fichier › Espaces de travail ▸ » : liste (l'actif coché), « Nouvel espace de travail… »,
  « Renommer… », « Supprimer… » ; changer d'espace mémorise les onglets ouverts dans l'espace quitté et ouvre ceux
  de l'espace choisi ; persistant ; un espace « Par défaut » existe toujours (reprend les onglets actuels à la
  migration).

**Tests :**
- [ ] Unitaires : bascule d'espace (sauvegarde/restauration des listes d'onglets).
- [ ] UI : créer un second espace, y ouvrir un dépôt, revenir au premier : ses onglets sont restaurés ; couleur
  d'onglet persistée dans `settings.json`.

**Commit :** `H2 — Ajoute les couleurs d'onglets et les espaces de travail`

### Task 46: H3 — Recherche de commits dans le contenu des diffs et par chemin

**Comportement :** le champ de recherche de l'historique gagne un sélecteur de mode : « Message, auteur, SHA »
(actuel, en mémoire), « Contenu des modifications » (`git log -G<texte>` sur la portée courante, en arrière-plan,
avec génération et délai de frappe), « Chemin » (`git log -- <chemin>` avec motifs). Résultat : la liste n'affiche
que les commits trouvés (comme le filtre actuel) ; indicateur de recherche en cours ; erreur de motif affichée.

**Tests :**
- [ ] Moteur : `-G` et filtre par chemin (oids attendus).
- [ ] UI : le mode « Contenu » trouve le commit qui a introduit une chaîne absente des messages.

**Commit :** `H3 — Recherche les commits dans le contenu des diffs et par chemin`

### Task 47: H4 — Diff d'images par balayage ou superposition

**Comportement :** l'aperçu d'images (`repo/image_diff.rs`) gagne trois modes : Côte à côte (actuel), Balayage
(les deux images superposées, un curseur vertical déplaçable révèle l'ancienne à gauche et la nouvelle à droite),
Superposition (opacité de la nouvelle réglable au curseur) ; mode mémorisé (réglage). Images de tailles
différentes : alignées en haut à gauche, à la même échelle.

**Tests :**
- [ ] Unitaires : calcul des rectangles/UV du balayage pour une position donnée, à tailles égales et différentes.
- [ ] UI : un commit modifiant un PNG montre le sélecteur de mode ; « Balayage » affiche le curseur.

**Commit :** `H4 — Ajoute les modes balayage et superposition au diff d'images`

### Task 48: Synthèse de la documentation

**Comportement :** mettre à jour `README.md` (fonctionnalités : menus, worktrees, options ajoutées ; « Limites
connues » : retirer ce qui est fait — worktrees, LFS, git-flow, reflog, commandes personnalisées ; nombre de
tests réel) et `docs/feuille-de-route.md` (intro et « Reste à faire » : N2 terminé, T1 et T2 faits, raccourcis
tranchés, mention de cette branche). Pas de code.

**Commit :** `Met à jour le README et la feuille de route après les options de menu`
