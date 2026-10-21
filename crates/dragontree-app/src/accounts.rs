//! Fenêtre « Comptes » : GitHub, GitLab et Azure DevOps, plusieurs comptes à la fois.
//! Les jetons (PAT) vont dans le Gestionnaire d'identifiants Windows, jamais dans les réglages.

use std::collections::{BTreeSet, HashMap};

use dragontree_forges::{
    Account, ForgeError, ForgeKind, VerifiedUser, recommended_scopes, secrets, token_creation_url,
};
use dragontree_git::discovery::FoundRepo;
use dragontree_i18n::tr;
use egui::{RichText, Ui};

use crate::app::{AppEvent, Shared};
use crate::icons;
use crate::theme::Palette;
use crate::toasts::Toast;

pub enum ForgeEvent {
    /// Vérification d'un nouveau compte (formulaire).
    Verified { req: u64, sent: Submitted, result: Result<VerifiedUser, ForgeError> },
    /// Revérification d'un compte existant.
    Rechecked { account_id: String, result: Result<VerifiedUser, ForgeError> },
}

/// Valeurs du formulaire envoyées à la vérification : le compte est enregistré avec celles-ci, pas avec ce que le
/// formulaire contient à l'arrivée de la réponse (le champ a pu être modifié entre-temps).
#[derive(Clone)]
pub struct Submitted {
    kind: ForgeKind,
    host: String,
    token: String,
}

#[derive(Clone)]
enum AccountStatus {
    Checking,
    Ok,
    Error(String),
}

struct AccountForm {
    kind: ForgeKind,
    host: String,
    token: String,
    orgs: String,
    verifying: Option<u64>,
    error: Option<String>,
}

impl AccountForm {
    fn new(kind: ForgeKind, suggested_orgs: &[String]) -> AccountForm {
        AccountForm {
            kind,
            host: kind.default_host().to_string(),
            token: String::new(),
            orgs: if kind == ForgeKind::AzureDevOps { suggested_orgs.join(", ") } else { String::new() },
            verifying: None,
            error: None,
        }
    }

    fn org_list(&self) -> Vec<String> {
        self.orgs.split([',', ';', ' ']).map(str::trim).filter(|s| !s.is_empty()).map(str::to_string).collect()
    }
}

#[derive(Default)]
pub struct AccountsWindow {
    pub open: bool,
    form: Option<AccountForm>,
    next_req: u64,
    status: HashMap<String, AccountStatus>,
    suggested_orgs: Option<Vec<String>>,
}

pub fn kind_icon(kind: ForgeKind) -> &'static str {
    match kind {
        ForgeKind::GitHub => icons::GITHUB_LOGO,
        ForgeKind::GitLab => icons::GITLAB_LOGO,
        ForgeKind::AzureDevOps => icons::CLOUD,
    }
}

/// Organisations Azure DevOps présentes dans les remotes des dépôts listés (lecture de `.git/config`).
pub fn suggest_azure_orgs(repos: &[FoundRepo]) -> Vec<String> {
    let mut orgs = BTreeSet::new();
    for r in repos.iter().take(300) {
        let Ok(config) = std::fs::read_to_string(r.path.join(".git").join("config")) else { continue };
        for line in config.lines() {
            let Some(url) = line.trim().strip_prefix("url").map(|s| s.trim_start_matches([' ', '=']).trim()) else {
                continue;
            };
            if let Some(org) = azure_org_from_url(url) {
                orgs.insert(org);
            }
        }
    }
    orgs.into_iter().collect()
}

fn azure_org_from_url(url: &str) -> Option<String> {
    let rest = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://"))?;
    let rest = rest.split_once('@').map_or(rest, |(_, r)| r);
    let (host, path) = rest.split_once('/')?;
    let host = host.to_lowercase();
    if host == "dev.azure.com" {
        return path.split('/').next().filter(|s| !s.is_empty()).map(str::to_string);
    }
    host.strip_suffix(".visualstudio.com").filter(|s| !s.is_empty() && !s.contains('.')).map(str::to_string)
}

