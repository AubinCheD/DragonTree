//! Fenêtre « Pull requests » (Fichier › Pull requests…) : les pull requests ouvertes qui concernent l'utilisateur, sur
//! tous ses comptes, de la plus récemment mise à jour à la plus ancienne. Lecture en tâche de fond à l'ouverture, toutes
//! les [`REFRESH_EVERY`] tant que la fenêtre reste ouverte et sur demande ; l'erreur d'un compte s'affiche à sa place
//! sans empêcher les autres. Filtres : créées par moi, à relire, par compte, texte. Une pull request s'ouvre dans le
//! navigateur, et son dépôt dans un onglet quand une copie locale le désigne (remotes lus sur disque, en tâche de fond).
//!
//! Les jetons ne sont lus que dans les tâches de fond (Gestionnaire d'identifiants) et n'apparaissent jamais dans les
//! erreurs affichées.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use dragontree_forges::{Account, ForgeError, PullRequest, RepoLocation, secrets};
use dragontree_git::GitError;
use dragontree_git::discovery::{FoundRepo, RepoKind, remote_urls};
use dragontree_i18n::{tr, trn};
use egui::{Align, Layout, RichText, Sense};

use crate::accounts::kind_icon;
use crate::app::{AppAction, AppEvent, Shared};
use crate::icons;
use crate::settings::path_key;
use crate::theme::Palette;
use crate::widgets;

/// Intervalle du rafraîchissement automatique, tant que la fenêtre est ouverte.
pub const REFRESH_EVERY: Duration = Duration::from_secs(5 * 60);

/// Lecture des pull requests d'un compte, sur un thread de fond. Les tests d'interface en passent une qui répond sans
/// réseau ni Gestionnaire d'identifiants (`StartOptions::pull_request_source`).
pub type PullRequestSource = fn(&Account) -> Result<Vec<PullRequest>, ForgeError>;

/// Lecture réelle : jeton du compte dans le Gestionnaire d'identifiants, puis l'API de sa forge.
pub fn fetch_from_forge(account: &Account) -> Result<Vec<PullRequest>, ForgeError> {
    match secrets::load_token(&account.id) {
        Ok(Some(token)) => dragontree_forges::list_pull_requests(account, &token),
        Ok(None) => Err(ForgeError::Secret(tr!("jeton absent du Gestionnaire d'identifiants").into())),
        Err(e) => Err(e),
    }
}

pub enum PullRequestEvent {
    /// Pull requests d'un compte (ou l'erreur affichée), pour le rafraîchissement `generation`.
    Loaded { generation: u64, account_id: String, result: Result<Vec<PullRequest>, String> },
    /// Copies locales et URL de leurs remotes, pour l'indexation `generation`.
    LocalClones { generation: u64, clones: Vec<LocalClone> },
}

/// Copie de travail locale et URL de ses remotes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalClone {
    pub path: PathBuf,
    pub urls: Vec<String>,
}

/// Pull requests d'un compte : la dernière lecture réussie, ou son erreur.
#[derive(Default)]
struct AccountLoad {
    loading: bool,
    prs: Vec<PullRequest>,
    error: Option<String>,
    /// Copie locale du dépôt de chaque pull request (même ordre que `prs`).
    local: Vec<Option<PathBuf>>,
}

/// Filtre principal de la liste.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PrFilter {
    #[default]
    All,
    /// Ouvertes par l'utilisateur du compte.
    Mine,
    /// Relecture attendue de l'utilisateur.
    ToReview,
}

pub struct PullRequestsWindow {
    pub open: bool,
    source: PullRequestSource,
    /// Rafraîchissement en cours : une réponse d'un rafraîchissement plus ancien est ignorée.
    generation: u64,
    loads: HashMap<String, AccountLoad>,
    /// Dernier rafraîchissement lancé (`None` : à lancer à la prochaine image) et son heure (secondes Unix).
    last_refresh: Option<Instant>,
    refreshed_at: Option<i64>,
    clones: Vec<LocalClone>,
    clones_generation: u64,
    filter: PrFilter,
    /// Compte choisi ; `None` : tous.
    account_filter: Option<String>,
    text: String,
}

