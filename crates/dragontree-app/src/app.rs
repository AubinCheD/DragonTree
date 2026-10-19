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
use crate::commands::{self, Command, CommandContext, Scope};
use crate::jobs::Jobs;
use crate::repo::{RepoEvent, RepoTab};
use crate::repo_list::{RepoListState, ScanEvent};
use crate::settings::{Settings, SettingsFile, ThemeChoice, path_key};
use crate::settings_ui::SettingsWindow;
use crate::tabbar::{self, TabBarAction, TabInfo};
use crate::theme::{self, Palette};
use crate::toasts::{CommandLog, Toast, Toasts};
use crate::welcome::{self, WelcomeAction};

pub type TabId = u64;

/// Événements envoyés par les tâches d'arrière-plan.
pub enum AppEvent {
    Repo(TabId, RepoEvent),
    Scan(ScanEvent),
    Forge(ForgeEvent),
    Clone(CloneEvent),
    Trusted(PathBuf, Result<(), GitError>),
    Initialized(PathBuf, Result<(), GitError>),
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
    ShowSettings,
    ShowLog,
    AddScanRoot,
    RemoveScanRoot(PathBuf),
    Rescan,
    TrustRepo(PathBuf),
    QuickOpen,
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
    clone: Option<CloneWindow>,
    settings_window: Option<SettingsWindow>,
    log_open: bool,
    quick_open: Option<crate::quick_open::QuickOpen>,
    start: StartOptions,
    started_at: Instant,
    screenshot_requested: bool,
    window_focused: bool,
    last_sweep: Instant,
    applied_theme: (ThemeChoice, f32),
    /// Langue de Windows, lue au démarrage (réglage « Système »).
    system_lang: Lang,
    window_title: String,
    /// Onglets de dépôt fermés pendant la session (Ctrl+Maj+T).
    closed_tabs: ClosedTabs,
}

impl DragonTreeApp {
    pub fn new(cc: &eframe::CreationContext<'_>, start: StartOptions) -> DragonTreeApp {
        let ctx = cc.egui_ctx.clone();
        theme::install_fonts(&ctx);
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
        theme::apply(&ctx, settings.theme, settings.ui_scale);

        let git = match &settings.git_path {
            Some(p) => Some(Git::with_exe(p.clone())).filter(|g| g.version().is_ok()).or_else(|| Git::discover().ok()),
            None => Git::discover().ok(),
        };
        let repo_list = RepoListState::load_cache(start.profile_dir.as_deref());
        let applied_theme = (settings.theme, settings.ui_scale);
        let (mut shared, rx) = Shared::new(ctx.clone(), settings, settings_store, start.profile_dir.clone(), git);
        if let Some(msg) = settings_error {
            shared.toasts.push(Toast::error(tr!("Réglages réinitialisés"), msg));
        }
        if shared.git.is_none() {
            shared.toasts.push(Toast::error(
                tr!("git est introuvable"),
                tr!("Installez Git pour Windows (https://git-scm.com) ou indiquez le chemin de git.exe dans les réglages."),
            ));
        }

        let accounts = AccountsWindow::default();
        let mut app = DragonTreeApp {
            shared,
            rx,
            tabs: Vec::new(),
            active: 0,
            next_tab_id: 1,
            repo_list,
            accounts,
            clone: None,
            settings_window: None,
            log_open: false,
            quick_open: None,
            start,
            started_at: Instant::now(),
            screenshot_requested: false,
            window_focused: true,
            last_sweep: Instant::now(),
            applied_theme,
            system_lang,
            window_title: String::new(),
            closed_tabs: ClosedTabs::default(),
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
            Some("settings") => app.settings_window = Some(SettingsWindow::new(&app.shared.settings)),
            Some("clone") => app.clone = Some(CloneWindow::new(&app.shared)),
            Some("log") => app.log_open = true,
            Some("quick") => app.quick_open = Some(crate::quick_open::QuickOpen::default()),
            _ => {}
        }
        app.repo_list.start_scan(&app.shared);
        app
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
                AppEvent::Scan(ev) => self.repo_list.handle(ev, &mut self.shared),
                AppEvent::Forge(ev) => self.accounts.handle(ev, &mut self.shared),
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
                self.apply_action(action);
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
        let lang = self.shared.settings.language.unwrap_or(self.system_lang);
        if lang != dragontree_i18n::lang() {
            dragontree_i18n::set_lang(lang);
            ctx.request_repaint();
        }
    }

