//! Fichier › Pull requests… : pull requests de tous les comptes dans une seule liste, filtres, erreur d'un compte à sa
//! place, ouverture dans le navigateur et du dépôt cloné. Comptes fictifs ; les réponses des forges sont celles de
//! `fake_forges` (`StartOptions::pull_request_source`) : ni réseau ni Gestionnaire d'identifiants.

mod common;

use std::path::{Path, PathBuf};

use common::*;
use dragontree_app::settings::{Settings, path_key};
use dragontree_app::{DragonTreeApp, StartOptions};
use dragontree_forges::{Account, ForgeError, ForgeKind, PullRequest, PullRequestList, RepoLocation};
use egui::Key;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

type H = Harness<'static, DragonTreeApp>;

const HOME: &str = "Ajoute la page d'accueil";
const DISCOUNTS: &str = "Corrige le calcul des remises";
const RELEASE: &str = "Prépare la version 2";
const ALL: [&str; 3] = [HOME, DISCOUNTS, RELEASE];

fn account(id: &str, kind: ForgeKind, host: &str, username: &str, display_name: &str) -> Account {
    Account {
        id: id.into(),
        kind,
        host: host.into(),
        username: username.into(),
        display_name: display_name.into(),
        orgs: if kind == ForgeKind::AzureDevOps { vec!["fabrikam".into()] } else { vec![] },
        avatar_url: None,
    }
}

fn three_accounts() -> Vec<Account> {
    vec![
        account("compte-github", ForgeKind::GitHub, "github.com", "jsmith", "Jean Smith"),
        account("compte-gitlab", ForgeKind::GitLab, "gitlab.contoso.test", "jsmith", "Jean Smith"),
        account("compte-azure", ForgeKind::AzureDevOps, "dev.azure.com", "camille@example.com", "Camille Martin"),
    ]
}

fn pull_request(title: &str, repo: RepoLocation, number: u64, url: &str, updated_at: &str) -> PullRequest {
    PullRequest {
        account_id: String::new(),
        repo,
        number,
        title: title.into(),
        author: "jsmith".into(),
        draft: false,
        reviewers: vec![],
        created_by_me: false,
        review_requested: false,
        url: url.into(),
        updated_at: Some(updated_at.into()),
        source_branch: Some("feature/x".into()),
        target_branch: Some("main".into()),
    }
}

/// Réponses des forges : deux pull requests GitHub (l'une créée par l'utilisateur, l'autre à relire), un jeton refusé
/// par GitLab, une pull request Azure DevOps à relire (l'organisation contoso n'a pas répondu).
fn fake_forges(account: &Account) -> Result<PullRequestList, ForgeError> {
    let github = |owner: &str, repo: &str| RepoLocation::GitHub {
        host: "github.com".into(),
        owner: owner.into(),
        repo: repo.into(),
    };
    let mut prs = match account.id.as_str() {
        "compte-github" => {
            let mut home = pull_request(
                HOME,
                github("contoso", "northwind"),
                1347,
                "https://github.com/contoso/northwind/pull/1347",
                "2026-10-20T08:00:00Z",
            );
            home.created_by_me = true;
            let mut discounts = pull_request(
                DISCOUNTS,
                github("fabrikam-labs", "billing"),
                42,
                "https://github.com/fabrikam-labs/billing/pull/42",
                "2026-10-18T08:00:00Z",
            );
            discounts.review_requested = true;
            discounts.draft = true;
            discounts.author = "asmith".into();
            vec![home, discounts]
        }
        "compte-gitlab" => return Err(ForgeError::Unauthorized),
        "compte-azure" => {
            let repo = RepoLocation::Azure {
                host: "dev.azure.com".into(),
                org: "fabrikam".into(),
                project: "Mon Projet".into(),
                repo: "api".into(),
            };
            let url = "https://dev.azure.com/fabrikam/Mon%20Projet/_git/api/pullrequest/15";
            let mut release = pull_request(RELEASE, repo, 15, url, "2026-10-19T08:00:00Z");
            release.review_requested = true;
            vec![release]
        }
        other => panic!("compte inattendu : {other}"),
    };
    for pr in &mut prs {
        pr.account_id = account.id.clone();
    }
    let failed_orgs =
        if account.id == "compte-azure" { vec![("contoso".into(), ForgeError::Unauthorized)] } else { vec![] };
    Ok(PullRequestList { prs, failed_orgs })
}

fn settings_path(base: &Path) -> PathBuf {
    base.join("profil").join("settings.json")
}

/// Dossier `depots` scanné, avec un clone de `contoso/northwind` (remote GitHub fictif, jamais joint).
fn local_clone(base: &Path) -> PathBuf {
    let dir = base.join("depots").join("northwind");
    init_repo(&dir);
    commit_file(&dir, "a.txt", "1\n", "Premier commit");
    git(&dir, &["remote", "add", "origin", "https://github.com/contoso/northwind.git"]);
    dir
}

