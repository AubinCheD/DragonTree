//! Stashs complets : liste déroulante de Pop, renommage, stash des fichiers sélectionnés, snapshot, fichiers non
//! suivis dans le diff d'un stash.

mod common;

use std::path::{Path, PathBuf};

use common::*;
use dragontree_app::StartOptions;
use egui_kittest::kittest::{AccessKitNode, Queryable};

type H = egui_kittest::Harness<'static, dragontree_app::DragonTreeApp>;

/// Deux stashs sur `f.txt` : « premier » (contenu `un`) puis « second » (contenu `deux`, le plus récent).
fn two_stashes(base: &Path) -> PathBuf {
    let repo = base.join("fabrikam");
    init_repo(&repo);
    commit_file(&repo, "f.txt", "base\n", "Commit de base");
    std::fs::write(repo.join("f.txt"), "un\n").unwrap();
    git(&repo, &["stash", "push", "-q", "-m", "premier"]);
    std::fs::write(repo.join("f.txt"), "deux\n").unwrap();
    git(&repo, &["stash", "push", "-q", "-m", "second"]);
    repo
}

fn open(repo: &Path, base: &Path, view: &str) -> H {
    harness(StartOptions { open: vec![repo.to_path_buf()], view: Some(view.into()), ..start(base) })
}

/// Ligne de fichier (un bouton) de libellé `name`, pas le titre du diff du fichier sélectionné d'office, qui porte
/// le même nom.
fn file_row(name: &str) -> impl Fn(&AccessKitNode<'_>) -> bool + '_ {
    move |n| n.role() == egui::accesskit::Role::Button && n.label().as_deref() == Some(name)
}

fn stash_list(repo: &Path) -> String {
    // L'application réécrit refs/stash pendant un renommage : une lecture qui échoue vaut « pas encore ».
    try_git(repo, &["stash", "list", "--format=%gs"]).unwrap_or_default()
}

#[test]
fn the_pop_dropdown_applies_the_second_stash() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = two_stashes(tmp.path());
    let mut h = open(&repo, tmp.path(), "history");
    wait_until(&mut h, "stashs lus", |h| h.query_by_label("stash@{1}  On main: premier").is_some());
    click_label(&mut h, "Choisir le stash");
    // Le menu liste les deux stashs ; on ouvre le second (stash@{1}, « premier »).
    let entry = "stash@{1}  On main: premier";
    wait_until(&mut h, "liste des stashs", |h| h.query_all_by_label_contains(entry).count() == 2);
    assert!(menu_item(&h, "stash@{0}  On main: second").is_some());
    click_menu_item(&mut h, entry);
    let apply = format!("{} Appliquer", dragontree_app::icons::TRAY_ARROW_UP);
    click_menu_item(&mut h, &apply);
    wait_git(&mut h, "stash appliqué", || std::fs::read_to_string(repo.join("f.txt")).unwrap() == "un\n");
    // Appliquer ne supprime pas le stash.
    assert_eq!(stash_list(&repo).lines().count(), 2);
}

#[test]
fn a_stash_is_renamed_from_the_sidebar_section() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = two_stashes(tmp.path());
    let mut h = open(&repo, tmp.path(), "history");
    let label = "stash@{1}  On main: premier";
    wait_until(&mut h, "stash dans la barre latérale", |h| h.query_by_label(label).is_some());
    right_click_label(&mut h, label);
    click_label_containing(&mut h, "Renommer…");
    wait_until(&mut h, "dialogue de renommage", |h| has_text_input(h, "On main: premier"));
    let field =
        h.get_by(|n| n.role() == egui::accesskit::Role::TextInput && n.value().as_deref() == Some("On main: premier"));
    field.focus();
    field.type_text(" renommé");
    h.run_steps(2);
    click_label(&mut h, "Renommer");
    // Le renommage enregistre le nouveau stash puis supprime l'ancien : on attend l'état final (deux entrées),
    // pas l'instant où le nouveau message apparaît.
    wait_git(&mut h, "renommage terminé", || {
        let list = stash_list(&repo);
        list.lines().count() == 2 && list.contains("renommé")
    });
    let after = stash_list(&repo);
    let lines: Vec<&str> = after.lines().collect();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0], "On main: second", "position conservée");
    assert!(lines[1].contains("premier") && lines[1].contains("renommé"), "{after}");
}

