//! Application : onglets, liste des dépôts à droite, fenêtres globales, événements.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use dragontree_git::{Git, GitError};
use dragontree_i18n::{Lang, tr};
use egui::{Frame, Key, KeyboardShortcut, Modifiers};

use crate::accounts::{AccountsWindow, ForgeEvent};
use crate::clone::{CloneEvent, CloneWindow};
use crate::closed_tabs::ClosedTabs;
use crate::closing::Operations;
use crate::commands::{self, Command, CommandContext, Scope};
use crate::jobs::Jobs;
use crate::pull_requests::{PullRequestEvent, PullRequestSource, PullRequestsWindow};
use crate::repo::{RepoEvent, RepoTab};
use crate::repo_list::{RepoListState, ScanEvent};
use crate::settings::{Settings, SettingsFile, ThemeChoice, path_key};
use crate::settings_ui::{SettingsTab, SettingsWindow};
use crate::ssh_keys::{SshEvent, SshKeysWindow};
use crate::tabbar::{self, TabBarAction, TabInfo};
use crate::theme::{self, Palette};
use crate::toasts::{CommandLog, Toast, Toasts};
use crate::welcome::{self, WelcomeAction};
use crate::workspace_dialog::WorkspaceAction;

pub type TabId = u64;

/// Événements envoyés par les tâches d'arrière-plan.
pub enum AppEvent {
    Repo(TabId, RepoEvent),
    Scan(ScanEvent),
    Forge(ForgeEvent),
    Ssh(SshEvent),
    GitIdentity(crate::git_identity::IdentityEvent),
    Clone(CloneEvent),
    Trusted(PathBuf, Result<(), GitError>),
    Initialized(PathBuf, Result<(), GitError>),
    /// Résultat de la détection de `git lfs`.
    LfsDetected(bool),
    /// Éditeurs externes trouvés sur la machine (détection faite une fois, hors du thread de l'interface).
    EditorsDetected(Vec<crate::editors::Editor>),
    /// Préréglages d'outils de diff et de merge installés (même détection, même moment).
    ToolsDetected(Vec<crate::external_tools::DetectedTool>),
    /// Test de la CLI Claude Code (Réglages › IA) : sa version, ou le message d'erreur.
    AiChecked(Result<String, String>),
    /// Version de git lue pour « À propos » (numéro de la demande, version ou message d'erreur).
    GitVersion(u64, Result<String, String>),
    /// Fenêtre « Pull requests » : pull requests d'un compte, copies locales.
    PullRequests(PullRequestEvent),
}

/// Test de la CLI Claude Code lancé depuis les réglages.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum AiCheck {
    #[default]
    Idle,
    Running,
    Done(Result<String, String>),
}

/// Demandes des vues à l'application (traitées après l'affichage).
#[derive(Debug, Clone)]
pub enum AppAction {
    OpenRepo(PathBuf),
    OpenRepoNewTab(PathBuf),
    PickAndOpenRepo,
    InitRepo,
    ShowClone,
    ShowAccounts,
    ShowPullRequests,
    ShowSshKeys,
    ShowGitIdentity,
    ShowSettings,
    /// Réglages ouverts sur un onglet donné (Dépôt › Commandes personnalisées › Gérer les commandes…).
    ShowSettingsTab(SettingsTab),
    ShowLog,
    AddScanRoot,
    RemoveScanRoot(PathBuf),
    Rescan,
    TrustRepo(PathBuf),
    QuickOpen,
    /// Supprimer le worktree `path`, demandé par l'onglet `tab` : les onglets ouverts sur ce worktree sont d'abord
    /// fermés (ils perdraient leur dossier), sans être retenus pour Ctrl+Maj+T, puis l'onglet `tab` lance la
    /// suppression.
    RemoveWorktree {
        tab: TabId,
        path: PathBuf,
        force: bool,
    },
    /// Fermer l'application (Fichier › Quitter) : passe par `save`, comme la croix de la fenêtre.
    Quit,
    /// Commande du registre choisie dans une vue (palette Ctrl+P).
    RunCommand(Command),
}

/// État partagé passé aux vues.
pub struct Shared {
    pub git: Option<Git>,
    pub settings: Settings,
    pub settings_store: SettingsFile,
    pub settings_dirty: bool,
    pub jobs: Jobs,
    pub toasts: Toasts,
    pub log: CommandLog,
    pub actions: Vec<AppAction>,
    pub profile_dir: Option<PathBuf>,
    pub ctx: egui::Context,
    /// Dernier mode choisi dans le dialogue de pull pendant la session (`true` : rebase) : le dialogue suivant
    /// et le pull rapide le reprennent.
    pub pull_rebase: bool,
    /// Git LFS est installé : `None` tant que la détection (une fois par exécution, en tâche de fond) n'a pas répondu.
    pub lfs: Option<bool>,
    /// Éditeurs de « Ouvrir dans » : seul l'éditeur par défaut de Windows tant que la détection n'a pas répondu.
    pub editors: Vec<crate::editors::Editor>,
    /// Préréglages d'outils externes trouvés sur la machine (vide tant que la détection n'a pas répondu).
    pub tools: Vec<crate::external_tools::DetectedTool>,
    /// Dernier test de la CLI Claude Code (Réglages › IA).
    pub ai_check: AiCheck,
    /// Intervalle du fetch automatique imposé par les tests (`StartOptions::auto_fetch_every`) ; sinon celui des
    /// réglages.
    pub auto_fetch_every: Option<Duration>,
    /// Files d'opérations des dépôts, partagées par leurs onglets (voir `op_queue`).
    pub queues: crate::op_queue::OpQueues,
}

impl Shared {
    /// État partagé ; le récepteur reçoit les événements des tâches d'arrière-plan.
    pub fn new(
        ctx: egui::Context,
        settings: Settings,
        settings_store: SettingsFile,
        profile_dir: Option<PathBuf>,
        git: Option<Git>,
    ) -> (Shared, Receiver<AppEvent>) {
        let (jobs, rx) = Jobs::new(ctx.clone());
        let log = CommandLog::install(ctx.clone());
        let shared = Shared {
            git,
            settings,
            settings_store,
            settings_dirty: false,
            jobs,
            toasts: Toasts::default(),
            log,
            actions: Vec::new(),
            profile_dir,
            ctx,
            pull_rebase: false,
            lfs: None,
            editors: vec![crate::editors::Editor::system_default()],
            tools: Vec::new(),
            ai_check: AiCheck::Idle,
            auto_fetch_every: None,
            queues: Default::default(),
        };
        (shared, rx)
    }

    pub fn settings_file(&self) -> PathBuf {
        self.settings_store.path().to_path_buf()
    }

    pub fn save_settings(&mut self) {
        if let Err(e) = self.settings_store.save(&self.settings) {
            self.toasts.push(Toast::error(tr!("Impossible d'enregistrer les réglages"), e.to_string()));
        }
        self.settings_dirty = false;
    }