impl AccountsWindow {
    pub fn handle(&mut self, ev: ForgeEvent, shared: &mut Shared) {
        match ev {
            ForgeEvent::Verified { req, sent, result } => {
                self.on_verified(req, sent, result, shared, secrets::store_token);
            }
            ForgeEvent::Rechecked { account_id, result } => {
                let status = match result {
                    Ok(user) => {
                        if let Some(acc) = shared.settings.accounts.iter_mut().find(|a| a.id == account_id) {
                            acc.display_name = user.display_name;
                            if !user.orgs.is_empty() {
                                acc.orgs = user.orgs;
                            }
                            acc.avatar_url = user.avatar_url;
                            shared.settings_dirty = true;
                        }
                        AccountStatus::Ok
                    }
                    Err(e) => AccountStatus::Error(e.to_string()),
                };
                self.status.insert(account_id, status);
            }
        }
    }

    /// Réponse de la vérification : le compte est créé avec les valeurs envoyées (`sent`), jamais avec le formulaire
    /// relu ; `store` enregistre le jeton (le Gestionnaire d'identifiants hors des tests).
    fn on_verified(
        &mut self,
        req: u64,
        sent: Submitted,
        result: Result<VerifiedUser, ForgeError>,
        shared: &mut Shared,
        store: impl FnOnce(&str, &str) -> Result<(), ForgeError>,
    ) {
        let Some(form) = &mut self.form else { return };
        if form.verifying != Some(req) {
            return;
        }
        form.verifying = None;
        match result {
            Ok(user) => {
                let account = Account {
                    id: dragontree_forges::new_account_id(),
                    kind: sent.kind,
                    host: sent.host,
                    username: user.username.clone(),
                    display_name: user.display_name.clone(),
                    orgs: user.orgs.clone(),
                    avatar_url: user.avatar_url.clone(),
                };
                if let Err(e) = store(&account.id, &sent.token) {
                    form.error = Some(tr!("Impossible d'enregistrer le jeton : {e}", e));
                    return;
                }
                shared.toast(Toast::success(tr!("Compte ajouté : {account}", account = account.title())));
                self.status.insert(account.id.clone(), AccountStatus::Ok);
                shared.settings.accounts.push(account);
                shared.save_settings();
                self.form = None;
            }
            Err(e) => form.error = Some(explain_error(&e, sent.kind)),
        }
    }

    fn start_verify(&mut self, shared: &Shared) {
        let Some(form) = &mut self.form else { return };
        self.next_req += 1;
        let req = self.next_req;
        form.verifying = Some(req);
        form.error = None;
        let sent =
            Submitted { kind: form.kind, host: form.host.trim().to_string(), token: form.token.trim().to_string() };
        let orgs = form.org_list();
        shared.jobs.spawn("verify", move || {
            let result = dragontree_forges::verify(sent.kind, &sent.host, &sent.token, &orgs);
            Some(AppEvent::Forge(ForgeEvent::Verified { req, sent, result }))
        });
    }

    fn recheck(&mut self, shared: &Shared, account: &Account) {
        self.status.insert(account.id.clone(), AccountStatus::Checking);
        let acc = account.clone();
        shared.jobs.spawn("recheck", move || {
            let result = match secrets::load_token(&acc.id) {
                Ok(Some(token)) => dragontree_forges::verify(acc.kind, &acc.host, &token, &acc.orgs),
                Ok(None) => Err(ForgeError::Secret(tr!("jeton absent du Gestionnaire d'identifiants").into())),
                Err(e) => Err(e),
            };
            Some(AppEvent::Forge(ForgeEvent::Rechecked { account_id: acc.id.clone(), result }))
        });
    }

