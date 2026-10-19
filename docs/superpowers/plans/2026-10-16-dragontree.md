# DragonTree — clone de Git Fork en Rust — Plan d'implémentation

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Recréer Git Fork (v2.23) en Rust natif : onglets par dépôt, barre latérale droite listant automatiquement les dépôts trouvés sous des dossiers racines, comptes GitHub/GitLab/Azure DevOps multiples et clonage intégré, en consommant beaucoup moins que les ~300 Mo de Fork.

**Architecture:** Workspace Cargo à trois crates. `dragontree-git` est le moteur : il pilote `git.exe` par arguments (jamais via un shell), parse les sorties `-z`/porcelain, calcule le graphe et la découverte des dépôts. `dragontree-forges` gère les API GitHub/GitLab/Azure DevOps, les jetons dans le Gestionnaire d'identifiants Windows et l'injection d'authentification dans git. `dragontree-app` est l'interface egui/eframe (rendu glow) : toute commande git s'exécute hors du thread UI et revient par canal.

**Tech Stack:** Rust 1.99 (toolchain `x86_64-pc-windows-gnu`), eframe/egui 0.37 (glow), ureq 3 (native-tls/SChannel), keyring 4, notify 8, rfd 0.17, ignore 0.4, similar, syntect (fancy-regex), chrono, serde/serde_json, police Phosphor (MIT) embarquée.