    pub fn toast(&mut self, t: Toast) {
        self.toasts.push(t);
    }

    /// « Ouvrir un terminal » avec le terminal des réglages ; notification si le choix était indisponible.
    pub fn open_terminal(&mut self, path: &std::path::Path) {
        let git = self.git.as_ref().map(|g| g.exe().to_path_buf());
        match crate::terminal::open(self.settings.terminal, path, git.as_deref()) {
            Ok(None) => {}
            Ok(Some((choice, crate::terminal::Unavailable::Missing))) => self.toast(Toast::info(tr!(
                "Terminal : {choice} est introuvable, un autre terminal est utilisé à la place.",
                choice = choice.label()
            ))),
            Ok(Some((choice, crate::terminal::Unavailable::Failed))) => self.toast(Toast::info(tr!(
                "Terminal : {choice} n'a pas pu être lancé, un autre terminal est utilisé à la place.",
                choice = choice.label()
            ))),
            Err(e) => self.toast(Toast::error(tr!("Aucun terminal n'a pu être lancé"), e)),
        }
    }
}

/// Options de lancement (ligne de commande).
#[derive(Debug, Default, Clone)]
pub struct StartOptions {
    pub open: Vec<PathBuf>,
    pub screenshot: Option<PathBuf>,
    pub screenshot_after: Duration,
    pub theme: Option<ThemeChoice>,
    pub lang: Option<Lang>,
    pub view: Option<String>,
    pub profile_dir: Option<PathBuf>,
    pub add_root: Option<PathBuf>,
    pub select_commit: Option<usize>,
    pub window: Option<String>,
    /// Dossier `.ssh` de la fenêtre « Clés SSH » : réservé aux tests (et à `--profile`) ; `~/.ssh` par défaut.
    pub ssh_dir: Option<PathBuf>,
    /// Lecture des pull requests d'un compte (fenêtre « Pull requests ») : réservée aux tests, qui répondent sans
    /// réseau ni Gestionnaire d'identifiants ; `pull_requests::fetch_from_forge` par défaut.
    pub pull_request_source: Option<PullRequestSource>,
    /// Intervalle du fetch automatique, à la place de celui des réglages : réservé aux tests (`Duration::ZERO` : dès
    /// l'ouverture du dépôt).
    pub auto_fetch_every: Option<Duration>,
}

pub enum TabKind {
    Welcome,
    Repo(Box<RepoTab>),
}

pub struct Tab {
    pub id: TabId,
    pub kind: TabKind,
}

pub struct DragonTreeApp {
    shared: Shared,
    rx: Receiver<AppEvent>,
    tabs: Vec<Tab>,
    active: usize,
    next_tab_id: TabId,
    repo_list: RepoListState,
    accounts: AccountsWindow,
    ssh_keys: SshKeysWindow,
    git_identity: crate::git_identity::GitIdentityWindow,
    shortcuts_window: crate::help::ShortcutsWindow,
    about_window: crate::help::AboutWindow,
    clone: Option<CloneWindow>,
    pull_requests: PullRequestsWindow,
    settings_window: Option<SettingsWindow>,
    log_open: bool,
    quick_open: Option<crate::quick_open::QuickOpen>,
    start: StartOptions,
    started_at: Instant,
    screenshot_requested: bool,
    window_focused: bool,
    last_sweep: Instant,
    applied_theme: (ThemeChoice, f32),
    applied_diff_font: crate::diff_style::DiffFont,
    /// Dernier changement de police du diff demandé : un chargement plus ancien qui finit après lui est ignoré.
    diff_font_generation: std::sync::Arc<std::sync::Mutex<u64>>,
    /// Langue de Windows, lue au démarrage (réglage « Système »).
    system_lang: Lang,
    window_title: String,
    /// Onglets de dépôt fermés pendant la session (Ctrl+Maj+T).
    closed_tabs: ClosedTabs,
    workspace_dialog: crate::workspace_dialog::WorkspaceDialog,
    /// Fermeture pendant des opérations (RV4a, voir `closing`).
    closing: crate::closing::Closing,
    /// Première instance du profil (RV4b, posée par `main`) : reçoit les dépôts des instances lancées ensuite.
    instance: Option<crate::single_instance::PrimaryInstance>,
}

impl DragonTreeApp {
    pub fn new(cc: &eframe::CreationContext<'_>, start: StartOptions) -> DragonTreeApp {
        let ctx = cc.egui_ctx.clone();
        let settings_path = match &start.profile_dir {
            Some(dir) => dir.join("settings.json"),
            None => Settings::file(),
        };
        let (settings_store, mut settings, settings_error) = SettingsFile::load(&settings_path);
        if let Some(t) = start.theme {
            settings.theme = t;
        }
        if let Some(l) = start.lang {
            settings.language = Some(l);
        }
        let system_lang = Lang::system();
        dragontree_i18n::set_lang(settings.language.unwrap_or(system_lang));
        if let Some(root) = &start.add_root
            && !settings.scan_roots.iter().any(|r| path_key(r) == path_key(root))
        {
            settings.scan_roots.push(root.clone());
        }
        theme::install_fonts(&ctx, settings.diff_font);
        theme::apply(&ctx, settings.theme, settings.ui_scale);

        let git = match &settings.git_path {
            Some(p) => Some(Git::with_exe(p.clone())).filter(|g| g.version().is_ok()).or_else(|| Git::discover().ok()),
            None => Git::discover().ok(),
        };
        let repo_list = RepoListState::load_cache(start.profile_dir.as_deref());
        let applied_theme = (settings.theme, settings.ui_scale);
        let applied_diff_font = settings.diff_font;
        let (mut shared, rx) = Shared::new(ctx.clone(), settings, settings_store, start.profile_dir.clone(), git);
        shared.auto_fetch_every = start.auto_fetch_every;
        if let Some(msg) = settings_error {
            shared.toasts.push(Toast::error(tr!("Réglages réinitialisés"), msg));
        }
        // Détection de git lfs : une fois, hors du thread de l'interface.
        match shared.git.clone() {
            Some(git) => shared
                .jobs
                .spawn("lfs-detect", move || Some(AppEvent::LfsDetected(dragontree_git::lfs_available(&git)))),
            None => shared.lfs = Some(false),
        }
        // Détection des éditeurs externes (Program Files, Toolbox JetBrains, PATH) : une fois, hors du thread de l'interface.
        shared.jobs.spawn("editors-detect", || Some(AppEvent::EditorsDetected(crate::editors::detect_system())));
        shared.jobs.spawn("tools-detect", || Some(AppEvent::ToolsDetected(crate::external_tools::detect_system())));
        if shared.git.is_none() {
            shared.toasts.push(Toast::error(
                tr!("git est introuvable"),
                tr!("Installez Git pour Windows (https://git-scm.com) ou indiquez le chemin de git.exe dans les réglages."),
            ));
        }

        let accounts = AccountsWindow::default();
        let ssh_dir = start
            .ssh_dir
            .clone()
            .or_else(|| dirs::home_dir().map(|h| h.join(".ssh")))
            .unwrap_or_else(|| PathBuf::from(".ssh"));
        let mut app = DragonTreeApp {
            shared,
            rx,
            tabs: Vec::new(),
            active: 0,
            next_tab_id: 1,
            repo_list,
            accounts,
            ssh_keys: SshKeysWindow::new(ssh_dir),
            git_identity: Default::default(),
            shortcuts_window: Default::default(),
            about_window: Default::default(),
            clone: None,
            pull_requests: PullRequestsWindow::new(
                start.pull_request_source.unwrap_or(crate::pull_requests::fetch_from_forge),
            ),
            settings_window: None,
            log_open: false,
            quick_open: None,
            start,
            started_at: Instant::now(),
            screenshot_requested: false,
            window_focused: true,
            last_sweep: Instant::now(),
            applied_theme,
            applied_diff_font,
            diff_font_generation: Default::default(),
            system_lang,
            window_title: String::new(),
            closed_tabs: ClosedTabs::default(),
            workspace_dialog: Default::default(),
            closing: Default::default(),
            instance: None,
        };

        // Onglets : ceux de la ligne de commande, sinon ceux de la dernière session.
        let to_open: Vec<PathBuf> =
            if app.start.open.is_empty() { app.shared.settings.open_tabs.clone() } else { app.start.open.clone() };
        let active =
            if app.start.open.is_empty() { app.shared.settings.active_tab } else { to_open.len().saturating_sub(1) };
        for p in to_open {
            if p.is_dir() {
                app.open_repo(&p, true);
            }
        }
        if app.tabs.is_empty() {
            app.new_welcome_tab();
        }
        app.active = active.min(app.tabs.len() - 1);
        if let Some(view) = app.start.view.clone() {
            for tab in &mut app.tabs {
                if let TabKind::Repo(r) = &mut tab.kind {
                    r.set_view_by_name(&view);
                    r.select_commit_on_load = app.start.select_commit;
                }
            }
        }
        match app.start.window.as_deref() {
            Some("accounts") => app.accounts.open = true,
            Some("ssh") => app.ssh_keys.show(),
            Some("settings") => app.settings_window = Some(SettingsWindow::new(&app.shared.settings)),
            Some("clone") => app.clone = Some(CloneWindow::new(&app.shared)),
            Some("pullrequests") => app.pull_requests.show(),
            Some("log") => app.log_open = true,
            Some("quick") => app.quick_open = Some(crate::quick_open::QuickOpen::default()),
            _ => {}
        }
        app.repo_list.start_scan(&app.shared);
        app
    }

