//! Dépôt › Statistiques… : la fenêtre calcule en arrière-plan et affiche les comptes du dépôt.

mod common;

use std::path::{Path, PathBuf};

use common::*;
use dragontree_app::StartOptions;
use egui_kittest::kittest::Queryable;

/// main : 3 commits ; `fonction` : 2 commits de plus (5 au total, un contributeur).
fn repo(base: &Path) -> PathBuf {
    let dir = base.join("fabrikam");
    init_repo(&dir);
    commit_file(&dir, "a.txt", "1\n", "Premier commit");
    commit_file(&dir, "b.txt", "2\n", "Deuxième commit");
    commit_file(&dir, "c.txt", "3\n", "Troisième commit");
    git(&dir, &["checkout", "-q", "-b", "fonction"]);
    commit_file(&dir, "d.txt", "4\n", "Quatrième commit");
    commit_file(&dir, "e.txt", "5\n", "Cinquième commit");
    git(&dir, &["checkout", "-q", "main"]);
    dir
}

#[test]
fn the_statistics_window_shows_the_commit_counts() {
    let base = tempfile::tempdir().unwrap();
    let repo = repo(base.path());
    let mut h = harness(StartOptions { open: vec![repo], view: Some("history".into()), ..start(base.path()) });
    wait_until(&mut h, "dépôt chargé", |h| h.query_by_label("Troisième commit").is_some());

    open_menu(&mut h, "Dépôt");
    click_menu_item(&mut h, "Statistiques…");
    wait_until(&mut h, "commits de toutes les branches", |h| {
        h.query_by_label("Commits (toutes les branches) : 5").is_some()
    });
    assert!(h.query_by_label("Commits (branche courante) : 3").is_some());
    assert!(h.query_by_label("Fichiers suivis : 3").is_some());
    assert!(h.query_by_label("1 contributeur").is_some());
}