**Spec:** demande de l'utilisateur (conversation du 2026-10-16), résumée ici :
- Recoder Fork « en entier » : graphe des commits, changements locaux (stage par fichier/hunk/ligne), commit, branches, tags, remotes, stash, fetch/pull/push, merge/rebase/cherry-pick/revert/reset, conflits, rebase interactif, historique et blame de fichier.
- Un onglet par dépôt, onglets créés manuellement.
- Choisir un ou plusieurs dossiers racines (ex. `DEV`) ; l'outil trouve tous les dépôts des sous-dossiers et les liste dans une barre à droite (liste automatique, indépendante des onglets).
- Comptes Azure DevOps, GitHub et GitLab, plusieurs en même temps ; cloner directement depuis l'outil.
- Plus tard (hors périmètre ici, mais l'architecture le prévoit) : centraliser les PR de tous les comptes.
- Tout en Rust, performance maximale, faible RAM (Fork ≈ 300 Mo).

## Global Constraints

- Toolchain : `stable-x86_64-pc-windows-gnu` existante. Pas de dépendance compilant du C (pas de gcc sur la machine).
- `.cargo/config.toml` passe `-C dlltool=<projet>/.tools/mingw64/bin/dlltool.exe` et `-L native=<projet>/.tools/mingw64/lib`. `scripts/setup-toolchain.ps1` recrée `.tools/` depuis repo.msys2.org (binutils 2.47-4 + DLL d'exécution + bibliothèques d'import de crt-git r380).
- Rendu eframe : **glow uniquement** (mesuré : 45 Mo privés contre 450 Mo avec wgpu). `default-features = false` sur eframe.
- Le moteur git est le `git.exe` de l'utilisateur (détecté dans le PATH, surchargeable dans les réglages). Chaque processus est lancé avec `CREATE_NO_WINDOW` et `GIT_TERMINAL_PROMPT=0`. Les lectures ajoutent `GIT_OPTIONAL_LOCKS=0`.
- Arguments passés en tableau (jamais de chaîne shell). Sorties lues en `-z` dès que possible. `-c core.quotepath=false` sur les commandes qui affichent des chemins.
- Interface en français. Les verbes git restent tels quels (Fetch, Pull, Push, Stash, Commit, Merge, Rebase, Cherry-pick).
- Secrets (PAT) uniquement dans le Gestionnaire d'identifiants Windows (keyring, service `DragonTree`). Jamais dans `settings.json`, les logs ou la ligne de commande.
- Injection d'auth git via `GIT_CONFIG_COUNT`/`GIT_CONFIG_KEY_n`/`GIT_CONFIG_VALUE_n` (`http.<origine>.extraHeader`), seulement pour les clones lancés depuis un compte et les dépôts associés explicitement à un compte.
- Réglages : `%APPDATA%\DragonTree\settings.json`. Cache des dépôts découverts : `%LOCALAPPDATA%\DragonTree\repos-cache.json`.
- Pas de commit git sans demande explicite de l'utilisateur (règle du harnais). Les étapes « Commit » de ce plan sont donc remplacées par « vérifier build + tests ».
- Objectif mémoire : < 150 Mo de mémoire privée avec 3 onglets ouverts, dont un dépôt de 86 k commits et 1 559 refs.

## Review Focus

1. **Dépôts « dubious ownership »** (`safe.directory`, présents sur la machine : `DEV/Projet3/Projet`, `Projet4/Projet`, `Pax/Projet`, `Survey2/Projet`). La découverte les liste quand même. L'ouverture affiche l'erreur et un bouton « Faire confiance à ce dépôt » (`git config --global --add safe.directory <chemin>`). Test : `runner_detects_dubious_ownership` (Task 1) et `open_reports_dubious_ownership` (Task 8).
2. **Très gros dépôt** (86 k commits, 1 559 branches). Historique chargé en < 2 s, défilement fluide (virtualisation), largeur du graphe bornée, mémoire du graphe ≈ 16 octets par arête. Test : `graph_memory_is_linear` (Task 7) et mesure manuelle (Task 24).
3. **Chemins avec espaces, accents ou CRLF** (`Peppol - Copie`, fichiers accentués, fins de ligne CRLF). Le stage partiel doit préserver les `\r`. Tests : `status_paths_with_spaces_and_unicode` (Task 2), `stage_lines_preserves_crlf` (Task 6), `discovery_handles_spaces` (Task 9).
4. **Opération git qui échoue en route** (conflit de merge, rebase arrêté, push rejeté, auth refusée). Le stderr est remonté, un bandeau d'état propose Continuer/Annuler et l'UI ne gèle jamais. Tests : `merge_conflict_sets_operation_state` et `push_rejected_returns_stderr` (Task 8).
5. **Actions UI concurrentes** (clics rapides stage/unstage, changement d'onglet pendant un chargement, fermeture d'onglet pendant une opération). Écritures sérialisées par dépôt, résultats périmés ignorés grâce à un numéro de génération, aucune collision `index.lock`. Test : `serial_worker_preserves_order` (Task 12).

---

## Structure des fichiers

```
Cargo.toml                       workspace + profils + dépendances partagées
.cargo/config.toml               dlltool + chemin des libs d'import (machine locale)
scripts/setup-toolchain.ps1      recrée .tools/ (MSYS2) et .cargo/config.toml
assets/fonts/Phosphor.ttf        icônes (MIT) + assets/fonts/LICENSE-Phosphor.txt
crates/dragontree-git/src/
  lib.rs        ré-exports
  oid.rs        Oid (20 octets)
  error.rs      GitError
  runner.rs     Git, GitCommand, Output, CancelToken
  progress.rs   parse des lignes de progression (--progress)
  status.rs     porcelain v2 -z
  refs.rs       for-each-ref + HEAD
  log.rs        format de log + parseur incrémental
  diff.rs       diff unifié + name-status -z
  patch.rs      patchs partiels (hunks/lignes)
  graph.rs      disposition des voies du graphe
  stash.rs      stash list
  blame.rs      blame --porcelain
  repo.rs       Repo : ouverture + lectures
  ops.rs        Repo : écritures (stage, commit, branches, merge…)
  net.rs        clone/fetch/pull/push + NetOptions
  rebase.rs     rebase interactif (todo + exec)
  conflict.rs   parse/résolution des marqueurs de conflit
  discovery.rs  scan parallèle des dépôts + lecture HEAD
crates/dragontree-git/tests/        tests d'intégration (dépôts temporaires)
crates/dragontree-forges/src/
  lib.rs, model.rs (ForgeKind, Account, RemoteRepo, VerifiedUser, ForgeError)
  http.rs (agent ureq), github.rs, gitlab.rs, azure.rs
  auth.rs (env git, correspondance remote↔compte, URL de création de jeton)
  secrets.rs (keyring)
crates/dragontree-forges/tests/fixtures/*.json
crates/dragontree-app/src/
  main.rs        options eframe, polices, CLI (--screenshot, --open)
  app.rs         DragonTreeApp : onglets, panneaux, dialogues globaux, événements
  settings.rs    Settings (serde) + chargement/sauvegarde
  theme.rs       visuels clair/sombre, couleurs diff/graphe, polices
  icons.rs       constantes Phosphor (copie de codepoints.rs, MIT)
  jobs.rs        Jobs (threads + canal) et SerialWorker par dépôt
  toasts.rs      notifications + journal des commandes
  tabbar.rs      barre d'onglets
  repo_list.rs   barre droite : dépôts découverts
  welcome.rs     page « nouvel onglet »
  widgets.rs     petits widgets communs (bouton icône, badge, champ de recherche)
  repo/mod.rs        RepoTab : état, événements, chargements
  repo/sidebar.rs    branches/remotes/tags/stashes/sous-modules
  repo/toolbar.rs    Fetch/Pull/Push/Stash/Branche/Terminal/Explorateur
  repo/history.rs    liste des commits + graphe + filtre
  repo/graph_paint.rs dessin du graphe
  repo/details.rs    détails du commit + fichiers
  repo/diff_view.rs  diff unifié/côte à côte, sélection de lignes, actions
  repo/changes.rs    changements locaux + boîte de commit
  repo/dialogs.rs    dialogues d'opérations
  repo/actions.rs    construction/lancement des opérations git
  repo/blame.rs      historique/blame d'un fichier
  repo/rebase_ui.rs  éditeur de rebase interactif
  repo/conflicts.rs  résolution de conflits
  repo/watcher.rs    surveillance du dossier (notify)
  highlight.rs   coloration syntaxique (syntect, chargement paresseux)
  accounts.rs    fenêtre des comptes
  clone.rs       fenêtre de clonage
  settings_ui.rs fenêtre des réglages
  quick_open.rs  Ctrl+P
```

---

## Milestone 0 — Toolchain et workspace

### Task 0: Workspace + toolchain reproductible

**Files:**
- Create : `Cargo.toml`, `.cargo/config.toml`, `.gitignore`, `scripts/setup-toolchain.ps1`, `README.md`, `crates/*/Cargo.toml`, `crates/*/src/lib.rs|main.rs`, `assets/fonts/*`

**Interfaces:**
- Produces : dépendances de workspace déclarées une fois (`[workspace.dependencies]`) et réutilisées par `dep.workspace = true`. Binaire `dragontree` (crate `dragontree-app`).

- [ ] **Step 1 :** écrire `scripts/setup-toolchain.ps1`. Il télécharge les paquets MSYS2 listés dans les contraintes et vérifie leurs SHA-256, puis extrait `as.exe`, `dlltool.exe` et les DLL vers `.tools/mingw64/bin` et les `lib*.a` d'import absents du dossier self-contained de rustup vers `.tools/mingw64/lib`. Il écrit enfin `.cargo/config.toml` avec les chemins absolus.
- [ ] **Step 2 :** profils : `release` = `opt-level=3, lto="thin", codegen-units=4, strip=true, panic="unwind"` ; `dev` = `opt-level=1` pour le crate app et `opt-level=3` pour les dépendances (`[profile.dev.package."*"]`).
- [ ] **Step 3 :** lancer `cargo build`. Attendu : `Finished`, sans erreur.

---

## Milestone 1 — Moteur git (`dragontree-git`)

### Task 1: Oid, erreurs, exécution de commandes

**Files:** Create `oid.rs`, `error.rs`, `runner.rs`, `progress.rs` ; Test `tests/runner.rs`

**Interfaces — Produces :**
- `pub struct Oid(pub [u8; 20])` : `from_hex(&str) -> Option<Oid>`, `to_hex() -> String`, `short() -> String` (7 caractères), `Display`, `Debug`, `Hash`, `Ord`, `Copy`.
- `pub enum GitError { NotFound, Spawn(std::io::Error), Failed { cmd: String, code: i32, stderr: String }, DubiousOwnership { path: String }, NotARepository { path: String }, Cancelled, Parse(String), Io(std::io::Error) }`, avec `Display` en français et la méthode `stderr_summary() -> String`.
- `pub struct Git` (Clone, Send + Sync) : `discover() -> Result<Git, GitError>`, `with_exe(PathBuf) -> Git`, `exe() -> &Path`, `version() -> Result<String, GitError>`, `cmd(&self, cwd: &Path) -> GitCommand`.
- `pub struct GitCommand`, builder : `arg`, `args`, `env(k, v)`, `envs(&[(String, String)])`, `stdin(Vec<u8>)`, `cancel(CancelToken)`, `read_only()` (ajoute `GIT_OPTIONAL_LOCKS=0`), `non_interactive()` (ajoute `GCM_INTERACTIVE=never`), `allow_codes(&[i32])`, `display() -> String`, `run() -> Result<Output, GitError>`, `run_streaming(on_stdout: &mut dyn FnMut(&[u8]), on_stderr_line: &mut dyn FnMut(&str)) -> Result<Output, GitError>`.
- `pub struct Output { pub stdout: Vec<u8>, pub stderr: String, pub code: i32 }` : `stdout_str() -> String` (lossy).
- `#[derive(Clone, Default)] pub struct CancelToken(Arc<AtomicBool>)` : `cancel()`, `is_cancelled()`.
- `pub struct Progress { pub phase: String, pub percent: Option<u8>, pub current: u64, pub total: Option<u64> }` et `pub fn parse_progress(line: &str) -> Option<Progress>`.

- [ ] **Step 1 : tests.** `runner_reports_version` : `Git::discover()?.version()` commence par `"git version"`. `runner_failure_has_stderr` : `git rev-parse --verify nope` dans un dossier temporaire initialisé donne `Failed` avec un stderr non vide. `runner_detects_dubious_ownership` : un stderr fixture `"fatal: detected dubious ownership in repository at 'C:/x/y'\n..."` passé à `GitError::from_failure(cmd, 128, stderr)` donne `DubiousOwnership { path: "C:/x/y" }`. `runner_cancel_kills_process` : `git cat-file --batch` sans stdin fermé, annulé après 200 ms, donne `Cancelled` en moins de 2 s. `progress_parses_receiving` : `"Receiving objects:  45% (123/273), 1.2 MiB | 2 MiB/s"` donne `phase="Receiving objects", percent=Some(45), current=123, total=Some(273)`.
- [ ] **Step 2 :** lancer `cargo test -p dragontree-git --test runner`. Attendu : échec de compilation.
- [ ] **Step 3 :** implémenter. Sous Windows : `std::os::windows::process::CommandExt::creation_flags(0x0800_0000)`. stdout et stderr sont lus dans deux threads. La boucle d'attente `try_wait` toutes les 20 ms tue l'enfant si le jeton est annulé. Le stderr est découpé sur `\r` et `\n` pour le streaming.
- [ ] **Step 4 :** relancer les tests. Attendu : PASS.

### Task 2: Status porcelain v2

**Files:** Create `status.rs` ; Test : module `#[cfg(test)]`

**Interfaces — Produces :**
- `pub enum FileState { Unmodified, Modified, Added, Deleted, Renamed, Copied, TypeChanged, Untracked, Ignored }`
- `pub enum ConflictKind { BothModified, AddedByUs, AddedByThem, DeletedByUs, DeletedByThem, BothAdded, BothDeleted }`
- `pub struct StatusEntry { pub path: String, pub orig_path: Option<String>, pub index: FileState, pub worktree: FileState, pub conflict: Option<ConflictKind>, pub is_submodule: bool }`
- `pub struct StatusSnapshot { pub branch: Option<String>, pub head_oid: Option<Oid>, pub upstream: Option<String>, pub ahead: u32, pub behind: u32, pub entries: Vec<StatusEntry> }`, avec les méthodes `staged() -> Vec<&StatusEntry>`, `unstaged() -> Vec<&StatusEntry>` (inclut les non suivis), `conflicted() -> Vec<&StatusEntry>`, `is_clean() -> bool`.
- `pub fn parse_status_v2(bytes: &[u8]) -> Result<StatusSnapshot, GitError>`. Commande associée : `status --porcelain=v2 -z --branch --untracked-files=all`.

- [ ] **Step 1 : tests sur octets fixtures.** `status_ordinary` : `"1 .M N... 100644 100644 100644 <h> <h> src/a.rs\0"` donne index `Unmodified` et worktree `Modified`. `status_rename` : enregistrement `2 R. ... R100 new.rs\0old.rs\0` donne `orig_path=Some("old.rs")` et index `Renamed`. `status_unmerged` : `u UU ...` donne `conflict=Some(BothModified)`. `status_untracked` : `"? dir/n.txt\0"` donne worktree `Untracked`. `status_branch_headers` : `# branch.oid (initial)`, `# branch.head main`, `# branch.ab +2 -1` donnent `head_oid=None`, `ahead=2`, `behind=1`. `status_paths_with_spaces_and_unicode` : `"1 .M ... Peppol - Copie/é tè.cs\0"` est conservé à l'identique.
- [ ] **Step 2 :** implémenter (découpage sur NUL ; un enregistrement `2` consomme le champ NUL suivant).
- [ ] **Step 3 :** lancer `cargo test -p dragontree-git status`. Attendu : PASS.

### Task 3: Refs + HEAD

**Files:** Create `refs.rs`

**Interfaces — Produces :**
- `pub enum HeadState { Branch(String), Detached(Oid), Unborn(String) }`
- `pub struct LocalBranch { pub name: String, pub oid: Oid, pub upstream: Option<String>, pub ahead: u32, pub behind: u32, pub upstream_gone: bool, pub is_head: bool }`
- `pub struct RemoteBranch { pub remote: String, pub name: String, pub full: String, pub oid: Oid, pub is_symbolic: bool }`
- `pub struct TagRef { pub name: String, pub oid: Oid, pub annotated: bool }`
- `pub enum RefKind { Local, Remote, Tag }` et `pub struct RefLabel { pub kind: RefKind, pub name: String, pub is_head: bool }`
- `pub struct RefsSnapshot { pub head: HeadState, pub head_oid: Option<Oid>, pub locals: Vec<LocalBranch>, pub remotes: Vec<RemoteBranch>, pub tags: Vec<TagRef>, pub labels: HashMap<Oid, Vec<RefLabel>> }`
- `pub const FOR_EACH_REF_FORMAT: &str = "%(refname)%1f%(objectname)%1f%(*objectname)%1f%(upstream:short)%1f%(upstream:track,nobracket)%1f%(HEAD)%1f%(symref)";`
- `pub fn read_head(git_dir: &Path) -> Result<HeadState, GitError>` : lit le fichier `HEAD` ; un SHA donne `Detached`, `ref: refs/heads/x` donne `Branch(x)`, à déclasser en `Unborn(x)` par `parse_refs` si la branche n'existe pas.
- `pub fn parse_refs(output: &str, head: HeadState) -> RefsSnapshot`

- [ ] **Step 1 : tests.** `refs_local_with_tracking` : ligne `refs/heads/main␟<oid>␟␟origin/main␟ahead 2, behind 1␟*␟` donne `ahead=2`, `behind=1` et `is_head`. `refs_gone_upstream` : `track = "gone"` donne `upstream_gone`. `refs_annotated_tag_peeled` : `*objectname` non vide donne ce commit comme `oid` et `annotated=true`. `refs_remote_head_symbolic` : `refs/remotes/origin/HEAD` avec un symref donne `is_symbolic`. `refs_labels_grouped_by_oid`. `refs_unborn` : HEAD `Branch("main")` sans branche locale donne `Unborn("main")`.
- [ ] **Step 2 :** implémenter.
- [ ] **Step 3 :** lancer `cargo test -p dragontree-git refs`. Attendu : PASS.

### Task 4: Log incrémental

**Files:** Create `log.rs`

**Interfaces — Produces :**
- `pub struct Commit { pub id: Oid, pub parents: Box<[Oid]>, pub author: Arc<str>, pub author_email: Arc<str>, pub author_time: i64, pub committer: Arc<str>, pub committer_time: i64, pub summary: Box<str> }`
- `pub enum LogScope { All, Head, Revs(Vec<String>) }` ; `All` signifie `--branches --remotes --tags HEAD` (exclut refs/stash). `pub enum LogOrder { Date, Topo }`.
- `pub struct LogOptions { pub scope: LogScope, pub order: LogOrder, pub max_count: Option<usize>, pub paths: Vec<String>, pub follow: bool }`
- `pub const LOG_FORMAT: &str = "%H%x1f%P%x1f%an%x1f%ae%x1f%at%x1f%cn%x1f%ct%x1f%s";`
- `pub fn log_args(opts: &LogOptions) -> Vec<String>` (inclut `-z`, `--format=…`, ordre, scope, `--` et les chemins).
- `pub struct LogParser` : `new()`, `feed(&mut self, chunk: &[u8], out: &mut Vec<Commit>) -> Result<(), GitError>`, `finish(&mut self, out: &mut Vec<Commit>) -> Result<(), GitError>`. Les noms et e-mails sont internés.

- [ ] **Step 1 : tests.** `log_parses_merge_and_root` : un commit à deux parents et un commit racine (`%P` vide). `log_feed_split_anywhere` : la même entrée découpée en morceaux de 1 à 7 octets donne un résultat identique. `log_interns_authors` : `Arc::ptr_eq` sur deux commits du même auteur. `log_args_all_scope` contient `--branches`, `--remotes`, `--tags`, `HEAD` et `--date-order`.
- [ ] **Step 2 :** implémenter.
- [ ] **Step 3 :** lancer `cargo test -p dragontree-git log`. Attendu : PASS.

### Task 5: Parseurs de diff

**Files:** Create `diff.rs`

**Interfaces — Produces :**
- `pub enum LineKind { Context, Added, Removed }`
- `pub struct DiffLine { pub kind: LineKind, pub old_no: Option<u32>, pub new_no: Option<u32>, pub text: String, pub no_newline: bool }` : `text` est brut, sans le préfixe ni le `\n`, mais **avec** un éventuel `\r`.
- `pub struct Hunk { pub old_start: u32, pub old_count: u32, pub new_start: u32, pub new_count: u32, pub header: String, pub lines: Vec<DiffLine> }`
- `pub struct FileDiff { pub old_path: Option<String>, pub new_path: Option<String>, pub header_lines: Vec<String>, pub is_binary: bool, pub is_new: bool, pub is_deleted: bool, pub hunks: Vec<Hunk>, pub truncated: bool }`, avec `display_path() -> &str`.
- `pub fn parse_unified_diff(text: &str) -> Vec<FileDiff>`
- `pub enum ChangeKind { Added, Modified, Deleted, Renamed, Copied, TypeChanged, Unmerged }` et `pub struct FileChange { pub path: String, pub old_path: Option<String>, pub kind: ChangeKind }`
- `pub fn parse_name_status_z(bytes: &[u8]) -> Vec<FileChange>`
- `pub struct DiffOptions { pub context: u32, pub ignore_whitespace: bool }` (Default : 3, false)
- `pub const MAX_DIFF_BYTES: usize = 8 * 1024 * 1024;` au-delà, `truncated = true` et les hunks sont vides.

- [ ] **Step 1 : tests.** `diff_modified_two_hunks` (numéros de ligne old/new exacts). `diff_new_file` (`--- /dev/null`, `is_new`). `diff_deleted`. `diff_rename_with_changes` (`rename from`/`rename to`). `diff_binary` (`Binary files … differ` donne `is_binary`). `diff_no_newline_marker` (`\ No newline at end of file` met `no_newline` sur la ligne précédente). `diff_keeps_cr` (une ligne `+abc\r` garde `text == "abc\r"`). `name_status_rename_z` : `"R087\0old\0new\0M\0x\0"`.
- [ ] **Step 2 :** implémenter.
- [ ] **Step 3 :** lancer `cargo test -p dragontree-git diff`. Attendu : PASS.

### Task 6: Patchs partiels (stage, unstage ou discard de lignes)

**Files:** Create `patch.rs` ; Test `tests/patch_apply.rs`

**Interfaces — Produces :**
- `pub enum PatchMode { Stage, Unstage, Discard }` : `Stage` applique à l'index (`apply --cached`), `Unstage` applique à l'index en inverse (`--cached --reverse`), `Discard` applique au worktree en inverse (`--reverse`). `fn is_reverse(&self) -> bool` (vrai pour Unstage et Discard).
- `pub struct HunkSelection { pub hunk: usize, pub lines: Option<Vec<usize>> }` ; `None` désigne le hunk entier, les indices portent sur `hunk.lines`.
- `pub fn build_patch(diff: &FileDiff, sel: &[HunkSelection], reverse: bool) -> Option<String>`

Règles (le « côté de référence » est l'ancien côté en mode direct et le nouveau en mode inverse) :

```
direct  : +/- sélectionné → gardé ; '-' non sélectionné → contexte ; '+' non sélectionné → supprimé
inverse : +/- sélectionné → gardé ; '+' non sélectionné → contexte ; '-' non sélectionné → supprimé
compteurs recalculés ; hunk sans +/- ignoré ; aucun hunk → None
cas « pas de \n final » : une ligne convertie en contexte qui porte no_newline et qui est suivie
d'ajouts gardés est émise en « -texte » + marqueur puis « +texte » (avec \n) avant ces ajouts
(et inversement en mode reverse)
```

- [ ] **Step 1 : tests unitaires.** `patch_forward_drops_unselected_adds`, `patch_reverse_keeps_unselected_adds_as_context`, `patch_whole_hunk`, `patch_header_counts`, `patch_none_when_empty`.
- [ ] **Step 2 : tests d'intégration** (dépôt temporaire, `Repo::apply_patch` de la Task 8, via `git apply` en attendant). `stage_selected_lines_only` : après application, `git diff --cached` ne contient que la ligne choisie. `unstage_selected_lines_only`. `discard_selected_lines_only` (le fichier de travail perd seulement ces lignes). `stage_lines_preserves_crlf` : un fichier CRLF commité tel quel avec `core.autocrlf=false` garde `\r\n` dans l'index (`git show :f`). `stage_lines_no_newline_at_eof`.
- [ ] **Step 3 :** implémenter.
- [ ] **Step 4 :** lancer `cargo test -p dragontree-git patch`. Attendu : PASS.

### Task 7: Disposition du graphe

**Files:** Create `graph.rs`

**Interfaces — Produces :**
- `pub struct GraphRow { pub lane: u16, pub color: u8, pub is_merge: bool }`
- `pub struct Edge { pub child_row: u32, pub parent_row: u32 /* u32::MAX = inconnu */, pub child_lane: u16, pub lane: u16, pub parent_lane: u16, pub color: u8 }`
- `pub struct GraphLayout { pub rows: Vec<GraphRow>, pub edges: Vec<Edge>, pub max_lane: u16 }` : `edges_in(&self, first: u32, last: u32) -> impl Iterator<Item = &Edge>` donne les arêtes qui croisent l'intervalle.
- `pub struct GraphBuilder` : `new()`, `push(&mut self, id: Oid, parents: &[Oid])`, `finish(self) -> GraphLayout`.
- `pub const PALETTE_SIZE: u8 = 10;`

Algorithme (voies fixes, recyclage du premier emplacement libre) :

```
lanes: Vec<Option<Oid>>, lane_color: Vec<u8>, pending: HashMap<Oid, SmallVec<u32>>
pour chaque commit (row) :
  matching = voies attendant ce commit ; node_lane = min(matching) ou premier libre
  color = couleur de node_lane si matching non vide, sinon nouvelle couleur
  arêtes de pending[id] : parent_row = row, parent_lane = node_lane
  vider les voies de matching
  parent 0 : lanes[node_lane] = p ; arête (lane = node_lane, color)
  parent k>0 : si une voie attend déjà p → arête vers cette voie (sa couleur),
               sinon premier libre, nouvelle couleur
  ajouter chaque arête à pending[p] ; supprimer les voies None en fin de vecteur
```

- [ ] **Step 1 : tests.** `graph_linear` : 3 commits sur une seule voie, 2 arêtes. `graph_branch_merge` : merge M(A,B), A→C, B→C ; B est sur la voie 1, les deux voies convergent en C. `graph_fork_converges_at_parent` : deux têtes avec le même parent ont des arêtes sur des voies distinctes et le même `parent_row`. `graph_unknown_parent_open_edge` : `parent_row == u32::MAX`. `graph_lane_reuse` : une voie libérée est réutilisée. `graph_memory_is_linear` : 100 k commits synthétiques sur 50 branches donnent `edges.len() < 1,3 × commits`.
- [ ] **Step 2 :** implémenter.
- [ ] **Step 3 :** lancer `cargo test -p dragontree-git graph`. Attendu : PASS.

### Task 8: Repo — lectures, écritures, réseau

**Files:** Create `repo.rs`, `ops.rs`, `net.rs`, `stash.rs` ; Test `tests/repo_ops.rs`, `tests/net_ops.rs`

**Interfaces — Produces** (toutes les méthodes renvoient `Result<_, GitError>`) :
- `pub struct Repo { pub git: Git, pub workdir: PathBuf, pub git_dir: PathBuf, pub common_dir: PathBuf }`
- `Repo::open(git: &Git, path: &Path)` (appelle `rev-parse --show-toplevel --absolute-git-dir --git-common-dir`).
- Lectures : `refs() -> RefsSnapshot`, `status() -> StatusSnapshot`, `log(opts: &LogOptions, cancel: &CancelToken) -> Vec<Commit>`, `commit_info(id: Oid) -> CommitInfo { id, parents, author, author_email, author_time, committer, committer_email, committer_time, message }`, `commit_files(id: Oid, parent: Option<Oid>) -> Vec<FileChange>`, `diff_commit_file(id: Oid, parent: Option<Oid>, change: &FileChange, o: &DiffOptions) -> FileDiff`, `diff_worktree_file(path: &str, staged: bool, untracked: bool, o: &DiffOptions) -> FileDiff`, `stashes() -> Vec<StashEntry { index: usize, oid: Oid, message: String, time: i64 }>`, `remotes() -> Vec<Remote { name, fetch_url, push_url }>`, `submodules() -> Vec<Submodule { path, oid, status_char }>`, `operation_state() -> OpState { None, Merge, Rebase { interactive: bool }, CherryPick, Revert }`, `last_commit_message() -> String`, `blob(rev: &str, path: &str) -> Vec<u8>`, `config(key: &str) -> Option<String>`.
- Écritures : `stage(paths: &[String])` (`add -A --`), `unstage(paths)` (`restore --staged --` ; en dépôt sans HEAD : `rm --cached -r --`), `discard(tracked: &[String], untracked: &[String])`, `apply_patch(patch: &str, mode: PatchMode)` (`apply [--cached] [--reverse] --recount --whitespace=nowarn -`), `intent_to_add(path)`, `commit(message: &str, amend: bool)` (`commit -F -`), `checkout(target: &str)`, `checkout_new_tracking(remote_branch: &str, local: &str)`, `create_branch(name, start: &str, checkout: bool)`, `rename_branch(old, new)`, `delete_branch(name, force: bool)`, `merge(rev: &str, mode: MergeMode { Default, NoFf, FfOnly, Squash })`, `rebase(onto: &str)`, `cherry_pick(ids: &[Oid])`, `revert(id: Oid)`, `reset(rev: &str, mode: ResetMode { Soft, Mixed, Hard })`, `continue_op(state: OpState)`, `abort_op(state: OpState)`, `create_tag(name, rev, message: Option<&str>)`, `delete_tag(name)`, `stash_save(message: Option<&str>, include_untracked: bool, keep_index: bool)`, `stash_apply(i)`, `stash_pop(i)`, `stash_drop(i)`, `add_remote(name, url)`, `remove_remote(name)`, `set_upstream(local, remote_branch)`.
- Réseau (`net.rs`) : `pub struct NetOptions { pub env: Vec<(String, String)>, pub cancel: CancelToken, pub non_interactive: bool }` ; `Repo::fetch(remote: Option<&str>, prune: bool, n: &NetOptions, on: &mut dyn FnMut(Progress))`, `pull(remote: &str, branch: &str, rebase: bool, n, on)`, `push(remote: &str, local: &str, remote_branch: &str, set_upstream: bool, force_with_lease: bool, n, on)`, `push_tag(remote, tag, n, on)`, `delete_remote_branch(remote, branch, n, on)`. Fonctions libres : `pub fn clone_repo(git: &Git, url: &str, dest: &Path, branch: Option<&str>, recurse: bool, n: &NetOptions, on: &mut dyn FnMut(Progress)) -> Result<(), GitError>`, `pub fn init_repo(git: &Git, path: &Path) -> Result<(), GitError>`, `pub fn trust_directory(git: &Git, path: &Path) -> Result<(), GitError>`.

- [ ] **Step 1 : tests d'intégration** (helper `TestRepo::new()` : `git init -b main` dans un tempdir avec user.name et user.email locaux). `open_from_subdir_finds_toplevel`. `open_reports_dubious_ownership` (simulé par la fixture de la Task 1, plus un vrai test ignoré par défaut). `stage_unstage_commit_roundtrip`. `amend_changes_message`. `branch_create_checkout_rename_delete`. `merge_conflict_sets_operation_state` (deux branches modifient la même ligne : `OpState::Merge` et une entrée `conflicted()`, puis `abort_op` rend un état propre). `stash_save_list_pop`. `tag_annotated_and_delete`. `reset_hard_moves_head`. `discard_restores_file_and_removes_untracked`. `log_and_graph_on_real_repo`. Réseau avec un remote bare local (`file://`) : `clone_fetch_push_pull`, `push_rejected_returns_stderr` (non fast-forward : `Failed` dont le stderr contient `rejected`).
- [ ] **Step 2 :** implémenter.
- [ ] **Step 3 :** lancer `cargo test -p dragontree-git --tests`. Attendu : PASS.

### Task 9: Découverte des dépôts

**Files:** Create `discovery.rs` ; Test `tests/discovery.rs`

**Interfaces — Produces :**
- `pub enum RepoKind { Normal, Worktree, Submodule }`
- `#[derive(Serialize, Deserialize, Clone)] pub struct FoundRepo { pub path: PathBuf, pub name: String, pub root: PathBuf, pub kind: RepoKind }`
- `pub struct ScanOptions { pub max_depth: usize /*8*/, pub skip_dirs: Vec<String>, pub include_submodules: bool }` ; `Default` liste `node_modules, target, bin, obj, packages, .vs, .idea, .vscode, .gradle, .venv, venv, __pycache__, .nuget, dist, .next, .cache`.
- `pub fn scan(roots: &[PathBuf], opts: &ScanOptions, cancel: &CancelToken, on_found: &(dyn Fn(FoundRepo) + Sync)) -> Vec<FoundRepo>` : walker parallèle `ignore::WalkBuilder` (`standard_filters(false)`, `hidden(false)`) ; la détection de `.git` (dossier ou fichier) se fait dans `filter_entry`, qui ne descend jamais dans `.git`. Résultat trié par chemin.
- `pub fn read_head_branch(repo_path: &Path) -> Option<String>` (suit un fichier `.git` `gitdir:`), sans processus.

- [ ] **Step 1 : tests.** `discovery_finds_nested` (A, A/libs/B, C/D/E). `discovery_skips_node_modules`. `discovery_classifies_worktree_and_submodule` (fichiers `.git` avec `gitdir: …/worktrees/x` et `…/modules/y`). `discovery_respects_max_depth`. `discovery_handles_spaces` (`Peppol - Copie`). `read_head_branch_follows_gitdir_file`.
- [ ] **Step 2 :** implémenter.
- [ ] **Step 3 :** lancer `cargo test -p dragontree-git --test discovery`. Attendu : PASS.

### Task 10: Blame, rebase interactif, conflits (moteur)

**Files:** Create `blame.rs`, `rebase.rs`, `conflict.rs`

**Interfaces — Produces :**
- `pub struct BlameLine { pub commit: Oid, pub line_no: u32, pub text: String }` et `pub struct BlameCommit { pub id: Oid, pub author: String, pub author_time: i64, pub summary: String }`. `pub fn parse_blame_porcelain(text: &str) -> (Vec<BlameLine>, HashMap<Oid, BlameCommit>)`. `Repo::blame(path: &str, rev: Option<Oid>)`.
- `pub enum RebaseAction { Pick, Reword(String), Edit, Squash, Fixup, Drop }`, `pub struct RebaseItem { pub id: Oid, pub action: RebaseAction, pub summary: String }`. `pub fn build_todo(items: &[RebaseItem], msg_dir: &Path) -> std::io::Result<String>` : un `Reword` devient `pick <id>` puis `exec git commit --amend --only --no-verify -F "<fichier>"` ; `Drop` est omis. `Repo::interactive_rebase(base: &str, items: &[RebaseItem])` lance `rebase -i` avec `GIT_SEQUENCE_EDITOR` = l'exe courant `--write-todo <fichier>` et `GIT_EDITOR=true`. `Repo::rebase_candidates(base: &str) -> Vec<Commit>`.
- `pub struct ConflictRegion { pub start_line: usize, pub ours: Vec<String>, pub base: Option<Vec<String>>, pub theirs: Vec<String>, pub end_line: usize }`, `pub fn parse_conflicts(text: &str) -> Vec<ConflictRegion>`, `pub enum Resolution { Ours, Theirs, Both, BothReversed }`, `pub fn resolve(text: &str, choices: &[Resolution]) -> String`. `Repo::checkout_side(path, ours: bool)` (`checkout --ours|--theirs --`).

- [ ] **Step 1 : tests.** `blame_porcelain_two_commits`. `todo_reword_uses_exec`. `todo_drop_omitted`. `interactive_rebase_squash_and_reword` (intégration : trois commits deviennent deux, avec le message attendu). `conflict_parse_diff3`. `conflict_resolve_both`.
- [ ] **Step 2 :** implémenter. Le mode `--write-todo` de l'exe est géré dans `dragontree-app/src/main.rs`, avant toute UI : copier le fichier source vers le chemin passé par git.
- [ ] **Step 3 :** lancer `cargo test -p dragontree-git`. Attendu : PASS.

---

## Milestone 2 — Forges (`dragontree-forges`) — en parallèle (sous-agent)

### Task 11: Comptes, API, secrets, auth git

**Files:** tous les fichiers de `crates/dragontree-forges` ; Test `tests/fixtures/*.json` + tests unitaires

**Interfaces — Produces :**
- `#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)] pub enum ForgeKind { GitHub, GitLab, AzureDevOps }`, avec `label() -> &'static str` et `default_host() -> &'static str` (`github.com`, `gitlab.com`, `dev.azure.com`).
- `#[derive(Serialize, Deserialize, Clone, Debug)] pub struct Account { pub id: String, pub kind: ForgeKind, pub host: String, pub username: String, pub display_name: String, pub orgs: Vec<String>, pub avatar_url: Option<String> }`
- `pub struct VerifiedUser { pub username: String, pub display_name: String, pub avatar_url: Option<String>, pub orgs: Vec<String> }`
- `#[derive(Clone, Debug)] pub struct RemoteRepo { pub account_id: String, pub name: String, pub namespace: String, pub clone_url: String, pub ssh_url: Option<String>, pub web_url: String, pub description: Option<String>, pub private: bool, pub default_branch: Option<String>, pub updated_at: Option<String> }`
- `pub enum ForgeError { Http { status: u16, body: String }, Unauthorized, Network(String), Parse(String), Secret(String) }` (Display en français).
- `pub fn verify(kind: ForgeKind, host: &str, token: &str, orgs_hint: &[String]) -> Result<VerifiedUser, ForgeError>`
- `pub fn list_repos(account: &Account, token: &str) -> Result<Vec<RemoteRepo>, ForgeError>`
- `pub fn git_auth_env(account: &Account, token: &str) -> Vec<(String, String)>`
- `pub fn account_matches_url(account: &Account, url: &str) -> bool`
- `pub fn token_creation_url(kind: ForgeKind, host: &str, org: Option<&str>) -> String`
- `pub fn new_account_id() -> String` (16 caractères hexadécimaux aléatoires, à partir de l'horloge et d'un compteur)
- `secrets` : `store_token(account_id: &str, token: &str)`, `load_token(account_id: &str) -> Result<Option<String>, ForgeError>`, `delete_token(account_id: &str)`.
- Réservé pour plus tard (PR centralisées) : `list_pull_requests`, à ne pas implémenter.

Détails API :

```
GitHub   api = https://api.github.com (github.com) | https://<host>/api/v3
         Authorization: Bearer <tok> ; GET /user ; GET /user/orgs ; GET /user/repos?per_page=100&page=N&sort=updated&affiliation=owner,collaborator,organization_member
GitLab   api = https://<host>/api/v4 ; PRIVATE-TOKEN: <tok> ; GET /user ; GET /projects?membership=true&simple=true&per_page=100&page=N&order_by=last_activity_at
Azure    Authorization: Basic base64(":"+tok)
         GET https://app.vssps.visualstudio.com/_apis/profile/profiles/me?api-version=7.1
         GET https://app.vssps.visualstudio.com/_apis/accounts?memberId=<publicAlias>&api-version=7.1
         repli si PAT limité à une organisation : GET https://dev.azure.com/<org>/_apis/connectionData
         GET https://dev.azure.com/<org>/_apis/git/repositories?api-version=7.1 (pour chaque org)
         statut 203 ou réponse HTML → Unauthorized
Auth git (variables d'environnement) : GIT_CONFIG_COUNT=n, GIT_CONFIG_KEY_i=http.<origine>.extraHeader,
         GIT_CONFIG_VALUE_i="Authorization: Basic <b64(user:tok)>" ; user = login (GitHub/GitLab), vide (Azure)
         origines : https://<host>/ ; Azure : https://dev.azure.com/<org>/ et https://<org>.visualstudio.com/ par org
```

- [ ] **Step 1 : tests.** `github_repos_fixture_parses`, `gitlab_projects_fixture_parses`, `azure_repositories_fixture_parses` (namespace = `org/projet`), `azure_203_is_unauthorized`, `auth_env_github` (KEY_0 = `http.https://github.com/.extraHeader`, VALUE_0 décodé = `Authorization: Basic base64(login:tok)`), `auth_env_azure_two_orgs` (COUNT=4), `matches_url_azure_org_and_vs_host`, `matches_url_github_enterprise_host`, `token_url_github_has_scopes`. Un test réseau réel `#[ignore]` lit `AZURE_DEVOPS_EXT_PAT` s'il est présent : `verify` puis `list_repos`, sans afficher le jeton.
- [ ] **Step 2 :** implémenter (ureq 3 `Agent` avec timeouts de 10 s pour la connexion et 30 s au total, native-tls).
- [ ] **Step 3 :** lancer `cargo test -p dragontree-forges`. Attendu : PASS.

---

## Milestone 3 — Application

### Task 12: Squelette de l'app

**Files:** Create `main.rs`, `app.rs`, `settings.rs`, `theme.rs`, `icons.rs`, `jobs.rs`, `toasts.rs`, `tabbar.rs`, `welcome.rs`, `widgets.rs`

**Interfaces — Produces :**
- `Settings` (serde, `#[serde(default)]`) : `theme: ThemeChoice { System, Light, Dark }`, `git_path: Option<PathBuf>`, `scan_roots: Vec<PathBuf>`, `scan_max_depth: usize`, `scan_skip_dirs: Vec<String>`, `show_submodules: bool`, `status_sweep: bool`, `default_clone_dir: Option<PathBuf>`, `open_tabs: Vec<PathBuf>`, `active_tab: usize`, `recent_repos: Vec<PathBuf>`, `accounts: Vec<Account>`, `repo_accounts: BTreeMap<String, String>`, `diff_context: u32`, `ignore_whitespace: bool`, `split_diff: bool`, `ui_scale: f32`, `auto_fetch_minutes: u32`, `repo_list_visible: bool`, `favorites: Vec<PathBuf>`, `hidden_repos: Vec<PathBuf>`, `push_after_commit: bool`. `Settings::load() -> Settings`, `save(&self)` (écriture atomique : fichier `.tmp` puis renommage).
- `jobs.rs` : `pub struct Jobs { tx: Sender<AppEvent>, ctx: egui::Context }`, `spawn(&self, name: &str, f: impl FnOnce() -> AppEvent + Send + 'static)`. `pub struct SerialWorker` : `new(ctx, tx) -> SerialWorker`, `submit(&self, f: impl FnOnce() -> AppEvent + Send + 'static)` ; un thread par dépôt, exécution dans l'ordre. `pub enum AppEvent { Repo { tab: TabId, ev: RepoEvent }, Discovery(DiscoveryEvent), Forge(ForgeEvent), Clone(CloneEvent), Toast(Toast) }`.
- `main.rs` : options eframe (glow, titre « DragonTree », taille 1400×900, `persist_window: true`). CLI : `--screenshot <png> [--after-ms N]`, `--open <chemin>`, `--write-todo <src> <dest>`. Polices : Segoe UI (`C:\Windows\Fonts\segoeui.ttf`, gras `segoeuib.ttf`), mono Cascadia Mono, sinon Consolas, sinon Hack ; Phosphor en repli des familles proportionnelle et mono.
- Barre d'onglets : onglets repo et « + ». Ctrl+T, Ctrl+W, Ctrl+Tab, Ctrl+Maj+Tab, Ctrl+1…9, clic molette pour fermer, glisser pour réordonner, menu contextuel (Fermer, Fermer les autres, Ouvrir dans l'Explorateur, Copier le chemin).
- Page d'accueil : boutons Ouvrir…, Cloner…, Initialiser…, Ajouter un dossier à scanner…, et liste des dépôts récents.

- [ ] **Step 1 : test** `serial_worker_preserves_order` : 100 tâches qui poussent i dans un `Vec` partagé donnent `0..100` dans l'ordre.
- [ ] **Step 2 :** implémenter.
- [ ] **Step 3 :** lancer `cargo run -p dragontree-app -- --screenshot shot.png --after-ms 1500` puis inspecter l'image (onglet d'accueil et barre droite visibles).