    /// Instance unique du profil (RV4b) : gardée jusqu'à la fin de l'application (son mutex désigne la première
    /// instance) ; ses messages sont lus à chaque image. Posée par `main`, jamais par `new` : les tests d'interface
    /// créent plusieurs applications sur leurs profils.
    pub fn set_instance(&mut self, instance: crate::single_instance::PrimaryInstance) {
        instance.wake(self.shared.ctx.clone());
        self.instance = Some(instance);
    }

    /// Dépôts transmis par les instances lancées ensuite sur le même profil : ouverts dans des onglets (ou l'onglet
    /// existant activé), et la fenêtre passe au premier plan, rendue si elle était réduite. Appelée par `logic`, qui
    /// tourne aussi fenêtre réduite.
    fn receive_forwarded(&mut self, ctx: &egui::Context) {
        let Some(instance) = &self.instance else { return };
        let messages = instance.take_messages();
        if messages.is_empty() {
            return;
        }
        let paths = messages.into_iter().flat_map(|m| m.open);
        self.shared.actions.extend(paths.map(AppAction::OpenRepo));
        if ctx.input(|i| i.viewport().minimized == Some(true)) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    }

    fn alloc_tab_id(&mut self) -> TabId {
        self.next_tab_id += 1;
        self.next_tab_id
    }

    fn new_welcome_tab(&mut self) {
        let id = self.alloc_tab_id();
        self.tabs.push(Tab { id, kind: TabKind::Welcome });
        self.active = self.tabs.len() - 1;
    }

    /// Ouvre un dépôt : active l'onglet existant, remplace l'onglet d'accueil actif, ou en crée un.
    fn open_repo(&mut self, path: &Path, force_new: bool) {
        let key = path_key(path);
        if !force_new
            && let Some(i) =
                self.tabs.iter().position(|t| matches!(&t.kind, TabKind::Repo(r) if path_key(&r.path) == key))
        {
            self.active = i;
            return;
        }
        let id = self.alloc_tab_id();
        let tab = RepoTab::new(id, path.to_path_buf(), &mut self.shared);
        let replace_welcome = matches!(self.tabs.get(self.active).map(|t| &t.kind), Some(TabKind::Welcome));
        if replace_welcome && !force_new {
            self.tabs[self.active] = Tab { id, kind: TabKind::Repo(Box::new(tab)) };
        } else {
            self.tabs.push(Tab { id, kind: TabKind::Repo(Box::new(tab)) });
            self.active = self.tabs.len() - 1;
        }
        self.shared.settings.push_recent(path);
        self.remember_tabs();
    }

    fn close_tab(&mut self, index: usize) {
        if index >= self.tabs.len() {
            return;
        }
        self.remember_closed(index);
        self.tabs.remove(index);
        if self.tabs.is_empty() {
            self.new_welcome_tab();
        }
        if self.active > index || self.active >= self.tabs.len() {
            self.active = self.active.saturating_sub(1).min(self.tabs.len() - 1);
        }
        self.remember_tabs();
    }

    /// Ferme les onglets ouverts sur `path` sans les retenir pour Ctrl+Maj+T : leur dossier va être supprimé.
    fn close_tabs_of(&mut self, path: &Path) {
        let key = path_key(path);
        let before = self.tabs.len();
        while let Some(index) =
            self.tabs.iter().position(|t| matches!(&t.kind, TabKind::Repo(r) if path_key(&r.path) == key))
        {
            self.tabs.remove(index);
            if self.active > index {
                self.active -= 1;
            }
        }
        if self.tabs.len() == before {
            return;
        }
        if self.tabs.is_empty() {
            self.new_welcome_tab();
        }
        self.active = self.active.min(self.tabs.len() - 1);
        self.remember_tabs();
    }

    /// Empile l'onglet `index` (s'il s'agit d'un dépôt) avant sa fermeture ; l'onglet d'accueil n'est pas retenu.
    fn remember_closed(&mut self, index: usize) {
        if let Some(TabKind::Repo(r)) = self.tabs.get(index).map(|t| &t.kind) {
            self.closed_tabs.push(r.path.clone(), index);
        }
    }

