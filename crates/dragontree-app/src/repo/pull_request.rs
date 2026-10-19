//! Créer une pull request : Dépôt › Créer une pull request… et menu d'une branche locale. Le dialogue demande la
//! branche cible, le titre (sujet du dernier commit par défaut), la description, le brouillon et des relecteurs. Avec
//! un compte qui correspond au remote, la pull request est créée par l'API de la forge (le jeton est lu en tâche de
//! fond) et le dialogue en donne le lien ; sinon, ou si l'API échoue, il ouvre la page de création de la forge dans
//! le navigateur. Une branche sans branche suivie (ou en avance) est d'abord poussée si la case est cochée.

use dragontree_forges::{Account, CreatedPullRequest, ForgeError, PullRequestDraft, RepoLocation};
use dragontree_i18n::{tr, trn};
use egui::{RichText, Ui};

use super::dialogs::warning;
use super::{Dialog, RepoEvent, RepoTab};
use crate::app::{AppEvent, Shared};
use crate::theme::Palette;
use crate::toasts::Toast;

/// Où en est la création.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrStage {
    /// Formulaire en cours de saisie.
    Edit,
    /// La branche est poussée avant la création.
    Pushing,
    /// La requête part à la forge.
    Creating,
    Done(CreatedPullRequest),
    /// L'API a refusé ou n'a pas répondu : le message, et le navigateur en repli.
    Failed(String),
}

/// La branche source n'est pas à jour sur le remote.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushNeed {
    /// Rien à pousser.
    No,
    /// Pas de branche suivie (ou disparue) : le push la crée.
    NoUpstream,
    /// Des commits sont en avance sur la branche suivie.
    Ahead(u32),
}

/// Formulaire du dialogue (voir `Dialog::PullRequest`).
#[derive(Debug, Clone)]
pub struct PullRequestForm {
    pub source: String,
    /// Nom de la branche sur le remote (celui de sa branche suivie, sinon le nom local) : tête de la pull request.
    pub head: String,
    /// Numéro de ce dialogue : les réponses asynchrones d'un autre dialogue (fermé, rouvert) sont ignorées.
    pub generation: u64,
    pub target: String,
    /// La cible a été choisie à la main : la branche par défaut lue ensuite ne la remplace pas.
    pub target_touched: bool,
    /// Branches du remote proposées comme cible.
    pub targets: Vec<String>,
    pub title: String,
    pub description: String,
    pub draft: bool,
    pub reviewers: String,
    pub remote: String,
    pub location: Option<RepoLocation>,
    /// Compte qui crée la pull request par l'API ; `None` : page web seulement.
    pub account: Option<Account>,
    pub push_need: PushNeed,
    pub push_first: bool,
    pub stage: PrStage,
}

/// Ce que fait le bouton principal du dialogue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PrAction {
    /// Créer par l'API (après le push éventuel).
    Create,
    /// Ouvrir la page de création de la forge.
    OpenWeb,
    /// Ouvrir la pull request créée.
    OpenCreated,
    /// Rien à faire (travail en cours, ou forge non reconnue).
    None,
}

impl PullRequestForm {
    fn action(&self) -> PrAction {
        match (&self.stage, &self.location, &self.account) {
            (PrStage::Done(_), ..) => PrAction::OpenCreated,
            (PrStage::Edit | PrStage::Failed(_), Some(_), Some(_)) => PrAction::Create,
            (PrStage::Edit | PrStage::Failed(_), Some(_), None) => PrAction::OpenWeb,
            _ => PrAction::None,
        }
    }

    fn busy(&self) -> bool {
        matches!(self.stage, PrStage::Pushing | PrStage::Creating)
    }

    /// Page de création de la forge pour ces branches.
    fn web_url(&self) -> Option<String> {
        self.location.as_ref().map(|l| l.web_create_url(&self.head, &self.target))
    }

    fn reviewer_list(&self) -> Vec<String> {
        self.reviewers.split([',', ';', ' ']).map(str::trim).filter(|r| !r.is_empty()).map(str::to_string).collect()
    }

    fn draft_request(&self) -> PullRequestDraft {
        PullRequestDraft {
            source_branch: self.head.clone(),
            target_branch: self.target.clone(),
            title: self.title.trim().to_string(),
            description: self.description.trim().to_string(),
            draft: self.draft,
            reviewers: self.reviewer_list(),
        }
    }

    /// Titre et libellé du bouton principal du dialogue.
    pub(super) fn title_and_ok(&self) -> (String, String) {
        let ok = match self.action() {
            PrAction::Create | PrAction::None => tr!("Créer la pull request"),
            PrAction::OpenWeb => tr!("Ouvrir dans le navigateur"),
            PrAction::OpenCreated => tr!("Ouvrir la pull request"),
        };
        (tr!("Créer une pull request").into(), ok.into())
    }
}

