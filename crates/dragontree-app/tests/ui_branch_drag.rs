//! Glisser une branche de la barre latérale sur une autre : menu fusionner / rebaser, simulé par des événements
//! pointeur (appui sur la ligne source, déplacements, relâchement sur la ligne cible).

mod common;

use std::path::{Path, PathBuf};

use common::*;
use dragontree_app::{DragonTreeApp, StartOptions};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

/// Dépôt `fabrikam` : `main` (courante), `feature` (un commit de plus) et `release` (sur le premier commit).
fn repo(base: &Path) -> PathBuf {
    let dir = base.join("fabrikam");
    init_repo(&dir);
    commit_file(&dir, "a.txt", "1\n", "Premier commit");
    git(&dir, &["branch", "release"]);
    git(&dir, &["checkout", "-q", "-b", "feature"]);
    commit_file(&dir, "b.txt", "2\n", "Travail sur la fonction");
    git(&dir, &["checkout", "-q", "main"]);
    dir
}

fn open(base: &Path, repo: &Path) -> Harness<'static, DragonTreeApp> {
    let mut h = harness(StartOptions { open: vec![repo.to_path_buf()], view: Some("history".into()), ..start(base) });
    wait_until(&mut h, "dépôt chargé", |h| h.query_by_label("Premier commit").is_some());
    h
}

/// Centre de la ligne de la barre latérale de la branche `name` (l'élément le plus à gauche de ce libellé).
fn row_center(h: &Harness<'static, DragonTreeApp>, name: &str) -> egui::Pos2 {
    h.query_all_by_label(name)
        .map(|n| n.rect().center())
        .min_by(|a, b| a.x.total_cmp(&b.x))
        .unwrap_or_else(|| panic!("ligne de {name} absente"))
}

/// Amène le pointeur en `pos` et, avec `pressed`, y appuie ou relâche le bouton principal, le tout en une image :
/// egui rapporte l'appui ou le relâchement à la disposition de l'image précédente, celle d'où vient `pos`, même si la
/// barre latérale bouge encore (sections lues en tâche de fond).
fn pointer_at(h: &mut Harness<'static, DragonTreeApp>, pos: egui::Pos2, pressed: Option<bool>) {
    let events = &mut h.input_mut().events;
    events.push(egui::Event::PointerMoved(pos));
    if let Some(pressed) = pressed {
        let button = egui::PointerButton::Primary;
        events.push(egui::Event::PointerButton { pos, button, pressed, modifiers: egui::Modifiers::NONE });
    }
    h.step();
}

/// Glisse la ligne `from` jusqu'à la ligne `to` et la relâche ; la cible est relue à chaque image.
fn drag_row(h: &mut Harness<'static, DragonTreeApp>, from: &str, to: &str) {
    wait_until(h, "lignes des branches", |h| {
        h.query_all_by_label(from).count() > 0 && h.query_all_by_label(to).count() > 0
    });
    let start = row_center(h, from);
    pointer_at(h, start, Some(true));
    // Au-delà du seuil de glisser, par étapes.
    for i in 1..=4 {
        let end = row_center(h, to);
        pointer_at(h, start + (end - start) * (i as f32 / 4.0), None);
    }
    let end = row_center(h, to);
    pointer_at(h, end, Some(false));
    h.remove_cursor();
    h.run_steps(3);
}

#[test]
fn dragging_a_branch_onto_the_current_branch_offers_to_merge_it() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo(tmp.path());
    let mut h = open(tmp.path(), &repo);

    drag_row(&mut h, "feature", "main");
    let merge = "Fusionner « feature » dans « main »…";
    wait_until(&mut h, "menu du dépôt", |h| h.query_by_label_contains(merge).is_some());
    assert!(h.query_by_label_contains("Rebaser « main » sur « feature »…").is_some());

    click_label_containing(&mut h, merge);
    wait_until(&mut h, "dialogue de fusion", |h| h.query_by_label("Fusionner « feature » dans « main »").is_some());
    // Rien n'a été lancé avant la confirmation.
    assert_eq!(git(&repo, &["branch", "--show-current"]).trim(), "main");
    assert!(!repo.join("b.txt").exists());
}

#[test]
fn dragging_between_two_other_branches_checks_out_the_target_then_opens_the_merge() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo(tmp.path());
    let mut h = open(tmp.path(), &repo);

    drag_row(&mut h, "feature", "release");
    let entry = "Checkout de « release » puis fusionner « feature »…";
    wait_until(&mut h, "menu du dépôt", |h| h.query_by_label_contains(entry).is_some());
    click_label_containing(&mut h, entry);
    wait_until(&mut h, "dialogue de fusion", |h| h.query_by_label("Fusionner « feature » dans « release »").is_some());
    assert_eq!(git(&repo, &["branch", "--show-current"]).trim(), "release");
}