### Task 13: Barre droite — dépôts découverts

**Files:** Create `repo_list.rs`

**Interfaces — Produces :**
- `pub struct RepoListState { pub repos: Vec<ListedRepo>, pub scanning: bool, pub filter: String, pub last_scan: Option<SystemTime> }`, `pub struct ListedRepo { pub found: FoundRepo, pub branch: Option<String>, pub dirty: Option<u32>, pub ahead: u32, pub behind: u32 }`.
- `DiscoveryEvent { Found(FoundRepo), Finished(Vec<FoundRepo>), Status { path: PathBuf, dirty: u32, ahead: u32, behind: u32, branch: Option<String> } }`.
- Comportement : le cache est chargé au démarrage, puis un scan tourne en arrière-plan à chaque démarrage et à la demande (⟳). Ensuite vient le balayage de statut (4 en parallèle, `status --porcelain=v2 --branch --untracked-files=normal`, read_only, non_interactive), si activé. La recherche floue (sous-séquence, insensible à la casse et aux accents) porte sur le nom et le chemin relatif. Affichage : favoris en premier, puis groupes par dossier racine. Chaque ligne montre une icône selon le type, le nom, le chemin relatif parent en grisé, la branche à droite, ● s'il y a des modifications et ↑n ↓n. Un clic ouvre le dépôt ou active son onglet existant. Menu contextuel : Ouvrir dans un nouvel onglet, Explorateur, Terminal, Copier le chemin, Épingler/Désépingler, Masquer. Boutons : ajouter un dossier racine (rfd), rescanner. Le panneau est redimensionnable et masquable (Ctrl+B).