impl RepoTab {
    /// Ouvre le dialogue pour `source` (la branche courante par défaut).
    pub(super) fn open_pull_request(&mut self, shared: &Shared, source: Option<String>) {
        let Some(refs) = self.refs.clone() else { return };
        let Some(source) = source.or_else(|| refs.current_branch().map(str::to_string)) else {
            return;
        };
        let Some(branch) = refs.local(&source) else { return };
        let Some((remote, head)) = push_target(&refs, branch, self.default_remote()) else { return };
        let url = self.remotes.iter().find(|r| r.name == remote).map(|r| r.fetch_url.clone()).unwrap_or_default();
        let account = self.pr_account(shared, &url);
        let location = RepoLocation::parse(&url, account.as_ref());
        // Un compte qui ne désigne pas ce dépôt n'est pas utilisé.
        let account = account.filter(|_| location.is_some());
        let targets: Vec<String> = refs
            .remotes
            .iter()
            .filter(|r| r.remote == remote && !r.is_symbolic && r.name != head)
            .map(|r| r.name.clone())
            .collect();
        let target = ["main", "master", "develop"]
            .into_iter()
            .find(|c| targets.iter().any(|t| t == c))
            .map(str::to_string)
            .or_else(|| targets.first().cloned())
            .unwrap_or_default();
        let title = self
            .history
            .index
            .get(&branch.oid)
            .and_then(|i| self.history.commits.get(*i))
            .map(|c| c.summary.to_string())
            .unwrap_or_else(|| source.clone());
        let push_need = match (&branch.upstream, branch.upstream_gone, branch.ahead) {
            (None, ..) | (_, true, _) => PushNeed::NoUpstream,
            (Some(_), false, ahead) if ahead > 0 => PushNeed::Ahead(ahead),
            _ => PushNeed::No,
        };
        self.pr_generation += 1;
        let generation = self.pr_generation;
        self.dialog = Some(Dialog::PullRequest(Box::new(PullRequestForm {
            source,
            head,
            generation,
            target,
            target_touched: false,
            targets,
            title,
            description: String::new(),
            draft: false,
            reviewers: String::new(),
            remote,
            location,
            account,
            push_need,
            push_first: push_need != PushNeed::No,
            stage: PrStage::Edit,
        })));
        // La branche par défaut du remote (`<remote>/HEAD`) demande git : lue en tâche de fond.
        if let Some(repo) = self.repo.clone() {
            let id = self.id;
            shared.jobs.spawn("pr-default-branch", move || {
                let default = repo.refs().ok().and_then(|refs| repo.default_branch(&refs));
                Some(AppEvent::Repo(id, RepoEvent::PullRequestDefault(generation, default)))
            });
        }
    }

    /// Compte du dépôt si l'URL du remote est la sienne, sinon un compte enregistré qui correspond à cette URL.
    fn pr_account(&self, shared: &Shared, url: &str) -> Option<Account> {
        self.net_account(shared).filter(|a| dragontree_forges::account_matches_url(a, url)).or_else(|| {
            shared.settings.accounts.iter().find(|a| dragontree_forges::account_matches_url(a, url)).cloned()
        })
    }

    /// La branche par défaut du remote est lue : elle devient la cible si l'utilisateur n'en a pas choisi.
    pub(super) fn on_pull_request_default(&mut self, generation: u64, default: Option<String>) {
        let Some(Dialog::PullRequest(form)) = &mut self.dialog else { return };
        if form.generation != generation {
            return;
        }
        if let Some(default) = default.filter(|d| form.targets.contains(d))
            && !form.target_touched
        {
            form.target = default;
        }
    }