#[test]
fn selected_files_are_stashed_from_the_changes_view() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("northwind");
    init_repo(&repo);
    commit_file(&repo, "a.txt", "a\n", "Commit de base");
    commit_file(&repo, "b.txt", "b\n", "Second commit");
    std::fs::write(repo.join("a.txt"), "a2\n").unwrap();
    std::fs::write(repo.join("b.txt"), "b2\n").unwrap();
    let mut h = open(&repo, tmp.path(), "changes");
    wait_until(&mut h, "fichiers modifiés", |h| {
        h.query_by(file_row("a.txt")).is_some() && h.query_by(file_row("b.txt")).is_some()
    });
    click_by(&mut h, "ligne de a.txt", egui::PointerButton::Secondary, file_row("a.txt"));
    click_label_containing(&mut h, "Stash des fichiers sélectionnés…");
    wait_until(&mut h, "dialogue", |h| h.query_by_label("Mettre de côté les fichiers").is_some());
    click_label(&mut h, "Mettre de côté");
    // Le stash est listé avant que la copie de travail soit remise à zéro : on attend les deux.
    let read = |name: &str| std::fs::read_to_string(repo.join(name)).unwrap_or_default();
    wait_git(&mut h, "stash créé et fichier remis à zéro", || {
        !stash_list(&repo).trim().is_empty() && read("a.txt") == "a\n"
    });
    assert_eq!(read("b.txt"), "b2\n", "l'autre fichier reste modifié");
}

#[test]
fn the_snapshot_command_keeps_the_changes_in_place() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("contoso");
    init_repo(&repo);
    commit_file(&repo, "a.txt", "a\n", "Commit de base");
    std::fs::write(repo.join("a.txt"), "a2\n").unwrap();
    let mut h = open(&repo, tmp.path(), "history");
    wait_until(&mut h, "commit affiché", |h| h.query_by_label("Commit de base").is_some());
    wait_until(&mut h, "statut lu", |h| h.query_by_label("Pop").is_some());
    open_menu(&mut h, "Dépôt");
    click_menu_item(&mut h, "Snapshot…");
    wait_until(&mut h, "dialogue", |h| h.query_by_label("Enregistrer un snapshot").is_some());
    click_label(&mut h, "Enregistrer");
    wait_git(&mut h, "snapshot créé", || !stash_list(&repo).trim().is_empty());
    assert_eq!(std::fs::read_to_string(repo.join("a.txt")).unwrap(), "a2\n", "copie de travail intacte");
}

#[test]
fn the_untracked_files_of_a_stash_are_listed_in_its_changes() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("fabrikam");
    init_repo(&repo);
    commit_file(&repo, "f.txt", "base\n", "Commit de base");
    std::fs::write(repo.join("f.txt"), "modifié\n").unwrap();
    std::fs::write(repo.join("neuf.txt"), "non suivi\n").unwrap();
    git(&repo, &["stash", "push", "-q", "-u", "-m", "mon stash"]);
    let mut h = open(&repo, tmp.path(), "history");
    wait_until(&mut h, "ligne de stash", |h| h.query_all_by_label("On main: mon stash").count() == 1);
    click_label(&mut h, "On main: mon stash");
    wait_until(&mut h, "fichier suivi", |h| h.query_by(file_row("f.txt")).is_some());
    // Son diff s'affiche (contenu ajouté).
    click_by(&mut h, "fichier non suivi", egui::PointerButton::Primary, file_row("neuf.txt"));
    wait_until(&mut h, "diff du fichier non suivi (une ligne ajoutée)", |h| {
        h.query_by_label_contains("+1  -0").is_some()
    });
}
