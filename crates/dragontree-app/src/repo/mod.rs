//! Onglet d'un dépôt : barre d'outils, navigation (branches…), historique et changements locaux.

mod actions;
mod blame;
mod changes;
mod conflicts;
mod details;
mod dialogs;
pub mod diff_view;
mod graph_paint;
mod history;
mod image_diff;
mod rebase_ui;
mod sidebar;
mod toolbar;
mod watcher;

use std::collections::{BTreeSet, HashMap};
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use dragontree_git::graph::{GraphBuilder, GraphLayout};
use dragontree_git::{
    CancelToken, Commit, CommitInfo, DiffOptions, FileChange, FileDiff, GitError, HeadState, LogOptions, Oid, OpState,
    Progress, RefsSnapshot, Remote, Repo, StashEntry, StatusSnapshot, Submodule,
};
use dragontree_i18n::{tr, trn};
use egui::{Frame, RichText, Ui};

use crate::app::{AppAction, AppEvent, Shared, TabId};
use crate::icons;
use crate::repo_list::QuickStatus;
use crate::theme::Palette;
use crate::toasts::Toast;

pub use blame::FileHistoryWindow;
pub use dialogs::Dialog;
pub use diff_view::DiffViewState;

/// Nombre de commits du premier affichage (le reste suit en arrière-plan).
const FIRST_BATCH: usize = 2000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepoView {
    Changes,
    History,
}

/// Ce qu'il faut recharger après une opération ou un changement sur disque.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Refresh {
    pub refs: bool,
    pub status: bool,
    pub log: bool,
    pub extras: bool,
}

impl Refresh {
    pub const NONE: Refresh = Refresh { refs: false, status: false, log: false, extras: false };
    pub const STATUS: Refresh = Refresh { refs: false, status: true, log: false, extras: false };
    pub const REFS: Refresh = Refresh { refs: true, status: true, log: false, extras: false };
    pub const ALL: Refresh = Refresh { refs: true, status: true, log: true, extras: true };

    pub fn merge(self, o: Refresh) -> Refresh {
        Refresh {
            refs: self.refs || o.refs,
            status: self.status || o.status,
            log: self.log || o.log,
            extras: self.extras || o.extras,
        }
    }

    pub fn with_extras(mut self) -> Refresh {
        self.extras = true;
        self
    }
}

/// Côté d'un fichier dans les changements locaux.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Side {
    Conflict,
    Unstaged,
    Staged,
}

pub struct RunningOp {
    pub id: u64,
    pub title: String,
    pub progress: Option<Progress>,
    pub cancel: CancelToken,
    pub silent: bool,
}

pub struct HistoryState {
    pub commits: Arc<Vec<Commit>>,
    pub graph: Arc<GraphLayout>,
    pub index: Arc<HashMap<Oid, usize>>,
    pub loading: bool,
    pub complete: bool,
    pub error: Option<GitError>,
    pub selected: Option<Oid>,
    /// Commit affiché hors de la liste (stash) avec son libellé.
    pub external: Option<(Oid, String)>,
    pub filter: String,
    pub filter_applied: String,
    pub filtered: Option<Vec<usize>>,
    pub scroll_to: Option<usize>,
    pub details: Option<(CommitInfo, Vec<FileChange>)>,
    pub details_loading: bool,
    pub selected_file: Option<usize>,
    pub diff: Option<FileDiff>,
    pub diff_loading: bool,
    pub diff_state: DiffViewState,
    pub highlighter: crate::highlight::Highlighter,
    pub image: image_diff::ImagePreview,
    tips_signature: u64,
    log_cancel: Option<CancelToken>,
    pub viewport_height: f32,
    /// N'afficher que l'historique de HEAD (sinon toutes les branches, distantes et tags).
    pub current_branch_only: bool,
}

impl Default for HistoryState {
    fn default() -> Self {
        HistoryState {
            commits: Arc::new(Vec::new()),
            graph: Arc::new(GraphLayout::default()),
            index: Arc::new(HashMap::new()),
            loading: false,
            complete: false,
            error: None,
            selected: None,
            external: None,
            filter: String::new(),
            filter_applied: String::new(),
            filtered: None,
            scroll_to: None,
            details: None,
            details_loading: false,
            selected_file: None,
            diff: None,
            diff_loading: false,
            diff_state: DiffViewState::default(),
            highlighter: crate::highlight::Highlighter::default(),
            image: image_diff::ImagePreview::default(),
            tips_signature: 0,
            log_cancel: None,
            viewport_height: 400.0,
            current_branch_only: false,
        }
    }
}