    /// Champs du dialogue ; renvoie vrai si on peut valider.
    pub(super) fn pull_request_body(ui: &mut Ui, form: &mut PullRequestForm, p: &Palette) -> bool {
        let editable = !form.busy() && !matches!(form.stage, PrStage::Done(_));
        match &form.location {
            Some(location) => {
                ui.label(RichText::new(tr!("Dépôt : {name}", name = location.display_name())).color(p.weak));
            }
            None => {
                warning(ui, tr!("Forge non reconnue : l'URL du remote n'est ni GitHub, ni GitLab, ni Azure DevOps."), p)
            }
        }
        ui.add_enabled_ui(editable, |ui| {
            egui::Grid::new("pr-grid").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                ui.label(RichText::new(tr!("Branche source")).color(p.weak));
                ui.label(RichText::new(&form.source).strong());
                if form.head != form.source {
                    ui.label(RichText::new(tr!("(sur le remote : {name})", name = &form.head)).color(p.weak));
                }
                ui.end_row();
                ui.label(RichText::new(tr!("Branche cible")).color(p.weak));
                egui::ComboBox::from_id_salt("pr-target").selected_text(form.target.as_str()).show_ui(ui, |ui| {
                    for t in form.targets.clone() {
                        if ui.selectable_label(form.target == t, &t).clicked() {
                            form.target = t;
                            form.target_touched = true;
                        }
                    }
                });
                ui.end_row();
                ui.label(tr!("Titre"));
                ui.add(egui::TextEdit::singleline(&mut form.title).desired_width(330.0));
                ui.end_row();
                ui.label(tr!("Description"));
                ui.add(egui::TextEdit::multiline(&mut form.description).desired_rows(4).desired_width(330.0));
                ui.end_row();
                if form.account.is_some() && dragontree_forges::supports_reviewers(form.location_kind()) {
                    ui.label(tr!("Relecteurs"));
                    ui.add(
                        egui::TextEdit::singleline(&mut form.reviewers)
                            .hint_text(tr!("identifiants séparés par des virgules (facultatif)"))
                            .desired_width(330.0),
                    );
                    ui.end_row();
                }
            });
            ui.checkbox(&mut form.draft, tr!("Brouillon (draft)"));
            match form.push_need {
                PushNeed::No => {}
                PushNeed::NoUpstream => {
                    ui.checkbox(
                        &mut form.push_first,
                        tr!("Pousser d'abord « {branch} » vers {remote}", branch = &form.head, remote = &form.remote),
                    );
                }
                PushNeed::Ahead(n) => {
                    let remote = form.remote.clone();
                    let text = trn!(
                        n,
                        "Pousser d'abord le commit en avance vers {remote}",
                        "Pousser d'abord les {n} commits en avance vers {remote}",
                        remote
                    );
                    ui.checkbox(&mut form.push_first, text);
                }
            }
        });
        match &form.account {
            Some(account) => {
                ui.label(RichText::new(tr!("Compte : {title}", title = account.title())).color(p.weak));
            }
            None if form.location.is_some() => {
                warning(
                    ui,
                    tr!("Aucun compte ne correspond à ce remote : la page de création s'ouvrira dans le navigateur."),
                    p,
                );
            }
            None => {}
        }
        match form.stage.clone() {
            PrStage::Edit => {}
            PrStage::Pushing => {
                ui.label(RichText::new(tr!("Push de la branche…")).color(p.weak));
            }
            PrStage::Creating => {
                ui.label(RichText::new(tr!("Création de la pull request…")).color(p.weak));
            }
            PrStage::Done(pr) => {
                ui.label(RichText::new(tr!("Pull request n° {number} créée.", number = pr.number)).strong());
                if ui.link(&pr.url).clicked() {
                    crate::util::open_url(ui.ctx(), &pr.url);
                }
                if let Some(w) = &pr.warning {
                    warning(ui, w, p);
                }
            }
            PrStage::Failed(message) => {
                warning(ui, &tr!("La création a échoué : {error}", error = message), p);
            }
        }
        // Autre voie quand l'API est possible : la page de création de la forge.
        if form.account.is_some()
            && !matches!(form.stage, PrStage::Done(_))
            && let Some(url) = form.web_url()
            && ui.button(tr!("Ouvrir dans le navigateur")).clicked()
        {
            crate::util::open_url(ui.ctx(), &url);
        }
        match form.action() {
            PrAction::OpenCreated => true,
            PrAction::None => false,
            PrAction::Create | PrAction::OpenWeb => {
                !form.busy() && !form.title.trim().is_empty() && !form.target.is_empty() && form.target != form.head
            }
        }
    }

    /// Bouton principal : crée par l'API, ouvre la page de création, ou ouvre la pull request créée.
    pub(super) fn submit_pull_request(&mut self, shared: &mut Shared, mut form: PullRequestForm) {
        match form.action() {
            PrAction::OpenCreated => {
                if let PrStage::Done(pr) = &form.stage {
                    crate::util::open_url(&shared.ctx, &pr.url);
                }
            }
            PrAction::OpenWeb => {
                if let Some(url) = form.web_url() {
                    crate::util::open_url(&shared.ctx, &url);
                }
            }
            PrAction::Create if form.push_first && form.push_need != PushNeed::No => {
                let generation = form.generation;
                form.stage = PrStage::Pushing;
                let (remote, source, head) = (form.remote.clone(), form.source.clone(), form.head.clone());
                let set_upstream = form.push_need == PushNeed::NoUpstream;
                self.dialog = Some(Dialog::PullRequest(Box::new(form)));
                let before = self.next_op_id;
                self.op_push(shared, remote, source, head, set_upstream, false, false);
                // Sans opération lancée (dépôt fermé), rien n'attend le push.
                if self.next_op_id != before {
                    self.pr_after_push = Some((self.next_op_id, generation));
                }
            }
            PrAction::Create => self.start_pull_request(shared, form),
            PrAction::None => {}
        }
    }

    /// Le push demandé par le dialogue se termine : la création suit s'il a réussi.
    pub(super) fn after_pr_push(&mut self, shared: &Shared, op: u64, succeeded: bool) {
        let Some((pending, generation)) = self.pr_after_push else { return };
        if pending != op {
            return;
        }
        self.pr_after_push = None;
        // Le dialogue a pu être fermé (ou rouvert pour une autre branche) pendant le push : on ne touche qu'au sien.
        if !matches!(&self.dialog, Some(Dialog::PullRequest(f)) if f.generation == generation) {
            return;
        }
        let Some(Dialog::PullRequest(form)) = self.dialog.take() else { return };
        let mut form = *form;
        if succeeded {
            // Le push a fait de la branche une branche suivie.
            form.push_need = PushNeed::No;
            self.start_pull_request(shared, form);
        } else {
            form.stage = PrStage::Failed(tr!("Le push de la branche a échoué (voir la notification).").into());
            self.dialog = Some(Dialog::PullRequest(Box::new(form)));
        }
    }

    /// Envoie la requête à la forge en tâche de fond (le jeton est lu dans le Gestionnaire d'identifiants).
    fn start_pull_request(&mut self, shared: &Shared, mut form: PullRequestForm) {
        let (Some(account), Some(location)) = (form.account.clone(), form.location.clone()) else { return };
        let request = form.draft_request();
        let form_generation = form.generation;
        form.stage = PrStage::Creating;
        self.dialog = Some(Dialog::PullRequest(Box::new(form)));
        let (id, generation) = (self.id, form_generation);
        shared.jobs.spawn("pull-request", move || {
            let result = match dragontree_forges::secrets::load_token(&account.id) {
                Ok(Some(token)) => dragontree_forges::create_pull_request(&account, &token, &location, &request),
                Ok(None) => Err(ForgeError::InvalidInput(tr!("aucun jeton n'est enregistré pour ce compte").into())),
                Err(e) => Err(e),
            };
            Some(AppEvent::Repo(id, RepoEvent::PullRequest(generation, result)))
        });
    }

    /// Réponse de la forge : le dialogue affiche le lien ou l'erreur (et garde le navigateur en repli) ; s'il a été
    /// fermé entre-temps, une notification donne le résultat.
    pub(super) fn on_pull_request(
        &mut self,
        shared: &mut Shared,
        generation: u64,
        result: Result<CreatedPullRequest, ForgeError>,
    ) {
        if let Some(Dialog::PullRequest(form)) = &mut self.dialog
            && form.generation == generation
        {
            form.stage = match result {
                Ok(pr) => PrStage::Done(pr),
                Err(e) => PrStage::Failed(e.to_string()),
            };
            return;
        }
        match result {
            Ok(pr) => shared.toast(Toast::success(tr!(
                "Pull request n° {number} créée : {url}",
                number = pr.number,
                url = pr.url
            ))),
            Err(e) => shared.toast(Toast::error(tr!("Création de la pull request impossible"), e.to_string())),
        }
    }
}