    /// Empile en bloc tous les onglets de dépôt sauf `keep` (« Fermer les autres », « Tout fermer »).
    fn remember_closed_all(&mut self, keep: Option<usize>) {
        let closing =
            self.tabs.iter().enumerate().filter(|(i, _)| Some(*i) != keep).filter_map(|(i, t)| match &t.kind {
                TabKind::Repo(r) => Some((r.path.clone(), i)),
                TabKind::Welcome => None,
            });
        self.closed_tabs.push_all(closing);
    }

    /// Rouvre le dernier onglet fermé, à sa position d'origine si possible, sinon à la fin, et l'active.
    fn reopen_closed_tab(&mut self) {
        let Some(closed) = self.closed_tabs.pop() else { return };
        let key = path_key(&closed.path);
        if let Some(i) = self.tabs.iter().position(|t| matches!(&t.kind, TabKind::Repo(r) if path_key(&r.path) == key))
        {
            self.active = i;
            self.remember_tabs();
            return;
        }
        // Un onglet d'accueil seul (barre vide après une fermeture) est remplacé plutôt que gardé à côté.
        let only_welcome = matches!(self.tabs.as_slice(), [Tab { kind: TabKind::Welcome, .. }]);
        self.open_repo(&closed.path, !only_welcome);
        if !only_welcome {
            let tab = self.tabs.pop().expect("l'onglet vient d'être ajouté");
            let at = closed.index.min(self.tabs.len());
            self.tabs.insert(at, tab);
            self.active = at;
            self.remember_tabs();
        }
    }

    fn remember_tabs(&mut self) {
        self.shared.settings.open_tabs = self
            .tabs
            .iter()
            .filter_map(|t| match &t.kind {
                TabKind::Repo(r) => Some(r.path.clone()),
                TabKind::Welcome => None,
            })
            .collect();
        let repos_before =
            self.tabs[..self.active.min(self.tabs.len())].iter().filter(|t| matches!(t.kind, TabKind::Repo(_))).count();
        self.shared.settings.active_tab = repos_before;
        self.shared.settings_dirty = true;
    }

    /// Passe à l'espace de travail `name` : les onglets de l'espace quitté sont rangés dans les réglages, ceux de
    /// l'espace choisi sont rouverts.
    fn switch_workspace(&mut self, name: &str) {
        // L'onglet actif change sans passer par les réglages (clic, Ctrl+Tab, Ctrl+1…9) : on le relit avant de ranger.
        self.remember_tabs();
        if self.shared.settings.switch_workspace(name) {
            self.load_workspace_tabs();
        }
    }

    /// Remplace les onglets par ceux de `Settings::open_tabs` : un dépôt déjà ouvert garde son onglet (et son état),
    /// les autres se ferment ; la barre n'est jamais vide.
    fn load_workspace_tabs(&mut self) {
        let wanted = self.shared.settings.open_tabs.clone();
        let active = self.shared.settings.active_tab;
        let mut old = std::mem::take(&mut self.tabs);
        for path in wanted.into_iter().filter(|p| p.is_dir()) {
            let key = path_key(&path);
            if let Some(i) = old.iter().position(|t| matches!(&t.kind, TabKind::Repo(r) if path_key(&r.path) == key)) {
                self.tabs.push(old.remove(i));
            } else {
                let id = self.alloc_tab_id();
                let tab = RepoTab::new(id, path, &mut self.shared);
                self.tabs.push(Tab { id, kind: TabKind::Repo(Box::new(tab)) });
            }
        }
        if self.tabs.is_empty() {
            self.new_welcome_tab();
        }
        self.active = active.min(self.tabs.len() - 1);
        self.remember_tabs();
    }

    fn apply_workspace_action(&mut self, action: WorkspaceAction) {
        let settings = &mut self.shared.settings;
        match action {
            WorkspaceAction::Create(name) => {
                if let Ok(name) = settings.create_workspace(&name) {
                    self.switch_workspace(&name);
                    self.shared.settings_dirty = true;
                }
            }
            WorkspaceAction::Rename { old, new } => {
                if settings.rename_workspace(&old, &new).is_ok() {
                    self.shared.settings_dirty = true;
                }
            }
            WorkspaceAction::Delete(name) => {
                let was_active = settings.current_workspace_name() == name;
                if settings.delete_workspace(&name) {
                    if was_active {
                        self.load_workspace_tabs();
                    }
                    self.shared.settings_dirty = true;
                }
            }
        }
    }

    fn pick_folder(title: &str) -> Option<PathBuf> {
        rfd::FileDialog::new().set_title(title).pick_folder()
    }

