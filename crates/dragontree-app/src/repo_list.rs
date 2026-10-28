//! Barre latérale droite : dépôts trouvés automatiquement sous les dossiers racines.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use dragontree_git::discovery::{FoundRepo, RepoKind, ScanOptions, repo_head, scan};
use dragontree_git::status::parse_status_v2;
use dragontree_git::{CancelToken, GitError, HeadState};
use dragontree_i18n::tr;
use egui::{Align2, FontId, RichText, Sense, Ui};

use crate::app::{AppAction, AppEvent, Shared};
use crate::icons;
use crate::settings::{cache_dir, path_key};
use crate::theme::Palette;
use crate::toasts::Toast;
use crate::widgets;
use crate::workspaces::TabColor;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QuickStatus {
    pub dirty: u32,
    pub ahead: u32,
    pub behind: u32,
    /// Dépôt refusé par git (`safe.directory`).
    pub untrusted: bool,
    pub error: Option<String>,
}

/// Un dépôt trouvé sous un dossier racine, avec ce que la liste en sait.
#[derive(Debug, Clone, PartialEq)]
pub struct ListedRepo {
    pub repo: FoundRepo,
    /// Son état rapide ; `None` : pas encore relevé.
    pub status: Option<QuickStatus>,
    /// Sa branche, lue sur le disque ; `None` : illisible.
    pub head: Option<HeadState>,
}

/// Les dépôts de DragonTree tels qu'il les connaît en ce moment, en mémoire (ses réglages ne sont écrits qu'ensuite),
/// pour un hôte qui les montre ailleurs : le téléphone de la suite (voir `DragonTreeApp::repo_directory`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RepoDirectory {
    /// Les dossiers des onglets de dépôt, dans l'ordre de la barre (ceux ouverts à distance compris).
    pub tabs: Vec<PathBuf>,
    /// Celui de l'onglet affiché ; `None` : un onglet d'accueil.
    pub active: Option<PathBuf>,
    /// Épinglés en haut de la liste.
    pub favorites: Vec<PathBuf>,
    /// Ouverts récemment, le plus récent d'abord.
    pub recent: Vec<PathBuf>,
    /// Les dossiers racines analysés, dans l'ordre des réglages.
    pub roots: Vec<PathBuf>,
    /// Les dépôts trouvés sous les racines, dans l'ordre du scan (ceux d'une racine retirée peuvent y rester jusqu'à
    /// la fin du scan suivant).
    pub found: Vec<ListedRepo>,
    /// Masqués de la liste.
    pub hidden: Vec<PathBuf>,
    /// Couleur de l'onglet d'un dépôt (clé : `path_key` de son dossier).
    pub tab_colors: BTreeMap<String, TabColor>,
    /// Un scan des dossiers racines est en cours.
    pub scanning: bool,
    /// Le relevé de l'état des dépôts trouvés est en cours.
    pub sweeping: bool,
}

pub enum ScanEvent {
    Found(FoundRepo),
    Finished(Vec<FoundRepo>),
    Status { key: String, status: QuickStatus, head: Option<HeadState> },
    SweepDone,
}

pub struct RepoListState {
    pub repos: Vec<FoundRepo>,
    heads: HashMap<String, Option<HeadState>>,
    statuses: HashMap<String, QuickStatus>,
    pub scanning: bool,
    pub sweeping: bool,
    pub filter: String,
    cancel: Option<CancelToken>,
    collapsed_roots: HashSet<String>,
}

fn cache_file() -> PathBuf {
    cache_dir().join("repos-cache.json")
}

impl RepoListState {
    /// Liste du dernier scan (affichée tout de suite, puis rafraîchie en arrière-plan).
    pub fn load_cache(profile: Option<&Path>) -> RepoListState {
        let file = profile.map(|p| p.join("repos-cache.json")).unwrap_or_else(cache_file);
        let repos: Vec<FoundRepo> =
            std::fs::read_to_string(file).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        let mut state = RepoListState {
            repos: Vec::new(),
            heads: HashMap::new(),
            statuses: HashMap::new(),
            scanning: false,
            sweeping: false,
            filter: String::new(),
            cancel: None,
            collapsed_roots: HashSet::new(),
        };
        state.set_repos(repos);
        state
    }

