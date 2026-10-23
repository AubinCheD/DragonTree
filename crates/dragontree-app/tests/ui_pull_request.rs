//! Dépôt › Créer une pull request : la page de création de la forge s'ouvre dans le navigateur quand aucun compte ne
//! correspond au remote. Aucune requête réseau : le remote est une adresse fictive, jamais jointe.

mod common;

use std::path::{Path, PathBuf};

use common::*;
use dragontree_app::settings::Settings;
use dragontree_app::{DragonTreeApp, StartOptions};
use dragontree_forges::{Account, ForgeKind};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};

type H = Harness<'static, DragonTreeApp>;

/// Dépôt sur la branche `topic` (un commit de plus que `main`), avec un remote GitHub fictif et sa branche `main`.
fn repo(base: &Path) -> PathBuf {
    let dir = base.join("northwind");
    init_repo(&dir);
    commit_file(&dir, "a.txt", "1\n", "Premier commit");
    git(&dir, &["remote", "add", "origin", "https://github.com/contoso/northwind.git"]);
    git(&dir, &["update-ref", "refs/remotes/origin/main", "HEAD"]);
    git(&dir, &["checkout", "-q", "-b", "topic"]);
    commit_file(&dir, "b.txt", "2\n", "Ajoute la page d'accueil");
    dir
}

/// Profil de test : pas de fetch automatique (le remote est fictif), avec ou sans compte GitHub.
fn seed(base: &Path, with_account: bool) {
    let accounts = if with_account {
        vec![Account {
            id: "0123456789abcdef".into(),
            kind: ForgeKind::GitHub,
            host: "github.com".into(),
            username: "jsmith".into(),
            display_name: "Jean Smith".into(),
            orgs: vec![],
            avatar_url: None,
        }]
    } else {
        vec![]
    };
    let settings = Settings { auto_fetch_minutes: 0, ..Settings::default() };
    settings.save_to(&base.join("profil").join("settings.json")).unwrap();
    seed_accounts(base, &accounts);
}

fn open(base: &Path, repo: &Path) -> H {
    let mut h = harness(StartOptions { open: vec![repo.to_path_buf()], view: Some("history".into()), ..start(base) });
    wait_until(&mut h, "dépôt chargé", |h| h.query_by_label("Ajoute la page d'accueil").is_some());
    h
}

/// Ouvre Dépôt › Créer une pull request… une fois l'entrée utilisable (les branches sont lues en tâche de fond).
fn open_dialog(h: &mut H) {
    open_menu(h, "Dépôt");
    wait_until(h, "entrée utilisable", |h| {
        menu_item(h, "Créer une pull request…").is_some_and(|n| !n.accesskit_node().is_disabled())
    });
    click_menu_item(h, "Créer une pull request…");
    wait_until(h, "dialogue", |h| h.query_by_label("Branche cible").is_some());
    h.run_steps(3);
}

/// Attend une demande d'ouverture d'adresse (`OpenUrl`) pendant les prochaines images.
fn opened_url(h: &mut H) -> String {
    let mut opened = None;
    wait_until(h, "adresse ouverte", |h| {
        opened = h.output().platform_output.commands.iter().find_map(|c| match c {
            egui::OutputCommand::OpenUrl(u) => Some(u.url.clone()),
            _ => None,
        });
        opened.is_some()
    });
    opened.unwrap()
}

#[test]
fn without_an_account_the_dialog_opens_the_forge_creation_page() {
    let tmp = tempfile::tempdir().unwrap();
    seed(tmp.path(), false);
    let repo = repo(tmp.path());
    let mut h = open(tmp.path(), &repo);
    open_dialog(&mut h);
    // Titre repris du dernier commit, aucun compte : on le dit et le bouton ouvre le navigateur.
    assert!(has_text_input(&h, "Ajoute la page d'accueil"));
    assert!(h.query_by_label_contains("Aucun compte ne correspond").is_some());
    assert!(h.query_by_label("Créer la pull request").is_none());
    h.get_by_label("Ouvrir dans le navigateur").click();
    assert_eq!(opened_url(&mut h), "https://github.com/contoso/northwind/compare/main...topic?expand=1");
    assert_eq!(git(&repo, &["rev-parse", "--abbrev-ref", "HEAD"]).trim(), "topic", "rien n'est modifié dans le dépôt");
}

#[test]
fn with_a_matching_account_the_api_is_the_main_action_and_the_browser_stays_available() {
    let tmp = tempfile::tempdir().unwrap();
    seed(tmp.path(), true);
    let repo = repo(tmp.path());
    let mut h = open(tmp.path(), &repo);
    open_dialog(&mut h);
    assert!(h.query_by_label_contains("GitHub · jsmith").is_some(), "le compte est affiché");
    assert!(h.query_by_label("Créer la pull request").is_some());
    // La branche n'a pas de branche suivie : le dialogue propose de la pousser d'abord.
    assert!(h.query_by_label_contains("Pousser d'abord « topic » vers origin").is_some());
    // Le navigateur reste une voie de repli (rien n'est envoyé à la forge).
    h.get_by_label("Ouvrir dans le navigateur").click();
    assert_eq!(opened_url(&mut h), "https://github.com/contoso/northwind/compare/main...topic?expand=1");
}