impl PullRequestsWindow {
    pub fn new(source: PullRequestSource) -> PullRequestsWindow {
        PullRequestsWindow {
            open: false,
            source,
            generation: 0,
            loads: HashMap::new(),
            last_refresh: None,
            refreshed_at: None,
            clones: Vec::new(),
            clones_generation: 0,
            filter: PrFilter::All,
            account_filter: None,
            text: String::new(),
        }
    }

    /// Ouvre la fenêtre ; la liste est relue à la prochaine image (la précédente reste affichée en attendant).
    pub fn show(&mut self) {
        self.open = true;
        self.last_refresh = None;
    }

    fn loading(&self) -> bool {
        self.loads.values().any(|l| l.loading)
    }

    /// Un rafraîchissement est à lancer : à l'ouverture, pour un compte jamais lu (ajouté depuis), ou après
    /// [`REFRESH_EVERY`] sans lecture en cours.
    fn refresh_due(&self, accounts: &[Account], now: Instant) -> bool {
        let never_read = accounts.iter().any(|a| !self.loads.contains_key(&a.id));
        let stale = match self.last_refresh {
            None => true,
            Some(last) => !self.loading() && now.saturating_duration_since(last) >= REFRESH_EVERY,
        };
        never_read || stale
    }

    /// Relit les pull requests de chaque compte (une tâche par compte) et les copies locales.
    fn refresh(&mut self, shared: &Shared, repos: &[FoundRepo]) {
        self.generation += 1;
        self.last_refresh = Some(Instant::now());
        self.refreshed_at = Some(widgets::now_ts());
        let accounts = &shared.settings.accounts;
        self.loads.retain(|id, _| accounts.iter().any(|a| &a.id == id));
        for account in accounts {
            self.loads.entry(account.id.clone()).or_default().loading = true;
            let (account, source, generation) = (account.clone(), self.source, self.generation);
            shared.jobs.spawn("pull-requests", move || {
                // Une panique devient l'erreur du compte : sans réponse, il resterait « en cours » et Actualiser grisé.
                let result = match std::panic::catch_unwind(|| source(&account)) {
                    Ok(read) => read.map_err(|e| e.to_string()),
                    Err(payload) => Err(GitError::Internal(crate::jobs::panic_message(payload.as_ref())).to_string()),
                };
                Some(AppEvent::PullRequests(PullRequestEvent::Loaded { generation, account_id: account.id, result }))
            });
        }
        self.index_local_clones(shared, repos);
    }

    /// Relit les remotes des copies locales (dépôts récents, puis ceux de la liste) en tâche de fond. Sans effet
    /// quand la fenêtre est fermée ; appelée aussi à la fin d'un scan de la liste des dépôts.
    pub fn index_local_clones(&mut self, shared: &Shared, repos: &[FoundRepo]) {
        if !self.open || shared.settings.accounts.is_empty() {
            return;
        }
        self.clones_generation += 1;
        let generation = self.clones_generation;
        let candidates = local_candidates(&shared.settings.recent_repos, repos);
        shared.jobs.spawn("pull-requests-local", move || {
            let clones = candidates
                .into_iter()
                .map(|path| LocalClone { urls: remote_urls(&path), path })
                .filter(|clone| !clone.urls.is_empty())
                .collect();
            Some(AppEvent::PullRequests(PullRequestEvent::LocalClones { generation, clones }))
        });
    }