    fn save_cache(&self, profile: Option<&Path>) {
        let file = profile.map(|p| p.join("repos-cache.json")).unwrap_or_else(cache_file);
        if let Some(dir) = file.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(json) = serde_json::to_string(&self.repos) {
            let _ = std::fs::write(file, json);
        }
    }

    fn set_repos(&mut self, repos: Vec<FoundRepo>) {
        self.heads = repos.iter().map(|r| (path_key(&r.path), repo_head(&r.path))).collect();
        self.repos = repos;
    }

    pub fn start_scan(&mut self, shared: &Shared) {
        if let Some(c) = self.cancel.take() {
            c.cancel();
        }
        let roots = shared.settings.scan_roots.clone();
        if roots.is_empty() {
            self.set_repos(Vec::new());
            return;
        }
        let opts = ScanOptions {
            max_depth: shared.settings.scan_max_depth,
            skip_dirs: shared.settings.scan_skip_dirs.clone(),
            include_submodules: shared.settings.scan_deep,
        };
        let cancel = CancelToken::new();
        self.cancel = Some(cancel.clone());
        self.scanning = true;
        let sender = shared.jobs.sender();
        shared.jobs.spawn("scan", move || {
            let progress = sender.clone();
            let found = scan(&roots, &opts, &cancel, &|r| progress.send(AppEvent::Scan(ScanEvent::Found(r))));
            (!cancel.is_cancelled()).then_some(AppEvent::Scan(ScanEvent::Finished(found)))
        });
    }

    /// Calcule l'état de chaque dépôt (4 en parallèle, sans verrou ni fenêtre d'authentification).
    pub fn start_sweep(&mut self, shared: &Shared) {
        let Some(git) = shared.git.clone() else { return };
        if !shared.settings.status_sweep || self.sweeping || self.repos.is_empty() {
            return;
        }
        self.sweeping = true;
        let queue = Arc::new(Mutex::new(self.repos.iter().map(|r| r.path.clone()).collect::<Vec<_>>()));
        let remaining = Arc::new(std::sync::atomic::AtomicUsize::new(4));
        for i in 0..4 {
            let (queue, git, sender, remaining) = (queue.clone(), git.clone(), shared.jobs.sender(), remaining.clone());
            shared.jobs.spawn(&format!("sweep-{i}"), move || {
                loop {
                    let next = queue.lock().unwrap().pop();
                    let Some(path) = next else { break };
                    let status = quick_status(&git, &path);
                    // La branche aussi : elle a pu changer hors de DragonTree depuis le dernier scan.
                    let head = repo_head(&path);
                    sender.send(AppEvent::Scan(ScanEvent::Status { key: path_key(&path), status, head }));
                }
                (remaining.fetch_sub(1, std::sync::atomic::Ordering::SeqCst) == 1)
                    .then_some(AppEvent::Scan(ScanEvent::SweepDone))
            });
        }
    }

    /// Les dépôts trouvés, dans l'ordre du scan, avec leur état relevé et leur branche.
    pub fn listed(&self) -> Vec<ListedRepo> {
        let listed = |r: &FoundRepo| {
            let key = path_key(&r.path);
            let head = self.heads.get(&key).cloned().flatten();
            ListedRepo { repo: r.clone(), status: self.statuses.get(&key).cloned(), head }
        };
        self.repos.iter().map(listed).collect()
    }

    /// L'état relevé du dépôt `path` et sa branche ; `None` : pas encore relevé, ou pas dans la liste.
    pub fn status_of(&self, path: &Path) -> Option<(QuickStatus, Option<HeadState>)> {
        let key = path_key(path);
        let status = self.statuses.get(&key)?.clone();
        Some((status, self.heads.get(&key).cloned().flatten()))
    }

    /// Met à jour l'état d'un dépôt à partir de l'onglet qui l'affiche (sans relancer git).
    pub fn update_status(&mut self, path: &Path, status: QuickStatus, head: Option<HeadState>) {
        let key = path_key(path);
        if self.heads.contains_key(&key) {
            self.heads.insert(key.clone(), head);
            self.statuses.insert(key, status);
        }
    }

