//! Compte associé au dépôt (Dépôt › Utiliser le compte… / Ne plus utiliser de compte, réglages du dépôt) : son
//! jeton authentifie fetch, pull et push (voir `actions::auth_env`), sans rien écrire dans `.git/config`. Sans
//! association, git garde sa configuration habituelle (Gestionnaire d'identifiants…).

use dragontree_forges::{Account, ForgeKind, account_matches_url};
use dragontree_git::Remote;
use dragontree_i18n::tr;
use egui::{RichText, Ui};

use super::{Dialog, RepoTab};
use crate::app::{AppAction, Shared};
use crate::settings::Settings;
use crate::theme::Palette;
use crate::toasts::Toast;

/// Compte proposé pour le dépôt ; `remote` : le premier remote dont une URL (fetch ou push) relève du compte.
#[derive(Debug, Clone, PartialEq)]
pub struct AccountChoice {
    pub id: String,
    pub title: String,
    pub kind: ForgeKind,
    pub remote: Option<String>,
}

/// Comptes proposés pour un dépôt : ceux qui correspondent à l'un de ses remotes d'abord, puis les autres (leur
/// jeton ne serait envoyé à aucun de ces remotes), chaque groupe dans l'ordre des réglages.
pub fn account_choices(accounts: &[Account], remotes: &[Remote]) -> Vec<AccountChoice> {
    let remote_of = |account: &Account| {
        remotes
            .iter()
            .find(|r| [&r.fetch_url, &r.push_url].into_iter().any(|url| account_matches_url(account, url)))
            .map(|r| r.name.clone())
    };
    let mut choices: Vec<AccountChoice> = accounts
        .iter()
        .map(|a| AccountChoice { id: a.id.clone(), title: a.title(), kind: a.kind, remote: remote_of(a) })
        .collect();
    // Tri stable : l'ordre des réglages reste celui de chaque groupe.
    choices.sort_by_key(|c| c.remote.is_none());
    choices
}

/// Compte coché à l'ouverture de « Utiliser le compte… » : celui du dépôt, sinon le premier qui correspond à un
/// remote.
fn preselected(choices: &[AccountChoice], current: Option<&str>) -> Option<String> {
    let current = current.filter(|id| choices.iter().any(|c| c.id == *id));
    current.map(str::to_string).or_else(|| choices.iter().find(|c| c.remote.is_some()).map(|c| c.id.clone()))
}

/// Choix du compte (boutons radio) : les comptes qui correspondent à un remote, suivis du nom de ce remote, puis les
/// autres sous un intertitre ; avec `none`, une première entrée « Aucun » (configuration habituelle de git).
pub(super) fn account_picker(
    ui: &mut Ui,
    choices: &[AccountChoice],
    chosen: &mut Option<String>,
    none: bool,
    p: &Palette,
) {
    if none {
        ui.radio_value(chosen, None, tr!("Aucun : configuration habituelle de git (Gestionnaire d'identifiants…)"));
    }
    let mut others_titled = false;
    for choice in choices {
        if choice.remote.is_none() && !others_titled {
            others_titled = true;
            ui.add_space(4.0);
            ui.label(RichText::new(tr!("Autres comptes, sans remote correspondant :")).color(p.weak).size(12.0));
        }
        ui.horizontal(|ui| {
            let title = format!("{} {}", crate::accounts::kind_icon(choice.kind), choice.title);
            ui.radio_value(chosen, Some(choice.id.clone()), title);
            if let Some(remote) = &choice.remote {
                ui.label(RichText::new(format!("({remote})")).color(p.weak).size(12.0));
            }
        });
    }
    let unmatched = choices.iter().any(|c| chosen.as_ref() == Some(&c.id) && c.remote.is_none());
    if unmatched {
        super::dialogs::warning(
            ui,
            tr!("Ce compte ne correspond à aucun remote : son jeton ne sera envoyé à aucun serveur de ce dépôt."),
            p,
        );
    }
}