    fn process_events(&mut self) {
        while let Ok(ev) = self.rx.try_recv() {
            match ev {
                AppEvent::Repo(id, ev) => {
                    if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == id)
                        && let TabKind::Repo(r) = &mut tab.kind
                    {
                        r.handle_event(ev, &mut self.shared);
                        if let Some((status, head)) = r.quick_status() {
                            self.repo_list.update_status(&r.path, status, head);
                        }
                    }
                }
                AppEvent::Scan(ev) => {
                    let finished = matches!(ev, ScanEvent::Finished(_));
                    self.repo_list.handle(ev, &mut self.shared);
                    // Nouvelle liste des dépôts : la fenêtre des pull requests relit leurs remotes.
                    if finished {
                        self.pull_requests.index_local_clones(&self.shared, &self.repo_list.repos);
                    }
                }
                AppEvent::PullRequests(ev) => self.pull_requests.handle(ev, &self.shared.settings.accounts),
                AppEvent::Forge(ev) => self.accounts.handle(ev, &mut self.shared),
                AppEvent::Ssh(ev) => self.ssh_keys.handle(ev, &mut self.shared),
                AppEvent::GitIdentity(ev) => self.git_identity.handle(ev, &mut self.shared),
                AppEvent::Clone(CloneEvent::Done(done)) => {
                    crate::clone::finish_clone(&mut self.shared, &done, self.clone.is_some());
                    if let Some(c) = &mut self.clone {
                        c.on_done(&done);
                    }
                }
                AppEvent::Clone(ev) => {
                    if let Some(c) = &mut self.clone {
                        c.handle(ev);
                    }
                }
                AppEvent::Trusted(path, result) => match result {
                    Ok(()) => {
                        self.shared.toast(Toast::success(tr!("{path} est maintenant approuvé", path = path.display())));
                        self.repo_list.start_sweep(&self.shared);
                        for tab in &mut self.tabs {
                            if let TabKind::Repo(r) = &mut tab.kind
                                && path_key(&r.path) == path_key(&path)
                            {
                                r.reopen(&mut self.shared);
                            }
                        }
                    }
                    Err(e) => {
                        self.shared.toast(Toast::error(tr!("safe.directory n'a pas pu être ajouté"), e.to_string()))
                    }
                },
                AppEvent::LfsDetected(available) => self.shared.lfs = Some(available),
                AppEvent::EditorsDetected(editors) => self.shared.editors = editors,
                AppEvent::ToolsDetected(tools) => self.shared.tools = tools,
                AppEvent::AiChecked(result) => self.shared.ai_check = AiCheck::Done(result),
                AppEvent::GitVersion(req, version) => self.about_window.handle(req, version),
                AppEvent::Initialized(path, result) => match result {
                    Ok(()) => self.shared.actions.push(AppAction::OpenRepo(path)),
                    Err(e) => self.shared.toast(Toast::error(tr!("Création du dépôt impossible"), e.to_string())),
                },
            }
        }
    }

    fn process_actions(&mut self, ctx: &egui::Context) {
        // Une commande peut demander une autre action (Réglages…) : traitée dans la même image.
        while !self.shared.actions.is_empty() {
            let actions = std::mem::take(&mut self.shared.actions);
            for action in actions {
                self.apply_action(action, ctx);
            }
        }
        if self.shared.settings_dirty {
            self.shared.save_settings();
        }
        let wanted = (self.shared.settings.theme, self.shared.settings.ui_scale);
        if wanted != self.applied_theme {
            theme::apply(ctx, wanted.0, wanted.1);
            self.applied_theme = wanted;
        }
        // Nouvelle police du diff : relecture des fichiers de police hors du thread de l'interface.
        if self.shared.settings.diff_font != self.applied_diff_font {
            self.applied_diff_font = self.shared.settings.diff_font;
            let generation = {
                let mut latest = self.diff_font_generation.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                *latest += 1;
                *latest
            };
            let (font, ctx, latest) = (self.applied_diff_font, ctx.clone(), self.diff_font_generation.clone());
            std::thread::spawn(move || {
                theme::install_fonts_if_latest(&ctx, font, &latest, generation);
                ctx.request_repaint();
            });
        }
        let lang = self.shared.settings.language.unwrap_or(self.system_lang);
        if lang != dragontree_i18n::lang() {
            dragontree_i18n::set_lang(lang);
            ctx.request_repaint();
        }
    }

    fn apply_action(&mut self, action: AppAction, ctx: &egui::Context) {
        match action {
            AppAction::OpenRepo(p) => self.open_repo(&p, false),
            AppAction::OpenRepoNewTab(p) => self.open_repo(&p, true),
            AppAction::PickAndOpenRepo => {
                if let Some(p) = Self::pick_folder(tr!("Ouvrir un dépôt git")) {
                    self.open_repo(&p, false);
                }
            }
            AppAction::InitRepo => {
                if let (Some(p), Some(git)) =
                    (Self::pick_folder(tr!("Dossier du nouveau dépôt")), self.shared.git.clone())
                {
                    self.shared.jobs.spawn("init", move || {
                        let r = dragontree_git::init_repo(&git, &p);
                        Some(AppEvent::Initialized(p, r))
                    });
                }
            }
            AppAction::ShowClone => {
                if self.clone.is_none() {
                    self.clone = Some(CloneWindow::new(&self.shared));
                }
            }
            AppAction::ShowAccounts => self.accounts.open = true,
            AppAction::ShowPullRequests => self.pull_requests.show(),
            AppAction::ShowSshKeys => self.ssh_keys.show(),
            AppAction::ShowGitIdentity => self.git_identity.show(&self.shared),
            AppAction::ShowSettings => {
                if self.settings_window.is_none() {
                    self.settings_window = Some(SettingsWindow::new(&self.shared.settings));
                }
            }
            AppAction::ShowSettingsTab(tab) => {
                self.settings_window.get_or_insert_with(|| SettingsWindow::new(&self.shared.settings)).show_tab(tab)
            }
            AppAction::ShowLog => self.log_open = true,
            AppAction::AddScanRoot => {
                if let Some(p) = Self::pick_folder(tr!("Dossier contenant vos dépôts (scan récursif)")) {
                    if !self.shared.settings.scan_roots.iter().any(|r| path_key(r) == path_key(&p)) {
                        self.shared.settings.scan_roots.push(p);
                        self.shared.settings_dirty = true;
                    }
                    self.repo_list.start_scan(&self.shared);
                }
            }
            AppAction::RemoveScanRoot(p) => {
                self.shared.settings.scan_roots.retain(|r| path_key(r) != path_key(&p));
                self.shared.settings_dirty = true;
                self.repo_list.start_scan(&self.shared);
            }
            AppAction::Rescan => {
                self.repo_list.start_scan(&self.shared);
            }
            AppAction::TrustRepo(p) => {
                if let Some(git) = self.shared.git.clone() {
                    self.shared.jobs.spawn("trust", move || {
                        let r = dragontree_git::trust_directory(&git, &p);
                        Some(AppEvent::Trusted(p, r))
                    });
                }
            }
            AppAction::QuickOpen => {
                self.quick_open = Some(crate::quick_open::QuickOpen::default());
            }
            AppAction::RemoveWorktree { tab, path, force } => {
                // Fermés avant que la suppression ne parte : aucun onglet ne reste sur un dossier supprimé.
                self.close_tabs_of(&path);
                if let Some(TabKind::Repo(r)) = self.tabs.iter_mut().find(|t| t.id == tab).map(|t| &mut t.kind) {
                    r.op_remove_worktree(&self.shared, path, force);
                }
            }
            AppAction::RunCommand(cmd) => self.run_command(cmd),
            AppAction::Quit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
        }
    }

    /// État lu par le registre de commandes pour griser ou cocher les entrées : seul point qui le construit
    /// (barre de menus, raccourcis, palette).
    fn command_context(&self) -> CommandContext<'_> {
        let repo = match self.tabs.get(self.active).map(|t| &t.kind) {
            Some(TabKind::Repo(r)) if r.repo.is_some() => Some(r.as_ref()),
            _ => None,
        };
        CommandContext {
            settings: &self.shared.settings,
            repo,
            can_reopen_tab: !self.closed_tabs.is_empty(),
            lfs_available: self.shared.lfs,
            editors: &self.shared.editors,
        }
    }

    /// Commandes proposées par la palette Ctrl+P, avec leur libellé : celles du registre utilisables en ce moment,
    /// puis les commandes personnalisées du dépôt sous leur nom, les éditeurs et les espaces de travail.
    fn palette_commands(&self) -> Vec<(Command, String)> {
        let cx = self.command_context();
        let registry = Command::ALL.iter().copied().filter(|c| c.in_palette() && c.enabled(&cx));
        let mut list: Vec<(Command, String)> = registry.map(|c| (c, c.palette_label().to_string())).collect();
        let custom = commands::custom_entries(cx.settings).into_iter().filter(|(c, _)| c.enabled(&cx));
        list.extend(custom.map(|(c, name)| (c, tr!("Commande personnalisée : {name}", name))));
        let editors = cx.editors.iter().enumerate().map(|(i, e)| (Command::OpenIn(i), e.kind.label()));
        list.extend(editors.filter(|(c, _)| c.enabled(&cx)).map(|(c, name)| (c, tr!("Ouvrir dans {name}", name))));
        // Un espace de travail par entrée (hors l'actif), sous son nom.
        let current = cx.settings.current_workspace_name();
        let others = cx.settings.workspace_names().into_iter().enumerate().filter(|(_, name)| name != current);
        list.extend(others.map(|(i, name)| {
            (Command::Workspace(i), tr!("Espace de travail : {name}", name = Settings::workspace_label(&name)))
        }));
        list
    }

    /// Exécute une commande du registre (barre de menus, raccourci, palette) si elle est utilisable ; une
    /// commande de dépôt va à l'onglet actif.
    fn run_command(&mut self, cmd: Command) {
        if !cmd.enabled(&self.command_context()) {
            return;
        }
        match cmd.scope() {
            Scope::App => self.run_app_command(cmd),
            // `enabled` a vérifié que l'onglet actif est un dépôt ouvert.
            Scope::Repo => {
                if let Some(TabKind::Repo(r)) = self.tabs.get_mut(self.active).map(|t| &mut t.kind) {
                    r.run_command(cmd, &mut self.shared);
                }
            }
        }
    }

    /// Relit le journal de chaque dépôt ouvert : un réglage global change ce que le graphe parcourt.
    fn reload_logs(&mut self) {
        for tab in &mut self.tabs {
            if let TabKind::Repo(r) = &mut tab.kind {
                r.load_log(&self.shared);
            }
        }
    }

    /// Commande de portée App ; une commande sans traitement ici est un oubli : assertion en debug.
    fn run_app_command(&mut self, cmd: Command) {
        match cmd {
            Command::NewTab => self.new_welcome_tab(),
            Command::CloseTab => self.close_tab(self.active),
            Command::ReopenClosedTab => self.reopen_closed_tab(),
            Command::Workspace(i) => {
                if let Some(name) = self.shared.settings.workspace_names().get(i).cloned() {
                    self.switch_workspace(&name);
                }
            }
            Command::NewWorkspace => self.workspace_dialog.show_new(),
            Command::RenameWorkspace => {
                self.workspace_dialog.show_rename(self.shared.settings.current_workspace_name())
            }
            Command::DeleteWorkspace => {
                self.workspace_dialog.show_delete(self.shared.settings.current_workspace_name())
            }
            Command::OpenRepo => self.shared.actions.push(AppAction::PickAndOpenRepo),
            Command::Clone => self.shared.actions.push(AppAction::ShowClone),
            Command::InitRepo => self.shared.actions.push(AppAction::InitRepo),
            Command::QuickOpen => self.shared.actions.push(AppAction::QuickOpen),
            Command::Accounts => self.shared.actions.push(AppAction::ShowAccounts),
            Command::PullRequests => self.shared.actions.push(AppAction::ShowPullRequests),
            Command::SshKeys => self.shared.actions.push(AppAction::ShowSshKeys),
            Command::GitIdentity => self.shared.actions.push(AppAction::ShowGitIdentity),
            Command::KeyboardShortcuts => self.shortcuts_window.show(),
            Command::About => self.about_window.show(&self.shared),
            Command::Settings => self.shared.actions.push(AppAction::ShowSettings),
            Command::ManageCustomCommands => {
                self.shared.actions.push(AppAction::ShowSettingsTab(SettingsTab::Commands))
            }
            Command::ShowLog => self.shared.actions.push(AppAction::ShowLog),
            Command::Quit => self.shared.actions.push(AppAction::Quit),
            Command::ToggleDetailsPanel => {
                self.shared.settings.details_panel_visible = !self.shared.settings.details_panel_visible;
                self.shared.settings_dirty = true;
            }
            Command::Layout(layout) => {
                self.shared.settings.history_layout = layout;
                self.shared.settings_dirty = true;
            }
            Command::SortCommits(topological) => {
                if self.shared.settings.topological_order != topological {
                    self.shared.settings.topological_order = topological;
                    self.shared.settings_dirty = true;
                    self.reload_logs();
                }
            }
            Command::ShowTags => {
                self.shared.settings.show_tags = !self.shared.settings.show_tags;
                self.shared.settings_dirty = true;
                self.reload_logs();
            }
            Command::ShowRemoteBranches => {
                self.shared.settings.show_remote_branches = !self.shared.settings.show_remote_branches;
                self.shared.settings_dirty = true;
                self.reload_logs();
            }
            Command::ShowStashes => {
                self.shared.settings.show_stashes = !self.shared.settings.show_stashes;
                self.shared.settings_dirty = true;
                self.reload_logs();
            }
            // Un nouveau statut : git relit (ou non) les fichiers ignorés, et la sélection se revalide
            // (un fichier non suivi masqué ne reste pas sélectionné).
            Command::ShowIgnoredFiles | Command::HideUntrackedFiles => {
                let settings = &mut self.shared.settings;
                let flag = if cmd == Command::ShowIgnoredFiles {
                    &mut settings.show_ignored_files
                } else {
                    &mut settings.hide_untracked_files
                };
                *flag = !*flag;
                self.shared.settings_dirty = true;
                for tab in &mut self.tabs {
                    if let TabKind::Repo(r) = &mut tab.kind {
                        r.refresh(&self.shared, crate::repo::Refresh::STATUS);
                    }
                }
            }
            Command::ToggleRepoList => {
                self.shared.settings.repo_list_visible = !self.shared.settings.repo_list_visible;
                self.shared.settings_dirty = true;
            }
            Command::Language(lang) => {
                self.shared.settings.language = lang;
                self.shared.settings_dirty = true;
            }
            Command::Theme(choice) => {
                self.shared.settings.theme = choice;
                self.shared.settings_dirty = true;
            }
            Command::ZoomIn | Command::ZoomOut | Command::ZoomReset => {
                let settings = &mut self.shared.settings;
                settings.ui_scale = match cmd {
                    Command::ZoomIn => crate::settings::step_ui_scale(settings.ui_scale, 1),
                    Command::ZoomOut => crate::settings::step_ui_scale(settings.ui_scale, -1),
                    _ => Settings::default().ui_scale,
                };
                self.shared.settings_dirty = true;
            }
            _ => debug_assert!(false, "{cmd:?} : commande App sans traitement dans DragonTreeApp::run_app_command"),
        }
    }

    /// Fichiers déposés sur la fenêtre : un `.patch` ou `.diff` ouvre l'aperçu d'application dans l'onglet actif.
    fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).collect());
        let Some(path) = dropped.iter().find(|p| crate::repo::is_patch_file(p)).cloned() else {
            if !dropped.is_empty() {
                self.shared.toast(Toast::info(tr!("Déposez un fichier .patch ou .diff pour l'appliquer")));
            }
            return;
        };
        match self.tabs.get_mut(self.active).map(|t| &mut t.kind) {
            Some(TabKind::Repo(r)) if r.can_run(Command::ApplyPatch) => r.open_patch_file(&self.shared, path),
            _ => self
                .shared
                .toast(Toast::info(tr!("Ouvrez un dépôt (et fermez le dialogue en cours) pour appliquer un patch"))),
        }
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        // Raccourcis du registre, les plus spécifiques d'abord. Celui d'une commande grisée est consommé quand
        // même : sinon un raccourci moins spécifique (Ctrl+T pour Ctrl+Maj+T) prendrait l'appui.
        for (shortcut, cmd) in commands::shortcuts() {
            if ctx.input_mut(|i| i.consume_shortcut(&shortcut)) {
                self.run_command(cmd);
            }
        }
        // Changement d'onglet, hors registre.
        let sc = |m: Modifiers, k: Key| KeyboardShortcut::new(m, k);
        let consume = |s: KeyboardShortcut| ctx.input_mut(|i| i.consume_shortcut(&s));
        if consume(sc(Modifiers::COMMAND | Modifiers::SHIFT, Key::Tab)) {
            self.active = (self.active + self.tabs.len() - 1) % self.tabs.len();
        }
        if consume(sc(Modifiers::COMMAND, Key::Tab)) {
            self.active = (self.active + 1) % self.tabs.len();
        }
        let digits =
            [Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5, Key::Num6, Key::Num7, Key::Num8, Key::Num9];
        for (i, k) in digits.into_iter().enumerate() {
            if consume(sc(Modifiers::COMMAND, k)) && i < self.tabs.len() {
                self.active = if k == Key::Num9 { self.tabs.len() - 1 } else { i };
            }
        }
    }

    fn tab_infos(&self) -> Vec<TabInfo> {
        self.tabs
            .iter()
            .map(|t| match &t.kind {
                TabKind::Welcome => TabInfo {
                    id: t.id,
                    title: tr!("Nouvel onglet").into(),
                    tooltip: tr!("Ouvrir, cloner ou créer un dépôt").into(),
                    busy: false,
                    is_repo: false,
                    color: None,
                },
                TabKind::Repo(r) => TabInfo {
                    id: t.id,
                    title: r.title(),
                    // Chemin du dépôt, puis le compte qui authentifie fetch, pull et push s'il y en a un.
                    tooltip: match r.account(&self.shared.settings) {
                        Some(account) => {
                            format!("{}\n{}", r.path.display(), tr!("Compte : {title}", title = account.title()))
                        }
                        None => r.path.display().to_string(),
                    },
                    busy: r.is_busy(),
                    is_repo: true,
                    color: self.shared.settings.tab_color_of(&r.path),
                },
            })
            .collect()
    }

    /// Position actuelle de l'onglet `id` ; `None` s'il a été fermé entre-temps.
    fn tab_position(&self, id: TabId) -> Option<usize> {
        self.tabs.iter().position(|t| t.id == id)
    }

    /// Applique les actions de la barre d'onglets. Les onglets y sont désignés par identifiant : la position est
    /// recherchée à l'application, car un onglet fermé plus tôt (ou dans la même image) la décale.
    fn apply_tab_actions(&mut self, actions: Vec<TabBarAction>, ctx: &egui::Context) {
        for a in actions {
            match a {
                TabBarAction::Select(id) => {
                    if let Some(i) = self.tab_position(id) {
                        self.active = i;
                    }
                }
                TabBarAction::Close(id) => {
                    if let Some(i) = self.tab_position(id) {
                        self.close_tab(i);
                    }
                }
                TabBarAction::CloseOthers(id) => {
                    let Some(i) = self.tab_position(id) else { continue };
                    self.remember_closed_all(Some(i));
                    let keep = self.tabs.remove(i);
                    self.tabs = vec![keep];
                    self.active = 0;
                    self.remember_tabs();
                }
                TabBarAction::CloseAll => {
                    self.remember_closed_all(None);
                    self.tabs.clear();
                    self.new_welcome_tab();
                    self.remember_tabs();
                }
                TabBarAction::New => self.new_welcome_tab(),
                TabBarAction::SetColor(id, color) => {
                    if let Some(TabKind::Repo(r)) = self.tabs.iter().find(|t| t.id == id).map(|t| &t.kind) {
                        self.shared.settings.set_tab_color(&r.path.clone(), color);
                        self.shared.settings_dirty = true;
                    }
                }
                TabBarAction::Move(id, with) => {
                    if let (Some(from), Some(to)) = (self.tab_position(id), self.tab_position(with)) {
                        self.tabs.swap(from, to);
                        if self.active == from {
                            self.active = to;
                        } else if self.active == to {
                            self.active = from;
                        }
                        self.remember_tabs();
                    }
                }
                TabBarAction::ToggleRepoList => {
                    self.shared.settings.repo_list_visible = !self.shared.settings.repo_list_visible;
                    self.shared.settings_dirty = true;
                }
                TabBarAction::OpenExplorer(id) => {
                    if let Some(TabKind::Repo(r)) = self.tabs.iter().find(|t| t.id == id).map(|t| &t.kind) {
                        crate::util::open_in_explorer(&r.path);
                    }
                }
                TabBarAction::CopyPath(id) => {
                    if let Some(TabKind::Repo(r)) = self.tabs.iter().find(|t| t.id == id).map(|t| &t.kind) {
                        ctx.copy_text(r.path.display().to_string());
                    }
                }
            }
        }
    }

    fn maybe_screenshot(&mut self, ctx: &egui::Context) {
        let Some(path) = self.start.screenshot.clone() else { return };
        if self.screenshot_requested {
            return;
        }
        if self.started_at.elapsed() < self.start.screenshot_after {
            ctx.request_repaint_after(Duration::from_millis(100));
            return;
        }
        self.screenshot_requested = true;
        let ctx2 = ctx.clone();
        ctx.request_screenshot(move |image| {
            let [w, h] = image.size;
            let mut bytes = Vec::with_capacity(w * h * 4);
            for c in &image.pixels {
                bytes.extend_from_slice(&c.to_srgba_unmultiplied());
            }
            if let Some(img) = image::RgbaImage::from_raw(w as u32, h as u32, bytes) {
                let _ = img.save(&path);
            }
            ctx2.send_viewport_cmd(egui::ViewportCommand::Close);
            ctx2.request_repaint();
        });
    }
}