    pub fn handle(&mut self, ev: ScanEvent, shared: &mut Shared) {
        match ev {
            ScanEvent::Found(r) => {
                let key = path_key(&r.path);
                if !self.repos.iter().any(|x| path_key(&x.path) == key) {
                    self.heads.insert(key, repo_head(&r.path));
                    self.repos.push(r);
                }
            }
            ScanEvent::Finished(found) => {
                self.scanning = false;
                self.cancel = None;
                self.set_repos(found);
                self.save_cache(shared.profile_dir.as_deref());
                self.start_sweep(shared);
            }
            ScanEvent::Status { key, status, head } => {
                if self.heads.contains_key(&key) {
                    self.heads.insert(key.clone(), head);
                }
                self.statuses.insert(key, status);
            }
            ScanEvent::SweepDone => self.sweeping = false,
        }
    }

    pub fn ui(&mut self, ui: &mut Ui, shared: &mut Shared, open_keys: &HashSet<String>, p: &Palette) {
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new(tr!("Dépôts")).strong().size(15.0));
            widgets::count_pill(ui, self.visible_count(shared), p);
            if self.scanning || self.sweeping {
                ui.add(egui::Spinner::new().size(12.0));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if widgets::icon_button(ui, icons::GEAR_SIX, tr!("Réglages du scan")).clicked() {
                    shared.actions.push(AppAction::ShowSettings);
                }
                if widgets::icon_button(ui, icons::ARROWS_CLOCKWISE, tr!("Rescanner les dossiers")).clicked() {
                    shared.actions.push(AppAction::Rescan);
                }
                // Un dépôt de plus : un dossier sur le disque, un dépôt distant à cloner, ou un dossier à scanner.
                let add = widgets::icon_button(ui, icons::PLUS, tr!("Ajouter un dépôt…"));
                widgets::a11y(&add, tr!("Ajouter un dépôt…"), true);
                egui::Popup::menu(&add).show(|ui| {
                    if ui.button(format!("{} {}", icons::FOLDER_OPEN, tr!("Ouvrir un dépôt…"))).clicked() {
                        shared.actions.push(AppAction::PickAndOpenRepo);
                    }
                    if ui.button(format!("{} {}", icons::DOWNLOAD_SIMPLE, tr!("Cloner un dépôt distant…"))).clicked()
                    {
                        shared.actions.push(AppAction::ShowClone);
                    }
                    ui.separator();
                    if ui
                        .button(format!("{} {}", icons::TREE_STRUCTURE, tr!("Ajouter un dossier à scanner…")))
                        .clicked()
                    {
                        shared.actions.push(AppAction::AddScanRoot);
                    }
                });
            });
        });
        widgets::search_field(ui, &mut self.filter, tr!("Rechercher un dépôt (Ctrl+P)"), "repo-list-filter");
        ui.add_space(4.0);

        if shared.settings.scan_roots.is_empty() {
            ui.add_space(20.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new(icons::TREE_STRUCTURE).size(30.0).color(p.weak));
                ui.label(RichText::new(tr!("Aucun dossier scanné")).color(p.weak));
                ui.add_space(6.0);
                if ui.button(format!("{} {}", icons::FOLDER_OPEN, tr!("Ajouter un dossier…"))).clicked() {
                    shared.actions.push(AppAction::AddScanRoot);
                }
            });
            return;
        }

        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 1.0;
            let hidden: HashSet<String> = shared.settings.hidden_repos.iter().map(|p| path_key(p)).collect();
            if !self.filter.trim().is_empty() {
                let mut scored: Vec<(i32, &FoundRepo)> = self
                    .repos
                    .iter()
                    .filter(|r| !hidden.contains(&path_key(&r.path)))
                    .filter_map(|r| {
                        let hay = format!("{} {} {}", r.name, r.relative_parent(), self.branch_label(&r.path));
                        widgets::fuzzy_score(&self.filter, &hay).map(|s| (s, r))
                    })
                    .collect();
                scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.name.cmp(&b.1.name)));
                let rows: Vec<FoundRepo> = scored.into_iter().map(|(_, r)| r.clone()).collect();
                if rows.is_empty() {
                    ui.label(RichText::new(tr!("Aucun dépôt ne correspond.")).color(p.weak));
                }
                for r in &rows {
                    self.repo_row(ui, r, shared, open_keys, p);
                }
                return;
            }

            let favorites: Vec<FoundRepo> =
                self.repos.iter().filter(|r| shared.settings.is_favorite(&r.path)).cloned().collect();
            if !favorites.is_empty() {
                section_label(ui, &format!("{} {}", icons::PUSH_PIN, tr!("Épinglés")), p);
                for r in &favorites {
                    self.repo_row(ui, r, shared, open_keys, p);
                }
                ui.add_space(6.0);
            }

            for root in shared.settings.scan_roots.clone() {
                let root_key = path_key(&root);
                let repos: Vec<FoundRepo> = self
                    .repos
                    .iter()
                    .filter(|r| path_key(&r.root) == root_key && !hidden.contains(&path_key(&r.path)))
                    .cloned()
                    .collect();
                let collapsed = self.collapsed_roots.contains(&root_key);
                let title = root
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| root.display().to_string());
                let (rect, resp) = widgets::row_response(ui, ("root", &root_key), 24.0, false, Sense::click());
                let caret = if collapsed { icons::CARET_RIGHT } else { icons::CARET_DOWN };
                ui.painter().text(
                    rect.left_center() + egui::vec2(8.0, 0.0),
                    Align2::CENTER_CENTER,
                    caret,
                    FontId::proportional(12.0),
                    p.weak,
                );
                ui.painter().text(
                    rect.left_center() + egui::vec2(20.0, 0.0),
                    Align2::LEFT_CENTER,
                    format!("{}  {}", icons::FOLDER_SIMPLE, title),
                    FontId::new(13.0, crate::theme::bold_family()),
                    p.text,
                );
                ui.painter().text(
                    rect.right_center() - egui::vec2(8.0, 0.0),
                    Align2::RIGHT_CENTER,
                    repos.len().to_string(),
                    FontId::proportional(11.0),
                    p.weak,
                );
                let resp = resp.on_hover_text(root.display().to_string());
                if resp.clicked() {
                    if collapsed {
                        self.collapsed_roots.remove(&root_key);
                    } else {
                        self.collapsed_roots.insert(root_key.clone());
                    }
                }
                resp.context_menu(|ui| {
                    if ui.button(format!("{} {}", icons::ARROWS_CLOCKWISE, tr!("Rescanner"))).clicked() {
                        shared.actions.push(AppAction::Rescan);
                        ui.close();
                    }
                    if ui.button(format!("{} {}", icons::FOLDER_OPEN, tr!("Ouvrir dans l'Explorateur"))).clicked() {
                        crate::util::open_in_explorer(&root);
                        ui.close();
                    }
                    ui.separator();
                    if ui.button(format!("{} {}", icons::X, tr!("Retirer ce dossier de la liste"))).clicked() {
                        shared.actions.push(AppAction::RemoveScanRoot(root.clone()));
                        ui.close();
                    }
                });
                if !collapsed {
                    for r in &repos {
                        self.repo_row(ui, r, shared, open_keys, p);
                    }
                    if repos.is_empty() && !self.scanning {
                        ui.label(RichText::new(format!("    {}", tr!("Aucun dépôt trouvé"))).color(p.weak).size(12.0));
                    }
                }
                ui.add_space(4.0);
            }
        });
    }

    fn visible_count(&self, shared: &Shared) -> usize {
        let hidden: HashSet<String> = shared.settings.hidden_repos.iter().map(|p| path_key(p)).collect();
        self.repos.iter().filter(|r| !hidden.contains(&path_key(&r.path))).count()
    }

    fn branch_label(&self, path: &Path) -> String {
        match self.heads.get(&path_key(path)) {
            Some(Some(HeadState::Branch(b) | HeadState::Unborn(b))) => b.clone(),
            Some(Some(HeadState::Detached(oid))) => tr!("HEAD détachée ({sha})", sha = oid.short()),
            _ => String::new(),
        }
    }

    fn repo_row(&self, ui: &mut Ui, r: &FoundRepo, shared: &mut Shared, open_keys: &HashSet<String>, p: &Palette) {
        let key = path_key(&r.path);
        let is_open = open_keys.contains(&key);
        let status = self.statuses.get(&key);
        let (rect, resp) = widgets::row_response(ui, ("repo", &key), 38.0, false, Sense::click());
        widgets::a11y(&resp, &r.name, true);
        let painter = ui.painter_at(rect);
        let icon = match r.kind {
            RepoKind::Normal => icons::GIT_BRANCH,
            RepoKind::Worktree => icons::TREE_STRUCTURE,
            RepoKind::Submodule => icons::CUBE,
        };
        let untrusted = status.is_some_and(|s| s.untrusted);
        let icon_color = if untrusted {
            p.modified_fg
        } else if is_open {
            p.accent
        } else {
            p.weak
        };
        painter.text(
            rect.left_top() + egui::vec2(16.0, 12.0),
            Align2::CENTER_CENTER,
            if untrusted { icons::WARNING } else { icon },
            FontId::proportional(14.0),
            icon_color,
        );

        // Indicateurs à droite : modifications, avance/retard.
        let mut right = rect.right() - 8.0;
        if let Some(s) = status {
            if s.behind > 0 {
                let r2 = painter.text(
                    egui::pos2(right, rect.top() + 12.0),
                    Align2::RIGHT_CENTER,
                    format!("{}{}", icons::ARROW_DOWN, s.behind),
                    FontId::proportional(11.5),
                    p.accent,
                );
                right = r2.left() - 6.0;
            }
            if s.ahead > 0 {
                let r2 = painter.text(
                    egui::pos2(right, rect.top() + 12.0),
                    Align2::RIGHT_CENTER,
                    format!("{}{}", icons::ARROW_UP, s.ahead),
                    FontId::proportional(11.5),
                    p.added_fg,
                );
                right = r2.left() - 6.0;
            }
            if s.dirty > 0 {
                let r2 = painter.text(
                    egui::pos2(right, rect.top() + 12.0),
                    Align2::RIGHT_CENTER,
                    format!("● {}", s.dirty),
                    FontId::proportional(11.5),
                    p.modified_fg,
                );
                right = r2.left() - 6.0;
            }
        }

        let name_font =
            if is_open { FontId::new(13.5, crate::theme::bold_family()) } else { FontId::proportional(13.5) };
        let name_rect =
            egui::Rect::from_min_max(rect.left_top() + egui::vec2(30.0, 2.0), egui::pos2(right, rect.top() + 21.0));
        let mut job = egui::text::LayoutJob::single_section(
            r.name.clone(),
            egui::TextFormat { font_id: name_font, color: p.text, ..Default::default() },
        );
        let parent = r.relative_parent();
        if !parent.is_empty() {
            job.append(
                &format!("  {parent}"),
                0.0,
                egui::TextFormat { font_id: FontId::proportional(11.5), color: p.weak, ..Default::default() },
            );
        }
        job.wrap = egui::text::TextWrapping {
            max_width: name_rect.width(),
            max_rows: 1,
            break_anywhere: true,
            overflow_character: Some('…'),
        };
        let galley = painter.layout_job(job);
        painter.galley(egui::pos2(name_rect.left(), rect.top() + 12.0 - galley.size().y / 2.0), galley, p.text);

        let branch = if untrusted {
            tr!("dépôt non approuvé (safe.directory)").to_string()
        } else {
            self.branch_label(&r.path)
        };
        let mut job = egui::text::LayoutJob::single_section(
            format!("{}  {branch}", icons::GIT_BRANCH),
            egui::TextFormat {
                font_id: FontId::proportional(11.5),
                color: if untrusted { p.modified_fg } else { p.weak },
                ..Default::default()
            },
        );
        job.wrap = egui::text::TextWrapping {
            max_width: rect.width() - 38.0,
            max_rows: 1,
            break_anywhere: true,
            overflow_character: Some('…'),
        };
        let galley = painter.layout_job(job);
        painter.galley(egui::pos2(rect.left() + 30.0, rect.top() + 27.0 - galley.size().y / 2.0), galley, p.weak);

        let resp = resp.on_hover_text(r.path.display().to_string());
        if resp.clicked() {
            shared.actions.push(AppAction::OpenRepo(r.path.clone()));
        }
        resp.context_menu(|ui| {
            if ui.button(format!("{} {}", icons::GIT_BRANCH, tr!("Ouvrir"))).clicked() {
                shared.actions.push(AppAction::OpenRepo(r.path.clone()));
                ui.close();
            }
            if ui.button(format!("{} {}", icons::PLUS, tr!("Ouvrir dans un nouvel onglet"))).clicked() {
                shared.actions.push(AppAction::OpenRepoNewTab(r.path.clone()));
                ui.close();
            }
            ui.separator();
            if ui.button(format!("{} {}", icons::FOLDER_OPEN, tr!("Ouvrir dans l'Explorateur"))).clicked() {
                crate::util::open_in_explorer(&r.path);
                ui.close();
            }
            if ui.button(format!("{} {}", icons::TERMINAL_WINDOW, tr!("Ouvrir un terminal"))).clicked() {
                shared.open_terminal(&r.path);
                ui.close();
            }
            ui.menu_button(format!("{} {}", icons::CODE, tr!("Ouvrir dans")), |ui| {
                for editor in shared.editors.clone() {
                    if ui.button(format!("{} {}", editor.kind.icon(), editor.kind.label())).clicked() {
                        shared.settings.last_editor = Some(editor.kind.id().to_string());
                        shared.settings_dirty = true;
                        // Plusieurs solutions à départager : le choix se fait dans l'onglet du dépôt.
                        match crate::editors::open(&editor, &r.path) {
                            Ok(crate::editors::OpenOutcome::Launched) => {}
                            Ok(crate::editors::OpenOutcome::Choose(_)) => shared.toast(Toast::info(tr!(
                                "Plusieurs solutions : ouvrez le dépôt dans un onglet pour choisir celle à ouvrir"
                            ))),
                            Err(e) => shared.toast(Toast::error(
                                tr!("{editor} n'a pas pu être lancé", editor = editor.kind.label()),
                                e,
                            )),
                        }
                        ui.close();
                    }
                }
            });
            if ui.button(format!("{} {}", icons::COPY, tr!("Copier le chemin"))).clicked() {
                ui.ctx().copy_text(r.path.display().to_string());
                ui.close();
            }
            ui.separator();
            let fav = shared.settings.is_favorite(&r.path);
            if ui
                .button(format!(
                    "{} {}",
                    icons::PUSH_PIN,
                    if fav { tr!("Désépingler") } else { tr!("Épingler en haut") }
                ))
                .clicked()
            {
                shared.settings.toggle_favorite(&r.path);
                shared.settings_dirty = true;
                ui.close();
            }
            if ui.button(format!("{} {}", icons::EYE_SLASH, tr!("Masquer de la liste"))).clicked() {
                shared.settings.hidden_repos.push(r.path.clone());
                shared.settings_dirty = true;
                ui.close();
            }
            if untrusted {
                ui.separator();
                if ui
                    .button(format!("{} {}", icons::SHIELD_CHECK, tr!("Faire confiance à ce dépôt (safe.directory)")))
                    .clicked()
                {
                    shared.actions.push(AppAction::TrustRepo(r.path.clone()));
                    ui.close();
                }
            }
        });
    }
}

