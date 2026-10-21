//! Compte associé à un dépôt existant : Dépôt › Utiliser le compte… propose d'abord les comptes qui correspondent à un
//! remote, Dépôt › Ne plus utiliser de compte retire l'association, les réglages du dépôt la changent aussi. Comptes
//! fictifs, sans jeton (associer un compte ne lit pas le Gestionnaire d'identifiants), aucune requête réseau.

mod common;

use std::path::{Path, PathBuf};

use common::*;
use dragontree_app::settings::Settings;
use dragontree_app::{DragonTreeApp, StartOptions};
use dragontree_forges::{Account, ForgeKind};
use egui_kittest::Harness;
use egui_kittest::kittest::{AccessKitNode, NodeT, Queryable};

type H = Harness<'static, DragonTreeApp>;

const GITHUB: &str = "compte-github";
const GITLAB: &str = "compte-gitlab";

fn account(id: &str, kind: ForgeKind, host: &str, username: &str) -> Account {
    Account {
        id: id.into(),
        kind,
        host: host.into(),
        username: username.into(),
        display_name: String::new(),
        orgs: vec![],
        avatar_url: None,
    }
}

/// Dépôt `northwind` dont le remote est sur le GitLab fictif de Contoso.
fn repo(base: &Path) -> PathBuf {
    let dir = base.join("northwind");
    init_repo(&dir);
    commit_file(&dir, "a.txt", "1\n", "Premier commit");
    git(&dir, &["remote", "add", "origin", "https://gitlab.contoso.test/team/northwind.git"]);
    dir
}

fn settings_path(base: &Path) -> PathBuf {
    base.join("profil").join("settings.json")
}

/// Profil de test : deux comptes (GitHub d'abord, qui ne correspond pas au remote), pas de fetch automatique (le
/// remote est fictif), et l'association éventuelle de `repo` à `associated`.
fn seed(base: &Path, accounts: Vec<Account>, associated: Option<(&Path, &str)>) {
    let mut settings = Settings { auto_fetch_minutes: 0, accounts, ..Settings::default() };
    if let Some((repo, id)) = associated {
        settings.set_account_for_repo(repo, Some(id));
    }
    settings.save_to(&settings_path(base)).unwrap();
}

fn two_accounts() -> Vec<Account> {
    vec![
        account(GITHUB, ForgeKind::GitHub, "github.com", "ada"),
        account(GITLAB, ForgeKind::GitLab, "gitlab.contoso.test", "jsmith"),
    ]
}

fn open(base: &Path, repo: &Path) -> H {
    let mut h = harness(StartOptions { open: vec![repo.to_path_buf()], view: Some("history".into()), ..start(base) });
    wait_until(&mut h, "dépôt chargé", |h| h.query_by_label("Premier commit").is_some());
    h
}

/// Identifiants des comptes associés à un dépôt dans le fichier de réglages.
fn saved_associations(base: &Path) -> Vec<String> {
    Settings::load_from(&settings_path(base)).repo_accounts.into_values().collect()
}

/// Le compte `title` est affiché dans la barre d'outils de l'onglet (un bouton : la notification qui confirme
/// l'association est un simple texte).
fn shows_account(h: &H, title: &str) -> bool {
    h.query_by(|n| n.role() == egui::accesskit::Role::Button && n.label().is_some_and(|l| l.contains(title))).is_some()
}

/// Bouton radio dont le libellé contient `part`.
fn is_radio(n: &AccessKitNode<'_>, part: &str) -> bool {
    n.role() == egui::accesskit::Role::RadioButton && n.label().is_some_and(|l| l.contains(part))
}

fn radio_y(h: &H, part: &str) -> f32 {
    h.get_by(|n| is_radio(n, part)).rect().center().y
}

fn is_selected(h: &H, part: &str) -> bool {
    h.get_by(|n| is_radio(n, part)).accesskit_node().toggled() == Some(egui::accesskit::Toggled::True)
}

fn click_radio(h: &mut H, part: &str) {
    click_by(h, &format!("choix « {part} »"), egui::PointerButton::Primary, |n| is_radio(n, part));
    h.run_steps(2);
}

const GITLAB_TITLE: &str = "GitLab · jsmith (gitlab.contoso.test)";
const GITHUB_TITLE: &str = "GitHub · ada";