#[derive(Default)]
pub struct ChangesState {
    pub selected: Option<(Side, String)>,
    pub multi: BTreeSet<(Side, String)>,
    /// Point de départ d'une sélection Maj+clic (fichier, pas position : la liste change).
    pub anchor: Option<(Side, String)>,
    pub diff: Option<FileDiff>,
    pub diff_key: Option<(Side, String)>,
    pub diff_loading: bool,
    pub diff_state: DiffViewState,
    pub highlighter: crate::highlight::Highlighter,
    pub image: image_diff::ImagePreview,
    pub subject: String,
    pub body: String,
    pub amend: bool,
    pub push_after: bool,
    /// Message saisi avant de cocher « Amend » (restauré si on décoche).
    saved_message: Option<(String, String)>,
    /// Message de fusion (MERGE_MSG) déjà repris dans la zone de commit.
    merge_msg_loaded: bool,
    /// Commit lancé : le message n'est effacé (et le push éventuel lancé) qu'en cas de succès.
    pending_commit: Option<PendingCommit>,
}

struct PendingCommit {
    op: u64,
    after: AfterCommit,
}

/// Suite d'un commit réussi.
enum AfterCommit {
    Nothing,
    Push {
        remote: String,
        local: String,
        remote_branch: String,
        set_upstream: bool,
    },
    /// Push demandé mais non lancé ; le texte explique pourquoi.
    PushSkipped(String),
}

#[derive(Default)]
struct Gens {
    refs: u64,
    status: u64,
    log: u64,
    extras: u64,
    details: u64,
    commit_diff: u64,
    work_diff: u64,
}

/// Événements des tâches d'arrière-plan de l'onglet.
pub enum RepoEvent {
    Opened(Result<Arc<Repo>, GitError>),
    Refs(u64, Result<RefsSnapshot, GitError>),
    Status(u64, Result<(StatusSnapshot, OpState), GitError>),
    Log { generation: u64, complete: bool, result: Result<LogData, GitError> },
    Extras(u64, Vec<StashEntry>, Vec<Remote>, Vec<Submodule>),
    Details(u64, Oid, Result<(CommitInfo, Vec<FileChange>), GitError>),
    CommitDiff(u64, Result<FileDiff, GitError>),
    WorkDiff(u64, (Side, String), Result<FileDiff, GitError>),
    OpProgress(u64, Progress),
    OpDone { id: u64, title: String, result: Result<Option<String>, GitError>, refresh: Refresh },
    FsChanged { git_dir: bool, worktree: bool },
    LastMessage(String),
    Blame(blame::BlameEvent),
    RebaseList(Result<Vec<Commit>, GitError>),
    Images { history: bool, key: String, old: Option<image_diff::DecodedImage>, new: Option<image_diff::DecodedImage> },
}

pub struct LogData {
    pub commits: Arc<Vec<Commit>>,
    pub graph: Arc<GraphLayout>,
    pub index: Arc<HashMap<Oid, usize>>,
}

pub struct RepoTab {
    pub id: TabId,
    pub path: PathBuf,
    pub repo: Option<Arc<Repo>>,
    pub open_error: Option<GitError>,
    pub refs: Option<Arc<RefsSnapshot>>,
    pub status: Option<Arc<StatusSnapshot>>,
    pub op_state: OpState,
    pub stashes: Arc<Vec<StashEntry>>,
    pub remotes: Arc<Vec<Remote>>,
    pub submodules: Arc<Vec<Submodule>>,
    pub view: RepoView,
    pub history: HistoryState,
    pub changes: ChangesState,
    pub sidebar: sidebar::SidebarState,
    pub dialog: Option<Dialog>,
    pub running: Vec<RunningOp>,
    pub file_history: Option<FileHistoryWindow>,
    pub rebase: Option<rebase_ui::RebaseEditor>,
    pub conflict_editor: Option<conflicts::ConflictEditor>,
    /// Sélection initiale demandée en ligne de commande (`--select`).
    pub select_commit_on_load: Option<usize>,
    gens: Gens,
    worker: crate::jobs::SerialWorker,
    watcher: Option<watcher::RepoWatcher>,
    pending_refresh: Option<(Instant, Refresh)>,
    stale: Refresh,
    last_fetch: Instant,
    next_op_id: u64,
    quick_dirty: bool,
    /// Échec d'authentification du fetch automatique déjà signalé (remis à zéro au succès).
    auth_warned: bool,
}

impl RepoTab {
    pub fn new(id: TabId, path: PathBuf, shared: &mut Shared) -> RepoTab {
        let mut tab = RepoTab {
            id,
            path,
            repo: None,
            open_error: None,
            refs: None,
            status: None,
            op_state: OpState::None,
            stashes: Arc::new(Vec::new()),
            remotes: Arc::new(Vec::new()),
            submodules: Arc::new(Vec::new()),
            view: RepoView::History,
            history: HistoryState::default(),
            changes: ChangesState { push_after: shared.settings.push_after_commit, ..Default::default() },
            sidebar: sidebar::SidebarState::default(),
            dialog: None,
            running: Vec::new(),
            file_history: None,
            rebase: None,
            conflict_editor: None,
            select_commit_on_load: None,
            gens: Gens::default(),
            worker: crate::jobs::SerialWorker::new(&format!("repo-{id}")),
            watcher: None,
            pending_refresh: None,
            stale: Refresh::NONE,
            last_fetch: Instant::now(),
            next_op_id: 0,
            quick_dirty: false,
            auth_warned: false,
        };
        tab.reopen(shared);
        tab
    }