    /// Réponse d'une tâche de fond ; `accounts` : comptes des réglages (forge d'un hôte auto-hébergé).
    pub fn handle(&mut self, ev: PullRequestEvent, accounts: &[Account]) {
        match ev {
            PullRequestEvent::Loaded { generation, account_id, result } => {
                if generation != self.generation {
                    return;
                }
                let Some(load) = self.loads.get_mut(&account_id) else { return };
                load.loading = false;
                match result {
                    Ok(prs) => {
                        load.prs = prs;
                        load.error = None;
                    }
                    Err(e) => {
                        load.prs.clear();
                        load.error = Some(e);
                    }
                }
                let account = accounts.iter().find(|a| a.id == account_id);
                load.local = link_local_clones(&load.prs, account, &self.clones);
            }
            PullRequestEvent::LocalClones { generation, clones } => {
                if generation != self.clones_generation {
                    return;
                }
                self.clones = clones;
                for (id, load) in &mut self.loads {
                    let account = accounts.iter().find(|a| &a.id == id);
                    load.local = link_local_clones(&load.prs, account, &self.clones);
                }
            }
        }
    }

    /// Lignes affichées : pull requests des comptes choisis qui passent les filtres, de la plus récemment mise à jour
    /// à la plus ancienne (comptes dans l'ordre des réglages à date égale).
    fn rows<'a>(&'a self, accounts: &'a [Account]) -> Vec<Row<'a>> {
        let mut rows: Vec<Row<'a>> = Vec::new();
        for account in accounts.iter().filter(|a| self.account_filter.as_ref().is_none_or(|id| id == &a.id)) {
            let Some(load) = self.loads.get(&account.id) else { continue };
            for (i, pr) in load.prs.iter().enumerate() {
                if passes(pr, self.filter, &self.text) {
                    let local = load.local.get(i).and_then(Option::as_deref);
                    rows.push(Row { account, pr, local });
                }
            }
        }
        rows.sort_by_key(|row| std::cmp::Reverse(row.pr.updated_timestamp()));
        rows
    }

    pub fn ui(&mut self, ctx: &egui::Context, shared: &mut Shared, repos: &[FoundRepo], p: &Palette) {
        if !self.open {
            return;
        }
        let now = Instant::now();
        if self.refresh_due(&shared.settings.accounts, now) {
            self.refresh(shared, repos);
        }
        if let Some(last) = self.last_refresh {
            ctx.request_repaint_after(REFRESH_EVERY.saturating_sub(now.saturating_duration_since(last)));
        }
        let mut open = true;
        let mut refresh = false;
        widgets::centered_window(ctx, format!("{} {}", icons::GIT_PULL_REQUEST, tr!("Pull requests")))
            .id(egui::Id::unique("pull-requests-window"))
            .open(&mut open)
            .default_size([860.0, 620.0])
            .show(ctx, |ui| refresh = self.contents(ui, shared, p));
        if refresh {
            self.refresh(shared, repos);
        }
        if !open {
            self.open = false;
        }
    }