impl eframe::App for DragonTreeApp {
    /// Appelée à chaque image, avant `ui`, et seule quand la fenêtre est réduite.
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let ops = Operations::new(&self.shared.queues, self.clone.as_ref());
        self.closing.intercept(ctx, &ops, self.screenshot_requested);
        self.receive_forwarded(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.process_events();
        self.handle_shortcuts(&ctx);
        self.handle_dropped_files(&ctx);
        let p = Palette::current(&ctx);

        // Retour au premier plan : rafraîchir l'onglet actif.
        let focused = ctx.input(|i| i.viewport().focused.unwrap_or(true));
        if focused && !self.window_focused {
            if let Some(TabKind::Repo(r)) = self.tabs.get_mut(self.active).map(|t| &mut t.kind) {
                r.on_focus_regained(&mut self.shared);
            }
            if self.last_sweep.elapsed() > Duration::from_secs(120) {
                self.last_sweep = Instant::now();
                self.repo_list.start_sweep(&self.shared);
            }
        }
        self.window_focused = focused;

        let title = match self.tabs.get(self.active).map(|t| &t.kind) {
            Some(TabKind::Repo(r)) => format!("{} — DragonTree", r.title()),
            _ => "DragonTree".to_string(),
        };
        if title != self.window_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.window_title = title;
        }