    /// (Ré)ouvre le dépôt (après une erreur ou un `safe.directory`).
    pub fn reopen(&mut self, shared: &mut Shared) {
        self.open_error = None;
        self.repo = None;
        let Some(git) = shared.git.clone() else {
            self.open_error = Some(GitError::NotFound);
            return;
        };
        let path = self.path.clone();
        let id = self.id;
        shared
            .jobs
            .spawn("open", move || Some(AppEvent::Repo(id, RepoEvent::Opened(Repo::open(&git, &path).map(Arc::new)))));
    }

    pub fn title(&self) -> String {
        match &self.repo {
            Some(r) => r.name(),
            None => self
                .path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| self.path.display().to_string()),
        }
    }

    /// Racine de la copie de travail (le dossier ouvert peut en être un sous-dossier).
    pub fn workdir(&self) -> PathBuf {
        let dir = self.repo.as_ref().map_or_else(|| self.path.clone(), |r| r.workdir.clone());
        crate::util::native_path(&dir)
    }

    pub fn is_busy(&self) -> bool {
        self.running.iter().any(|o| !o.silent) || (self.repo.is_none() && self.open_error.is_none())
    }

    pub fn set_view_by_name(&mut self, name: &str) {
        self.view =
            if name.starts_with("change") || name.starts_with("local") { RepoView::Changes } else { RepoView::History };
    }

    /// État résumé pour la liste de droite, s'il a changé depuis le dernier appel.
    pub fn quick_status(&mut self) -> Option<(QuickStatus, Option<HeadState>)> {
        if !std::mem::take(&mut self.quick_dirty) {
            return None;
        }
        let status = self.status.as_ref()?;
        let q = QuickStatus {
            dirty: status.entries.iter().filter(|e| e.worktree != dragontree_git::FileState::Ignored).count() as u32,
            ahead: status.ahead,
            behind: status.behind,
            untrusted: false,
            error: None,
        };
        Some((q, self.refs.as_ref().map(|r| r.head.clone())))
    }

    pub fn diff_options(shared: &Shared) -> DiffOptions {
        DiffOptions { context: shared.settings.diff_context, ignore_whitespace: shared.settings.ignore_whitespace }
    }

    // --- Chargements --------------------------------------------------------------------

    pub(crate) fn refresh(&mut self, shared: &Shared, what: Refresh) {
        let Some(repo) = self.repo.clone() else { return };
        let id = self.id;
        if what.refs {
            self.gens.refs += 1;
            let (g, r) = (self.gens.refs, repo.clone());
            shared.jobs.spawn("refs", move || Some(AppEvent::Repo(id, RepoEvent::Refs(g, r.refs()))));
        }
        if what.status {
            self.gens.status += 1;
            let (g, r) = (self.gens.status, repo.clone());
            shared.jobs.spawn("status", move || {
                let res = r.status().map(|s| (s, r.operation_state()));
                Some(AppEvent::Repo(id, RepoEvent::Status(g, res)))
            });
        }
        if what.extras {
            self.gens.extras += 1;
            let (g, r) = (self.gens.extras, repo.clone());
            shared.jobs.spawn("extras", move || {
                let stashes = r.stashes().unwrap_or_default();
                let remotes = r.remotes().unwrap_or_default();
                let subs = r.submodules().unwrap_or_default();
                Some(AppEvent::Repo(id, RepoEvent::Extras(g, stashes, remotes, subs)))
            });
        }
        if what.log {
            self.load_log(shared);
        }
    }

    pub(crate) fn load_log(&mut self, shared: &Shared) {
        let Some(repo) = self.repo.clone() else { return };
        if let Some(c) = self.history.log_cancel.take() {
            c.cancel();
        }
        let cancel = CancelToken::new();
        self.history.log_cancel = Some(cancel.clone());
        self.gens.log += 1;
        self.history.loading = true;
        let (generation, id, sender) = (self.gens.log, self.id, shared.jobs.sender());
        let scope = if self.history.current_branch_only {
            dragontree_git::LogScope::Head
        } else {
            dragontree_git::LogScope::All
        };
        shared.jobs.spawn("log", move || {
            let build = |commits: Vec<Commit>| -> LogData {
                let mut g = GraphBuilder::new();
                let mut index = HashMap::with_capacity(commits.len());
                for (i, c) in commits.iter().enumerate() {
                    g.push(c.id, &c.parents);
                    index.insert(c.id, i);
                }
                LogData { commits: Arc::new(commits), graph: Arc::new(g.finish()), index: Arc::new(index) }
            };
            let first = LogOptions { max_count: Some(FIRST_BATCH), scope: scope.clone(), ..LogOptions::default() };
            let commits = match repo.log(&first, &cancel) {
                Ok(c) => c,
                Err(GitError::Cancelled) => return None,
                Err(e) => {
                    return Some(AppEvent::Repo(id, RepoEvent::Log { generation, complete: true, result: Err(e) }));
                }
            };
            let partial = commits.len() >= FIRST_BATCH;
            sender.send(AppEvent::Repo(
                id,
                RepoEvent::Log { generation, complete: !partial, result: Ok(build(commits)) },
            ));
            if !partial {
                return None;
            }
            match repo.log(&LogOptions { scope, ..LogOptions::default() }, &cancel) {
                Ok(all) => {
                    Some(AppEvent::Repo(id, RepoEvent::Log { generation, complete: true, result: Ok(build(all)) }))
                }
                Err(GitError::Cancelled) => None,
                Err(e) => Some(AppEvent::Repo(id, RepoEvent::Log { generation, complete: true, result: Err(e) })),
            }
        });
    }

    pub(crate) fn load_details(&mut self, shared: &Shared, oid: Oid) {
        let Some(repo) = self.repo.clone() else { return };
        self.gens.details += 1;
        self.history.details_loading = true;
        self.history.diff = None;
        self.history.selected_file = None;
        let (g, id) = (self.gens.details, self.id);
        shared.jobs.spawn("details", move || {
            let res = repo.commit_info(oid).and_then(|info| {
                let files = repo.commit_files(oid, info.parents.first().copied())?;
                Ok((info, files))
            });
            Some(AppEvent::Repo(id, RepoEvent::Details(g, oid, res)))
        });
    }

    pub(crate) fn load_commit_diff(&mut self, shared: &Shared, file_index: usize) {
        let Some(repo) = self.repo.clone() else { return };
        let Some((info, files)) = &self.history.details else { return };
        let Some(change) = files.get(file_index).cloned() else { return };
        let (oid, parent) = (info.id, info.parents.first().copied());
        self.history.selected_file = Some(file_index);
        self.history.diff_loading = true;
        self.history.diff_state = DiffViewState::default();
        self.gens.commit_diff += 1;
        let (g, id, o) = (self.gens.commit_diff, self.id, Self::diff_options(shared));
        shared.jobs.spawn("commit-diff", move || {
            Some(AppEvent::Repo(id, RepoEvent::CommitDiff(g, repo.diff_commit_file(oid, parent, &change, &o))))
        });
    }

    pub(crate) fn load_work_diff(&mut self, shared: &Shared, side: Side, path: String, keep_selection: bool) {
        let Some(repo) = self.repo.clone() else { return };
        let untracked = side == Side::Unstaged
            && self
                .status
                .as_ref()
                .and_then(|s| s.entries.iter().find(|e| e.path == path))
                .is_some_and(|e| e.worktree == dragontree_git::FileState::Untracked);
        if !keep_selection {
            self.changes.diff_state = DiffViewState::default();
        }
        self.changes.diff_loading = true;
        self.gens.work_diff += 1;
        let (g, id, o) = (self.gens.work_diff, self.id, Self::diff_options(shared));
        shared.jobs.spawn("work-diff", move || {
            let res = repo.diff_worktree_file(&path, side == Side::Staged, untracked, &o);
            Some(AppEvent::Repo(id, RepoEvent::WorkDiff(g, (side, path), res)))
        });
    }

    fn tips_signature(refs: &RefsSnapshot) -> u64 {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        refs.head.hash(&mut h);
        refs.head_oid.hash(&mut h);
        for b in &refs.locals {
            (&b.name, b.oid).hash(&mut h);
        }
        for b in &refs.remotes {
            (&b.full, b.oid).hash(&mut h);
        }
        for t in &refs.tags {
            (&t.name, t.oid).hash(&mut h);
        }
        h.finish()
    }

    // --- Événements ---------------------------------------------------------------------

    pub fn handle_event(&mut self, ev: RepoEvent, shared: &mut Shared) {
        match ev {
            RepoEvent::Opened(Ok(repo)) => {
                self.watcher = watcher::RepoWatcher::start(
                    &repo.workdir,
                    &repo.git_dir,
                    self.id,
                    shared.jobs.sender(),
                    shared.settings.scan_skip_dirs.clone(),
                );
                self.repo = Some(repo);
                self.refresh(shared, Refresh::ALL);
            }
            RepoEvent::Opened(Err(e)) => self.open_error = Some(e),
            RepoEvent::Refs(g, res) if g == self.gens.refs => match res {
                Ok(refs) => {
                    let sig = Self::tips_signature(&refs);
                    if sig != self.history.tips_signature {
                        let first_load = self.history.tips_signature == 0;
                        self.history.tips_signature = sig;
                        if !first_load || self.history.commits.is_empty() && !self.history.loading {
                            self.load_log(shared);
                        }
                    }
                    self.refs = Some(Arc::new(refs));
                    self.quick_dirty = true;
                }
                Err(e) => shared.toast(Toast::error(tr!("Lecture des branches impossible"), e.to_string())),
            },
            RepoEvent::Status(g, res) if g == self.gens.status => match res {
                Ok((status, op)) => {
                    self.op_state = op;
                    self.status = Some(Arc::new(status));
                    self.quick_dirty = true;
                    self.after_status_refresh(shared);
                }
                Err(e) => shared.toast(Toast::error(tr!("Lecture de l'état impossible"), e.to_string())),
            },
            RepoEvent::Log { generation, complete, result } if generation == self.gens.log => {
                self.history.loading = !complete;
                match result {
                    Ok(data) => {
                        self.history.commits = data.commits;
                        self.history.graph = data.graph;
                        self.history.index = data.index;
                        self.history.error = None;
                        self.history.complete = complete;
                        self.history.filter_applied.clear();
                        self.history.filtered = None;
                        if let Some(n) = self.select_commit_on_load.take()
                            && let Some(c) = self.history.commits.get(n)
                        {
                            let oid = c.id;
                            self.select_commit(shared, oid, true);
                        }
                    }
                    Err(e) => self.history.error = Some(e),
                }
            }
            RepoEvent::Extras(g, stashes, remotes, subs) if g == self.gens.extras => {
                self.stashes = Arc::new(stashes);
                self.remotes = Arc::new(remotes);
                self.submodules = Arc::new(subs);
            }
            RepoEvent::Details(g, oid, res) if g == self.gens.details => {
                self.history.details_loading = false;
                match res {
                    Ok(d) => {
                        let has_files = !d.1.is_empty();
                        self.history.details = Some(d);
                        if has_files {
                            self.load_commit_diff(shared, 0);
                        }
                    }
                    Err(e) => {
                        shared.toast(Toast::error(tr!("Commit {sha} illisible", sha = oid.short()), e.to_string()))
                    }
                }
            }
            RepoEvent::CommitDiff(g, res) if g == self.gens.commit_diff => {
                self.history.diff_loading = false;
                match res {
                    Ok(d) => self.history.diff = Some(d),
                    Err(e) => shared.toast(Toast::error(tr!("Diff impossible"), e.to_string())),
                }
            }
            RepoEvent::WorkDiff(g, key, res) if g == self.gens.work_diff => {
                self.changes.diff_loading = false;
                match res {
                    Ok(d) => {
                        self.changes.diff = Some(d);
                        self.changes.diff_key = Some(key);
                        self.changes.diff_state.prune_selection(self.changes.diff.as_ref());
                    }
                    Err(e) => {
                        self.changes.diff = None;
                        shared.toast(Toast::error(tr!("Diff impossible"), e.to_string()));
                    }
                }
            }
            RepoEvent::OpProgress(id, p) => {
                if let Some(op) = self.running.iter_mut().find(|o| o.id == id) {
                    op.progress = Some(p);
                }
            }
            RepoEvent::OpDone { id, title, result, refresh } => {
                let silent = self.running.iter().find(|o| o.id == id).is_some_and(|o| o.silent);
                self.running.retain(|o| o.id != id);
                let succeeded = result.is_ok();
                match result {
                    Ok(msg) => {
                        if silent {
                            self.auth_warned = false;
                        } else if let Some(msg) = msg {
                            shared.toast(Toast::success(msg));
                        }
                    }
                    Err(GitError::Cancelled) => shared.toast(Toast::info(tr!("{title} : annulé", title))),
                    Err(e) if silent => {
                        // En arrière-plan, seules les erreurs d'authentification sont signalées
                        // (une fois) : un jeton expiré ne doit pas passer inaperçu.
                        if is_auth_error(&e) {
                            if !self.auth_warned {
                                self.auth_warned = true;
                                shared.toast(Toast::error(
                                    tr!("{title} automatique : authentification refusée", title),
                                    tr!(
                                        "{error}\n\nVérifiez le compte associé à ce dépôt (Comptes) ou vos identifiants git.",
                                        error = e
                                    ),
                                ));
                            }
                        } else if !matches!(e, GitError::Failed { .. }) {
                            shared.toast(Toast::error(tr!("{title} a échoué", title), e.to_string()));
                        }
                    }
                    Err(e) => shared.toast(Toast::error(tr!("{title} a échoué", title), e.to_string())),
                }
                self.refresh(shared, refresh);
                if self.changes.pending_commit.as_ref().is_some_and(|c| c.op == id)
                    && let Some(commit) = self.changes.pending_commit.take()
                    && succeeded
                {
                    self.after_commit(shared, commit.after);
                }
            }
            RepoEvent::FsChanged { git_dir, worktree } => {
                let what = Refresh { refs: git_dir, status: git_dir || worktree, log: false, extras: git_dir };
                let deadline = Instant::now() + Duration::from_millis(350);
                self.pending_refresh = Some(match self.pending_refresh {
                    Some((_, prev)) => (deadline, prev.merge(what)),
                    None => (deadline, what),
                });
            }
            RepoEvent::LastMessage(msg) => {
                let (subject, body) = split_message(&msg);
                self.changes.subject = subject;
                self.changes.body = body;
            }
            RepoEvent::Blame(ev) => {
                if let Some(w) = &mut self.file_history {
                    w.handle(ev);
                }
            }
            RepoEvent::RebaseList(res) => match res {
                Ok(list) => {
                    if let Some(editor) = &mut self.rebase {
                        editor.set_commits(list);
                    }
                }
                Err(e) => {
                    self.rebase = None;
                    shared.toast(Toast::error(tr!("Rebase interactif impossible"), e.to_string()));
                }
            },
            RepoEvent::Images { history, key, old, new } => {
                let preview = if history { &mut self.history.image } else { &mut self.changes.image };
                preview.set(&shared.ctx, key, old, new);
            }
            // Résultat périmé (une requête plus récente est en cours).
            _ => {}
        }
    }

    /// Après un nouvel état : garder une sélection valide et recharger le diff affiché.
    fn after_status_refresh(&mut self, shared: &Shared) {
        let Some(status) = self.status.clone() else { return };
        if self.view == RepoView::Changes && self.changes.selected.is_none() {
            self.ensure_change_selected(shared);
            return;
        }
        let exists = |side: Side, path: &str| -> bool {
            match side {
                Side::Staged => status.staged().iter().any(|e| e.path == path),
                Side::Unstaged => status.unstaged().iter().any(|e| e.path == path),
                Side::Conflict => status.conflicted().iter().any(|e| e.path == path),
            }
        };
        self.changes.multi.retain(|(s, p)| exists(*s, p));
        if let Some((side, path)) = self.changes.selected.clone() {
            if exists(side, &path) {
                if self.view == RepoView::Changes && side != Side::Conflict {
                    self.load_work_diff(shared, side, path, true);
                }
            } else {
                // Le fichier a changé de côté (indexé/désindexé) : on le suit s'il existe ailleurs.
                let other = if side == Side::Staged { Side::Unstaged } else { Side::Staged };
                if exists(other, &path) {
                    self.changes.selected = Some((other, path.clone()));
                    self.changes.multi = [(other, path.clone())].into();
                    self.load_work_diff(shared, other, path, false);
                } else {
                    self.changes.selected = None;
                    self.changes.diff = None;
                    self.changes.diff_key = None;
                }
            }
        }
    }

    pub(crate) fn select_commit(&mut self, shared: &Shared, oid: Oid, scroll: bool) {
        self.history.selected = Some(oid);
        self.history.external = None;
        if scroll && let Some(&row) = self.history.index.get(&oid) {
            let row = match &self.history.filtered {
                Some(f) => f.iter().position(|&i| i == row).unwrap_or(0),
                None => row,
            };
            self.history.scroll_to = Some(row);
        }
        self.load_details(shared, oid);
    }

    pub(crate) fn select_external(&mut self, shared: &Shared, oid: Oid, label: String) {
        self.view = RepoView::History;
        self.history.selected = None;
        self.history.external = Some((oid, label));
        self.load_details(shared, oid);
    }

    /// Minuteries : rafraîchissement différé (surveillance du disque) et fetch automatique.
    pub fn tick(&mut self, shared: &mut Shared, active: bool) {
        if let Some((deadline, what)) = self.pending_refresh {
            let now = Instant::now();
            if now >= deadline {
                self.pending_refresh = None;
                if active {
                    self.refresh(shared, what);
                } else {
                    self.stale = self.stale.merge(what);
                }
            } else {
                shared.ctx.request_repaint_after(deadline - now);
            }
        }
        if active && self.stale != Refresh::NONE {
            let what = std::mem::take(&mut self.stale);
            self.refresh(shared, what);
        }
        let minutes = shared.settings.auto_fetch_minutes;
        if minutes > 0 && self.repo.is_some() && !self.remotes.is_empty() && self.running.is_empty() {
            let every = Duration::from_secs(u64::from(minutes) * 60);
            if self.last_fetch.elapsed() >= every {
                self.last_fetch = Instant::now();
                self.op_fetch(shared, None, true, true);
            } else {
                shared.ctx.request_repaint_after(every - self.last_fetch.elapsed());
            }
        }
    }

    pub fn on_focus_regained(&mut self, shared: &mut Shared) {
        if self.repo.is_some() {
            self.refresh(shared, Refresh::REFS);
        }
    }

    // --- Interface ----------------------------------------------------------------------

    pub fn ui(&mut self, ui: &mut Ui, shared: &mut Shared) {
        let p = Palette::current(ui.ctx());
        if let Some(err) = self.open_error.clone() {
            self.error_page(ui, shared, &err, &p);
            return;
        }
        if self.repo.is_none() {
            ui.centered_and_justified(|ui| {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(tr!("Ouverture de {path}…", path = self.path.display()));
                });
            });
            return;
        }
        self.handle_repo_shortcuts(ui.ctx(), shared);

        egui::Panel::top(("toolbar", self.id))
            .frame(
                Frame::new()
                    .fill(p.panel)
                    .inner_margin(egui::Margin::symmetric(8, 4))
                    .stroke(egui::Stroke::new(1.0, p.border)),
            )
            .show(ui, |ui| self.toolbar_ui(ui, shared, &p));

        if self.op_state != OpState::None {
            egui::Panel::top(("op-banner", self.id))
                .frame(Frame::new().fill(p.warning_bg).inner_margin(egui::Margin::symmetric(10, 6)))
                .show(ui, |ui| self.op_banner(ui, shared, &p));
        }
        if let Some(op) = self.running.iter().find(|o| !o.silent) {
            let (title, progress, cancel) = (op.title.clone(), op.progress.clone(), op.cancel.clone());
            egui::Panel::top(("progress", self.id))
                .frame(Frame::new().fill(p.hover).inner_margin(egui::Margin::symmetric(10, 4)))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        let text = match &progress {
                            Some(pg) => match (pg.percent, pg.total) {
                                (Some(pc), Some(t)) => format!("{title} — {} {pc}% ({}/{t})", pg.phase, pg.current),
                                (Some(pc), None) => format!("{title} — {} {pc}%", pg.phase),
                                _ => format!("{title} — {} {}", pg.phase, pg.current),
                            },
                            None => format!("{title}…"),
                        };
                        ui.label(text);
                        if let Some(pc) = progress.as_ref().and_then(|p| p.percent) {
                            ui.add(egui::ProgressBar::new(f32::from(pc) / 100.0).desired_width(160.0));
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button(tr!("Annuler")).clicked() {
                                cancel.cancel();
                            }
                        });
                    });
                });
        }

        egui::Panel::left(("nav", self.id))
            .resizable(true)
            .default_size(250.0)
            .size_range(170.0..=520.0)
            .frame(Frame::new().fill(p.sidebar).inner_margin(egui::Margin::symmetric(6, 6)))
            .show(ui, |ui| self.sidebar_ui(ui, shared, &p));

        egui::CentralPanel::default().frame(Frame::new().fill(p.bg)).show(ui, |ui| match self.view {
            RepoView::History => self.history_ui(ui, shared, &p),
            RepoView::Changes => self.changes_ui(ui, shared, &p),
        });

        let ctx = ui.ctx().clone();
        self.dialogs_ui(&ctx, shared, &p);
        if let Some(w) = &mut self.file_history {
            let (keep, select) = w.ui(&ctx, shared, &p);
            if !keep {
                self.file_history = None;
            }
            if let Some(oid) = select {
                self.view = RepoView::History;
                self.select_commit(shared, oid, true);
            }
        }
        self.rebase_window(&ctx, shared, &p);
        self.conflict_window(&ctx, shared, &p);
    }

    fn handle_repo_shortcuts(&mut self, ctx: &egui::Context, shared: &mut Shared) {
        use egui::{Key, KeyboardShortcut, Modifiers};
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F5)) {
            self.refresh(shared, Refresh::ALL);
        }
        if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND | Modifiers::SHIFT, Key::L)))
        {
            shared.actions.push(AppAction::ShowLog);
        }
        if ctx.memory(|m| m.focused().is_none())
            && ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::Num1)))
        {
            self.view = RepoView::Changes;
        }
    }

    fn error_page(&mut self, ui: &mut Ui, shared: &mut Shared, err: &GitError, p: &Palette) {
        ui.vertical_centered(|ui| {
            ui.add_space(70.0);
            ui.label(RichText::new(icons::WARNING_CIRCLE).size(46.0).color(p.removed_fg));
            ui.add_space(8.0);
            ui.label(RichText::new(self.path.display().to_string()).strong().size(15.0));
            ui.add_space(6.0);
            ui.add(egui::Label::new(RichText::new(err.to_string()).color(p.weak)).wrap());
            ui.add_space(16.0);
            match err {
                GitError::DubiousOwnership { path } => {
                    ui.label(RichText::new(tr!(
                        "Git refuse ce dépôt car son propriétaire Windows n'est pas l'utilisateur courant.\nDragonTree peut l'ajouter à votre configuration globale : git config --global --add safe.directory {path}",
                        path
                    )).color(p.weak));
                    ui.add_space(10.0);
                    if ui.button(RichText::new(format!("{}  {}", icons::SHIELD_CHECK, tr!("Faire confiance à ce dépôt"))).size(14.0)).clicked() {
                        shared.actions.push(AppAction::TrustRepo(PathBuf::from(path)));
                    }
                }
                GitError::NotARepository { .. } => {
                    ui.label(RichText::new(tr!("Ce dossier n'est pas (ou plus) un dépôt git.")).color(p.weak));
                }
                GitError::NotFound
                    if ui.button(format!("{} {}", icons::GEAR_SIX, tr!("Réglages"))).clicked() => {
                        shared.actions.push(AppAction::ShowSettings);
                    }
                _ => {}
            }
            ui.add_space(8.0);
            if ui.button(format!("{} {}", icons::ARROWS_CLOCKWISE, tr!("Réessayer"))).clicked() {
                self.reopen(shared);
            }
        });
    }

    fn op_banner(&mut self, ui: &mut Ui, shared: &mut Shared, p: &Palette) {
        let conflicts = self.status.as_ref().map_or(0, |s| s.conflicted().len());
        let (label, can_skip) = match self.op_state {
            OpState::Merge => (tr!("Fusion en cours"), false),
            OpState::Rebase { interactive: true } => (tr!("Rebase interactif en cours"), true),
            OpState::Rebase { .. } => (tr!("Rebase en cours"), true),
            OpState::CherryPick => (tr!("Cherry-pick en cours"), false),
            OpState::Revert => (tr!("Revert en cours"), false),
            OpState::None => return,
        };
        ui.horizontal(|ui| {
            ui.label(RichText::new(icons::WARNING).color(p.modified_fg).size(16.0));
            ui.label(RichText::new(label).strong());
            if conflicts > 0 {
                let text = trn!(conflicts, "— {n} fichier en conflit à résoudre", "— {n} fichiers en conflit à résoudre");
                ui.label(RichText::new(text).color(p.conflict_fg));
            } else {
                ui.label(RichText::new(tr!("— aucun conflit restant")).color(p.weak));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let state = self.op_state;
                let abort = dialogs::abort_label(state);
                if ui.button(format!("{} {abort}…", icons::X)).clicked() {
                    self.dialog = Some(Dialog::Confirm {
                        title: tr!("{abort} ?", abort),
                        text: tr!("Les conflits déjà résolus et les modifications apportées par l'opération en cours seront perdus : la branche revient à son état d'avant l'opération.").into(),
                        action: dialogs::ConfirmAction::AbortOp(state),
                    });
                }
                if can_skip && ui.button(tr!("Passer ce commit")).clicked() {
                    self.run_simple(shared, "Rebase", Refresh::ALL, |r| r.skip_rebase_commit().map(|_| None));
                }
                if ui.add_enabled(conflicts == 0, egui::Button::new(format!("{} {}", icons::CHECK, tr!("Continuer")))).clicked() {
                    self.run_simple(shared, tr!("Continuer"), Refresh::ALL, move |r| r.continue_op(state).map(|_| None));
                }
                if conflicts > 0 && ui.button(format!("{} {}", icons::GIT_DIFF, tr!("Voir les conflits"))).clicked() {
                    self.view = RepoView::Changes;
                }
            });
        });
        let _ = shared;
    }
}