impl RepoTab {
    /// Compte en vigueur pour le dépôt, dont le jeton authentifie les opérations réseau : le sien (voir
    /// [`RepoTab::own_account`]), sinon, pour un worktree lié, celui de la copie de travail principale.
    pub fn account<'s>(&self, settings: &'s Settings) -> Option<&'s Account> {
        self.own_account(settings).or_else(|| {
            let main = self.repo.as_ref()?.main_workdir()?;
            settings.account_for_repo(&main)
        })
    }

    /// Compte associé à ce dépôt lui-même (racine de la copie de travail, sinon dossier de l'onglet) : c'est celui
    /// que « Ne plus utiliser de compte » retire.
    pub fn own_account<'s>(&self, settings: &'s Settings) -> Option<&'s Account> {
        settings.account_for_repo(&self.workdir()).or_else(|| settings.account_for_repo(&self.path))
    }

    /// Dépôt › Utiliser le compte… : le dialogue de choix ; sans compte enregistré, la fenêtre Comptes s'ouvre pour
    /// en ajouter un.
    pub(super) fn open_use_account(&mut self, shared: &mut Shared) {
        if shared.settings.accounts.is_empty() {
            shared.toast(Toast::info(tr!("Aucun compte enregistré : ajoutez-en un, puis associez-le au dépôt.")));
            shared.actions.push(AppAction::ShowAccounts);
            return;
        }
        let choices = account_choices(&shared.settings.accounts, &self.remotes);
        let current = self.account(&shared.settings).map(|a| a.id.clone());
        let chosen = preselected(&choices, current.as_deref());
        self.dialog = Some(Dialog::UseAccount { choices, chosen });
    }

    /// Associe le compte `id` au dépôt, ou retire l'association (`None`) ; une notification le confirme.
    pub(super) fn set_account(&mut self, shared: &mut Shared, id: Option<String>) {
        let before = self.account(&shared.settings).map(Account::title);
        // Une seule entrée, sous la racine de la copie de travail (comme après un clonage).
        shared.settings.set_account_for_repo(&self.path, None);
        shared.settings.set_account_for_repo(&self.workdir(), id.as_deref());
        shared.settings_dirty = true;
        match (before, self.account(&shared.settings).map(Account::title)) {
            (_, Some(account)) => {
                shared.toast(Toast::success(tr!("Fetch, pull et push utilisent le compte {account}", account)))
            }
            (Some(account), None) => shared.toast(Toast::info(tr!(
                "Le dépôt n'utilise plus le compte {account} : git reprend sa configuration habituelle",
                account
            ))),
            (None, None) => {}
        }
    }

    /// Champs du dialogue « Utiliser le compte… » ; valide dès qu'un compte est coché.
    pub(super) fn use_account_body(
        ui: &mut Ui,
        choices: &[AccountChoice],
        chosen: &mut Option<String>,
        p: &Palette,
    ) -> bool {
        ui.label(
            RichText::new(tr!(
                "Fetch, pull et push de ce dépôt s'authentifieront avec le jeton de ce compte (rien n'est écrit dans .git/config)."
            ))
            .color(p.weak),
        );
        ui.add_space(6.0);
        account_picker(ui, choices, chosen, false, p);
        chosen.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account(id: &str, kind: ForgeKind, host: &str, username: &str, orgs: &[&str]) -> Account {
        Account {
            id: id.into(),
            kind,
            host: host.into(),
            username: username.into(),
            display_name: username.into(),
            orgs: orgs.iter().map(|o| o.to_string()).collect(),
            avatar_url: None,
        }
    }

    fn remote(name: &str, fetch_url: &str, push_url: &str) -> Remote {
        Remote { name: name.into(), fetch_url: fetch_url.into(), push_url: push_url.into() }
    }

    #[test]
    fn accounts_matching_a_remote_are_proposed_first_with_that_remote() {
        let accounts = [
            account("github", ForgeKind::GitHub, "github.com", "ada", &[]),
            account("gitlab", ForgeKind::GitLab, "gitlab.contoso.test", "jsmith", &[]),
            account("azure", ForgeKind::AzureDevOps, "dev.azure.com", "camille", &["fabrikam"]),
            account("azure-autre", ForgeKind::AzureDevOps, "dev.azure.com", "camille", &["northwind"]),
        ];
        let remotes = [
            remote("origin", "git@gitlab.contoso.test:team/app.git", "git@gitlab.contoso.test:team/app.git"),
            // Seule l'URL de push relève d'un compte : il est proposé aussi.
            remote("miroir", "https://mirror.contoso.test/app.git", "https://dev.azure.com/fabrikam/P/_git/app"),
        ];
        let choices = account_choices(&accounts, &remotes);
        let summary: Vec<_> = choices.iter().map(|c| (c.id.as_str(), c.remote.as_deref())).collect();
        assert_eq!(
            summary,
            [("gitlab", Some("origin")), ("azure", Some("miroir")), ("github", None), ("azure-autre", None)],
            "comptes des remotes d'abord, puis les autres, chaque groupe dans l'ordre des réglages"
        );
        assert_eq!(choices[0].title, accounts[1].title());
        assert_eq!(choices[0].kind, ForgeKind::GitLab);
    }

    #[test]
    fn the_dialog_checks_the_current_account_otherwise_the_first_matching_one() {
        let choice = |id: &str, remote: Option<&str>| AccountChoice {
            id: id.into(),
            title: id.into(),
            kind: ForgeKind::GitHub,
            remote: remote.map(str::to_string),
        };
        let choices = [choice("gitlab", Some("origin")), choice("github", None)];
        assert_eq!(preselected(&choices, Some("github")).as_deref(), Some("github"), "le compte du dépôt");
        assert_eq!(preselected(&choices, None).as_deref(), Some("gitlab"), "sinon celui du remote");
        assert_eq!(preselected(&choices, Some("supprimé")).as_deref(), Some("gitlab"), "un compte disparu est ignoré");
        assert_eq!(preselected(&[choice("github", None)], None), None, "aucun remote correspondant : rien de coché");
    }

    #[test]
    fn without_a_matching_remote_every_account_is_still_proposed_in_order() {
        let accounts = [
            account("b", ForgeKind::GitHub, "github.com", "ada", &[]),
            account("a", ForgeKind::GitLab, "gitlab.com", "jsmith", &[]),
        ];
        let choices = account_choices(&accounts, &[remote("origin", r"C:\DEV\depot-local", "")]);
        assert_eq!(
            choices.iter().map(|c| (c.id.as_str(), c.remote.is_none())).collect::<Vec<_>>(),
            [("b", true), ("a", true)]
        );
        assert!(account_choices(&accounts, &[]).iter().all(|c| c.remote.is_none()));
        assert!(account_choices(&[], &[remote("origin", "https://github.com/x/y.git", "")]).is_empty());
    }
}