fn seed(base: &Path, accounts: Vec<Account>) {
    let settings = Settings { auto_fetch_minutes: 0, scan_roots: vec![base.join("depots")], ..Settings::default() };
    settings.save_to(&settings_path(base)).unwrap();
    seed_accounts(base, &accounts);
}

fn open_window(h: &mut H) {
    open_menu(h, "Fichier");
    click_menu_item(h, "Pull requests…");
}

/// Titres des pull requests affichées, dans l'ordre de la liste (de haut en bas).
fn shown(h: &H) -> Vec<&'static str> {
    let mut found: Vec<(f32, &'static str)> =
        ALL.iter().filter_map(|t| h.query_by_label(t).map(|n| (n.rect().top(), *t))).collect();
    found.sort_by(|a, b| a.0.total_cmp(&b.0));
    found.into_iter().map(|(_, t)| t).collect()
}

/// Adresse ouverte par la dernière image (celle d'un clic de `click_label`).
fn opened_url(h: &H) -> Option<String> {
    h.output().platform_output.commands.iter().find_map(|c| match c {
        egui::OutputCommand::OpenUrl(u) => Some(u.url.clone()),
        _ => None,
    })
}

#[test]
fn the_pull_requests_of_every_account_are_listed_together_with_filters() {
    let tmp = tempfile::tempdir().unwrap();
    let clone = local_clone(tmp.path());
    seed(tmp.path(), three_accounts());
    let mut h = harness(StartOptions { pull_request_source: Some(fake_forges), ..start(tmp.path()) });
    open_window(&mut h);

    // Tous les comptes dans une liste, de la plus récemment mise à jour à la plus ancienne.
    wait_until(&mut h, "pull requests de GitHub et d'Azure DevOps", |h| shown(h).len() == 3);
    assert_eq!(shown(&h), [HOME, RELEASE, DISCOUNTS]);
    assert!(h.query_by_label("3 pull requests").is_some());
    // Le jeton refusé par GitLab est signalé à la place du compte, sans empêcher les autres.
    let error = "GitLab · jsmith (gitlab.contoso.test) : Accès refusé";
    assert!(h.query_by_label_contains(error).is_some(), "erreur du compte GitLab affichée");
    // L'organisation Azure DevOps qui n'a pas répondu est signalée, sans cacher celle qui a répondu.
    let warning = "Azure DevOps · Camille Martin : l'organisation contoso n'a pas répondu";
    assert!(h.query_by_label_contains(warning).is_some(), "organisation en échec signalée");
    assert!(h.query_by_label("Brouillon").is_some());

    // Filtres : créées par moi, à relire, par compte, texte.
    click_label(&mut h, "Créées par moi");
    wait_until(&mut h, "créées par moi", |h| shown(h) == [HOME]);
    click_label(&mut h, "À relire");
    wait_until(&mut h, "à relire", |h| shown(h) == [RELEASE, DISCOUNTS]);
    click_label(&mut h, "Toutes");
    wait_until(&mut h, "toutes", |h| shown(h).len() == 3);
    h.get_by_role(egui::accesskit::Role::ComboBox).click();
    h.run_steps(3);
    // L'entrée de la liste des comptes (l'avertissement de l'organisation porte aussi le titre du compte).
    click_by(&mut h, "compte Azure DevOps dans la liste", egui::PointerButton::Primary, |n| {
        n.role() == egui::accesskit::Role::Button
            && text_of(n).is_some_and(|l| l.ends_with("Azure DevOps · Camille Martin"))
    });
    wait_until(&mut h, "compte Azure DevOps seulement", |h| shown(h) == [RELEASE]);
    assert!(h.query_by_label_contains(error).is_none(), "l'erreur d'un autre compte n'est plus affichée");
    h.get_by_role(egui::accesskit::Role::ComboBox).click();
    h.run_steps(3);
    click_label(&mut h, "Tous les comptes");
    wait_until(&mut h, "tous les comptes", |h| shown(h).len() == 3);
    let field =
        h.get_by(|n| n.placeholder().is_some_and(|p| p.ends_with("Filtrer par titre, dépôt, auteur ou branche")));
    field.focus();
    field.type_text("remises billing");
    h.run_steps(3);
    wait_until(&mut h, "filtre texte", |h| shown(h) == [DISCOUNTS]);
    for _ in 0.."remises billing".len() {
        h.key_press(Key::Backspace);
    }
    wait_until(&mut h, "filtre texte effacé", |h| shown(h).len() == 3);

    // Le titre ouvre la pull request dans le navigateur.
    click_label(&mut h, HOME);
    assert_eq!(opened_url(&h).as_deref(), Some("https://github.com/contoso/northwind/pull/1347"));

    // Le dépôt cloné sous le dossier scanné s'ouvre dans un onglet (les autres dépôts ne sont pas clonés).
    wait_until(&mut h, "clone local reconnu", |h| h.query_by_label_contains("Ouvrir le dépôt").is_some());
    assert_eq!(h.query_all_by_label_contains("Ouvrir le dépôt").count(), 1);
    click_label_containing(&mut h, "Ouvrir le dépôt");
    wait_until(&mut h, "onglet du dépôt ouvert", |_| {
        Settings::load_from(&settings_path(tmp.path())).open_tabs.iter().any(|p| path_key(p) == path_key(&clone))
    });
}