/// Erreur d'authentification (jeton expiré, accès refusé) : à signaler même en arrière-plan.
pub fn is_auth_error(e: &GitError) -> bool {
    const SIGNS: [&str; 8] = [
        "authentication failed",
        "could not read username",
        "could not read password",
        "access denied",
        "invalid username or password",
        "returned error: 401",
        "returned error: 403",
        "permission denied (publickey)",
    ];
    let GitError::Failed { stderr, .. } = e else { return false };
    let stderr = stderr.to_lowercase();
    SIGNS.iter().any(|s| stderr.contains(s))
}

/// Sépare un message de commit en sujet et description.
pub fn split_message(msg: &str) -> (String, String) {
    let msg = msg.trim_end();
    match msg.split_once('\n') {
        Some((subject, rest)) => (subject.trim_end().to_string(), rest.trim_start_matches(['\n', '\r']).to_string()),
        None => (msg.to_string(), String::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_message_subject_and_body() {
        assert_eq!(split_message("Sujet\n\nCorps\nligne 2\n"), ("Sujet".into(), "Corps\nligne 2".into()));
        assert_eq!(split_message("Seul"), ("Seul".into(), String::new()));
    }

    #[test]
    fn auth_errors_are_recognized() {
        let failed = |stderr: &str| GitError::Failed { cmd: "git fetch".into(), code: 128, stderr: stderr.into() };
        for stderr in [
            "fatal: Authentication failed for 'https://dev.azure.com/org/projet/_git/depot/'",
            "fatal: could not read Username for 'https://github.com': terminal prompts disabled",
            "remote: HTTP Basic: Access denied\nfatal: Authentication failed for 'https://gitlab.com/g/p.git/'",
            "fatal: unable to access 'https://x.example/r.git/': The requested URL returned error: 403",
            "fatal: unable to access 'https://x.example/r.git/': The requested URL returned error: 401",
        ] {
            assert!(is_auth_error(&failed(stderr)), "{stderr}");
        }
        for stderr in [
            "fatal: unable to access 'https://x.example/': Could not resolve host: x.example",
            "fatal: couldn't find remote ref main",
        ] {
            assert!(!is_auth_error(&failed(stderr)), "{stderr}");
        }
        assert!(!is_auth_error(&GitError::Cancelled));
    }

    #[test]
    fn refresh_merge() {
        let r = Refresh::STATUS.merge(Refresh { refs: true, ..Refresh::NONE });
        assert!(r.refs && r.status && !r.log);
    }
}
