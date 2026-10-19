//! Dépôt › Mettre à jour les branches locales et Supprimer les branches périmées, avec un remote local (bare).

mod common;

use std::path::{Path, PathBuf};

use common::*;
use dragontree_app::StartOptions;
use egui_kittest::kittest::{NodeT, Queryable};

type H = egui_kittest::Harness<'static, dragontree_app::DragonTreeApp>;

/// Remote nu et dépôt de travail cloné, avec un premier commit poussé sur `main`.
fn setup(base: &Path) -> (PathBuf, PathBuf) {
    let bare = base.join("remote.git");
    std::fs::create_dir_all(&bare).unwrap();
    git(&bare, &["init", "-q", "--bare", "-b", "main"]);
    let url = format!("file:///{}", bare.display().to_string().replace('\\', "/"));
    let work = base.join("travail");
    git(base, &["clone", "-q", &url, work.to_str().unwrap()]);
    for (k, v) in [("user.name", "Testeur"), ("user.email", "t@example.com"), ("commit.gpgsign", "false")] {
        git(&work, &["config", k, v]);
    }
    commit_file(&work, "a.txt", "1\n", "Premier commit");
    git(&work, &["push", "-q", "-u", "origin", "main"]);
    (bare, work)
}

fn open(base: &Path, repo: &Path) -> H {
    let mut h = harness(StartOptions { open: vec![repo.to_path_buf()], view: Some("history".into()), ..start(base) });
    wait_until(&mut h, "dépôt chargé", |h| h.query_by_label("Premier commit").is_some());
    h
}

/// Clique le bouton dès qu'il est utilisable, en une image : le dialogue centré bouge tant que son contenu change.
fn click_button(h: &mut H, label: &str) {
    click_label(h, label);
    h.run_steps(3);
}

#[test]
fn stale_branches_dialog_lists_a_gone_branch_and_deletes_it() {
    let tmp = tempfile::tempdir().unwrap();
    let (_bare, work) = setup(tmp.path());
    // `ancienne` : poussée puis supprimée sur le remote (l'amont a disparu après un fetch --prune).
    git(&work, &["branch", "ancienne"]);
    git(&work, &["push", "-q", "-u", "origin", "ancienne"]);
    git(&work, &["push", "-q", "origin", "--delete", "ancienne"]);
    git(&work, &["fetch", "-q", "--prune"]);
    assert!(git(&work, &["branch", "-vv"]).contains(": gone]"));

    let mut h = open(tmp.path(), &work);
    open_menu(&mut h, "Dépôt");
    click_menu_item(&mut h, "Supprimer les branches périmées…");
    // La liste est lue en tâche de fond : la case apparaît ensuite, décochée.
    wait_until(&mut h, "branche périmée listée", |h| {
        h.query_by_label_contains("branche suivie supprimée sur le remote").is_some()
    });
    h.run_steps(3);
    // Rien de coché : la suppression n'est pas permise.
    assert!(h.get_by_label("Supprimer").accesskit_node().is_disabled());
    click_label_containing(&mut h, "branche suivie supprimée sur le remote");
    h.run_steps(3);
    click_button(&mut h, "Supprimer");
    wait_git(&mut h, "branche supprimée", || {
        try_git(&work, &["rev-parse", "--verify", "--quiet", "refs/heads/ancienne"]).is_none()
    });
    assert!(git(&work, &["branch", "--list", "main"]).contains("main"));
}

#[test]
fn update_branches_fast_forwards_checked_branches_and_reports_the_count() {
    let tmp = tempfile::tempdir().unwrap();
    let (bare, work) = setup(tmp.path());
    git(&work, &["branch", "-q", "suite", "main"]);
    git(&work, &["push", "-q", "-u", "origin", "suite"]);
    // Un collègue fait avancer `suite` sur le remote ; le fetch la met en retard localement.
    let other = tmp.path().join("collegue");
    git(tmp.path(), &["clone", "-q", bare.to_str().unwrap(), other.to_str().unwrap()]);
    for (k, v) in [("user.name", "Autre"), ("user.email", "o@example.com"), ("commit.gpgsign", "false")] {
        git(&other, &["config", k, v]);
    }
    git(&other, &["checkout", "-q", "suite"]);
    commit_file(&other, "s.txt", "s\n", "Avance du collègue");
    git(&other, &["push", "-q", "origin", "suite"]);
    git(&work, &["fetch", "-q"]);
    let target = git(&other, &["rev-parse", "HEAD"]).trim().to_string();

    let mut h = open(tmp.path(), &work);
    // L'entrée n'est utilisable qu'une fois les branches lues.
    wait_until(&mut h, "branches lues", |h| h.query_all_by_label_contains("suite").count() > 0);
    open_menu(&mut h, "Dépôt");
    wait_until(&mut h, "entrée utilisable", |h| {
        menu_item(h, "Mettre à jour les branches locales…").is_some_and(|n| !n.accesskit_node().is_disabled())
    });
    click_menu_item(&mut h, "Mettre à jour les branches locales…");
    wait_until(&mut h, "branche proposée", |h| h.query_by_label_contains("en retard de 1 commit").is_some());
    click_button(&mut h, "Mettre à jour");
    wait_git(&mut h, "branche avancée", || {
        try_git(&work, &["rev-parse", "refs/heads/suite"]).is_some_and(|s| s.trim() == target)
    });
    wait_until(&mut h, "rapport", |h| h.query_by_label_contains("1 branche mise à jour").is_some());
    assert_eq!(git(&work, &["branch", "--show-current"]).trim(), "main");
}
