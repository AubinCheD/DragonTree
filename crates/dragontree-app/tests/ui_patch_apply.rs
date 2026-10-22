//! Appliquer un patch : fichier déposé sur la fenêtre, texte collé, aperçu, vérification, cible.

mod common;

use std::path::{Path, PathBuf};

use common::*;
use dragontree_app::StartOptions;
use egui_kittest::kittest::{NodeT, Queryable};

type H = egui_kittest::Harness<'static, dragontree_app::DragonTreeApp>;

/// Dépôt avec `a.txt` ; renvoie aussi un patch qui y ajoute une ligne.
fn repo_and_patch(base: &Path) -> (PathBuf, String) {
    let repo = base.join("fabrikam");
    init_repo(&repo);
    commit_file(&repo, "a.txt", "1\n2\n", "Commit de base");
    std::fs::write(repo.join("a.txt"), "1\n2\n3\n").unwrap();
    let patch = git(&repo, &["diff"]);
    git(&repo, &["checkout", "--", "a.txt"]);
    (repo, patch)
}

fn open(repo: &Path, base: &Path) -> H {
    let mut h = harness(StartOptions { open: vec![repo.to_path_buf()], view: Some("history".into()), ..start(base) });
    wait_until(&mut h, "dépôt chargé", |h| h.query_by_label("Commit de base").is_some());
    // Les références lues : sans elles, une commande de dépôt est encore grisée (le dépôt n'est pas « prêt »).
    wait_until(&mut h, "branche lue", |h| h.query_all_by_label("main").next().is_some());
    h
}

fn drop_file(h: &mut H, path: &Path) {
    h.input_mut().dropped_files.push(std::sync::Arc::new(TestFile(path.to_path_buf())));
    h.step();
}

fn content(repo: &Path) -> String {
    // Lecture pendant que git réécrit le fichier : vide plutôt qu'une erreur.
    std::fs::read_to_string(repo.join("a.txt")).unwrap_or_default()
}

#[test]
fn a_dropped_patch_file_opens_the_preview_and_apply_changes_the_file() {
    let tmp = tempfile::tempdir().unwrap();
    let (repo, patch) = repo_and_patch(tmp.path());
    let file = tmp.path().join("ajout.patch");
    std::fs::write(&file, patch).unwrap();
    let mut h = open(&repo, tmp.path());

    drop_file(&mut h, &file);
    wait_until(&mut h, "aperçu du patch", |h| h.query_by_label("Appliquer un patch").is_some());
    // Fichiers touchés et résultat de la vérification.
    wait_until(&mut h, "fichier touché", |h| h.query_by_label_contains("a.txt").is_some());
    assert!(h.query_by_label_contains("s'applique sans conflit").is_some());
    assert_eq!(content(&repo), "1\n2\n", "rien n'est appliqué avant la confirmation");

    click_label(&mut h, "Appliquer");
    wait_git(&mut h, "fichier modifié", || content(&repo) == "1\n2\n3\n");
    assert_eq!(git(&repo, &["diff", "--cached", "--name-only"]), "", "cible : copie de travail seulement");
}

#[test]
fn the_index_can_be_the_target() {
    let tmp = tempfile::tempdir().unwrap();
    let (repo, patch) = repo_and_patch(tmp.path());
    let file = tmp.path().join("ajout.diff");
    std::fs::write(&file, patch).unwrap();
    let mut h = open(&repo, tmp.path());
    drop_file(&mut h, &file);
    wait_until(&mut h, "aperçu du patch", |h| h.query_by_label("Appliquer un patch").is_some());
    click_label(&mut h, "Index");
    wait_until(&mut h, "cible : index", |h| checked(h, "Index"));
    click_label(&mut h, "Appliquer");
    wait_git(&mut h, "index modifié", || {
        try_git(&repo, &["diff", "--cached", "--name-only"]).is_some_and(|s| s.trim() == "a.txt")
    });
    assert_eq!(content(&repo), "1\n2\n", "copie de travail intacte");
}

#[test]
fn a_patch_that_does_not_apply_shows_git_s_error_and_cannot_be_applied() {
    let tmp = tempfile::tempdir().unwrap();
    let (repo, patch) = repo_and_patch(tmp.path());
    commit_file(&repo, "a.txt", "tout autre contenu\n", "Autre contenu");
    let file = tmp.path().join("ajout.patch");
    std::fs::write(&file, patch).unwrap();
    let mut h = open(&repo, tmp.path());
    drop_file(&mut h, &file);
    wait_until(&mut h, "aperçu du patch", |h| h.query_by_label("Appliquer un patch").is_some());
    wait_until(&mut h, "erreur de vérification", |h| h.query_by_label_contains("ne s'applique pas").is_some());
    assert!(h.get_by_label("Appliquer").accesskit_node().is_disabled());
}