#[test]
fn the_repository_menu_associates_an_account_matching_the_remote_then_dissociates_it() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo(tmp.path());
    seed(tmp.path(), two_accounts(), None);
    let mut h = open(tmp.path(), &repo);

    open_menu(&mut h, "Dépôt");
    assert!(menu_item_disabled(&h, "Ne plus utiliser de compte"), "aucun compte associé : rien à retirer");
    click_menu_item(&mut h, "Utiliser le compte…");
    wait_until(&mut h, "dialogue", |h| h.query_by_label("Utiliser").is_some());
    // Le compte du remote passe devant, présélectionné, avec le nom du remote.
    assert!(radio_y(&h, GITLAB_TITLE) < radio_y(&h, GITHUB_TITLE), "le compte qui correspond au remote d'abord");
    assert!(is_selected(&h, GITLAB_TITLE));
    assert!(h.query_by_label("(origin)").is_some());
    // Un compte qui ne correspond à aucun remote est signalé.
    let warning = "ne correspond à aucun remote :";
    assert!(h.query_by_label_contains(warning).is_none());
    click_radio(&mut h, GITHUB_TITLE);
    wait_until(&mut h, "avertissement", |h| h.query_by_label_contains(warning).is_some());
    click_radio(&mut h, GITLAB_TITLE);
    click_label(&mut h, "Utiliser");
    wait_until(&mut h, "association enregistrée", |_| saved_associations(tmp.path()) == [GITLAB]);
    // Le compte est affiché dans la barre d'outils de l'onglet.
    wait_until(&mut h, "compte affiché", |h| shows_account(h, GITLAB_TITLE));

    open_menu(&mut h, "Dépôt");
    click_menu_item(&mut h, "Ne plus utiliser de compte");
    wait_until(&mut h, "association retirée", |_| saved_associations(tmp.path()).is_empty());
    wait_until(&mut h, "compte retiré de la barre d'outils", |h| !shows_account(h, GITLAB_TITLE));
    open_menu(&mut h, "Dépôt");
    assert!(menu_item_disabled(&h, "Ne plus utiliser de compte"));
}

#[test]
fn the_repository_settings_dialog_changes_or_removes_the_account() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo(tmp.path());
    seed(tmp.path(), two_accounts(), Some((&repo, GITLAB)));
    let mut h = open(tmp.path(), &repo);
    wait_until(&mut h, "compte affiché", |h| shows_account(h, GITLAB_TITLE));

    open_menu(&mut h, "Dépôt");
    click_menu_item(&mut h, "Réglages du dépôt…");
    wait_until(&mut h, "dialogue des réglages", |h| h.query_by_label("Enregistrer").is_some());
    assert!(is_selected(&h, GITLAB_TITLE), "le compte associé est coché");
    click_radio(&mut h, "Aucun : configuration habituelle de git");
    click_label(&mut h, "Enregistrer");
    wait_until(&mut h, "association retirée", |_| saved_associations(tmp.path()).is_empty());

    open_menu(&mut h, "Dépôt");
    click_menu_item(&mut h, "Réglages du dépôt…");
    wait_until(&mut h, "dialogue des réglages", |h| h.query_by_label("Enregistrer").is_some());
    click_radio(&mut h, GITHUB_TITLE);
    click_label(&mut h, "Enregistrer");
    wait_until(&mut h, "autre compte associé", |_| saved_associations(tmp.path()) == [GITHUB]);
}

/// Un worktree lié du dépôt reprend le compte associé au dépôt principal, sauf s'il a le sien.
#[test]
fn a_linked_worktree_uses_the_account_of_the_main_repository_unless_it_has_its_own() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo(tmp.path());
    let wt = tmp.path().join("northwind-wt");
    git(&repo, &["worktree", "add", "-q", "-b", "fonction", wt.to_str().unwrap()]);
    seed(tmp.path(), two_accounts(), Some((&repo, GITLAB)));
    let mut h = open(tmp.path(), &wt);
    wait_until(&mut h, "compte du dépôt principal affiché", |h| shows_account(h, GITLAB_TITLE));
    // Rien à retirer sur le worktree lui-même : l'association est celle du dépôt principal.
    open_menu(&mut h, "Dépôt");
    assert!(menu_item_disabled(&h, "Ne plus utiliser de compte"));

    // Son propre compte passe devant celui du dépôt principal…
    click_menu_item(&mut h, "Utiliser le compte…");
    wait_until(&mut h, "dialogue", |h| h.query_by_label("Utiliser").is_some());
    assert!(is_selected(&h, GITLAB_TITLE), "le compte en vigueur est coché");
    click_radio(&mut h, GITHUB_TITLE);
    click_label(&mut h, "Utiliser");
    let both = |base: &Path| {
        let mut ids = saved_associations(base);
        ids.sort();
        ids == [GITHUB, GITLAB]
    };
    wait_until(&mut h, "association propre au worktree", |_| both(tmp.path()));
    wait_until(&mut h, "compte du worktree affiché", |h| shows_account(h, GITHUB_TITLE));
    assert!(!shows_account(&h, GITLAB_TITLE));

    // … et la retirer rend celui du dépôt principal.
    open_menu(&mut h, "Dépôt");
    click_menu_item(&mut h, "Ne plus utiliser de compte");
    wait_until(&mut h, "association du worktree retirée", |_| saved_associations(tmp.path()) == [GITLAB]);
    wait_until(&mut h, "compte du dépôt principal de nouveau", |h| shows_account(h, GITLAB_TITLE));
}

#[test]
fn without_any_account_the_menu_opens_the_accounts_window() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo(tmp.path());
    seed(tmp.path(), vec![], None);
    let mut h = open(tmp.path(), &repo);
    open_menu(&mut h, "Dépôt");
    click_menu_item(&mut h, "Utiliser le compte…");
    wait_until(&mut h, "fenêtre Comptes", |h| h.query_by_label("Aucun compte pour l'instant.").is_some());
    assert!(saved_associations(tmp.path()).is_empty());
}