    /// Contenu de la fenêtre ; renvoie vrai si l'utilisateur demande une nouvelle lecture.
    fn contents(&mut self, ui: &mut egui::Ui, shared: &mut Shared, p: &Palette) -> bool {
        let accounts = shared.settings.accounts.clone();
        if accounts.is_empty() {
            ui.add_space(20.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new(tr!("Aucun compte connecté.")).color(p.weak));
                ui.label(
                    RichText::new(tr!("Ajoutez un compte GitHub, GitLab ou Azure DevOps pour voir ses pull requests."))
                        .color(p.weak),
                );
                ui.add_space(6.0);
                if ui.button(format!("{} {}", icons::PLUS, tr!("Ajouter un compte…"))).clicked() {
                    shared.actions.push(AppAction::ShowAccounts);
                }
            });
            return false;
        }
        if self.account_filter.as_ref().is_some_and(|id| !accounts.iter().any(|a| &a.id == id)) {
            self.account_filter = None;
        }
        let mut refresh = false;
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.filter, PrFilter::All, tr!("Toutes"));
            ui.selectable_value(&mut self.filter, PrFilter::Mine, tr!("Créées par moi"));
            ui.selectable_value(&mut self.filter, PrFilter::ToReview, tr!("À relire"));
            ui.separator();
            let label = self
                .account_filter
                .as_ref()
                .and_then(|id| accounts.iter().find(|a| &a.id == id))
                .map_or_else(|| tr!("Tous les comptes").to_string(), Account::title);
            egui::ComboBox::from_id_salt("pull-requests-account").selected_text(label).width(240.0).show_ui(ui, |ui| {
                ui.selectable_value(&mut self.account_filter, None, tr!("Tous les comptes"));
                for a in &accounts {
                    let title = format!("{} {}", kind_icon(a.kind), a.title());
                    ui.selectable_value(&mut self.account_filter, Some(a.id.clone()), title);
                }
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let busy = self.loading();
                let button = egui::Button::new(format!("{} {}", icons::ARROWS_CLOCKWISE, tr!("Actualiser")));
                refresh = ui.add_enabled(!busy, button).clicked();
                if busy {
                    ui.spinner();
                } else if let Some(at) = self.refreshed_at {
                    let when = widgets::format_time(at, widgets::now_ts());
                    ui.label(RichText::new(tr!("Mise à jour : {when}", when)).size(12.0).color(p.weak));
                }
            });
        });
        ui.add_space(2.0);
        widgets::search_field(ui, &mut self.text, tr!("Filtrer par titre, dépôt, auteur ou branche"), "pr-filter");

        // État de chaque compte : lecture en cours (avant toute réponse) ou erreur, à sa place.
        for account in accounts.iter().filter(|a| self.account_filter.as_ref().is_none_or(|id| id == &a.id)) {
            let Some(load) = self.loads.get(&account.id) else { continue };
            if let Some(error) = &load.error {
                let text =
                    format!("{} {}", icons::WARNING, tr!("{account} : {error}", account = account.title(), error));
                ui.add(egui::Label::new(RichText::new(text).color(p.removed_fg)).wrap());
            } else if load.loading && load.prs.is_empty() {
                ui.horizontal(|ui| {
                    ui.spinner();
                    let text = tr!("Chargement des pull requests de {account}…", account = account.title());
                    ui.label(RichText::new(text).color(p.weak));
                });
            }
        }

        let rows = self.rows(&accounts);
        ui.label(RichText::new(trn!(rows.len(), "{n} pull request", "{n} pull requests")).color(p.weak).size(12.0));
        if rows.is_empty() && !self.loading() {
            ui.add_space(16.0);
            ui.vertical_centered(|ui| {
                let filtered = self.filter != PrFilter::All || !self.text.trim().is_empty();
                let text = if filtered {
                    tr!("Aucune pull request ne correspond aux filtres.")
                } else {
                    tr!("Aucune pull request ouverte.")
                };
                ui.label(RichText::new(text).color(p.weak));
            });
        }
        let now = widgets::now_ts();
        let mut actions = Vec::new();
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            for row in &rows {
                row_ui(ui, row, now, p, &mut actions);
            }
        });
        for action in actions {
            match action {
                RowAction::OpenUrl(url) => crate::util::open_url(ui.ctx(), &url),
                RowAction::OpenRepo(path) => shared.actions.push(AppAction::OpenRepo(path)),
            }
        }
        refresh
    }
}

/// Pull request affichée, avec son compte et sa copie locale.
struct Row<'a> {
    account: &'a Account,
    pr: &'a PullRequest,
    local: Option<&'a Path>,
}

enum RowAction {
    OpenUrl(String),
    OpenRepo(PathBuf),
}