        let chosen = egui::Panel::top("menu-bar")
            .frame(Frame::new().fill(p.sidebar).inner_margin(egui::Margin { left: 6, right: 6, top: 2, bottom: 0 }))
            .show(ui, |ui| crate::menu_bar::show(ui, &self.command_context()))
            .inner;
        if let Some(cmd) = chosen {
            self.run_command(cmd);
        }

        egui::Panel::top("tab-bar")
            .frame(Frame::new().fill(p.sidebar).inner_margin(egui::Margin { left: 6, right: 6, top: 4, bottom: 0 }))
            .show(ui, |ui| {
                let infos = self.tab_infos();
                let actions = tabbar::show(ui, &infos, self.active, self.shared.settings.repo_list_visible, &p);
                self.apply_tab_actions(actions, &ctx);
            });

        if self.shared.settings.repo_list_visible {
            let open_keys: HashSet<String> = self
                .tabs
                .iter()
                .filter_map(|t| match &t.kind {
                    TabKind::Repo(r) => Some(path_key(&r.path)),
                    TabKind::Welcome => None,
                })
                .collect();
            let resp = egui::Panel::right("repo-list")
                .resizable(true)
                .default_size(self.shared.settings.repo_list_width)
                .size_range(200.0..=520.0)
                .frame(Frame::new().fill(p.sidebar).inner_margin(egui::Margin::symmetric(8, 4)))
                .show(ui, |ui| self.repo_list.ui(ui, &mut self.shared, &open_keys, &p));
            let width = resp.response.rect.width();
            if (width - self.shared.settings.repo_list_width).abs() > 4.0 && !ctx.input(|i| i.pointer.any_down()) {
                self.shared.settings.repo_list_width = width;
                self.shared.settings_dirty = true;
            }
        }

