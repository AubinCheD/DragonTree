//! Fenêtre « Pull requests » (Fichier › Pull requests…) : les pull requests ouvertes qui concernent l'utilisateur, sur
//! tous ses comptes, de la plus récemment mise à jour à la plus ancienne. Lecture en tâche de fond à l'ouverture, toutes
//! les [`REFRESH_EVERY`] tant que la fenêtre reste ouverte et sur demande ; l'erreur d'un compte s'affiche à sa place
//! sans empêcher les autres, comme une organisation Azure DevOps qui n'a pas répondu alors qu'une autre l'a fait, et un
//! compte Azure DevOps sans organisation est expliqué au lieu d'être lu. Filtres : créées par moi, à relire, par compte,
//! texte. Une pull request s'ouvre dans le navigateur, et son dépôt dans un onglet quand une copie locale le désigne
//! (remotes lus sur disque, en tâche de fond).
//!
//! Un hôte qui pilote DragonTree à distance (le téléphone de la suite) les fait relire sans fenêtre
//! ([`PullRequestsWindow::refresh_in_background`]) et lit ce qui l'a été ([`PullRequestsSnapshot`]) : la même lecture
//! que la fenêtre, ouverte ou non.
//!
//! Les jetons ne sont lus que dans les tâches de fond (Gestionnaire d'identifiants) et n'apparaissent jamais dans les
//! erreurs affichées.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use dragontree_forges::{Account, ForgeError, PullRequest, PullRequestList, RepoLocation, secrets};
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

/// Délai minimal avant l'image suivante demandée par la fenêtre : quand [`REFRESH_EVERY`] est écoulé pendant une
/// lecture encore en cours, le délai restant serait nul à chaque image.
const MIN_REPAINT_DELAY: Duration = Duration::from_secs(1);

/// Lecture des pull requests d'un compte, sur un thread de fond. Les tests d'interface en passent une qui répond sans
/// réseau ni Gestionnaire d'identifiants (`StartOptions::pull_request_source`).
pub type PullRequestSource = fn(&Account) -> Result<PullRequestList, ForgeError>;

/// Lecture réelle : jeton du compte dans le Gestionnaire d'identifiants, puis l'API de sa forge.
pub fn fetch_from_forge(account: &Account) -> Result<PullRequestList, ForgeError> {
    match secrets::load_token(&account.id) {
        Ok(Some(token)) => dragontree_forges::list_pull_requests(account, &token),
        Ok(None) => Err(ForgeError::Secret(tr!("jeton absent du Gestionnaire d'identifiants").into())),
        Err(e) => Err(e),
    }
}

pub enum PullRequestEvent {
    /// Pull requests d'un compte (ou l'erreur affichée), pour le rafraîchissement `generation`.
    Loaded { generation: u64, account_id: String, result: Result<PullRequestList, String> },
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
    /// Lecture partielle : organisations Azure DevOps en échec (nom, erreur affichée) alors qu'une autre a répondu.
    failed_orgs: Vec<(String, String)>,
    /// Copies locales du dépôt de chaque pull request (même ordre que `prs`), la préférée d'abord.
    local: Vec<Vec<PathBuf>>,
}

/// Les pull requests telles que DragonTree les a lues, pour un hôte (voir `DragonTreeApp::pull_requests`).
#[derive(Debug, Clone, Default)]
pub struct PullRequestsSnapshot {
    /// Chaque compte des réglages, dans leur ordre, et où en est sa lecture.
    pub accounts: Vec<AccountPullRequests>,
    /// Les pull requests de tous les comptes, de la plus récemment mise à jour à la plus ancienne.
    pub prs: Vec<ListedPullRequest>,
    /// Une lecture est en cours.
    pub loading: bool,
    /// Le dernier rafraîchissement lancé (fenêtre ou hôte), et son heure en secondes Unix ; `None` : jamais lues.
    pub refreshed: Option<Instant>,
    pub refreshed_at: Option<i64>,
}

/// Où en est la lecture des pull requests d'un compte.
#[derive(Debug, Clone)]
pub struct AccountPullRequests {
    pub account: Account,
    /// Lecture demandée au moins une fois (sinon : compte ajouté depuis le dernier rafraîchissement).
    pub read: bool,
    pub loading: bool,
    /// Pourquoi ses pull requests manquent (jeton refusé, compte Azure DevOps sans organisation…).
    pub error: Option<String>,
    /// Organisations Azure DevOps qui n'ont pas répondu alors qu'une autre l'a fait (nom, erreur).
    pub failed_orgs: Vec<(String, String)>,
}