#[test]
fn a_format_patch_offers_git_am_and_creates_the_commit() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("northwind");
    init_repo(&repo);
    commit_file(&repo, "a.txt", "1\n", "Commit de base");
    commit_file(&repo, "a.txt", "1\n2\n", "Ajoute deux");
    let patch = git(&repo, &["format-patch", "-1", "--stdout"]);
    git(&repo, &["reset", "-q", "--hard", "HEAD~1"]);
    let file = tmp.path().join("0001.patch");
    std::fs::write(&file, patch).unwrap();
    let mut h = open(&repo, tmp.path());
    drop_file(&mut h, &file);
    wait_until(&mut h, "aperçu du patch", |h| h.query_by_label("Appliquer un patch").is_some());
    click_label(&mut h, "Créer les commits (git am)");
    wait_until(&mut h, "cible : git am", |h| checked(h, "Créer les commits (git am)"));
    click_label(&mut h, "Créer les commits");
    wait_git(&mut h, "commit créé", || {
        try_git(&repo, &["log", "-1", "--format=%s"]).is_some_and(|s| s.trim() == "Ajoute deux")
    });
}

#[test]
fn a_pasted_patch_is_previewed_then_applied() {
    let tmp = tempfile::tempdir().unwrap();
    let (repo, patch) = repo_and_patch(tmp.path());
    let mut h = open(&repo, tmp.path());
    open_menu(&mut h, "Dépôt");
    click_menu_item(&mut h, "Coller un patch…");
    wait_until(&mut h, "dialogue de collage", |h| h.query_by_label("Coller un patch").is_some());
    let area = h.get_by(|n| n.placeholder() == Some("Collez le patch ici (Ctrl+V)"));
    area.focus();
    h.event(egui::Event::Paste(patch));
    // « Aperçu » n'est utilisable qu'une fois le texte collé.
    click_label(&mut h, "Aperçu");
    wait_until(&mut h, "aperçu du patch", |h| h.query_by_label("Appliquer un patch").is_some());
    wait_until(&mut h, "vérification", |h| h.query_by_label_contains("s'applique sans conflit").is_some());
    click_label(&mut h, "Appliquer");
    wait_git(&mut h, "fichier modifié", || content(&repo) == "1\n2\n3\n");
}

#[test]
fn a_dropped_file_that_is_not_a_patch_is_ignored_with_a_message() {
    let tmp = tempfile::tempdir().unwrap();
    let (repo, _) = repo_and_patch(tmp.path());
    let file = tmp.path().join("notes.txt");
    std::fs::write(&file, "bonjour\n").unwrap();
    let mut h = open(&repo, tmp.path());
    drop_file(&mut h, &file);
    wait_until(&mut h, "message", |h| h.query_by_label_contains("fichier .patch ou .diff").is_some());
    assert!(h.query_by_label("Appliquer un patch").is_none());
}

/// Fichier déposé tel que l'intégration native le fournit à egui.
#[derive(Debug)]
struct TestFile(PathBuf);

impl egui::DroppedFile for TestFile {
    fn path(&self) -> &Path {
        &self.0
    }

    fn bytes(&self) -> Result<Vec<u8>, String> {
        std::fs::read(&self.0).map_err(|e| e.to_string())
    }
}

#[test]
fn applying_a_patch_is_greyed_while_an_operation_is_in_progress() {
    let tmp = tempfile::tempdir().unwrap();
    let (repo, _) = repo_and_patch(tmp.path());
    // Fusion en conflit : une opération est en cours.
    git(&repo, &["switch", "-q", "-c", "autre"]);
    commit_file(&repo, "a.txt", "autre\n", "Côté autre");
    git(&repo, &["switch", "-q", "main"]);
    commit_file(&repo, "a.txt", "principal\n", "Côté principal");
    let _ = try_git(&repo, &["merge", "autre"]);
    let mut h = harness(StartOptions { open: vec![repo.clone()], view: Some("history".into()), ..start(tmp.path()) });
    wait_until(&mut h, "fusion en cours détectée", |h| h.query_by_label_contains("Abandonner la fusion").is_some());
    open_menu(&mut h, "Dépôt");
    wait_until(&mut h, "entrée", |h| menu_item(h, "Appliquer un patch…").is_some());
    assert!(menu_item_disabled(&h, "Appliquer un patch…"));
    assert!(menu_item_disabled(&h, "Coller un patch…"));
}