    pub fn ui(&mut self, ctx: &egui::Context, shared: &mut Shared, repos: &[FoundRepo], p: &Palette) {
        if !self.open {
            return;
        }
        let mut open = true;
        crate::widgets::centered_window(ctx, format!("{} {}", icons::USER_CIRCLE, tr!("Comptes")))
            .id(egui::Id::unique("accounts-window"))
            .open(&mut open)
            .default_size([620.0, 480.0])
            .show(ctx, |ui| {
                ui.label(RichText::new(tr!("Connectez vos comptes pour lister et cloner vos dépôts. Plusieurs comptes par service sont possibles.")).color(p.weak));
                ui.add_space(8.0);
                let accounts = shared.settings.accounts.clone();
                if accounts.is_empty() && self.form.is_none() {
                    ui.label(RichText::new(tr!("Aucun compte pour l'instant.")).color(p.weak));
                }
                let mut remove = None;
                for acc in &accounts {
                    egui::Frame::group(ui.style()).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(kind_icon(acc.kind)).size(22.0).color(p.accent));
                            ui.vertical(|ui| {
                                ui.label(RichText::new(acc.title()).strong());
                                let mut sub = format!("{} · {}", acc.kind.label(), acc.host);
                                if !acc.orgs.is_empty() {
                                    sub.push_str(&format!(" · {}", tr!("organisations : {orgs}", orgs = acc.orgs.join(", "))));
                                }
                                ui.label(RichText::new(sub).color(p.weak).size(12.0));
                            });
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.button(format!("{} {}", icons::TRASH, tr!("Supprimer"))).clicked() {
                                    remove = Some(acc.id.clone());
                                }
                                if ui.button(format!("{} {}", icons::ARROWS_CLOCKWISE, tr!("Vérifier"))).clicked() {
                                    self.recheck(shared, acc);
                                }
                                match self.status.get(&acc.id) {
                                    Some(AccountStatus::Checking) => {
                                        ui.spinner();
                                    }
                                    Some(AccountStatus::Ok) => {
                                        ui.label(RichText::new(format!("{} {}", icons::CHECK_CIRCLE, tr!("connecté"))).color(p.added_fg));
                                    }
                                    Some(AccountStatus::Error(e)) => {
                                        ui.label(RichText::new(format!("{} {}", icons::WARNING_CIRCLE, tr!("erreur"))).color(p.removed_fg)).on_hover_text(e);
                                    }
                                    None => {}
                                }
                            });
                        });
                    });
                }
                if let Some(id) = remove {
                    let _ = secrets::delete_token(&id);
                    shared.settings.accounts.retain(|a| a.id != id);
                    shared.settings.repo_accounts.retain(|_, v| *v != id);
                    shared.save_settings();
                    self.status.remove(&id);
                }

                ui.add_space(10.0);
                if self.form.is_none() {
                    ui.horizontal(|ui| {
                        ui.label(tr!("Ajouter :"));
                        for kind in ForgeKind::ALL {
                            if ui.button(format!("{} {}", kind_icon(kind), kind.label())).clicked() {
                                let orgs = self.suggested_orgs.get_or_insert_with(|| suggest_azure_orgs(repos)).clone();
                                self.form = Some(AccountForm::new(kind, &orgs));
                            }
                        }
                    });
                } else {
                    self.form_ui(ui, shared, p);
                }
            });
        if !open {
            self.open = false;
            self.form = None;
        }
    }

    fn form_ui(&mut self, ui: &mut Ui, shared: &mut Shared, p: &Palette) {
        let mut cancel = false;
        let mut verify = false;
        let Some(form) = &mut self.form else { return };
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(format!("{} {}", kind_icon(form.kind), tr!("Nouveau compte {service}", service = form.kind.label()))).strong().size(15.0));
            ui.add_space(6.0);
            egui::Grid::new("account-form").num_columns(2).spacing([10.0, 8.0]).show(ui, |ui| {
                ui.label(tr!("Serveur"));
                ui.add(egui::TextEdit::singleline(&mut form.host).desired_width(320.0))
                    .on_hover_text(tr!("Laissez la valeur par défaut, ou indiquez votre GitHub Enterprise / GitLab auto-hébergé / Azure DevOps Server"));
                ui.end_row();
                if form.kind == ForgeKind::AzureDevOps {
                    ui.label(tr!("Organisation(s)"));
                    ui.add(egui::TextEdit::singleline(&mut form.orgs).hint_text(tr!("ex. contoso (séparées par des virgules)")).desired_width(320.0))
                        .on_hover_text(tr!("Indispensable si votre jeton est limité à une organisation"));
                    ui.end_row();
                }
                ui.label(tr!("Jeton (PAT)"));
                ui.add(egui::TextEdit::singleline(&mut form.token).password(true).desired_width(320.0));
                ui.end_row();
            });
            let org_hint = form.org_list().first().cloned();
            ui.horizontal(|ui| {
                let url = token_creation_url(form.kind, form.host.trim(), org_hint.as_deref());
                if ui.link(format!("{} {}", icons::ARROW_SQUARE_OUT, tr!("Créer un jeton"))).clicked() {
                    crate::util::open_url(ui.ctx(), &url);
                }
                ui.label(RichText::new(tr!("Droits recommandés : {scopes}", scopes = recommended_scopes(form.kind))).color(p.weak).size(12.0));
            });
            if let Some(err) = &form.error {
                ui.add(egui::Label::new(RichText::new(format!("{} {err}", icons::WARNING)).color(p.removed_fg)).wrap());
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let busy = form.verifying.is_some();
                    let ok = !form.token.trim().is_empty() && !form.host.trim().is_empty() && !busy;
                    if ui.add_enabled(ok, egui::Button::new(RichText::new(format!("{} {}", icons::CHECK, tr!("Vérifier et ajouter"))).color(egui::Color32::WHITE)).fill(p.accent)).clicked() {
                        verify = true;
                    }
                    if busy {
                        ui.spinner();
                    }
                    if ui.button(tr!("Annuler")).clicked() {
                        cancel = true;
                    }
                });
            });
        });
        if cancel {
            self.form = None;
        } else if verify {
            self.start_verify(shared);
        }
    }
}