#[test]
fn accounts_that_could_not_be_read_do_not_show_an_empty_list() {
    let tmp = tempfile::tempdir().unwrap();
    // Un jeton refusé (GitLab) et un compte Azure DevOps sans organisation, jamais lu (`fake_forges` paniquerait).
    let mut no_orgs = account("compte-sans-org", ForgeKind::AzureDevOps, "dev.azure.com", "camille@example.com", "");
    no_orgs.orgs.clear();
    seed(tmp.path(), vec![three_accounts().swap_remove(1), no_orgs]);
    let mut h = harness(StartOptions { pull_request_source: Some(fake_forges), ..start(tmp.path()) });
    open_window(&mut h);
    wait_until(&mut h, "erreur du compte GitLab", |h| h.query_by_label_contains("Accès refusé").is_some());
    let explanation = "Azure DevOps · camille@example.com : aucune organisation connue pour ce compte.";
    assert!(h.query_by_label_contains(explanation).is_some(), "compte sans organisation expliqué");
    assert!(h.query_by_label("Aucune pull request ouverte.").is_none(), "aucun compte n'a répondu");
    assert!(h.query_by_label("0 pull request").is_some());
}

#[test]
fn without_any_account_the_window_offers_to_add_one() {
    let tmp = tempfile::tempdir().unwrap();
    seed(tmp.path(), vec![]);
    let mut h = harness(StartOptions {
        pull_request_source: Some(|_| panic!("aucun compte : rien à lire")),
        ..start(tmp.path())
    });
    // Par la palette de commandes.
    h.key_press_modifiers(egui::Modifiers::COMMAND, Key::P);
    let hint = "Dépôt, branche ou commande…";
    wait_until(&mut h, "palette", |h| h.query_by(|n| n.placeholder() == Some(hint)).is_some());
    h.get_by(|n| n.placeholder() == Some(hint)).type_text("Pull requests");
    h.run_steps(3);
    h.key_press(Key::Enter);
    wait_until(&mut h, "fenêtre vide", |h| h.query_by_label("Aucun compte connecté.").is_some());
    click_label_containing(&mut h, "Ajouter un compte…");
    wait_until(&mut h, "fenêtre Comptes", |h| h.query_by_label("Aucun compte pour l'instant.").is_some());
}

/// Un hôte (le téléphone de la suite) fait relire les pull requests sans ouvrir la fenêtre et lit ce qui l'a été :
/// chaque compte et son erreur à sa place, toutes les pull requests de la plus récente à la plus ancienne, avec les
/// copies locales de leur dépôt (relues fenêtre fermée) ; une seconde demande pendant la lecture n'en lance pas une
/// autre.
#[test]
fn a_host_reads_the_pull_requests_without_the_window() {
    let tmp = tempfile::tempdir().unwrap();
    let clone = local_clone(tmp.path());
    seed(tmp.path(), three_accounts());
    let mut h = harness(StartOptions { pull_request_source: Some(fake_forges), ..start(tmp.path()) });
    wait_until(&mut h, "comptes lus", |h| h.state().pull_requests().accounts.len() == 3);
    let before = h.state().pull_requests();
    assert!(before.prs.is_empty() && before.refreshed.is_none(), "rien de lu sans demande");
    assert!(before.accounts.iter().all(|a| !a.read && !a.loading));

    assert!(h.state_mut().refresh_pull_requests());
    assert!(h.state().pull_requests().loading);
    assert!(!h.state_mut().refresh_pull_requests(), "une lecture est déjà en cours");
    wait_until(&mut h, "pull requests lues", |h| {
        let s = h.state().pull_requests();
        !s.loading && s.prs.len() == 3
    });
    let s = h.state().pull_requests();
    let titles: Vec<&str> = s.prs.iter().map(|p| p.pr.title.as_str()).collect();
    assert_eq!(titles, [HOME, RELEASE, DISCOUNTS], "de la plus récente à la plus ancienne");
    assert_eq!(s.prs[0].account, "compte-github");
    assert!(s.refreshed.is_some() && s.refreshed_at.is_some());
    let gitlab = &s.accounts[1];
    assert!(gitlab.read && gitlab.error.as_deref().is_some_and(|e| e.contains("Accès refusé")), "{gitlab:?}");
    assert_eq!(s.accounts[2].failed_orgs.iter().map(|(org, _)| org.as_str()).collect::<Vec<_>>(), ["contoso"]);
    // La copie locale du dépôt de la première, relue fenêtre fermée (aussi à la fin du scan des dépôts).
    wait_until(&mut h, "copie locale reconnue", |h| {
        h.state().pull_requests().prs[0].local.iter().any(|p| path_key(p) == path_key(&clone))
    });
    assert!(h.state().pull_requests().prs[1].local.is_empty(), "pas clonée");
    assert!(h.query_by_label("3 pull requests").is_none(), "la fenêtre reste fermée");
}