- [ ] **Step 1 :** implémenter.
- [ ] **Step 2 :** ajouter `DEV` comme racine, faire une capture et vérifier que les 13 dépôts apparaissent (y compris ceux en « dubious ownership »).

### Task 14: Onglet dépôt — chargement, barre latérale gauche, barre d'outils

**Files:** Create `repo/mod.rs`, `repo/sidebar.rs`, `repo/toolbar.rs`, `repo/actions.rs`

**Interfaces — Produces :**
- `pub struct RepoTab { pub id: TabId, pub path: PathBuf, pub name: String, pub repo: Option<Arc<Repo>>, pub open_error: Option<GitError>, pub refs: Option<Arc<RefsSnapshot>>, pub status: Option<Arc<StatusSnapshot>>, pub stashes: Vec<StashEntry>, pub remotes: Vec<Remote>, pub op_state: OpState, pub view: RepoView /* Changes | History */, pub history: HistoryState, pub changes: ChangesState, pub running: Vec<RunningOp>, pub dialog: Option<Dialog>, pub gen: u64, worker: SerialWorker }`.
- `RepoEvent { Opened(Result<Arc<Repo>, GitError>), Refs(u64, RefsSnapshot), Status(u64, StatusSnapshot), Log { gen: u64, commits: Arc<Vec<Commit>>, graph: Arc<GraphLayout>, partial: bool }, CommitInfo(u64, CommitInfo, Vec<FileChange>), Diff(u64, DiffTarget, Result<FileDiff, GitError>), Stashes(Vec<StashEntry>), Remotes(Vec<Remote>), OpDone { op: String, result: Result<String, GitError>, refresh: Refresh }, Progress { op: String, p: Progress } }`.
- `RepoTab::refresh(&mut self, what: Refresh { refs, status, log, stashes })`. Deux temps pour le log : `max_count = 2000` d'abord, puis le complet.
- Barre latérale : entrées « Modifications locales (n) », « Tous les commits », puis sections Branches (arbre par `/`, branche courante en gras avec ✓, ↑↓), Remotes, Tags, Stashes, Sous-modules ; filtre en haut. Double-clic sur une branche : checkout. Menus contextuels branchés sur `actions.rs`.
- Barre d'outils : Fetch, Pull, Push, Stash, Pop, Nouvelle branche, Terminal (`wt.exe -d <path>`, sinon `cmd /K cd /d`), Explorateur, VS Code (si `code` est dans le PATH). Une animation tourne pendant les opérations.