fn explain_error(e: &ForgeError, kind: ForgeKind) -> String {
    match e {
        ForgeError::Unauthorized if kind == ForgeKind::AzureDevOps => {
            tr!("Jeton refusé. S'il est limité à une organisation, indiquez-la dans « Organisation(s) » ; sinon vérifiez sa validité et ses droits.").into()
        }
        ForgeError::Unauthorized => tr!("Jeton refusé : vérifiez qu'il est valide et qu'il a les droits recommandés.").into(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::SettingsFile;

    #[test]
    fn azure_orgs_from_remote_urls() {
        assert_eq!(
            azure_org_from_url("https://dev-fabrikam.visualstudio.com/Projet/_git/Depot").as_deref(),
            Some("dev-fabrikam")
        );
        assert_eq!(azure_org_from_url("https://moi@dev.azure.com/acme/Projet/_git/repo").as_deref(), Some("acme"));
        assert_eq!(azure_org_from_url("https://github.com/acme/repo.git"), None);
        assert_eq!(azure_org_from_url("git@ssh.dev.azure.com:v3/acme/p/r"), None);
    }

    fn window_with_form(shared_dir: &std::path::Path) -> (AccountsWindow, Shared) {
        let (store, settings, _) = SettingsFile::load(&shared_dir.join("settings.json"));
        let (shared, _events) = Shared::new(egui::Context::default(), settings, store, None, None);
        let mut window = AccountsWindow::default();
        let mut form = AccountForm::new(ForgeKind::GitHub, &[]);
        form.verifying = Some(1);
        window.form = Some(form);
        (window, shared)
    }

    fn verified_user() -> VerifiedUser {
        VerifiedUser { username: "ada".into(), display_name: "Ada".into(), avatar_url: None, orgs: Vec::new() }
    }

    #[test]
    fn the_account_is_saved_with_the_verified_values_not_the_edited_form() {
        let dir = tempfile::tempdir().unwrap();
        let (mut window, mut shared) = window_with_form(dir.path());
        let sent =
            Submitted { kind: ForgeKind::GitHub, host: "github.contoso.test".into(), token: "jeton-verifie".into() };
        // Le formulaire est modifié pendant la vérification.
        let form = window.form.as_mut().unwrap();
        form.token = "jeton-non-verifie".into();
        form.host = "autre.fabrikam.test".into();
        let mut stored = None;
        window.on_verified(1, sent, Ok(verified_user()), &mut shared, |id, token| {
            stored = Some((id.to_string(), token.to_string()));
            Ok(())
        });
        let (id, token) = stored.expect("le jeton est enregistré");
        assert_eq!(token, "jeton-verifie");
        let account = &shared.settings.accounts[0];
        assert_eq!((account.id.as_str(), account.host.as_str()), (id.as_str(), "github.contoso.test"));
        assert!(window.form.is_none());
    }

    #[test]
    fn a_verification_answer_for_an_older_request_stores_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let (mut window, mut shared) = window_with_form(dir.path());
        let sent = Submitted { kind: ForgeKind::GitHub, host: "github.contoso.test".into(), token: "jeton".into() };
        window.on_verified(2, sent, Ok(verified_user()), &mut shared, |_, _| panic!("rien à enregistrer"));
        assert!(shared.settings.accounts.is_empty());
        assert!(window.form.is_some());
    }
}