/// Une pull request lue, avec son compte et ses copies locales.
#[derive(Debug, Clone)]
pub struct ListedPullRequest {
    /// Identifiant du compte qui l'a lue.
    pub account: String,
    pub pr: PullRequest,
    /// Les copies locales de son dépôt (dépôts récents d'abord, puis ceux de la liste) ; vide : pas clonée.
    pub local: Vec<PathBuf>,
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
    /// Un hôte les a fait relire (`refresh_in_background`) : les copies locales sont relues même fenêtre fermée.
    hosted: bool,
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
            hosted: false,
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

    /// Délai avant l'image suivante : le prochain rafraîchissement automatique, au moins [`MIN_REPAINT_DELAY`] (une
    /// lecture plus longue que [`REFRESH_EVERY`] ne fait pas redessiner la fenêtre à chaque image ; sa réponse
    /// demande elle-même une image). `None` avant la première lecture.
    fn repaint_delay(&self, now: Instant) -> Option<Duration> {
        let last = self.last_refresh?;
        Some(REFRESH_EVERY.saturating_sub(now.saturating_duration_since(last)).max(MIN_REPAINT_DELAY))
    }

    /// Relit les pull requests de chaque compte (une tâche par compte) et les copies locales. Un compte Azure DevOps
    /// sans organisation n'est pas lu : l'explication tient lieu d'erreur.
    fn refresh(&mut self, shared: &Shared, repos: &[FoundRepo]) {
        self.generation += 1;
        self.last_refresh = Some(Instant::now());
        self.refreshed_at = Some(widgets::now_ts());
        let accounts = &shared.settings.accounts;
        self.loads.retain(|id, _| accounts.iter().any(|a| &a.id == id));
        for account in accounts {
            let load = self.loads.entry(account.id.clone()).or_default();
            if account.lacks_azure_orgs() {
                *load = AccountLoad { error: Some(no_azure_orgs_explanation()), ..AccountLoad::default() };
                continue;
            }
            load.loading = true;
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

    /// Relève, pour un hôte, les pull requests de chaque compte et les copies locales, comme à l'ouverture de la
    /// fenêtre, qui peut rester fermée (ouverte, elle montre la même lecture) ; faux si une lecture est déjà en cours :
    /// ses réponses arrivent. Les copies locales sont ensuite relues aussi à la fin de chaque scan, fenêtre fermée.
    pub fn refresh_in_background(&mut self, shared: &Shared, repos: &[FoundRepo]) -> bool {
        if self.loading() {
            return false;
        }
        self.hosted = true;
        self.refresh(shared, repos);
        true
    }

    /// Ce qui a été lu, pour un hôte ; `accounts` : comptes des réglages. Sans filtre : toutes les pull requests des
    /// comptes, de la plus récemment mise à jour à la plus ancienne.
    pub fn snapshot(&self, accounts: &[Account]) -> PullRequestsSnapshot {
        let mut prs = Vec::new();
        let mut states = Vec::new();
        for account in accounts {
            let load = self.loads.get(&account.id);
            states.push(AccountPullRequests {
                account: account.clone(),
                read: load.is_some(),
                loading: load.is_some_and(|l| l.loading),
                error: load.and_then(|l| l.error.clone()),
                failed_orgs: load.map(|l| l.failed_orgs.clone()).unwrap_or_default(),
            });
            for (i, pr) in load.map(|l| l.prs.as_slice()).unwrap_or_default().iter().enumerate() {
                let local = load.and_then(|l| l.local.get(i)).cloned().unwrap_or_default();
                prs.push(ListedPullRequest { account: account.id.clone(), pr: pr.clone(), local });
            }
        }
        prs.sort_by_key(|p| std::cmp::Reverse(p.pr.updated_timestamp()));
        PullRequestsSnapshot {
            accounts: states,
            prs,
            loading: self.loading(),
            refreshed: self.last_refresh,
            refreshed_at: self.refreshed_at,
        }
    }

    /// Relit les remotes des copies locales (dépôts récents, puis ceux de la liste) en tâche de fond. Sans effet
    /// quand la fenêtre est fermée, sauf pour un hôte ; appelée aussi à la fin d'un scan de la liste des dépôts.
    pub fn index_local_clones(&mut self, shared: &Shared, repos: &[FoundRepo]) {
        if !(self.open || self.hosted) || shared.settings.accounts.is_empty() {
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
                    Ok(list) => {
                        load.prs = list.prs;
                        load.failed_orgs = list.failed_orgs.into_iter().map(|(org, e)| (org, e.to_string())).collect();
                        load.error = None;
                    }
                    Err(e) => {
                        load.prs.clear();
                        load.failed_orgs.clear();
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
                    let local = load.local.get(i).and_then(|l| l.first()).map(PathBuf::as_path);
                    rows.push(Row { account, pr, local });
                }
            }
        }
        rows.sort_by_key(|row| std::cmp::Reverse(row.pr.updated_timestamp()));
        rows
    }

    /// Message de la liste vide : aucun pendant une lecture, ni quand chaque compte affiché est en erreur (les erreurs,
    /// affichées au-dessus, disent déjà pourquoi la liste est vide).
    fn empty_message(&self, accounts: &[Account]) -> Option<&'static str> {
        if self.loading() {
            return None;
        }
        let mut shown = accounts.iter().filter(|a| self.account_filter.as_ref().is_none_or(|id| id == &a.id));
        if shown.all(|a| self.loads.get(&a.id).is_none_or(|load| load.error.is_some())) {
            return None;
        }
        let filtered = self.filter != PrFilter::All || !self.text.trim().is_empty();
        Some(if filtered {
            tr!("Aucune pull request ne correspond aux filtres.")
        } else {
            tr!("Aucune pull request ouverte.")
        })
    }