#[test]
fn dragging_onto_a_branch_checked_out_elsewhere_offers_its_worktree_instead() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo(tmp.path());
    let wt = tmp.path().join("fabrikam-release");
    git(&repo, &["worktree", "add", "-q", wt.to_str().unwrap(), "release"]);
    let mut h = open(tmp.path(), &repo);
    // Le worktree lu (section Worktrees, ligne « branche — chemin ») : sans lui, le checkout serait tenté et refusé.
    wait_until(&mut h, "worktree listé", |h| {
        h.query_by(|n| n.label().is_some_and(|l| l.starts_with("release — "))).is_some()
    });

    drag_row(&mut h, "feature", "release");
    let entry = "Checkout de « release » puis fusionner « feature »…";
    wait_until(&mut h, "menu du dépôt", |h| h.query_by_label_contains(entry).is_some());
    click_label_containing(&mut h, entry);
    wait_until(&mut h, "proposition d'ouvrir le worktree", |h| h.query_by_label("Ouvrir le worktree").is_some());
    assert_eq!(git(&repo, &["branch", "--show-current"]).trim(), "main");
}

#[test]
fn a_simple_click_on_a_branch_row_starts_no_drag() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo(tmp.path());
    let mut h = open(tmp.path(), &repo);
    wait_until(&mut h, "ligne feature", |h| h.query_all_by_label("feature").count() > 0);
    let pos = row_center(&h, "feature");
    pointer_at(&mut h, pos, Some(true));
    pointer_at(&mut h, pos, Some(false));
    h.remove_cursor();
    h.run_steps(3);
    assert!(h.query_by_label_contains("Fusionner").is_none(), "pas de menu de dépôt après un clic");
}

/// `repo` plus un remote `origin` (dépôt nu voisin) portant la branche `hotfix`, absente en local.
fn with_remote_branch(base: &Path, repo: &Path) {
    let bare = base.join("origin.git");
    git(base, &["init", "-q", "--bare", bare.to_str().unwrap()]);
    git(repo, &["remote", "add", "origin", bare.to_str().unwrap()]);
    git(repo, &["checkout", "-q", "-b", "hotfix"]);
    commit_file(repo, "c.txt", "3\n", "Correctif");
    git(repo, &["push", "-q", "origin", "hotfix"]);
    git(repo, &["checkout", "-q", "main"]);
    git(repo, &["branch", "-q", "-D", "hotfix"]);
    git(repo, &["fetch", "-q", "origin"]);
}

#[test]
fn dragging_a_remote_branch_onto_the_current_branch_offers_to_merge_it() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo(tmp.path());
    with_remote_branch(tmp.path(), &repo);
    let mut h = open(tmp.path(), &repo);

    drag_row(&mut h, "hotfix", "main");
    let merge = "Fusionner « origin/hotfix » dans « main »…";
    wait_until(&mut h, "menu du dépôt", |h| h.query_by_label_contains(merge).is_some());
    assert!(h.query_by_label_contains("Rebaser « main » sur « origin/hotfix »…").is_some());
    click_label_containing(&mut h, merge);
    wait_until(&mut h, "dialogue de fusion", |h| {
        h.query_by_label("Fusionner « origin/hotfix » dans « main »").is_some()
    });
}

#[test]
fn a_failed_checkout_opens_no_merge_dialog_and_keeps_the_current_branch() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo(tmp.path());
    // `release` a un a.txt différent : un a.txt modifié et non commité empêche d'y basculer.
    git(&repo, &["checkout", "-q", "release"]);
    commit_file(&repo, "a.txt", "autre\n", "Autre version");
    git(&repo, &["checkout", "-q", "main"]);
    std::fs::write(repo.join("a.txt"), "modifié localement\n").unwrap();
    let mut h = open(tmp.path(), &repo);

    drag_row(&mut h, "feature", "release");
    let entry = "Checkout de « release » puis fusionner « feature »…";
    wait_until(&mut h, "menu du dépôt", |h| h.query_by_label_contains(entry).is_some());
    click_label_containing(&mut h, entry);
    wait_until(&mut h, "erreur de checkout", |h| h.query_by_label_contains("Checkout a échoué").is_some());
    h.run_steps(5);
    assert!(h.query_by_label_contains("Fusionner « feature »").is_none(), "pas de dialogue de fusion");
    assert_eq!(git(&repo, &["branch", "--show-current"]).trim(), "main");
}

#[test]
fn a_click_on_a_branch_row_still_selects_its_commit_in_the_history() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo(tmp.path());
    let mut h = harness(StartOptions { open: vec![repo.clone()], view: Some("changes".into()), ..start(tmp.path()) });
    wait_until(&mut h, "ligne feature", |h| h.query_all_by_label("feature").count() > 0);
    assert!(h.query_by_label("Travail sur la fonction").is_none(), "vue Changements : pas d'historique");
    let pos = row_center(&h, "feature");
    pointer_at(&mut h, pos, Some(true));
    pointer_at(&mut h, pos, Some(false));
    h.remove_cursor();
    wait_until(&mut h, "historique affiché", |h| h.query_by_label("Travail sur la fonction").is_some());
    assert!(h.query_by_label_contains("Fusionner").is_none(), "un clic n'ouvre pas de menu de dépôt");
}
