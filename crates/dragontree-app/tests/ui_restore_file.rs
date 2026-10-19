//! Remettre un fichier dans son état à un commit : depuis les détails d'un ancien commit.

mod common;

use std::path::{Path, PathBuf};

use common::*;
use dragontree_app::{StartOptions, icons};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

type H = Harness<'static, dragontree_app::DragonTreeApp>;

fn repo(base: &Path) -> PathBuf {
    let dir = base.join("fabrikam");
    init_repo(&dir);
    commit_file(&dir, "f.txt", "version 1\n", "Première version");
    commit_file(&dir, "f.txt", "version 2\n", "Deuxième version");
    dir
}

fn open(base: &Path, repo: &Path, view: &str, ready: &str) -> H {
    let mut h = harness(StartOptions { open: vec![repo.to_path_buf()], view: Some(view.into()), ..start(base) });
    wait_until(&mut h, "dépôt chargé", |h| h.query_by_label(ready).is_some());
    h
}

fn restore_to_commit() -> String {
    format!("{} Remettre ce fichier à cette version…", icons::ARROW_COUNTER_CLOCKWISE)
}

fn restore_before_commit() -> String {
    format!("{} Remettre ce fichier à la version d'avant ce commit…", icons::ARROW_COUNTER_CLOCKWISE)
}

/// Sélectionne le commit, puis ouvre le menu de son fichier `f.txt`.
fn file_menu_of_commit(h: &mut H, summary: &str) {
    h.get_by_label(summary).click();
    wait_until(h, "fichier du commit", |h| h.query_by_label("f.txt").is_some());
    h.run_steps(3);
    h.get_by_label("f.txt").click_secondary();
    wait_until(h, "menu du fichier", |h| h.query_by_label_contains("Historique du fichier").is_some());
    // Le menu contextuel prend sa place une image après son apparition : un clic trop tôt le manque.
    h.run_steps(3);
}

#[test]
fn a_file_goes_back_to_its_content_at_an_older_commit_and_is_staged() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo(tmp.path());
    let mut h = open(tmp.path(), &repo, "history", "Première version");
    file_menu_of_commit(&mut h, "Première version");
    // Commit racine : aucune version « d'avant ».
    assert!(h.query_by_label(&restore_before_commit()).is_none());
    h.get_by_label(&restore_to_commit()).click();
    h.run_steps(3);
    wait_git(&mut h, "fichier remis à la version 1", || {
        std::fs::read_to_string(repo.join("f.txt")).is_ok_and(|s| s == "version 1\n")
    });
    wait_git(&mut h, "résultat indexé", || {
        try_git(&repo, &["diff", "--cached", "--name-only"]).is_some_and(|s| s.trim() == "f.txt")
    });
}

#[test]
fn the_version_before_a_commit_is_offered_and_restored() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo(tmp.path());
    let mut h = open(tmp.path(), &repo, "history", "Deuxième version");
    file_menu_of_commit(&mut h, "Deuxième version");
    h.get_by_label(&restore_before_commit()).click();
    h.run_steps(3);
    wait_git(&mut h, "fichier remis à la version d'avant", || {
        std::fs::read_to_string(repo.join("f.txt")).is_ok_and(|s| s == "version 1\n")
    });
}

#[test]
fn local_changes_ask_for_a_destructive_confirmation_first() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo(tmp.path());
    std::fs::write(repo.join("f.txt"), "modification locale\n").unwrap();
    // La vue Changements prouve que l'état du dépôt est lu avant de passer à l'historique.
    let mut h = open(tmp.path(), &repo, "changes", "f.txt");
    open_menu(&mut h, "Affichage");
    click_menu_item(&mut h, "Tous les commits");
    wait_until(&mut h, "historique", |h| h.query_by_label("Première version").is_some());
    file_menu_of_commit(&mut h, "Première version");
    h.get_by_label(&restore_to_commit()).click();
    h.run_steps(3);
    wait_until(&mut h, "dialogue de confirmation", |h| h.query_by_label("Remettre le fichier").is_some());
    assert_eq!(
        std::fs::read_to_string(repo.join("f.txt")).unwrap(),
        "modification locale\n",
        "rien avant la confirmation"
    );
    // Entrée ne valide pas un dialogue destructeur.
    h.key_press(egui::Key::Enter);
    h.run_steps(5);
    assert_eq!(std::fs::read_to_string(repo.join("f.txt")).unwrap(), "modification locale\n");
    h.get_by_label("Remettre le fichier").click();
    wait_git(&mut h, "fichier remis à la version 1", || {
        std::fs::read_to_string(repo.join("f.txt")).is_ok_and(|s| s == "version 1\n")
    });
}

#[test]
fn an_ignored_file_is_not_overwritten_without_confirmation() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("fabrikam");
    init_repo(&dir);
    commit_file(&dir, ".env", "valeur du commit\n", "Ajoute l'environnement");
    git(&dir, &["rm", "-q", "--cached", ".env"]);
    commit_file(&dir, ".gitignore", ".env\n", "Ignore l'environnement");
    std::fs::write(dir.join(".env"), "valeur locale\n").unwrap();
    let mut h = open(tmp.path(), &dir, "history", "Ajoute l'environnement");
    h.get_by_label("Ajoute l'environnement").click();
    wait_until(&mut h, "fichier du commit", |h| h.query_by_label(".env").is_some());
    h.run_steps(3);
    h.get_by_label(".env").click_secondary();
    wait_until(&mut h, "menu du fichier", |h| h.query_by_label(&restore_to_commit()).is_some());
    h.run_steps(3);
    h.get_by_label(&restore_to_commit()).click();
    // Le fichier ignoré est une modification locale : confirmation exigée, rien n'est écrasé d'ici là.
    wait_until(&mut h, "dialogue de confirmation", |h| h.query_by_label("Remettre le fichier").is_some());
    assert_eq!(std::fs::read_to_string(dir.join(".env")).unwrap(), "valeur locale\n");
    h.get_by_label("Remettre le fichier").click();
    wait_git(&mut h, "fichier remis", || {
        std::fs::read_to_string(dir.join(".env")).is_ok_and(|s| s == "valeur du commit\n")
    });
}
