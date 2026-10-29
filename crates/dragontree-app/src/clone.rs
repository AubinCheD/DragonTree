//! Fenêtre « Cloner » : dépôts de tous les comptes (ou une URL), destination, progression. Un hôte qui pilote
//! DragonTree à distance (le téléphone de la suite) clone aussi sans fenêtre (`DragonTreeApp::clone_remote`) : même
//! clonage, suivi dans ses [`HostJob`].

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::{Duration, Instant};

use dragontree_forges::{Account, ForgeError, RemoteRepo, secrets};
use dragontree_git::{CancelToken, Git, GitError, NetOptions, Progress};
use dragontree_i18n::{tr, trn};
use egui::{RichText, Sense};

use crate::accounts::kind_icon;
use crate::app::{AppAction, AppEvent, Shared};
use crate::icons;
use crate::settings::path_key;
use crate::theme::Palette;
use crate::toasts::Toast;
use crate::widgets;

/// Événements de la fenêtre et des clonages ; une progression porte le numéro de son clonage (celui de la fenêtre, ou
/// d'un hôte), comme sa fin.
pub enum CloneEvent {
    Repos { account_id: String, result: Result<Vec<RemoteRepo>, ForgeError> },
    Progress(u64, Progress),
    Done(CloneDone),
}

/// Un clonage demandé par un hôte qui pilote DragonTree à distance (le téléphone de la suite), sans fenêtre : comme
/// « Cloner » de la fenêtre depuis une URL (voir `DragonTreeApp::clone_remote`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CloneRequest {
    pub url: String,
    /// Dossier où le dépôt est créé : un dossier existant, en chemin absolu.
    pub parent: PathBuf,
    /// Nom du dossier du dépôt dans `parent`, absent ou vide.
    pub folder: String,
    /// Branche à extraire ; `None` : celle par défaut du dépôt.
    pub branch: Option<String>,
    /// Compte qui authentifie le clonage et que le dépôt garde ensuite (voir `DragonTreeApp::account_for_url`) ;
    /// `None` : les identifiants de git.
    pub account: Option<String>,
    /// Sous-modules compris.
    pub recurse: bool,
}

/// Ce que propose la fenêtre « Cloner » à son ouverture, pour un hôte qui monte sa propre demande.
#[derive(Debug, Clone, Default)]
pub struct CloneDefaults {
    /// Dossier parent : celui du dernier clonage, sinon le premier dossier analysé, sinon `~\source\repos`.
    pub parent: PathBuf,
    /// Les comptes enregistrés, dans l'ordre des réglages.
    pub accounts: Vec<Account>,
}

/// Un clonage lancé par un hôte (`DragonTreeApp::clone_remote`) : sa progression, puis ce qu'il a donné.
#[derive(Debug, Clone)]
pub struct HostJob {
    pub id: u64,
    /// « Clonage de <dossier> ».
    pub title: String,
    pub dest: PathBuf,
    pub progress: Option<Progress>,
    /// `None` : en cours ; sinon réussi, ou pourquoi il a échoué (« Clonage annulé. » s'il l'a été).
    pub result: Option<Result<(), String>>,
    /// Son lancement, puis sa fin.
    pub at: Instant,
    pub(crate) cancel: CancelToken,
}

impl HostJob {
    /// Fin du clonage : ce qu'il a donné, et quand.
    pub(crate) fn finish(&mut self, result: &Result<(), GitError>) {
        self.progress = None;
        self.at = Instant::now();
        self.result = Some(match result {
            Ok(()) => Ok(()),
            Err(GitError::Cancelled) => Err(tr!("Clonage annulé.").into()),
            Err(e) => Err(e.to_string()),
        });
    }
}

/// Fin d'un clonage. Traitée par l'application même si la fenêtre a été fermée entre-temps.
pub struct CloneDone {
    pub id: u64,
    pub dest: PathBuf,
    pub parent: PathBuf,
    pub account: Option<String>,
    pub open_after: bool,
    pub result: Result<(), GitError>,
}

static NEXT_CLONE_ID: AtomicU64 = AtomicU64::new(1);

/// Clonages en cours, nettoyage compris (RV4a) : un clonage annulé (« Quitter quand même ») supprime son dossier partiel
/// sur son propre thread, qui mourrait avec le processus ; la fin de l'application l'attend un peu
/// ([`CloneTasks::wait_idle`]). Les clones désignent le même compteur.
#[derive(Clone, Default)]
pub struct CloneTasks(Arc<(Mutex<usize>, Condvar)>);

/// Clonage compté dans [`CloneTasks`] jusqu'à sa destruction (fin de la tâche, panique comprise).
struct CloneTask(CloneTasks);

impl Drop for CloneTask {
    fn drop(&mut self) {
        let (count, changed) = &*(self.0).0;
        *count.lock().unwrap_or_else(PoisonError::into_inner) -= 1;
        changed.notify_all();
    }
}

impl CloneTasks {
    fn start(&self) -> CloneTask {
        *self.0.0.lock().unwrap_or_else(PoisonError::into_inner) += 1;
        CloneTask(self.clone())
    }

    /// Attend la fin des clonages en cours, au plus `timeout` ; renvoie vrai s'il n'y en a plus.
    pub fn wait_idle(&self, timeout: Duration) -> bool {
        let (count, changed) = &*self.0;
        let guard = count.lock().unwrap_or_else(PoisonError::into_inner);
        let (guard, _) = changed.wait_timeout_while(guard, timeout, |n| *n > 0).unwrap_or_else(PoisonError::into_inner);
        *guard == 0
    }
}