        egui::CentralPanel::default().frame(Frame::new().fill(p.bg)).show(ui, |ui| {
            let active = self.active;
            // Fermeture décidée : plus de rafraîchissement ni de fetch automatique.
            if !self.closing.quitting() {
                for (i, tab) in self.tabs.iter_mut().enumerate() {
                    if let TabKind::Repo(r) = &mut tab.kind {
                        r.tick(&mut self.shared, i == active);
                    }
                }
            }
            match self.tabs.get_mut(active).map(|t| &mut t.kind) {
                Some(TabKind::Repo(r)) => r.ui(ui, &mut self.shared),
                Some(TabKind::Welcome) | None => {
                    if let Some(a) = welcome::show(ui, &self.shared.settings, &p) {
                        match a {
                            WelcomeAction::OpenFolder => self.shared.actions.push(AppAction::PickAndOpenRepo),
                            WelcomeAction::Clone => self.shared.actions.push(AppAction::ShowClone),
                            WelcomeAction::Init => self.shared.actions.push(AppAction::InitRepo),
                            WelcomeAction::AddScanRoot => self.shared.actions.push(AppAction::AddScanRoot),
                            WelcomeAction::OpenRecent(p) => self.shared.actions.push(AppAction::OpenRepo(p)),
                            WelcomeAction::Accounts => self.shared.actions.push(AppAction::ShowAccounts),
                        }
                    }
                }
            }
        });

        // Fenêtres globales.
        self.accounts.ui(&ctx, &mut self.shared, &self.repo_list.repos, &p);
        let active_repo = match self.tabs.get(self.active).map(|t| &t.kind) {
            Some(TabKind::Repo(r)) if r.repo.is_some() => Some(r.path.clone()),
            _ => None,
        };
        self.ssh_keys.ui(&ctx, &mut self.shared, active_repo.as_deref(), &p);
        self.git_identity.ui(&ctx, &mut self.shared, &p);
        if let Some(action) = self.workspace_dialog.ui(&ctx, &self.shared.settings, &p) {
            self.apply_workspace_action(action);
        }
        self.shortcuts_window.ui(&ctx, &p);
        self.about_window.ui(&ctx, &p);
        if let Some(c) = &mut self.clone
            && !c.ui(&ctx, &mut self.shared, &p)
        {
            self.clone = None;
        }
        self.pull_requests.ui(&ctx, &mut self.shared, &self.repo_list.repos, &p);
        if let Some(w) = &mut self.settings_window
            && !w.ui(&ctx, &mut self.shared, &p)
        {
            self.settings_window = None;
        }
        if self.log_open {
            let mut open = true;
            let mut reads = self.shared.settings.show_read_commands_in_log;
            self.shared.log.window(&ctx, &mut open, &mut reads, &p);
            if reads != self.shared.settings.show_read_commands_in_log {
                self.shared.settings.show_read_commands_in_log = reads;
                self.shared.settings_dirty = true;
            }
            self.log_open = open;
        }
        let palette_commands = if self.quick_open.is_some() { self.palette_commands() } else { Vec::new() };
        if let Some(q) = &mut self.quick_open {
            let mut tabs_repo: Vec<&mut RepoTab> = Vec::new();
            if let Some(TabKind::Repo(r)) = self.tabs.get_mut(self.active).map(|t| &mut t.kind) {
                tabs_repo.push(r);
            }
            let keep = q.ui(&ctx, &mut self.shared, &self.repo_list, tabs_repo.pop(), &palette_commands, &p);
            if !keep {
                self.quick_open = None;
            }
        }
        let ops = Operations::new(&self.shared.queues, self.clone.as_ref());
        self.closing.dialog(&ctx, &ops, &p);
        if let Some(action) = self.shared.toasts.show(&ctx, &p) {
            match action {
                crate::toasts::ToastAction::OpenGitIdentity => self.shared.actions.push(AppAction::ShowGitIdentity),
            }
        }

        self.process_actions(&ctx);
        self.maybe_screenshot(&ctx);
    }

    fn save(&mut self, _storage: &mut dyn eframe::Storage) {
        self.remember_tabs();
        self.shared.save_settings();
    }

    /// Fin de l'application, après `save` : les opérations qui tourneraient encore sont annulées (fermeture par une
    /// autre voie que la fenêtre), et les onglets ne font rien attendre en se fermant ; le processus se termine
    /// aussitôt après, et Windows tue avec lui ce qui resterait de git (Job Object).
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        Operations::new(&self.shared.queues, self.clone.as_ref()).stop_all();
        for tab in &mut self.tabs {
            if let TabKind::Repo(r) = &mut tab.kind {
                r.on_app_exit();
            }
        }
    }
}