fn section_label(ui: &mut Ui, text: &str, p: &Palette) {
    ui.add_space(2.0);
    ui.label(RichText::new(text).size(11.5).color(p.weak).strong());
}

/// État rapide d'un dépôt pour la liste. Calculé sans que l'utilisateur ait ouvert le dépôt : rien de ce que désigne
/// sa configuration n'est lancé, et l'état des sous-modules n'est pas calculé (ils ont leur propre configuration).
pub fn quick_status(git: &dragontree_git::Git, path: &Path) -> QuickStatus {
    let out = git
        .cmd(path)
        .read_only()
        .non_interactive()
        .args(dragontree_git::discovery::untrusted_config_args(git, path))
        .args(["status", "--porcelain=v2", "-z", "--branch", "--untracked-files=normal", "--ignore-submodules=dirty"])
        .run();
    match out {
        Ok(o) => match parse_status_v2(&o.stdout) {
            Ok(s) => QuickStatus {
                dirty: s.entries.iter().filter(|e| e.worktree != dragontree_git::FileState::Ignored).count() as u32,
                ahead: s.ahead,
                behind: s.behind,
                untrusted: false,
                error: None,
            },
            Err(e) => QuickStatus { error: Some(e.to_string()), ..Default::default() },
        },
        Err(GitError::DubiousOwnership { .. }) => QuickStatus { untrusted: true, ..Default::default() },
        Err(e) => QuickStatus { error: Some(e.to_string()), ..Default::default() },
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use dragontree_git::Git;

    use super::*;
    use crate::app::Shared;
    use crate::settings::SettingsFile;

    fn git_in(git: &Git, dir: &Path, args: &[&str]) {
        git.cmd(dir).args(args).run().unwrap_or_else(|e| panic!("git {args:?} : {e}"));
    }

    /// Traite les événements de la liste jusqu'à la fin du calcul des états.
    fn drain_sweep(list: &mut RepoListState, shared: &mut Shared, rx: &std::sync::mpsc::Receiver<AppEvent>) {
        loop {
            if let AppEvent::Scan(ev) = rx.recv_timeout(Duration::from_secs(20)).expect("fin du calcul des états") {
                let done = matches!(ev, ScanEvent::SweepDone);
                list.handle(ev, shared);
                if done {
                    return;
                }
            }
        }
    }

    /// Un dépôt trouvé sur le disque (archive extraite, dossier partagé) n'a pas été ouvert : l'état rapide ne lance
    /// aucun programme désigné par sa configuration (`core.fsmonitor`, filtre `clean`).
    #[test]
    fn quick_status_runs_no_program_from_the_repository_config() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("piege");
        std::fs::create_dir_all(&repo).unwrap();
        let git = crate::test_git();
        git_in(&git, &repo, &["init", "-q", "-b", "main"]);
        std::fs::write(repo.join(".gitattributes"), "*.txt filter=piege\n").unwrap();
        std::fs::write(repo.join("a.txt"), "a\n").unwrap();
        git_in(&git, &repo, &["add", "-A"]);
        git_in(&git, &repo, &["-c", "user.name=T", "-c", "user.email=t@x", "commit", "-q", "-m", "init"]);
        git_in(&git, &repo, &["config", "core.fsmonitor", "touch fsmonitor-lance; echo"]);
        git_in(&git, &repo, &["config", "filter.piege.clean", "touch filtre-lance; cat"]);
        // Fichier modifié : git doit le relire, donc passer par le filtre.
        std::fs::write(repo.join("a.txt"), "b\n").unwrap();
        let status = quick_status(&git, &repo);
        assert_eq!(status.error, None);
        assert!(!repo.join("fsmonitor-lance").exists(), "core.fsmonitor du dépôt lancé");
        assert!(!repo.join("filtre-lance").exists(), "filtre clean du dépôt lancé");
        assert_eq!(status.dirty, 1);
    }

    #[test]
    fn sweep_refreshes_the_branch_of_listed_repos() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("alpha");
        std::fs::create_dir_all(&repo).unwrap();
        let git = crate::test_git();
        git_in(&git, &repo, &["init", "-q", "-b", "main"]);
        git_in(
            &git,
            &repo,
            &["-c", "user.name=T", "-c", "user.email=t@x", "commit", "-q", "--allow-empty", "-m", "init"],
        );
        let profile = tmp.path().join("profil");
        let (store, settings, _) = SettingsFile::load(&profile.join("settings.json"));
        let (mut shared, rx) = Shared::new(egui::Context::default(), settings, store, Some(profile), Some(git.clone()));
        let mut list = RepoListState::load_cache(shared.profile_dir.as_deref());
        let found = FoundRepo {
            path: repo.clone(),
            name: "alpha".into(),
            root: tmp.path().to_path_buf(),
            kind: RepoKind::Normal,
        };
        list.handle(ScanEvent::Finished(vec![found]), &mut shared);
        drain_sweep(&mut list, &mut shared, &rx);
        assert_eq!(list.heads[&path_key(&repo)], Some(HeadState::Branch("main".into())));

        // Changement de branche hors de DragonTree (terminal, autre outil), dépôt non ouvert en onglet.
        git_in(&git, &repo, &["checkout", "-q", "-b", "feature/x"]);
        list.start_sweep(&shared);
        drain_sweep(&mut list, &mut shared, &rx);
        assert_eq!(list.heads[&path_key(&repo)], Some(HeadState::Branch("feature/x".into())), "branche à jour");
    }
}