    pub fn ui(&mut self, ctx: &egui::Context, shared: &mut Shared, repos: &[FoundRepo], p: &Palette) {
        if !self.open {
            return;
        }
        let now = Instant::now();
        if self.refresh_due(&shared.settings.accounts, now) {
            self.refresh(shared, repos);
        }
        if let Some(delay) = self.repaint_delay(now) {
            ctx.request_repaint_after(delay);
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

        // État de chaque compte : lecture en cours (avant toute réponse), erreur ou organisations en échec, à sa place.
        for account in accounts.iter().filter(|a| self.account_filter.as_ref().is_none_or(|id| id == &a.id)) {
            let Some(load) = self.loads.get(&account.id) else { continue };
            for (org, error) in &load.failed_orgs {
                let text = tr!(
                    "{account} : l'organisation {org} n'a pas répondu, ses pull requests manquent : {error}",
                    account = account.title(),
                    org,
                    error
                );
                let text = format!("{} {text}", icons::WARNING);
                ui.add(egui::Label::new(RichText::new(text).color(p.modified_fg)).wrap());
            }
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
        if let Some(text) = rows.is_empty().then(|| self.empty_message(&accounts)).flatten() {
            ui.add_space(16.0);
            ui.vertical_centered(|ui| ui.label(RichText::new(text).color(p.weak)));
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
                RowAction::OpenUrl(url) => crate::util::open_forge_url(ui.ctx(), &url),
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

/// Explication affichée à la place des pull requests d'un compte Azure DevOps sans organisation.
fn no_azure_orgs_explanation() -> String {
    tr!("aucune organisation connue pour ce compte. « Vérifier » (Fichier › Comptes…) les retrouve si le jeton le \
         permet ; sinon, supprimez le compte et ajoutez-le de nouveau en indiquant ses organisations.")
    .to_string()
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

/// Copies locales de chaque pull request : celles dont un remote désigne son dépôt (même forge, hôte et noms, sans la
/// casse), dans l'ordre de `clones` ; la fenêtre ouvre la première. Le compte de la pull request décide de la forge d'un
/// hôte auto-hébergé.
fn link_local_clones(prs: &[PullRequest], account: Option<&Account>, clones: &[LocalClone]) -> Vec<Vec<PathBuf>> {
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
                .filter(|(_, locations)| locations.iter().any(|l| l.same_repo(&pr.repo)))
                .map(|(path, _)| path.to_path_buf())
                .collect()
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
            clone("C:/depots/northwind-2", &["https://github.com/contoso/northwind"]),
        ];
        let gh = account("gh", ForgeKind::GitHub, "github.com");
        let northwind = vec![PathBuf::from("C:/depots/northwind"), PathBuf::from("C:/depots/northwind-2")];
        assert_eq!(link_local_clones(&prs, Some(&gh), &clones), [northwind, vec![], vec![]], "chaque copie, en ordre");
        // Hôte auto-hébergé sans « gitlab » dans le nom : reconnu grâce au compte GitLab de la pull request.
        let gl = account("gl", ForgeKind::GitLab, "git.contoso.test");
        assert_eq!(link_local_clones(&prs[2..], Some(&gl), &clones), [vec![PathBuf::from("C:/depots/outils")]]);
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
        let mut w = PullRequestsWindow::new(|_| Ok(PullRequestList::default()));
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
            result: Ok(PullRequestList {
                prs: vec![pr(title, github("contoso", "a"), "2026-10-01T00:00:00Z")],
                failed_orgs: vec![],
            }),
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

    #[test]
    fn the_repaint_delay_stays_at_least_a_second_while_a_read_outlasts_the_interval() {
        let start = Instant::now();
        let mut w = window_with(vec![("gh", vec![])]);
        assert_eq!(w.repaint_delay(start), None, "jamais rafraîchie");
        w.last_refresh = Some(start);
        assert_eq!(w.repaint_delay(start + Duration::from_secs(60)), Some(REFRESH_EVERY - Duration::from_secs(60)));
        // Lecture plus longue que l'intervalle : pas de nouvelle lecture, et pas une image après l'autre.
        w.loads.get_mut("gh").unwrap().loading = true;
        let late = start + REFRESH_EVERY + Duration::from_secs(30);
        assert!(!w.refresh_due(&[account("gh", ForgeKind::GitHub, "github.com")], late));
        assert_eq!(w.repaint_delay(late), Some(MIN_REPAINT_DELAY));
        assert_eq!(MIN_REPAINT_DELAY, Duration::from_secs(1));
    }

    #[test]
    fn an_azure_organisation_that_failed_while_another_answered_is_kept_as_a_warning() {
        let accounts = [account("az", ForgeKind::AzureDevOps, "dev.azure.com")];
        let mut w = window_with(vec![("az", vec![])]);
        w.generation = 1;
        let partial = PullRequestList {
            prs: vec![pr("à relire", github("contoso", "a"), "2026-10-01T00:00:00Z")],
            failed_orgs: vec![("contoso".into(), ForgeError::Unauthorized)],
        };
        w.handle(PullRequestEvent::Loaded { generation: 1, account_id: "az".into(), result: Ok(partial) }, &accounts);
        let load = &w.loads["az"];
        assert_eq!(load.prs.len(), 1);
        assert!(load.error.is_none());
        assert_eq!(load.failed_orgs, [("contoso".to_string(), ForgeError::Unauthorized.to_string())]);
        // La lecture suivante remplace les avertissements, une erreur aussi.
        w.handle(
            PullRequestEvent::Loaded { generation: 1, account_id: "az".into(), result: Err("refusé".into()) },
            &accounts,
        );
        assert!(w.loads["az"].failed_orgs.is_empty());
    }

    #[test]
    fn an_azure_account_without_organisation_is_explained_instead_of_read() {
        let dir = tempfile::tempdir().unwrap();
        let (store, mut settings, _) = crate::settings::SettingsFile::load(&dir.path().join("settings.json"));
        let mut azure = account("az", ForgeKind::AzureDevOps, "dev.azure.com");
        azure.orgs = vec![" ".into()];
        settings.accounts = vec![azure];
        let (shared, _events) = Shared::new(egui::Context::default(), settings, store, None, None);
        let mut w = PullRequestsWindow::new(|_| panic!("un compte sans organisation n'est pas lu"));
        w.refresh(&shared, &[]);
        assert!(!w.loading(), "aucune lecture lancée");
        assert_eq!(w.loads["az"].error.as_deref(), Some(no_azure_orgs_explanation().as_str()));
        assert_eq!(w.empty_message(&shared.settings.accounts), None, "l'explication, pas « Aucune pull request »");
    }

    #[test]
    fn the_empty_message_is_not_shown_when_every_shown_account_failed() {
        let accounts = [account("gh", ForgeKind::GitHub, "github.com"), account("gl", ForgeKind::GitLab, "gitlab.com")];
        let mut w = window_with(vec![("gh", vec![]), ("gl", vec![])]);
        for id in ["gh", "gl"] {
            w.loads.get_mut(id).unwrap().error = Some("refusé".into());
        }
        assert_eq!(w.empty_message(&accounts), None, "tous les comptes en erreur");
        w.loads.get_mut("gl").unwrap().error = None;
        assert_eq!(w.empty_message(&accounts), Some("Aucune pull request ouverte."), "un compte a répondu, sans rien");
        w.text = "remises".into();
        assert_eq!(w.empty_message(&accounts), Some("Aucune pull request ne correspond aux filtres."));
        w.account_filter = Some("gh".into());
        assert_eq!(w.empty_message(&accounts), None, "le seul compte affiché est en erreur");
        w.account_filter = None;
        w.loads.get_mut("gl").unwrap().loading = true;
        assert_eq!(w.empty_message(&accounts), None, "lecture en cours");
    }
}