/// Une ligne : titre (ouvre la pull request), marques et date ; dépôt (ouvre sa page), numéro, auteur, branches et
/// « Ouvrir le dépôt » s'il est cloné.
fn row_ui(ui: &mut egui::Ui, row: &Row, now: i64, p: &Palette, actions: &mut Vec<RowAction>) {
    let pr = row.pr;
    egui::Frame::new().inner_margin(egui::Margin::symmetric(6, 4)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.label(RichText::new(kind_icon(pr.kind())).size(18.0).color(p.weak)).on_hover_text(row.account.title());
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if let Some(ts) = pr.updated_timestamp() {
                            let when = widgets::format_time(ts, now);
                            ui.label(RichText::new(when).size(11.5).color(p.weak))
                                .on_hover_text(tr!("Mise à jour : {when}", when = widgets::format_date(ts)));
                        }
                        if pr.review_requested {
                            badge(ui, tr!("Relecture demandée"), p.accent);
                        }
                        if pr.draft {
                            badge(ui, tr!("Brouillon"), p.weak);
                        }
                        ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                            let title =
                                egui::Label::new(RichText::new(&pr.title).strong()).truncate().sense(Sense::click());
                            let title = ui
                                .add(title)
                                .on_hover_cursor(egui::CursorIcon::PointingHand)
                                .on_hover_text(tr!("Ouvrir dans le navigateur : {url}", url = pr.url));
                            if title.clicked() {
                                actions.push(RowAction::OpenUrl(pr.url.clone()));
                            }
                        });
                    });
                });
                ui.horizontal(|ui| {
                    let repo = ui
                        .link(RichText::new(pr.repo.display_name()).size(12.0))
                        .on_hover_text(tr!("Page du dépôt sur {forge}", forge = pr.kind().label()));
                    if repo.clicked() {
                        actions.push(RowAction::OpenUrl(pr.repo.web_url()));
                    }
                    ui.label(RichText::new(details(pr)).size(12.0).color(p.weak));
                    if let Some(path) = row.local {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let open = ui
                                .small_button(format!("{} {}", icons::FOLDER_OPEN, tr!("Ouvrir le dépôt")))
                                .on_hover_text(path.display().to_string());
                            if open.clicked() {
                                actions.push(RowAction::OpenRepo(path.to_path_buf()));
                            }
                        });
                    }
                });
            });
        });
    });
    ui.separator();
}

/// Marque encadrée (brouillon, relecture demandée).
fn badge(ui: &mut egui::Ui, text: &str, color: egui::Color32) {
    egui::Frame::new()
        .stroke(egui::Stroke::new(1.0, color))
        .corner_radius(8)
        .inner_margin(egui::Margin::symmetric(6, 0))
        .show(ui, |ui| ui.label(RichText::new(text).size(11.0).color(color)));
}

/// `#12 · par jsmith · feature/x → main` : les éléments connus seulement.
fn details(pr: &PullRequest) -> String {
    let mut parts = vec![pr.reference()];
    if !pr.author.is_empty() {
        parts.push(tr!("par {author}", author = pr.author));
    }
    match (&pr.source_branch, &pr.target_branch) {
        (Some(source), Some(target)) => parts.push(format!("{source} → {target}")),
        (Some(branch), None) | (None, Some(branch)) => parts.push(branch.clone()),
        (None, None) => {}
    }
    parts.join(" · ")
}

/// La pull request passe le filtre principal et contient chaque mot de `text` (titre, dépôt, auteur, branches ; sans
/// casse ni accents).
fn passes(pr: &PullRequest, filter: PrFilter, text: &str) -> bool {
    let kept = match filter {
        PrFilter::All => true,
        PrFilter::Mine => pr.created_by_me,
        PrFilter::ToReview => pr.review_requested,
    };
    if !kept {
        return false;
    }
    let haystack = widgets::fold(&format!(
        "{} {} {} {} {} {}",
        pr.title,
        pr.repo.display_name(),
        pr.reference(),
        pr.author,
        pr.source_branch.as_deref().unwrap_or_default(),
        pr.target_branch.as_deref().unwrap_or_default()
    ));
    widgets::fold(text).split_whitespace().all(|word| haystack.contains(word))
}