- [ ] **Step 1 :** implémenter, avec le bandeau d'erreur et le bouton « Faire confiance à ce dépôt » pour `DubiousOwnership`.
- [ ] **Step 2 :** lancer `--open C:\Users\moi\DEV\cockpit --screenshot` puis inspecter.

### Task 15: Historique — graphe, détails, diff

**Files:** Create `repo/history.rs`, `repo/graph_paint.rs`, `repo/details.rs`, `repo/diff_view.rs`

**Interfaces — Produces :**
- `pub struct HistoryState { pub commits: Arc<Vec<Commit>>, pub graph: Arc<GraphLayout>, pub index: HashMap<Oid, usize>, pub loading: bool, pub selected: Option<usize>, pub filter: String, pub filtered: Option<Vec<usize>>, pub info: Option<(CommitInfo, Vec<FileChange>)>, pub selected_file: Option<usize>, pub diff: Option<FileDiff>, pub graph_width: f32 }`
- `graph_paint::paint_row(painter, rect, row: u32, layout: &GraphLayout, lane_w: f32, palette: &[Color32], clip_lanes: u16)` : traits verticaux, courbes de Bézier pour les diagonales, nœud plein (cercle évidé si HEAD).
- Tableau virtualisé (`ScrollArea::show_rows`) : colonnes Graphe (largeur réglable) | badges de refs + sujet | Auteur | Date (locale, relative si < 7 jours) | SHA court. Navigation clavier haut/bas.
- Le filtre (message, auteur, SHA) masque le graphe pendant qu'il est actif.
- Panneau du bas : partie gauche avec infos du commit (SHA copiable, parents cliquables, auteur, date, message complet) et liste des fichiers ; partie droite avec le diff du fichier sélectionné.
- `diff_view::show(ui, diff: &FileDiff, opts: &DiffViewOpts, sel: &mut LineSelection) -> Option<DiffAction>`, avec `pub enum DiffAction { StageHunk(usize), UnstageHunk(usize), DiscardHunk(usize), StageLines, UnstageLines, DiscardLines }` (actions désactivées en mode lecture seule pour l'historique). Mise en évidence intra-ligne avec `similar::TextDiff::from_chars` sur les paires -/+.

- [ ] **Step 1 :** implémenter.
- [ ] **Step 2 :** ouvrir le gros dépôt, mesurer le temps jusqu'au premier affichage (journal) et la mémoire, puis faire une capture.

### Task 16: Changements locaux

**Files:** Create `repo/changes.rs`

**Interfaces — Produces :**
- `pub struct ChangesState { pub selected: Option<(Side, String)>, pub multi: BTreeSet<(Side, String)>, pub diff: Option<FileDiff>, pub line_sel: LineSelection, pub subject: String, pub body: String, pub amend: bool, pub push_after: bool }`, `pub enum Side { Unstaged, Staged }`.
- Disposition Fork : colonne gauche avec « Non indexés » (liste plus « Tout indexer »), « Indexés » (liste plus « Tout désindexer ») et la boîte de commit (sujet, description, Amend, Pousser après, bouton « Commit (n fichiers) », Ctrl+Entrée) ; à droite le diff avec actions hunk et ligne.
- Interactions : double-clic sur un fichier pour stage/unstage, Espace pour basculer, glisser entre les listes, menu contextuel (Indexer, Désindexer, Annuler les modifications…, Ignorer (.gitignore), Ouvrir, Afficher dans l'Explorateur, Copier le chemin, Historique du fichier, Blame).
- Bandeau d'opération (merge, rebase, cherry-pick, revert) avec Continuer et Annuler ; la section Conflits ouvre la Task 22.

- [ ] **Step 1 :** implémenter.
- [ ] **Step 2 :** dans un dépôt temporaire, indexer 2 lignes d'un hunk via l'UI puis vérifier avec `git diff --cached`.

### Task 17: Opérations, dialogues, surveillance

**Files:** Create `repo/dialogs.rs`, `repo/watcher.rs`, `toasts.rs` (journal)

**Interfaces — Produces :**
- `pub enum Dialog { CreateBranch { start: String, name: String, checkout: bool }, RenameBranch { old, new }, DeleteBranch { name, force: bool, remote_too: bool }, CreateTag { rev, name, message, push: bool }, Merge { rev, mode }, Rebase { onto }, Reset { rev, mode }, CherryPick { ids }, Revert { id }, Stash { message, untracked: bool, keep_index: bool }, Push { remote, local, remote_branch, set_upstream, force_with_lease, tags }, Pull { remote, branch, rebase: bool }, Fetch { all: bool, prune: bool }, Discard { paths }, AddRemote { name, url }, Confirm { title, text, action } }`. Validation de nom de branche ou de tag via `git check-ref-format --branch`.
- Journal : chaque commande lancée est enregistrée (`display()`, code, durée, stderr), dans la limite de 500 entrées. Panneau « Journal git » activable. Une erreur produit un toast avec un bouton « Détails ».
- `RepoWatcher` : `notify` récursif plus anti-rebond de 400 ms. Les événements dans `.git/objects` et les `*.lock` sont ignorés. `.git/{HEAD,refs,packed-refs,index}` déclenche refs + status, le worktree déclenche status. Un onglet inactif est seulement marqué et se rafraîchit quand on l'active. Rafraîchissement aussi au retour du focus fenêtre.
- Auto-fetch : toutes les `auto_fetch_minutes` pour les onglets ouverts (non_interactive, silencieux sauf erreur d'auth).

- [ ] **Step 1 :** implémenter.
- [ ] **Step 2 :** sur un dépôt temporaire avec un remote bare, enchaîner créer branche → commit → push (set upstream) → modifier côté remote → fetch, et vérifier ↓1 dans la barre latérale.

---

## Milestone 4 — Comptes et clonage

### Task 18: Fenêtres Comptes et Cloner

**Files:** Create `accounts.rs`, `clone.rs`

**Interfaces — Consumes :** Task 11 (`verify`, `list_repos`, `git_auth_env`, `secrets`), Task 8 (`clone_repo`, `init_repo`).
- Comptes : liste avec icône de forge, nom, login, hôte, organisations et état ✓/✗. Formulaire d'ajout : type, hôte (prérempli), jeton (masqué), organisations Azure (facultatif, détectées automatiquement), lien « Créer un jeton » (`token_creation_url`) avec les droits recommandés affichés. Boutons Vérifier et Supprimer (le compte et le secret). Vérification en arrière-plan, puis enregistrement des métadonnées dans les réglages et du jeton dans keyring.
- Cloner : onglet « Depuis mes comptes » (liste agrégée de tous les comptes ou d'un seul, recherche, colonnes nom, espace, privé, mise à jour) et onglet « URL ». Destination : dossier parent (par défaut `default_clone_dir` ou la première racine) et nom de dossier, avec aperçu du chemin final et alerte s'il existe. Options : branche, sous-modules, ouvrir après. Pendant le clonage : barre de progression et bouton Annuler (suppression du dossier seulement s'il a été créé par ce clone). Ensuite : `repo_accounts[chemin] = compte`, ouverture d'un onglet et ajout à la liste de droite.
- Injection d'auth pour fetch, pull et push si `repo_accounts` contient le dépôt.

- [ ] **Step 1 :** implémenter.
- [ ] **Step 2 :** ajouter un compte Azure DevOps avec le PAT de l'environnement (sans l'afficher), lister les dépôts, cloner un petit dépôt dans le scratchpad, vérifier l'ouverture de l'onglet, puis supprimer le compte de test et son secret.

---

## Milestone 5 — Fonctions avancées

### Task 19: Diff côte à côte, coloration syntaxique, images

**Files:** Modify `repo/diff_view.rs` ; Create `highlight.rs`
- `highlight.rs` : `SyntaxSet::load_defaults_newlines()` chargé paresseusement (`OnceLock`) au premier diff, choix de la syntaxe selon l'extension, cache LRU des lignes colorées par fichier (≤ 64 fichiers). Thème adapté au clair et au sombre. Désactivé au-delà de 20 000 lignes.
- Vue côte à côte : alignement des blocs -/+ par paires, lignes vides de remplissage, défilement synchronisé.
- Images (png, jpg, gif, bmp, webp, ico) : ancienne et nouvelle version via `Repo::blob`, affichées côte à côte avec leurs dimensions.

### Task 20: Historique et blame de fichier

**Files:** Create `repo/blame.rs`
- Fenêtre « Historique de <fichier> » : `log --follow -- <path>` à gauche, diff du fichier pour le commit choisi à droite. Bascule vers « Blame » : gouttière avec SHA court, auteur et date (couleur par commit), clic pour sélectionner le commit dans l'historique.

### Task 21: Éditeur de rebase interactif

**Files:** Create `repo/rebase_ui.rs`
- Entrée par le menu d'un commit : « Rebase interactif depuis ici ». Liste réordonnable par glisser-déposer, avec action par ligne (pick, reword avec éditeur de message, edit, squash, fixup, drop). Bouton Lancer : `Repo::interactive_rebase`, puis bandeau d'état si l'opération s'arrête.

### Task 22: Résolution de conflits

**Files:** Create `repo/conflicts.rs`
- Pour chaque fichier en conflit : « Garder la mienne », « Garder la leur », « Marquer comme résolu » (stage), « Ouvrir l'outil de merge externe » (`git mergetool --no-prompt -- <path>`, dans un thread). Éditeur intégré : chaque région de conflit est montrée avec les deux côtés et des boutons Mienne, Leur, Les deux, avec aperçu du résultat. Enregistrer écrit le fichier puis l'indexe.

### Task 23: Ctrl+P, sous-modules, réglages

**Files:** Create `quick_open.rs`, `settings_ui.rs`
- Ctrl+P : palette floue sur les dépôts découverts, les branches de l'onglet courant (checkout) et les commandes (Fetch, Pull, Push, Nouvelle branche…).
- Réglages : thème, échelle, chemin git, dossiers racines, profondeur, dossiers ignorés, sous-modules, balayage de statut, dossier de clonage par défaut, contexte et espaces du diff, auto-fetch, push après commit.
- Sous-modules : liste dans la barre latérale ; double-clic pour l'ouvrir dans un nouvel onglet ; menu Mettre à jour (`submodule update --init --recursive`).

---

## Milestone 6 — Finition et vérification

### Task 24: Mesures, README, revue finale

- [ ] Mesurer la RAM privée (PowerShell `PrivateMemorySize64`) avec 3 onglets (gros dépôt, cockpit, Peppol) et le temps de chargement de l'historique du gros dépôt. Attendu : < 150 Mo et < 2 s.
- [ ] `cargo clippy --workspace` sans erreur, et `cargo test --workspace` vert.
- [ ] README (FR) : prérequis, `scripts/setup-toolchain.ps1`, build, lancement, raccourcis, comptes et jetons (droits recommandés), limites connues.
- [ ] Revue de l'ensemble par un sous-agent neuf, puis correction des points confirmés.