/// Supprime le dossier d'un clonage échoué ou annulé, en réessayant : les processus git
/// arrêtés peuvent encore tenir des fichiers ouverts quelques instants.
pub fn remove_failed_clone(dir: &Path) {
    for _ in 0..15 {
        match std::fs::remove_dir_all(dir) {
            Ok(()) => return,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
            Err(_) => std::thread::sleep(Duration::from_millis(200)),
        }
    }
}

/// Suite d'un clonage, côté application : compte associé, ouverture, notifications.
/// `window_open` : la fenêtre affiche elle-même l'erreur (sinon, notification).
pub fn finish_clone(shared: &mut Shared, done: &CloneDone, window_open: bool) {
    if record_clone(shared, done, window_open) {
        if done.open_after {
            shared.actions.push(AppAction::OpenRepo(done.dest.clone()));
        }
        if under_root(shared, &done.dest) {
            shared.actions.push(AppAction::Rescan);
        }
    }
}

/// Ce que retient DragonTree d'un clonage fini, depuis la fenêtre ou un hôte : réussi, le compte qui l'a authentifié
/// reste celui du dépôt, son dossier parent devient le dossier de clonage proposé, et une notification le dit ; échoué
/// ou annulé, une notification, sauf si la fenêtre (`window_open`) l'affiche elle-même. Rend vrai s'il a réussi.
pub(crate) fn record_clone(shared: &mut Shared, done: &CloneDone, window_open: bool) -> bool {
    match &done.result {
        Ok(()) => {
            if let Some(acc) = &done.account {
                shared.settings.set_account_for_repo(&done.dest, Some(acc));
            }
            shared.settings.default_clone_dir = Some(done.parent.clone());
            shared.settings_dirty = true;
            shared.toast(Toast::success(tr!("Dépôt cloné dans {path}", path = done.dest.display())));
            return true;
        }
        Err(GitError::Cancelled) if !window_open => shared.toast(Toast::info(tr!("Clonage annulé"))),
        Err(e) if !window_open => shared.toast(Toast::error(tr!("Le clonage a échoué"), e.to_string())),
        Err(_) => {}
    }
    false
}

/// Le dépôt cloné dans `dest` est sous un dossier analysé : la liste des dépôts est à refaire.
pub(crate) fn under_root(shared: &Shared, dest: &Path) -> bool {
    shared.settings.scan_roots.iter().any(|r| path_key(dest).starts_with(&path_key(r)))
}

/// Dossier parent proposé pour un clonage : celui du dernier clonage, sinon le premier dossier analysé, sinon
/// `~\source\repos`.
pub fn default_parent(shared: &Shared) -> PathBuf {
    shared
        .settings
        .default_clone_dir
        .clone()
        .or_else(|| shared.settings.scan_roots.first().cloned())
        .or_else(|| dirs::home_dir().map(|h| h.join("source").join("repos")))
        .unwrap_or_default()
}

/// Noms de dossiers où un hôte ne fait jamais cloner, où qu'ils soient, en minuscules : secrets (clés, jetons,
/// configurations qui lancent des commandes) et dossiers du système. Copie de `PROTECTED_NAMES` d'Eyrie
/// (`crates/suite-mobile/src/paths.rs`, que DragonTree ne lit pas) : à changer ensemble.
const PROTECTED_NAMES: [&str; 9] = [
    ".ssh",
    ".gnupg",
    ".aws",
    ".azure",
    ".kube",
    ".docker",
    ".password-store",
    "$recycle.bin",
    "system volume information",
];

/// `dest`, la destination d'un clonage d'un hôte, est un dossier protégé ou dedans, comme pour le pont d'Eyrie
/// (`crates/suite-mobile/src/paths.rs`) : un des [`PROTECTED_NAMES`] dans son chemin, un dossier d'un des profils
/// `homes` dont le nom commence par un point (`.config`, `.claude`… : git, les shells, Claude Code les lisent
/// d'eux-mêmes, et certains y trouvent des commandes à lancer), ou un profil de PowerShell d'un des `documents` (son
/// `profile.ps1` est lancé à chaque démarrage de PowerShell). Sans casse, `\` ou `/`.
fn protected_dest(dest: &Path, homes: &[PathBuf], documents: &[PathBuf]) -> bool {
    let key = path_key(dest);
    let named = |c: Component| PROTECTED_NAMES.contains(&c.as_os_str().to_string_lossy().to_lowercase().as_str());
    let in_dot_folder = |home: &PathBuf| {
        let home = path_key(home);
        let rest = key.strip_prefix(&home).and_then(|r| r.strip_prefix('/'));
        !home.is_empty() && rest.is_some_and(|r| r.starts_with('.'))
    };
    let inside = |zone: PathBuf| {
        let zone = path_key(&zone);
        key == zone || key.starts_with(&format!("{zone}/"))
    };
    let powershell = |d: &PathBuf| inside(d.join("PowerShell")) || inside(d.join("WindowsPowerShell"));
    dest.components().any(named) || homes.iter().any(in_dot_folder) || documents.iter().any(powershell)
}