impl PullRequestForm {
    fn location_kind(&self) -> dragontree_forges::ForgeKind {
        self.location.as_ref().map_or(dragontree_forges::ForgeKind::GitHub, RepoLocation::kind)
    }
}

/// Où pousser la branche locale `branch` et sous quel nom elle existe sur le remote : sa branche suivie (le nom peut
/// différer du nom local : `fix` suit `origin/bugfix/123`), sinon le même nom sur le remote par défaut.
fn push_target(
    refs: &dragontree_git::RefsSnapshot,
    branch: &dragontree_git::LocalBranch,
    default_remote: Option<String>,
) -> Option<(String, String)> {
    match branch.upstream.as_deref().and_then(|u| refs.upstream_parts(u)) {
        Some(target) => Some(target),
        None => default_remote.map(|remote| (remote, branch.name.clone())),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use dragontree_git::{HeadState, LocalBranch, Oid, RefsSnapshot, RemoteBranch};

    use super::*;
    use crate::settings::SettingsFile;

    fn oid() -> Oid {
        Oid::from_hex(&"1".repeat(40)).unwrap()
    }

    fn local(name: &str, upstream: Option<&str>) -> LocalBranch {
        LocalBranch {
            name: name.into(),
            oid: oid(),
            upstream: upstream.map(str::to_string),
            ahead: 0,
            behind: 0,
            upstream_gone: false,
            is_head: false,
        }
    }

    fn refs(locals: Vec<LocalBranch>) -> RefsSnapshot {
        let remote = RemoteBranch {
            remote: "origin".into(),
            name: "bugfix/123".into(),
            full: "origin/bugfix/123".into(),
            oid: oid(),
            is_symbolic: false,
        };
        RefsSnapshot {
            head: HeadState::Detached(oid()),
            head_oid: Some(oid()),
            locals,
            remotes: vec![remote],
            tags: vec![],
            labels: HashMap::new(),
        }
    }

    #[test]
    fn the_head_is_the_tracked_branch_name_not_the_local_one() {
        let fix = local("fix", Some("origin/bugfix/123"));
        let snapshot = refs(vec![fix.clone(), local("neuve", None)]);
        assert_eq!(
            push_target(&snapshot, &fix, Some("origin".into())),
            Some(("origin".to_string(), "bugfix/123".to_string()))
        );
        // Sans branche suivie : même nom sur le remote par défaut.
        let neuve = snapshot.local("neuve").unwrap();
        assert_eq!(
            push_target(&snapshot, neuve, Some("origin".into())),
            Some(("origin".to_string(), "neuve".to_string()))
        );
        assert_eq!(push_target(&snapshot, neuve, None), None);
    }

    fn form(generation: u64) -> PullRequestForm {
        PullRequestForm {
            source: "topic".into(),
            head: "topic".into(),
            generation,
            target: "main".into(),
            target_touched: false,
            targets: vec!["main".into()],
            title: "t".into(),
            description: String::new(),
            draft: false,
            reviewers: String::new(),
            remote: "origin".into(),
            location: None,
            account: None,
            push_need: PushNeed::NoUpstream,
            push_first: true,
            stage: PrStage::Creating,
        }
    }

    fn created() -> CreatedPullRequest {
        CreatedPullRequest { url: "https://forge.contoso.test/pr/7".into(), number: 7, warning: None }
    }

    fn tab(dir: &std::path::Path) -> (RepoTab, Shared) {
        let (store, settings, _) = SettingsFile::load(&dir.join("settings.json"));
        let (mut shared, _events) = Shared::new(egui::Context::default(), settings, store, None, None);
        (RepoTab::new(1, dir.to_path_buf(), &mut shared), shared)
    }

    #[test]
    fn a_late_answer_for_another_dialog_is_not_shown_in_the_open_one() {
        let dir = tempfile::tempdir().unwrap();
        let (mut tab, mut shared) = tab(dir.path());
        tab.dialog = Some(Dialog::PullRequest(Box::new(form(2))));
        tab.on_pull_request(&mut shared, 1, Ok(created()));
        let Some(Dialog::PullRequest(f)) = &tab.dialog else { panic!("dialogue perdu") };
        assert_eq!(f.stage, PrStage::Creating, "réponse d'un ancien dialogue ignorée");
        tab.on_pull_request(&mut shared, 2, Ok(created()));
        let Some(Dialog::PullRequest(f)) = &tab.dialog else { panic!("dialogue perdu") };
        assert_eq!(f.stage, PrStage::Done(created()));
    }

    #[test]
    fn the_end_of_a_push_never_takes_a_dialog_that_is_not_its_own() {
        let dir = tempfile::tempdir().unwrap();
        let (mut tab, shared) = tab(dir.path());
        // Le dialogue de pull request a été annulé, un autre dialogue s'est ouvert depuis.
        tab.dialog = Some(Dialog::CreateBranch {
            start: "HEAD".into(),
            start_label: "HEAD".into(),
            name: "x".into(),
            checkout: true,
        });
        tab.pr_after_push = Some((5, 1));
        tab.after_pr_push(&shared, 5, true);
        assert!(matches!(tab.dialog, Some(Dialog::CreateBranch { .. })), "le dialogue ouvert est conservé");
        assert_eq!(tab.pr_after_push, None);
        // Un dialogue de pull request rouvert (autre numéro) n'est pas le sien non plus.
        tab.dialog = Some(Dialog::PullRequest(Box::new(form(3))));
        tab.pr_after_push = Some((6, 2));
        tab.after_pr_push(&shared, 6, false);
        let Some(Dialog::PullRequest(f)) = &tab.dialog else { panic!("dialogue perdu") };
        assert_eq!(f.stage, PrStage::Creating);
    }
}
