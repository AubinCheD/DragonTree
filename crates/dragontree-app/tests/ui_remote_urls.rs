//! Menu d'un remote : modifier l'URL (et l'URL de push distincte) et tester la connexion, avec un remote local.

mod common;

use std::path::{Path, PathBuf};

use common::*;
use dragontree_app::icons;
use dragontree_app::{DragonTreeApp, StartOptions};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

type H = Harness<'static, DragonTreeApp>;

fn file_url(path: &Path) -> String {
    format!("file:///{}", path.display().to_string().replace('\\', "/"))
}

/// Dépôt `fabrikam` dont le remote `origin` pointe sur `url`.
fn repo_with_origin(base: &Path, url: &str) -> PathBuf {
    let dir = base.join("fabrikam");
    init_repo(&dir);
    commit_file(&dir, "a.txt", "1\n", "Premier commit");
    git(&dir, &["remote", "add", "origin", url]);
    dir
}

fn open(base: &Path, repo: &Path) -> H {
    let mut h = harness(StartOptions { open: vec![repo.to_path_buf()], view: Some("history".into()), ..start(base) });
    wait_until(&mut h, "dépôt chargé", |h| h.query_by_label("Premier commit").is_some());
    h
}

/// Ouvre le menu du remote `origin` (ligne de la section Remotes) et clique l'entrée.
fn remote_menu(h: &mut H, entry: &str) {
    wait_until(h, "remote dans la barre latérale", |h| h.query_by_label("origin").is_some());
    h.get_by_label("origin").click_secondary();
    wait_until(h, &format!("entrée « {entry} »"), |h| h.query_by_label(entry).is_some());
    h.get_by_label(entry).click();
    h.run_steps(3);
}

/// Remplace le contenu du champ de saisie qui contient `old` par `new`.
fn replace_field(h: &mut H, old: &str, new: &str) {
    wait_until(h, &format!("champ « {old} »"), |h| has_text_input(h, old));
    let field = h.get_by(|n| n.role() == egui::accesskit::Role::TextInput && n.value().as_deref() == Some(old));
    field.focus();
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    h.run_steps(1);
    h.get_by(|n| n.role() == egui::accesskit::Role::TextInput && n.is_focused()).type_text(new);
    h.run_steps(2);
}

fn edit_label() -> String {
    format!("{} Modifier l'URL…", icons::PENCIL_SIMPLE)
}

fn test_label() -> String {
    format!("{} Tester la connexion", icons::PLUGS_CONNECTED)
}

#[test]
fn editing_the_url_from_the_remote_menu_changes_git_remote_get_url() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo_with_origin(tmp.path(), "https://git.fabrikam.test/ancien.git");
    let mut h = open(tmp.path(), &repo);
    remote_menu(&mut h, &edit_label());
    replace_field(&mut h, "https://git.fabrikam.test/ancien.git", "https://git.fabrikam.test/nouveau.git");
    h.get_by_label("Enregistrer").click();
    wait_git(&mut h, "URL modifiée", || {
        try_git(&repo, &["remote", "get-url", "origin"])
            .is_some_and(|u| u.trim() == "https://git.fabrikam.test/nouveau.git")
    });
}

#[test]
fn a_distinct_push_url_has_its_own_field() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo_with_origin(tmp.path(), "https://git.fabrikam.test/depot.git");
    git(&repo, &["remote", "set-url", "--push", "origin", "ssh://git@fabrikam.test/depot.git"]);
    let mut h = open(tmp.path(), &repo);
    remote_menu(&mut h, &edit_label());
    wait_until(&mut h, "deux champs", |h| {
        has_text_input(h, "https://git.fabrikam.test/depot.git")
            && has_text_input(h, "ssh://git@fabrikam.test/depot.git")
    });
    replace_field(&mut h, "ssh://git@fabrikam.test/depot.git", "ssh://git@fabrikam.test/autre.git");
    h.get_by_label("Enregistrer").click();
    wait_git(&mut h, "URL de push modifiée", || {
        try_git(&repo, &["remote", "get-url", "--push", "origin"])
            .is_some_and(|u| u.trim() == "ssh://git@fabrikam.test/autre.git")
    });
    assert_eq!(git(&repo, &["remote", "get-url", "origin"]).trim(), "https://git.fabrikam.test/depot.git");
}

#[test]
fn testing_a_reachable_remote_reports_success() {
    let tmp = tempfile::tempdir().unwrap();
    let bare = tmp.path().join("serveur.git");
    std::fs::create_dir_all(&bare).unwrap();
    git(&bare, &["init", "-q", "--bare", "-b", "main"]);
    let repo = repo_with_origin(tmp.path(), &file_url(&bare));
    git(&repo, &["push", "-q", "origin", "main"]);
    let mut h = open(tmp.path(), &repo);
    remote_menu(&mut h, &test_label());
    wait_until(&mut h, "succès signalé", |h| {
        h.query_by_label_contains("Connexion à origin réussie : 1 branche").is_some()
    });
}

#[test]
fn testing_an_unreachable_remote_reports_git_s_message() {
    let tmp = tempfile::tempdir().unwrap();
    let missing = tmp.path().join("n-existe-pas.git");
    let repo = repo_with_origin(tmp.path(), &file_url(&missing));
    let mut h = open(tmp.path(), &repo);
    remote_menu(&mut h, &test_label());
    wait_until(&mut h, "échec signalé", |h| h.query_by_label_contains("Test de la connexion a échoué").is_some());
    wait_until(&mut h, "message de git", |h| h.query_by_label_contains("n-existe-pas").is_some());
}
