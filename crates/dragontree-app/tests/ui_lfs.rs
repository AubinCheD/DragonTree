//! Git LFS : Dépôt › Git LFS (initialiser, suivre un motif, ne plus suivre). Les tests s'arrêtent proprement quand
//! `git lfs` est absent ; les verrous demandent un serveur, leur analyse est testée dans le moteur.

mod common;

use std::path::{Path, PathBuf};

use common::*;
use dragontree_app::{DragonTreeApp, StartOptions};
use dragontree_git::Git;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};

type H = Harness<'static, DragonTreeApp>;

/// Git LFS est installé ; sinon un message explique pourquoi le test s'arrête.
fn lfs_installed() -> bool {
    let ok = dragontree_git::lfs_available(&Git::discover().unwrap());
    if !ok {
        eprintln!("git lfs est absent : test ignoré");
    }
    ok
}

fn repo(base: &Path) -> PathBuf {
    let dir = base.join("fabrikam");
    init_repo(&dir);
    commit_file(&dir, "a.txt", "1\n", "Premier commit");
    dir
}

fn open(base: &Path, repo: &Path) -> H {
    let mut h = harness(StartOptions { open: vec![repo.to_path_buf()], view: Some("history".into()), ..start(base) });
    wait_until(&mut h, "dépôt chargé", |h| h.query_by_label("Premier commit").is_some());
    h
}

/// Ouvre Dépôt › Git LFS et attend que l'entrée soit utilisable (détection de git lfs en tâche de fond) ; le menu
/// reste ouvert.
fn open_enabled_lfs_entry(h: &mut H, entry: &str) {
    open_menu(h, "Dépôt");
    // Le sous-menu est grisé tant que la détection n'a pas répondu : cliqué plus tôt, il ne s'ouvre pas.
    wait_until(h, "sous-menu « Git LFS » utilisable", |h| {
        menu_item(h, "Git LFS").is_some_and(|n| !n.accesskit_node().is_disabled())
    });
    open_menu(h, "Git LFS");
    wait_until(h, &format!("entrée « {entry} » utilisable"), |h| {
        menu_item(h, entry).is_some_and(|n| !n.accesskit_node().is_disabled())
    });
}

/// Clique le bouton dès qu'il est utilisable, en une image : le dialogue centré bouge tant que son contenu change.
fn click_button(h: &mut H, label: &str) {
    click_label(h, label);
    h.run_steps(3);
}

fn type_in_focused(h: &mut H, text: &str) {
    wait_until(h, "champ avec le focus", |h| {
        h.query_by(|n| n.role() == egui::accesskit::Role::TextInput && n.is_focused()).is_some()
    });
    h.get_by(|n| n.role() == egui::accesskit::Role::TextInput && n.is_focused()).type_text(text);
    h.run_steps(2);
}

fn attributes(repo: &Path) -> String {
    std::fs::read_to_string(repo.join(".gitattributes")).unwrap_or_default()
}

#[test]
fn tracking_a_pattern_adds_its_line_to_gitattributes() {
    if !lfs_installed() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo(tmp.path());
    let mut h = open(tmp.path(), &repo);
    open_enabled_lfs_entry(&mut h, "Suivre un motif…");
    click_menu_item(&mut h, "Suivre un motif…");
    type_in_focused(&mut h, "*.bin");
    click_button(&mut h, "Suivre");
    wait_git(&mut h, "motif ajouté", || attributes(&repo).contains("*.bin filter=lfs"));
    assert!(attributes(&repo).contains("diff=lfs merge=lfs -text"));
}

#[test]
fn untracking_removes_the_pattern_and_is_greyed_without_any() {
    if !lfs_installed() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo(tmp.path());
    let mut h = open(tmp.path(), &repo);
    // Aucun motif suivi : « Ne plus suivre » est grisée (l'entrée « Suivre » est utilisable : la détection a répondu).
    open_enabled_lfs_entry(&mut h, "Suivre un motif…");
    assert!(menu_item_disabled(&h, "Ne plus suivre…"));
    h.key_press(egui::Key::Escape);
    h.run_steps(3);

    git(&repo, &["lfs", "track", "*.bin"]);
    // Le motif est relu avec l'état du dépôt (le menu reste ouvert, il se met à jour).
    open_enabled_lfs_entry(&mut h, "Ne plus suivre…");
    click_menu_item(&mut h, "Ne plus suivre…");
    wait_until(&mut h, "dialogue", |h| h.query_by_label("Ne plus suivre avec Git LFS").is_some());
    click_button(&mut h, "Ne plus suivre");
    wait_git(&mut h, "motif retiré", || !attributes(&repo).contains("*.bin"));
}

#[test]
fn initializing_enables_lfs_in_the_repository_only() {
    if !lfs_installed() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo(tmp.path());
    let mut h = open(tmp.path(), &repo);
    open_enabled_lfs_entry(&mut h, "Initialiser Git LFS");
    click_menu_item(&mut h, "Initialiser Git LFS");
    // git lfs écrit la configuration puis les crochets : on attend les deux.
    wait_git(&mut h, "filtre et crochets installés", || {
        try_git(&repo, &["config", "--local", "--get", "filter.lfs.clean"]).is_some_and(|s| s.contains("git-lfs"))
            && repo.join(".git").join("hooks").join("pre-push").exists()
    });
}