/// Copies locales où chercher le dépôt d'une pull request, par préférence : les dépôts récents (ceux des onglets
/// compris), puis ceux de la liste (dépôts principaux avant worktrees et sous-modules), sans doublon.
fn local_candidates(recent: &[PathBuf], repos: &[FoundRepo]) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    let main_first =
        repos.iter().filter(|r| r.kind == RepoKind::Normal).chain(repos.iter().filter(|r| r.kind != RepoKind::Normal));
    recent.iter().chain(main_first.map(|r| &r.path)).filter(|path| seen.insert(path_key(path))).cloned().collect()
}

/// Copie locale de chaque pull request : la première dont un remote désigne son dépôt (même forge, hôte et noms, sans
/// la casse). Le compte de la pull request décide de la forge d'un hôte auto-hébergé.
fn link_local_clones(prs: &[PullRequest], account: Option<&Account>, clones: &[LocalClone]) -> Vec<Option<PathBuf>> {
    if prs.is_empty() {
        return Vec::new();
    }
    let parsed: Vec<(&Path, Vec<RepoLocation>)> = clones
        .iter()
        .map(|c| (c.path.as_path(), c.urls.iter().filter_map(|url| RepoLocation::parse(url, account)).collect()))
        .collect();
    prs.iter()
        .map(|pr| {
            parsed
                .iter()
                .find(|(_, locations)| locations.iter().any(|l| l.same_repo(&pr.repo)))
                .map(|(path, _)| path.to_path_buf())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use dragontree_forges::ForgeKind;

    use super::*;

    fn account(id: &str, kind: ForgeKind, host: &str) -> Account {
        Account {
            id: id.into(),
            kind,
            host: host.into(),
            username: "jsmith".into(),
            display_name: String::new(),
            orgs: vec![],
            avatar_url: None,
        }
    }

    fn pr(title: &str, repo: RepoLocation, updated_at: &str) -> PullRequest {
        PullRequest {
            account_id: String::new(),
            url: format!("https://forge.test/{title}"),
            repo,
            number: 7,
            title: title.into(),
            author: "asmith".into(),
            draft: false,
            reviewers: vec![],
            created_by_me: false,
            review_requested: false,
            updated_at: Some(updated_at.into()),
            source_branch: Some("feature/accueil".into()),
            target_branch: Some("main".into()),
        }
    }

    fn github(owner: &str, repo: &str) -> RepoLocation {
        RepoLocation::GitHub { host: "github.com".into(), owner: owner.into(), repo: repo.into() }
    }

    fn clone(path: &str, urls: &[&str]) -> LocalClone {
        LocalClone { path: PathBuf::from(path), urls: urls.iter().map(|u| u.to_string()).collect() }
    }

    #[test]
    fn a_pull_request_is_linked_to_the_local_clone_of_its_repository() {
        let prs = [
            pr("a", github("contoso", "northwind"), "2026-10-20T00:00:00Z"),
            pr("b", github("contoso", "absent"), "2026-10-20T00:00:00Z"),
            pr(
                "c",
                RepoLocation::GitLab { host: "git.contoso.test".into(), project: "equipe/outils".into() },
                "2026-10-20T00:00:00Z",
            ),
        ];
        let clones = [
            clone("C:/depots/autre", &["https://github.com/fabrikam/northwind.git"]),
            clone("C:/depots/northwind", &["git@github.com:Contoso/Northwind.git"]),
            clone("C:/depots/outils", &["https://git.contoso.test/Equipe/outils.git"]),
        ];
        let gh = account("gh", ForgeKind::GitHub, "github.com");
        assert_eq!(
            link_local_clones(&prs, Some(&gh), &clones),
            [Some(PathBuf::from("C:/depots/northwind")), None, None]
        );
        // Hôte auto-hébergé sans « gitlab » dans le nom : reconnu grâce au compte GitLab de la pull request.
        let gl = account("gl", ForgeKind::GitLab, "git.contoso.test");
        assert_eq!(link_local_clones(&prs[2..], Some(&gl), &clones), [Some(PathBuf::from("C:/depots/outils"))]);
        assert!(link_local_clones(&[], Some(&gl), &clones).is_empty());
    }

    #[test]
    fn local_candidates_prefer_recent_repositories_then_main_ones() {
        let found = |path: &str, kind| FoundRepo {
            path: PathBuf::from(path),
            name: String::new(),
            root: PathBuf::from("C:/depots"),
            kind,
        };
        let repos = [
            found("C:/depots/wt", RepoKind::Worktree),
            found("C:/depots/a", RepoKind::Normal),
            found("C:/depots/B", RepoKind::Normal),
        ];
        let recent = [PathBuf::from("C:/depots/b"), PathBuf::from("C:/ailleurs/c")];
        let candidates = local_candidates(&recent, &repos);
        let shown: Vec<String> = candidates.iter().map(|p| p.display().to_string()).collect();
        assert_eq!(shown, ["C:/depots/b", "C:/ailleurs/c", "C:/depots/a", "C:/depots/wt"]);
    }

    #[test]
    fn filters_keep_mine_or_to_review_and_every_word_of_the_text() {
        let mut mine = pr("Ajoute la page d'accueil", github("contoso", "northwind"), "2026-10-20T00:00:00Z");
        mine.created_by_me = true;
        let mut review = pr("Corrige les remises", github("fabrikam", "billing"), "2026-10-20T00:00:00Z");
        review.review_requested = true;
        assert!(passes(&mine, PrFilter::All, "") && passes(&review, PrFilter::All, ""));
        assert!(passes(&mine, PrFilter::Mine, "") && !passes(&review, PrFilter::Mine, ""));
        assert!(!passes(&mine, PrFilter::ToReview, "") && passes(&review, PrFilter::ToReview, ""));
        assert!(passes(&mine, PrFilter::All, "ACCUEIL northwind"), "sans casse, chaque mot");
        assert!(passes(&review, PrFilter::All, "remisés"), "sans accents");
        assert!(passes(&review, PrFilter::All, "asmith feature/accueil #7"), "auteur, branche, numéro");
        assert!(!passes(&mine, PrFilter::All, "accueil billing"));
    }

    #[test]
    fn details_show_the_known_parts_only() {
        let mut p = pr("t", github("contoso", "northwind"), "2026-10-20T00:00:00Z");
        assert_eq!(details(&p), "#7 · par asmith · feature/accueil → main");
        p.author.clear();
        p.source_branch = None;
        assert_eq!(details(&p), "#7 · main");
    }

    fn window_with(loads: Vec<(&str, Vec<PullRequest>)>) -> PullRequestsWindow {
        let mut w = PullRequestsWindow::new(|_| Ok(Vec::new()));
        for (id, prs) in loads {
            w.loads.insert(id.into(), AccountLoad { prs, ..AccountLoad::default() });
        }
        w
    }

    #[test]
    fn rows_of_every_account_are_sorted_by_update_date_and_filtered_by_account() {
        let accounts = [account("gh", ForgeKind::GitHub, "github.com"), account("gl", ForgeKind::GitLab, "gitlab.com")];
        let mut w = window_with(vec![
            ("gh", vec![pr("ancienne", github("contoso", "a"), "2026-10-01T00:00:00Z")]),
            ("gl", vec![pr("récente", github("contoso", "b"), "2026-10-20T00:00:00Z")]),
            ("supprimé", vec![pr("orpheline", github("contoso", "c"), "2026-10-30T00:00:00Z")]),
        ]);
        let titles = |w: &PullRequestsWindow| w.rows(&accounts).iter().map(|r| r.pr.title.clone()).collect::<Vec<_>>();
        assert_eq!(titles(&w), ["récente", "ancienne"], "un compte supprimé n'est plus affiché");
        w.account_filter = Some("gh".into());
        assert_eq!(titles(&w), ["ancienne"]);
    }

    #[test]
    fn answers_of_an_older_refresh_are_ignored() {
        let accounts = [account("gh", ForgeKind::GitHub, "github.com")];
        let mut w = window_with(vec![("gh", vec![])]);
        w.generation = 2;
        w.loads.get_mut("gh").unwrap().loading = true;
        let answer = |generation, title: &str| PullRequestEvent::Loaded {
            generation,
            account_id: "gh".into(),
            result: Ok(vec![pr(title, github("contoso", "a"), "2026-10-01T00:00:00Z")]),
        };
        w.handle(answer(1, "périmée"), &accounts);
        assert!(w.loads["gh"].prs.is_empty() && w.loading(), "réponse d'un rafraîchissement précédent ignorée");
        w.handle(answer(2, "à jour"), &accounts);
        assert_eq!(w.loads["gh"].prs[0].title, "à jour");
        assert!(!w.loading());
        // Une erreur remplace la liste du compte ; les copies locales d'une indexation périmée sont ignorées aussi.
        w.handle(
            PullRequestEvent::Loaded { generation: 2, account_id: "gh".into(), result: Err("refusé".into()) },
            &accounts,
        );
        assert!(w.loads["gh"].prs.is_empty());
        assert_eq!(w.loads["gh"].error.as_deref(), Some("refusé"));
        w.clones_generation = 3;
        w.handle(PullRequestEvent::LocalClones { generation: 2, clones: vec![clone("C:/x", &["u"])] }, &accounts);
        assert!(w.clones.is_empty());
    }

    #[test]
    fn each_account_is_read_in_the_background_and_a_panic_becomes_its_error() {
        let dir = tempfile::tempdir().unwrap();
        let (store, mut settings, _) = crate::settings::SettingsFile::load(&dir.path().join("settings.json"));
        settings.accounts = vec![account("gh", ForgeKind::GitHub, "github.com")];
        let (shared, events) = Shared::new(egui::Context::default(), settings, store, None, None);
        let mut w = PullRequestsWindow::new(|_| panic!("lecture en échec"));
        w.refresh(&shared, &[]);
        assert!(w.loading());
        let event = events.recv_timeout(Duration::from_secs(10)).expect("réponse de la tâche de fond");
        let AppEvent::PullRequests(event) = event else { panic!("événement inattendu") };
        w.handle(event, &shared.settings.accounts);
        assert!(!w.loading(), "le compte n'est plus « en cours »");
        let error = w.loads["gh"].error.clone().unwrap();
        assert!(error.contains("lecture en échec"), "{error}");
    }

    #[test]
    fn a_refresh_is_due_on_opening_for_a_new_account_and_every_five_minutes() {
        let accounts = [account("gh", ForgeKind::GitHub, "github.com")];
        let start = Instant::now();
        let mut w = window_with(vec![("gh", vec![])]);
        assert!(w.refresh_due(&accounts, start), "jamais rafraîchie");
        w.last_refresh = Some(start);
        assert!(!w.refresh_due(&accounts, start + Duration::from_secs(60)));
        assert!(w.refresh_due(&accounts, start + REFRESH_EVERY));
        w.loads.get_mut("gh").unwrap().loading = true;
        assert!(!w.refresh_due(&accounts, start + REFRESH_EVERY), "pas de nouvelle lecture pendant la précédente");
        let two = [accounts[0].clone(), account("gl", ForgeKind::GitLab, "gitlab.com")];
        assert!(w.refresh_due(&two, start + Duration::from_secs(1)), "compte ajouté depuis la dernière lecture");
        w.show();
        assert!(w.open && w.last_refresh.is_none(), "rouverte : relue à la prochaine image");
    }
}