    fn apply_action(&mut self, action: AppAction) {
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
            AppAction::ShowSettings => {
                if self.settings_window.is_none() {
                    self.settings_window = Some(SettingsWindow::new(&self.shared.settings));
                }
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
            AppAction::RunCommand(cmd) => self.run_command(cmd),
        }
    }

    /// État lu par le registre de commandes pour griser ou cocher les entrées : seul point qui le construit
    /// (barre de menus, raccourcis, palette).
    fn command_context(&self) -> CommandContext<'_> {
        let repo = match self.tabs.get(self.active).map(|t| &t.kind) {
            Some(TabKind::Repo(r)) if r.repo.is_some() => Some(r.as_ref()),
            _ => None,
        };
        CommandContext { settings: &self.shared.settings, repo, can_reopen_tab: !self.closed_tabs.is_empty() }
    }

    /// Commandes proposées par la palette Ctrl+P : celles du registre utilisables en ce moment.
    fn palette_commands(&self) -> Vec<Command> {
        let cx = self.command_context();
        Command::ALL.iter().copied().filter(|c| c.in_palette() && c.enabled(&cx)).collect()
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

    /// Commande de portée App ; une commande sans traitement ici est un oubli : assertion en debug.
    fn run_app_command(&mut self, cmd: Command) {
        match cmd {
            Command::NewTab => self.new_welcome_tab(),
            Command::CloseTab => self.close_tab(self.active),
            Command::ReopenClosedTab => self.reopen_closed_tab(),
            Command::OpenRepo => self.shared.actions.push(AppAction::PickAndOpenRepo),
            Command::Clone => self.shared.actions.push(AppAction::ShowClone),
            Command::InitRepo => self.shared.actions.push(AppAction::InitRepo),
            Command::QuickOpen => self.shared.actions.push(AppAction::QuickOpen),
            Command::Accounts => self.shared.actions.push(AppAction::ShowAccounts),
            Command::Settings => self.shared.actions.push(AppAction::ShowSettings),
            Command::ShowLog => self.shared.actions.push(AppAction::ShowLog),
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
                },
                TabKind::Repo(r) => TabInfo {
                    id: t.id,
                    title: r.title(),
                    tooltip: r.path.display().to_string(),
                    busy: r.is_busy(),
                    is_repo: true,
                },
            })
            .collect()
    }

    fn apply_tab_actions(&mut self, actions: Vec<TabBarAction>, ctx: &egui::Context) {
        for a in actions {
            match a {
                TabBarAction::Select(i) => self.active = i.min(self.tabs.len() - 1),
                TabBarAction::Close(i) => self.close_tab(i),
                TabBarAction::CloseOthers(i) => {
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
                TabBarAction::Move(from, to) => {
                    if from < self.tabs.len() && to < self.tabs.len() {
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
                TabBarAction::OpenExplorer(i) => {
                    if let Some(TabKind::Repo(r)) = self.tabs.get(i).map(|t| &t.kind) {
                        crate::util::open_in_explorer(&r.path);
                    }
                }
                TabBarAction::CopyPath(i) => {
                    if let Some(TabKind::Repo(r)) = self.tabs.get(i).map(|t| &t.kind) {
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
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.process_events();
        self.handle_shortcuts(&ctx);
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
            for (i, tab) in self.tabs.iter_mut().enumerate() {
                if let TabKind::Repo(r) = &mut tab.kind {
                    r.tick(&mut self.shared, i == active);
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
        if let Some(c) = &mut self.clone
            && !c.ui(&ctx, &mut self.shared, &p)
        {
            self.clone = None;
        }
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
        self.shared.toasts.show(&ctx, &p);

        self.process_actions(&ctx);
        self.maybe_screenshot(&ctx);
    }

    fn save(&mut self, _storage: &mut dyn eframe::Storage) {
        self.remember_tabs();
        self.shared.save_settings();
    }
}