/// `dir` résolu (noms courts, jonctions, liens), sans le préfixe `\\?\` ; `None` : il n'existe pas.
fn resolved(dir: &Path) -> Option<PathBuf> {
    let full = std::fs::canonicalize(dir).ok()?;
    let text = full.to_string_lossy();
    Some(PathBuf::from(text.strip_prefix(r"\\?\").unwrap_or(&text)))
}

/// Les profils et les Documents où [`protected_dest`] cherche : celui de l'utilisateur, ses Documents (ceux du profil,
/// et ceux que Windows connaît : OneDrive les déplace), chacun tel qu'écrit et résolu.
fn user_dirs() -> (Vec<PathBuf>, Vec<PathBuf>) {
    let with_resolved = |list: Vec<PathBuf>| list.iter().filter_map(|d| resolved(d)).chain(list.clone()).collect();
    let home = dirs::home_dir();
    let documents = home.iter().map(|h| h.join("Documents")).chain(dirs::document_dir()).collect();
    (with_resolved(home.into_iter().collect()), with_resolved(documents))
}

/// La destination d'une demande de clonage d'un hôte (`parent\folder`), ou pourquoi elle ne part pas : URL vide, nom de
/// dossier qui n'est pas un simple nom, dossier parent qui n'existe pas (ou chemin relatif), dossier protégé
/// (secrets ou configuration, [`protected_dest`], ou nom qui commence par un point), destination déjà occupée (un
/// dossier vide convient, comme dans la fenêtre), branche vide ou qui serait lue comme une option, compte inconnu.
pub(crate) fn check_request(req: &CloneRequest, accounts: &[Account]) -> Result<PathBuf, String> {
    let url = req.url.trim();
    if url.is_empty() || url.starts_with('-') {
        return Err(tr!("URL du dépôt invalide : « {url} »", url));
    }
    let folder = req.folder.trim();
    let single = matches!(Path::new(folder).components().collect::<Vec<_>>().as_slice(), [Component::Normal(_)]);
    if !single || folder.contains(['/', '\\', ':']) {
        return Err(tr!("Nom de dossier invalide : « {name} »", name = folder));
    }
    if !req.parent.is_absolute() || !req.parent.is_dir() {
        return Err(tr!("Dossier parent introuvable : {path}", path = req.parent.display()));
    }
    let dest = req.parent.join(folder);
    // Écrite telle quelle et sous son parent résolu : une jonction ou un nom court ne cache pas un dossier protégé.
    // Un nom qui commence par un point est celui d'un dossier de configuration (`.config`, `.vscode`…), qu'un outil
    // lirait.
    let (homes, documents) = user_dirs();
    let written_and_resolved = [Some(dest.clone()), resolved(&req.parent).map(|p| p.join(folder))];
    if folder.starts_with('.') || written_and_resolved.iter().flatten().any(|d| protected_dest(d, &homes, &documents)) {
        return Err(tr!("Dossier protégé (secrets ou configuration) : {path}", path = dest.display()));
    }
    let empty = || std::fs::read_dir(&dest).is_ok_and(|mut d| d.next().is_none());
    if dest.exists() && !(dest.is_dir() && empty()) {
        return Err(tr!("Ce dossier existe déjà et n'est pas vide.").into());
    }
    if let Some(branch) = req.branch.as_deref().map(str::trim).filter(|b| b.is_empty() || b.starts_with('-')) {
        return Err(tr!("Nom de branche invalide : « {name} »", name = branch));
    }
    if let Some(id) = req.account.as_ref().filter(|id| !accounts.iter().any(|a| &a.id == *id)) {
        return Err(tr!("Compte inconnu : {id}", id));
    }
    Ok(dest)
}

/// Lance le clonage d'un hôte (demande contrôlée par [`check_request`], destination `dest`) : comme « Cloner » de la
/// fenêtre depuis une URL, sans ouverture à la fin (l'application l'ouvre en fond, voir `DragonTreeApp::clone_remote`).
pub(crate) fn start_host_clone(shared: &Shared, git: Git, req: CloneRequest, dest: PathBuf) -> HostJob {
    let id = NEXT_CLONE_ID.fetch_add(1, Ordering::Relaxed);
    let cancel = CancelToken::new();
    let title = tr!("Clonage de {name}", name = req.folder.trim());
    let spec = CloneSpec {
        id,
        url: req.url.trim().to_string(),
        dest: dest.clone(),
        parent: req.parent,
        branch: req.branch.map(|b| b.trim().to_string()).unwrap_or_default(),
        recurse: req.recurse,
        account: req.account,
        open_after: false,
        cancel: cancel.clone(),
    };
    spawn_clone(shared, git, spec);
    HostJob { id, title, dest, progress: None, result: None, at: Instant::now(), cancel }
}

/// Ce que lance un clonage, de la fenêtre ou d'un hôte.
struct CloneSpec {
    id: u64,
    url: String,
    dest: PathBuf,
    parent: PathBuf,
    /// Vide : la branche par défaut.
    branch: String,
    recurse: bool,
    account: Option<String>,
    open_after: bool,
    cancel: CancelToken,
}

/// Lance le clonage `s` en tâche de fond : sa progression et sa fin arrivent en [`CloneEvent`], avec son numéro ; le
/// jeton du compte authentifie git (variables d'environnement) ; un dossier créé pour lui est supprimé s'il échoue ou
/// est annulé.
fn spawn_clone(shared: &Shared, git: Git, s: CloneSpec) {
    let CloneSpec { id, url, dest, parent, branch, recurse, account, open_after, cancel } = s;
    let created_dir = !dest.exists();
    let acc = account.as_ref().and_then(|id| shared.settings.accounts.iter().find(|a| &a.id == id).cloned());
    let sender = shared.jobs.sender();
    let task = shared.clone_tasks.start();
    shared.jobs.spawn("clone", move || {
        let _task = task;
        let mut env = Vec::new();
        if let Some(acc) = &acc
            && let Ok(Some(token)) = secrets::load_token(&acc.id)
        {
            env = dragontree_forges::git_auth_env(acc, &token, &[url.as_str()]);
        }
        let net = NetOptions { env, cancel, non_interactive: false };
        let mut last = std::time::Instant::now();
        let result = dragontree_git::clone_repo(
            &git,
            &url,
            &dest,
            (!branch.is_empty()).then_some(branch.as_str()),
            recurse,
            &net,
            &mut |p| {
                if last.elapsed().as_millis() >= 80 || p.percent == Some(100) {
                    last = std::time::Instant::now();
                    sender.send(AppEvent::Clone(CloneEvent::Progress(id, p)));
                }
            },
        );
        // Nettoyage ici (et non dans la fenêtre) : il a lieu même si la fenêtre a été fermée.
        if result.is_err() && created_dir {
            remove_failed_clone(&dest);
        }
        Some(AppEvent::Clone(CloneEvent::Done(CloneDone { id, dest, parent, account, open_after, result })))
    });
}

enum Load {
    Loading,
    Loaded(Vec<RemoteRepo>),
    Failed(String),
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum Source {
    Accounts,
    Url,
}

pub struct CloneWindow {
    source: Source,
    account_filter: Option<String>,
    repos: HashMap<String, Load>,
    filter: String,
    selected: Option<(String, String)>,
    url: String,
    use_ssh: bool,
    parent_dir: String,
    folder: String,
    folder_touched: bool,
    branch: String,
    recurse: bool,
    open_after: bool,
    /// Compte qui authentifie le clonage (et que le dépôt cloné gardera) : celui du dépôt choisi, ou, depuis une URL,
    /// celui que l'on choisit (présélectionné d'après l'URL, voir `url_account_for`).
    account: Option<String>,
    /// URL pour laquelle le compte a été présélectionné : un choix fait à la main tient tant qu'elle ne change pas.
    url_account_for: String,
    /// Clonage en cours : identifiant et annulation.
    running: Option<(u64, CancelToken)>,
    progress: Option<Progress>,
    error: Option<String>,
    ssh_available: Option<String>,
    done: bool,
}

/// Date de mise à jour d'un dépôt pour la liste : relative si récente, absolue sinon ; `None` si la forge n'en donne
/// pas (Azure DevOps).
fn updated_label(r: &RemoteRepo, now: i64) -> Option<String> {
    r.updated_timestamp().map(|ts| widgets::format_time(ts, now))
}

/// Le compte présélectionné pour cloner `url` : le premier dont l'hôte (et l'organisation Azure) correspond.
pub(crate) fn default_account_for(url: &str, accounts: &[dragontree_forges::Account]) -> Option<String> {
    accounts.iter().find(|a| dragontree_forges::account_matches_url(a, url)).map(|a| a.id.clone())
}

/// Nom de dossier déduit d'une URL (`…/repo.git` → `repo`).
pub fn folder_from_url(url: &str) -> String {
    let trimmed = url.trim().trim_end_matches('/');
    let last = trimmed.rsplit(['/', ':']).next().unwrap_or("");
    last.strip_suffix(".git").unwrap_or(last).to_string()
}

impl CloneWindow {
    pub fn new(shared: &Shared) -> CloneWindow {
        let parent = default_parent(shared);
        let mut w = CloneWindow {
            source: if shared.settings.accounts.is_empty() { Source::Url } else { Source::Accounts },
            account_filter: None,
            repos: HashMap::new(),
            filter: String::new(),
            selected: None,
            url: String::new(),
            use_ssh: false,
            parent_dir: parent.display().to_string(),
            folder: String::new(),
            folder_touched: false,
            branch: String::new(),
            // Les sous-modules sont une demande explicite : leur clonage suit des URL choisies par le dépôt.
            recurse: false,
            open_after: true,
            account: None,
            url_account_for: String::new(),
            running: None,
            progress: None,
            error: None,
            ssh_available: None,
            done: false,
        };
        w.load_all(shared);
        w
    }

    fn load_all(&mut self, shared: &Shared) {
        for acc in &shared.settings.accounts {
            self.repos.insert(acc.id.clone(), Load::Loading);
            let acc = acc.clone();
            shared.jobs.spawn("list-repos", move || {
                let result = match secrets::load_token(&acc.id) {
                    Ok(Some(token)) => dragontree_forges::list_repos(&acc, &token),
                    Ok(None) => Err(ForgeError::Secret(tr!("jeton absent du Gestionnaire d'identifiants").into())),
                    Err(e) => Err(e),
                };
                Some(AppEvent::Clone(CloneEvent::Repos { account_id: acc.id.clone(), result }))
            });
        }
    }

    pub fn handle(&mut self, ev: CloneEvent) {
        match ev {
            CloneEvent::Repos { account_id, result } => {
                let load = match result {
                    Ok(r) => Load::Loaded(r),
                    Err(e) => Load::Failed(e.to_string()),
                };
                self.repos.insert(account_id, load);
            }
            CloneEvent::Progress(id, p) => {
                if self.running.as_ref().is_some_and(|(running, _)| *running == id) {
                    self.progress = Some(p);
                }
            }
            CloneEvent::Done(done) => self.on_done(&done),
        }
    }

    /// Annulation du clonage en cours, s'il y en a un.
    pub fn running(&self) -> Option<&CancelToken> {
        self.running.as_ref().map(|(_, cancel)| cancel)
    }

    /// État de la fenêtre à la fin de son clonage (le reste est fait par [`finish_clone`]).
    pub fn on_done(&mut self, done: &CloneDone) {
        if self.running.as_ref().is_none_or(|(id, _)| *id != done.id) {
            return;
        }
        self.running = None;
        self.progress = None;
        match &done.result {
            Ok(()) => self.done = true,
            Err(GitError::Cancelled) => self.error = Some(tr!("Clonage annulé.").into()),
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    fn dest(&self) -> PathBuf {
        PathBuf::from(self.parent_dir.trim()).join(self.folder.trim())
    }

    fn start_clone(&mut self, shared: &Shared) {
        let Some(git) = shared.git.clone() else {
            self.error = Some(tr!("git est introuvable.").into());
            return;
        };
        let account = if self.use_ssh { None } else { self.account.clone() };
        let id = NEXT_CLONE_ID.fetch_add(1, Ordering::Relaxed);
        let cancel = CancelToken::new();
        self.running = Some((id, cancel.clone()));
        self.error = None;
        self.progress = None;
        let spec = CloneSpec {
            id,
            url: self.url.trim().to_string(),
            dest: self.dest(),
            parent: PathBuf::from(self.parent_dir.trim()),
            branch: self.branch.trim().to_string(),
            recurse: self.recurse,
            account,
            open_after: self.open_after,
            cancel,
        };
        spawn_clone(shared, git, spec);
    }

    /// Renvoie faux quand la fenêtre est fermée.
    pub fn ui(&mut self, ctx: &egui::Context, shared: &mut Shared, p: &Palette) -> bool {
        if self.done {
            return false;
        }
        let mut open = true;
        // Bornée à la fenêtre principale (sous l'en-tête où elle s'ouvre) : la liste prend ce qui reste, sans jamais
        // agrandir la fenêtre (une liste qui se mesurait sur la fenêtre la faisait grandir à chaque image).
        let max_h = (ctx.content_rect().height() - 56.0 - 12.0).max(240.0);
        crate::widgets::centered_window(ctx, format!("{} {}", icons::DOWNLOAD_SIMPLE, tr!("Cloner un dépôt")))
            .id(egui::Id::unique("clone-window"))
            .open(&mut open)
            .default_size([640.0, max_h.min(540.0)])
            .max_height(max_h)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(
                        &mut self.source,
                        Source::Accounts,
                        format!("{} {}", icons::USER_CIRCLE, tr!("Depuis mes comptes")),
                    );
                    ui.selectable_value(
                        &mut self.source,
                        Source::Url,
                        format!("{} {}", icons::LINK, tr!("Depuis une URL")),
                    );
                });
                ui.separator();
                // La destination en bas de la fenêtre, d'abord : le haut sait alors ce qui lui reste.
                egui::Panel::bottom("clone-destination").frame(egui::Frame::new()).show_separator_line(false).show(
                    ui,
                    |ui| {
                        ui.separator();
                        self.destination_ui(ui, shared, p);
                    },
                );
                match self.source {
                    Source::Accounts => self.accounts_ui(ui, shared, p),
                    Source::Url => {
                        ui.label(tr!("URL du dépôt (HTTPS ou SSH)"));
                        let resp = ui.add(
                            egui::TextEdit::singleline(&mut self.url)
                                .hint_text(tr!("https://github.com/organisation/depot.git"))
                                .desired_width(f32::INFINITY),
                        );
                        if resp.changed() && !self.folder_touched {
                            self.folder = folder_from_url(&self.url);
                        }
                        self.url_account_ui(ui, shared);
                    }
                }
            });
        if !open && let Some((_, c)) = &self.running {
            c.cancel();
        }
        open
    }

    /// Le compte d'un clonage par URL : ceux qui correspondent à l'URL, ou aucun (les identifiants de git).
    fn url_account_ui(&mut self, ui: &mut egui::Ui, shared: &Shared) {
        let accounts = &shared.settings.accounts;
        self.preselect_for_url(accounts);
        let none = tr!("Aucun : identifiants de git");
        let current = self
            .account
            .as_ref()
            .and_then(|id| accounts.iter().find(|a| &a.id == id))
            .map_or(none.to_string(), |a| a.title());
        ui.horizontal(|ui| {
            let label = ui.label(tr!("Compte"));
            egui::ComboBox::from_id_salt("clone-url-account")
                .selected_text(current)
                .show_ui(ui, |ui| {
                    for a in accounts.iter().filter(|a| dragontree_forges::account_matches_url(a, self.url.trim())) {
                        ui.selectable_value(
                            &mut self.account,
                            Some(a.id.clone()),
                            format!("{} {}", kind_icon(a.kind), a.title()),
                        );
                    }
                    ui.selectable_value(&mut self.account, None, none);
                })
                .response
                .labelled_by(label.id);
        });
    }

    fn accounts_ui(&mut self, ui: &mut egui::Ui, shared: &mut Shared, p: &Palette) {
        let accounts = shared.settings.accounts.clone();
        if accounts.is_empty() {
            ui.add_space(20.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new(tr!("Aucun compte connecté.")).color(p.weak));
                if ui.button(format!("{} {}", icons::PLUS, tr!("Ajouter un compte…"))).clicked() {
                    shared.actions.push(AppAction::ShowAccounts);
                }
            });
            return;
        }
        ui.horizontal(|ui| {
            let label = self
                .account_filter
                .as_ref()
                .and_then(|id| accounts.iter().find(|a| &a.id == id))
                .map_or(tr!("Tous les comptes").to_string(), |a| a.title());
            let combo =
                egui::ComboBox::from_id_salt("clone-account").selected_text(label).width(260.0).show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.account_filter, None, tr!("Tous les comptes"));
                    for a in &accounts {
                        ui.selectable_value(
                            &mut self.account_filter,
                            Some(a.id.clone()),
                            format!("{} {}", kind_icon(a.kind), a.title()),
                        );
                    }
                });
            // Sans libellé à côté : son nom pour les lecteurs d'écran.
            let name = tr!("Compte").to_string();
            combo.response.widget_info(move || egui::WidgetInfo::labeled(egui::Role::ComboBox, true, name.clone()));
            if widgets::icon_button(ui, icons::ARROWS_CLOCKWISE, tr!("Recharger les listes")).clicked() {
                self.load_all(shared);
            }
            ui.allocate_ui(egui::vec2(ui.available_width(), 24.0), |ui| {
                widgets::search_field(ui, &mut self.filter, tr!("Filtrer les dépôts"), "clone-filter")
            });
        });
        for a in &accounts {
            match self.repos.get(&a.id) {
                Some(Load::Loading) => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(
                            RichText::new(tr!("Chargement des dépôts de {account}…", account = a.title()))
                                .color(p.weak),
                        );
                    });
                }
                Some(Load::Failed(e)) => {
                    ui.label(
                        RichText::new(format!(
                            "{} {}",
                            icons::WARNING,
                            tr!("{account} : {error}", account = a.title(), error = e)
                        ))
                        .color(p.removed_fg),
                    );
                }
                _ => {}
            }
        }
        let mut rows: Vec<(&str, &RemoteRepo, i32)> = Vec::new();
        for a in accounts.iter().filter(|a| self.account_filter.as_ref().is_none_or(|f| f == &a.id)) {
            if let Some(Load::Loaded(list)) = self.repos.get(&a.id) {
                for r in list {
                    let hay = r.full_name();
                    if let Some(score) = widgets::fuzzy_score(&self.filter, &hay) {
                        rows.push((a.id.as_str(), r, score));
                    }
                }
            }
        }
        if !self.filter.trim().is_empty() {
            rows.sort_by_key(|x| std::cmp::Reverse(x.2));
        }
        ui.label(RichText::new(trn!(rows.len(), "{n} dépôt", "{n} dépôts")).color(p.weak).size(12.0));
        let mut chosen: Option<(String, RemoteRepo)> = None;
        let now = widgets::now_ts();
        // Tout ce qui reste au-dessus de la destination, même si c'est peu.
        egui::ScrollArea::vertical().max_height(ui.available_height()).auto_shrink([false, false]).show_rows(
            ui,
            38.0,
            rows.len(),
            |ui, range| {
                ui.spacing_mut().item_spacing.y = 0.0;
                for i in range {
                    let (acc_id, r, _) = rows[i];
                    let selected = self.selected.as_ref().is_some_and(|(a, c)| a == acc_id && c == &r.clone_url);
                    let (rect, resp) =
                        widgets::row_response(ui, ("clone-row", acc_id, &r.clone_url), 38.0, selected, Sense::click());
                    let painter = ui.painter_at(rect);
                    let kind = accounts.iter().find(|a| a.id == acc_id).map(|a| a.kind);
                    if let Some(k) = kind {
                        painter.text(
                            rect.left_center() + egui::vec2(14.0, 0.0),
                            egui::Align2::CENTER_CENTER,
                            kind_icon(k),
                            egui::FontId::proportional(16.0),
                            p.weak,
                        );
                    }
                    // Date de mise à jour, à droite de la première ligne (à gauche du cadenas) ; le nom, rogné, ne la
                    // recouvre jamais.
                    let mut name_right = rect.right() - 34.0;
                    if let Some(when) = updated_label(r, now) {
                        let date_rect = painter.text(
                            rect.right_top() + egui::vec2(-34.0, 11.0),
                            egui::Align2::RIGHT_CENTER,
                            when,
                            egui::FontId::proportional(11.5),
                            p.weak,
                        );
                        name_right = date_rect.left() - 8.0;
                    }
                    let name_clip =
                        egui::Rect::from_min_max(rect.min, egui::pos2(name_right.max(rect.left()), rect.max.y));
                    painter.with_clip_rect(name_clip).text(
                        rect.left_top() + egui::vec2(32.0, 11.0),
                        egui::Align2::LEFT_CENTER,
                        &r.name,
                        egui::FontId::new(13.5, crate::theme::bold_family()),
                        p.text,
                    );
                    let mut sub = r.namespace.clone();
                    if let Some(d) = r.description.as_ref().filter(|d| !d.is_empty()) {
                        sub.push_str(&format!(" — {d}"));
                    }
                    painter.text(
                        rect.left_top() + egui::vec2(32.0, 27.0),
                        egui::Align2::LEFT_CENTER,
                        widgets::ellipsize_middle(&sub, 90),
                        egui::FontId::proportional(11.5),
                        p.weak,
                    );
                    if r.private {
                        painter.text(
                            rect.right_center() - egui::vec2(10.0, 0.0),
                            egui::Align2::RIGHT_CENTER,
                            icons::LOCK_SIMPLE,
                            egui::FontId::proportional(13.0),
                            p.weak,
                        );
                    }
                    if resp.clicked() {
                        chosen = Some((acc_id.to_string(), r.clone()));
                    }
                }
            },
        );
        if let Some((acc, r)) = chosen {
            self.choose_repo(acc, &r);
        }
    }

    /// Un dépôt d'un compte choisi dans la liste : son URL, et ce compte, que la présélection d'après l'URL ne remplace
    /// pas (voir `preselect_for_url`).
    fn choose_repo(&mut self, acc: String, r: &RemoteRepo) {
        self.selected = Some((acc.clone(), r.clone_url.clone()));
        self.account = Some(acc);
        self.url = if self.use_ssh { r.ssh_url.clone().unwrap_or(r.clone_url.clone()) } else { r.clone_url.clone() };
        self.url_account_for = self.url.clone();
        if !self.folder_touched {
            self.folder = r.name.clone();
        }
        self.branch.clear();
        self.ssh_available = r.ssh_url.clone();
    }

    /// Une URL tapée (ou changée) : le compte présélectionné est le premier qui lui correspond. Un compte choisi, à la
    /// main ou avec un dépôt de la liste, tient tant que l'URL ne change pas.
    fn preselect_for_url(&mut self, accounts: &[dragontree_forges::Account]) {
        if self.url_account_for != self.url {
            self.url_account_for = self.url.clone();
            self.account = default_account_for(self.url.trim(), accounts);
        }
    }

    fn destination_ui(&mut self, ui: &mut egui::Ui, shared: &mut Shared, p: &Palette) {
        egui::Grid::new("clone-dest").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
            let parent_label = ui.label(tr!("Dossier parent"));
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut self.parent_dir).desired_width(420.0))
                    .labelled_by(parent_label.id);
                if ui.button(tr!("Parcourir…")).clicked()
                    && let Some(dir) = rfd::FileDialog::new().set_title(tr!("Dossier où cloner")).pick_folder()
                {
                    self.parent_dir = dir.display().to_string();
                }
            });
            ui.end_row();
            let folder_label = ui.label(tr!("Nom du dossier"));
            if ui
                .add(egui::TextEdit::singleline(&mut self.folder).desired_width(260.0))
                .labelled_by(folder_label.id)
                .changed()
            {
                self.folder_touched = true;
            }
            ui.end_row();
            ui.label(tr!("Branche"));
            ui.add(
                egui::TextEdit::singleline(&mut self.branch)
                    .hint_text(tr!("(branche par défaut)"))
                    .desired_width(260.0),
            );
            ui.end_row();
        });
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.recurse, tr!("Sous-modules"));
            ui.checkbox(&mut self.open_after, tr!("Ouvrir après le clonage"));
            if self.source == Source::Accounts
                && let Some(ssh) = self.ssh_available.clone()
                && ui.checkbox(&mut self.use_ssh, tr!("Utiliser SSH")).changed()
            {
                self.url =
                    if self.use_ssh { ssh } else { self.selected.as_ref().map(|(_, u)| u.clone()).unwrap_or_default() };
            }
        });
        let dest = self.dest();
        let exists_non_empty = !self.folder.trim().is_empty()
            && dest.exists()
            && std::fs::read_dir(&dest).map(|mut d| d.next().is_some()).unwrap_or(false);
        ui.label(RichText::new(format!("→ {}", dest.display())).monospace().size(12.0).color(if exists_non_empty {
            p.removed_fg
        } else {
            p.weak
        }));
        if exists_non_empty {
            ui.label(
                RichText::new(format!("{} {}", icons::WARNING, tr!("Ce dossier existe déjà et n'est pas vide.")))
                    .color(p.removed_fg),
            );
        }
        if let Some(err) = &self.error {
            ui.add(
                egui::Label::new(RichText::new(format!("{} {err}", icons::WARNING_CIRCLE)).color(p.removed_fg)).wrap(),
            );
        }
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if let Some((_, cancel)) = self.running.clone() {
                ui.spinner();
                let text = match &self.progress {
                    Some(pg) => match pg.percent {
                        Some(pc) => format!("{} {pc}%", pg.phase),
                        None => format!("{} {}", pg.phase, pg.current),
                    },
                    None => tr!("Connexion…").into(),
                };
                ui.label(text);
                if let Some(pc) = self.progress.as_ref().and_then(|x| x.percent) {
                    ui.add(egui::ProgressBar::new(f32::from(pc) / 100.0).desired_width(200.0));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(tr!("Annuler")).clicked() {
                        cancel.cancel();
                    }
                });
            } else {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let ok = !self.url.trim().is_empty()
                        && !self.folder.trim().is_empty()
                        && !self.parent_dir.trim().is_empty()
                        && !exists_non_empty;
                    if ui
                        .add_enabled(
                            ok,
                            egui::Button::new(
                                RichText::new(format!("{} {}", icons::DOWNLOAD_SIMPLE, tr!("Cloner")))
                                    .strong()
                                    .color(egui::Color32::WHITE),
                            )
                            .fill(p.accent)
                            .min_size(egui::vec2(120.0, 30.0)),
                        )
                        .clicked()
                    {
                        self.start_clone(shared);
                    }
                });
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::{CloneWindow, Source, folder_from_url, updated_label};

    fn window() -> CloneWindow {
        CloneWindow {
            source: Source::Accounts,
            account_filter: None,
            repos: Default::default(),
            filter: String::new(),
            selected: None,
            url: String::new(),
            use_ssh: false,
            parent_dir: String::new(),
            folder: String::new(),
            folder_touched: false,
            branch: String::new(),
            recurse: false,
            open_after: true,
            account: None,
            url_account_for: String::new(),
            running: None,
            progress: None,
            error: None,
            ssh_available: None,
            done: false,
        }
    }

    fn github(id: &str) -> dragontree_forges::Account {
        dragontree_forges::Account {
            id: id.into(),
            kind: dragontree_forges::ForgeKind::GitHub,
            host: "github.com".into(),
            username: id.into(),
            display_name: String::new(),
            orgs: Vec::new(),
            avatar_url: None,
        }
    }

    /// Le dépôt d'un compte choisi dans la liste garde ce compte en passant par « Depuis une URL » : la présélection
    /// d'après l'URL ne vaut que pour une URL tapée.
    #[test]
    fn a_repository_picked_in_an_account_keeps_that_account_through_the_url_tab() {
        let accounts = [github("perso"), github("pro")];
        let mut w = window();
        w.choose_repo("pro".into(), &repo(None));
        w.preselect_for_url(&accounts);
        assert_eq!(w.account.as_deref(), Some("pro"), "celui du dépôt choisi, pas le premier qui correspond");
        w.url = "https://github.com/autre/depot.git".into();
        w.preselect_for_url(&accounts);
        assert_eq!(w.account.as_deref(), Some("perso"), "une autre URL : présélection d'après elle");
    }

    fn repo(updated_at: Option<&str>) -> dragontree_forges::RemoteRepo {
        dragontree_forges::RemoteRepo {
            account_id: "acc".into(),
            name: "api".into(),
            namespace: "contoso".into(),
            clone_url: "https://example.com/contoso/api.git".into(),
            ssh_url: None,
            web_url: "https://example.com/contoso/api".into(),
            description: None,
            private: false,
            default_branch: None,
            updated_at: updated_at.map(str::to_owned),
        }
    }

    #[test]
    fn the_update_date_is_relative_when_recent_and_absolute_otherwise() {
        let now = 1_296_068_803 + 3 * 3600;
        // Forme des dates de GitHub (`pushed_at`) et de GitLab (`last_activity_at`, avec fraction).
        assert_eq!(updated_label(&repo(Some("2011-01-26T19:06:43Z")), now).as_deref(), Some("il y a 3 h"));
        assert_eq!(updated_label(&repo(Some("2011-01-26T19:06:43.841Z")), now).as_deref(), Some("il y a 3 h"));
        let old = updated_label(&repo(Some("2011-01-26T19:06:43Z")), now + 30 * 86_400).unwrap();
        assert!(old.contains("2011"), "date absolue : {old}");
        assert_eq!(updated_label(&repo(None), now), None, "Azure DevOps : pas de date");
    }

    /// La fenêtre Cloner ouverte sur une liste de `n` dépôts, dans une fenêtre principale de cette taille ; rend le
    /// rectangle de la fenêtre Cloner et celui de la fenêtre principale.
    fn clone_window_in(size: [f32; 2], n: usize) -> (egui::Rect, egui::Rect) {
        let dir = tempfile::tempdir().unwrap();
        let (store, mut settings, _) = crate::settings::SettingsFile::load(&dir.path().join("settings.json"));
        // Trois comptes : un chargé, un en échec, un qui charge encore (une ligne chacun au-dessus de la liste).
        settings.accounts = vec![github("perso"), github("pro"), github("asso")];
        let (mut shared, _events) = crate::app::Shared::new(egui::Context::default(), settings, store, None, None);
        let mut w = window();
        w.repos.insert("pro".into(), super::Load::Failed("HTTP 401 : jeton refusé par le serveur".into()));
        w.repos.insert("asso".into(), super::Load::Loading);
        let list = (0..n)
            .map(|i| dragontree_forges::RemoteRepo {
                name: format!("depot-{i}"),
                account_id: "perso".into(),
                clone_url: format!("https://example.com/contoso/depot-{i}.git"),
                ..repo(None)
            })
            .collect();
        w.repos.insert("perso".into(), super::Load::Loaded(list));
        let p = crate::theme::Palette::dark();
        let mut h = egui_kittest::Harness::builder()
            .with_size(size)
            .with_context_setup(|ctx| crate::theme::install_fonts(ctx, Default::default()))
            .build_ui(|ui| {
                w.ui(&ui.ctx().clone(), &mut shared, &p);
            });
        h.run_steps(8);
        let window = h.ctx.memory(|m| m.area_rect(egui::Id::unique("clone-window"))).expect("fenêtre Cloner");
        (window, h.ctx.content_rect())
    }

    /// Une longue liste ne pousse pas la fenêtre hors de la fenêtre principale, même petite.
    #[test]
    fn a_long_list_stays_inside_a_small_main_window() {
        let (window, screen) = clone_window_in([900.0, 560.0], 200);
        assert!(screen.expand(1.0).contains_rect(window), "{window:?} hors de {screen:?}");
    }

    /// Dans une grande fenêtre, la fenêtre Cloner reste de taille raisonnable.
    #[test]
    fn the_window_stays_compact_in_a_large_main_window() {
        let (window, _) = clone_window_in([1600.0, 1000.0], 200);
        assert!(window.width() <= 680.0 && window.height() <= 600.0, "{window:?}");
    }

    /// Comme pour le pont d'Eyrie : un dossier de secrets où qu'il soit, un dossier de configuration du profil, un
    /// profil de PowerShell des Documents, et ce qu'ils contiennent, sans casse ; pas un dossier voisin au nom proche,
    /// ni un dossier à point hors du profil, ni un dépôt « PowerShell » hors des Documents.
    #[test]
    fn a_clone_never_lands_in_a_secrets_or_configuration_folder() {
        use std::path::{Path, PathBuf};
        let homes = [PathBuf::from(r"C:\Users\ada")];
        let documents = [PathBuf::from(r"C:\Users\ada\Documents"), PathBuf::from(r"D:\OneDrive\Documents")];
        let protected = |p: &str| super::protected_dest(Path::new(p), &homes, &documents);
        let refused = [
            r"D:\depots\.ssh",
            r"D:\.AWS\depot",
            r"E:\$Recycle.Bin\depot",
            r"C:\Users\ada\.config\git",
            r"c:/users/ADA/.claude",
            r"C:\Users\ada\Documents\PowerShell",
            r"D:\OneDrive\Documents\WindowsPowerShell\Modules\depot",
        ];
        for p in refused {
            assert!(protected(p), "{p}");
        }
        let allowed = [
            r"C:\Users\ada\source\repos\depot",
            r"C:\Users\adam\.config",
            r"D:\DEV\.outils\depot",
            r"D:\DEV\PowerShell",
            r"C:\Users\ada\Documents\PowerShellTools",
            r"D:\depots\aws",
        ];
        for p in allowed {
            assert!(!protected(p), "{p}");
        }
    }

    #[test]
    fn folder_names_from_urls() {
        assert_eq!(folder_from_url("https://github.com/acme/projet.git"), "projet");
        assert_eq!(folder_from_url("https://dev-fabrikam.visualstudio.com/Projet/_git/Depot"), "Depot");
        assert_eq!(folder_from_url("git@github.com:acme/outil.git"), "outil");
        assert_eq!(folder_from_url("https://gitlab.com/g/sous/projet/"), "projet");
    }
}
