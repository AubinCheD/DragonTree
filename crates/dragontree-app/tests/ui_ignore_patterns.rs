//! Ignorer des fichiers : Dépôt › Ignorer des fichiers… et « Ignorer… » du menu d'un fichier non suivi, avec
//! l'aperçu des fichiers concernés et le choix du fichier cible.

mod common;

use std::path::{Path, PathBuf};

use common::*;
use dragontree_app::icons;
use dragontree_app::{DragonTreeApp, StartOptions};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};

type H = Harness<'static, DragonTreeApp>;

/// Dépôt `fabrikam` : `suivi.log` et `a.txt` suivis ; `x.log`, `sub/y.log` et `notes.txt` non suivis.
fn repo(base: &Path) -> PathBuf {
    let dir = base.join("fabrikam");
    init_repo(&dir);
    commit_file(&dir, "a.txt", "1\n", "Premier commit");
    commit_file(&dir, "suivi.log", "1\n", "Journal suivi");
    std::fs::write(dir.join("x.log"), "x\n").unwrap();
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    std::fs::write(dir.join("sub").join("y.log"), "y\n").unwrap();
    std::fs::write(dir.join("notes.txt"), "n\n").unwrap();
    dir
}

fn open(base: &Path, repo: &Path) -> H {
    let mut h = harness(StartOptions { open: vec![repo.to_path_buf()], view: Some("changes".into()), ..start(base) });
    wait_until(&mut h, "fichiers non suivis", |h| h.query_all_by_label("notes.txt").next().is_some());
    h
}

/// Ligne de l'aperçu d'un fichier (icône puis chemin : le libellé diffère de celui de la liste des changements).
fn preview_row(path: &str) -> String {
    format!("{} {path}", icons::FILE)
}

fn has(h: &H, label: &str) -> bool {
    h.query_all_by_label(label).next().is_some()
}

/// Clique le bouton de validation une fois l'aperçu calculé, en une image (`click_label`) : centré, le dialogue bouge
/// quand l'aperçu change sa hauteur, et un clic réparti sur plusieurs images tomberait à côté et le fermerait.
fn validate(h: &mut H) {
    wait_until(h, "aperçu calculé", |h| !has(h, "Calcul de l'aperçu…"));
    click_label(h, "Ignorer");
}

/// Ouvre le dialogue par le menu Dépôt (la commande reste grisée tant que les références ne sont pas lues).
fn open_dialog_from_menu(h: &mut H) {
    open_menu(h, "Dépôt");
    wait_until(h, "commande utilisable", |h| {
        menu_item(h, "Ignorer des fichiers…").is_some_and(|n| !n.accesskit_node().is_disabled())
    });
    click_menu_item(h, "Ignorer des fichiers…");
    wait_until(h, "dialogue", |h| has(h, "Annuler"));
}

fn type_pattern(h: &mut H, text: &str) {
    wait_until(h, "champ du motif", |h| {
        h.query_by(|n| n.role() == egui::accesskit::Role::TextInput && n.is_focused()).is_some()
    });
    h.get_by(|n| n.role() == egui::accesskit::Role::TextInput && n.is_focused()).type_text(text);
    h.run_steps(2);
}

#[test]
fn typing_a_pattern_previews_the_untracked_files_and_validating_hides_them() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo(tmp.path());
    let mut h = open(tmp.path(), &repo);
    open_dialog_from_menu(&mut h);
    type_pattern(&mut h, "*.log");
    wait_until(&mut h, "aperçu des deux fichiers", |h| {
        has(h, &preview_row("x.log")) && has(h, &preview_row("sub/y.log"))
    });
    assert!(has(&h, "2 fichiers non suivis seront ignorés :"));
    assert!(!has(&h, &preview_row("notes.txt")), "un fichier hors motif n'est pas dans l'aperçu");
    validate(&mut h);
    wait_git(&mut h, "motif ajouté", || {
        std::fs::read_to_string(repo.join(".gitignore")).is_ok_and(|c| c == "*.log\n")
    });
    wait_git(&mut h, "fichiers ignorés", || {
        try_git(&repo, &["status", "--porcelain"]).is_some_and(|s| !s.contains(".log") && s.contains("notes.txt"))
    });
    wait_until(&mut h, "x.log retiré des non suivis", |h| !has(h, "x.log"));
}

#[test]
fn tracked_files_that_match_trigger_a_warning() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo(tmp.path());
    let mut h = open(tmp.path(), &repo);
    open_dialog_from_menu(&mut h);
    type_pattern(&mut h, "*.log");
    wait_until(&mut h, "avertissement", |h| h.query_by_label_contains("1 fichier déjà suivi correspond").is_some());
    assert!(h.query_by_label_contains("suivi.log").is_some());
}

#[test]
fn the_ignore_button_waits_for_a_pattern() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo(tmp.path());
    let mut h = open(tmp.path(), &repo);
    open_dialog_from_menu(&mut h);
    assert!(h.get_by_label("Ignorer").accesskit_node().is_disabled());
}

#[test]
fn the_file_menu_proposes_patterns_and_can_target_info_exclude() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo(tmp.path());
    let mut h = open(tmp.path(), &repo);
    h.query_all_by_label("x.log").next().unwrap().click_secondary();
    let entry = format!("{} Ignorer…", icons::EYE_SLASH);
    wait_until(&mut h, "entrée « Ignorer… »", |h| has(h, &entry));
    h.get_by_label(&entry).click();
    wait_until(&mut h, "motifs proposés", |h| has(h, "*.log"));
    h.get_by_label("*.log").click();
    wait_until(&mut h, "aperçu", |h| has(h, &preview_row("x.log")) && has(h, &preview_row("sub/y.log")));
    // Le dialogue est centré : ses boutons bougent d'une image à l'autre tant que la hauteur de l'aperçu change (clic
    // en une image).
    let exclude = ".git/info/exclude (local, jamais commité)";
    click_label(&mut h, exclude);
    wait_until(&mut h, "cible choisie", |h| checked(h, exclude));
    // Le changement de cible relance l'aperçu, qui reste celui du motif.
    wait_until(&mut h, "aperçu relancé", |h| has(h, &preview_row("x.log")));
    validate(&mut h);
    wait_git(&mut h, "motif dans info/exclude", || {
        std::fs::read_to_string(repo.join(".git").join("info").join("exclude")).is_ok_and(|c| c.contains("*.log"))
    });
    assert!(!repo.join(".gitignore").exists());
}

#[test]
fn a_folder_gitignore_is_offered_for_files_in_a_folder() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo(tmp.path());
    let mut h = open(tmp.path(), &repo);
    h.query_all_by_label("sub/y.log").next().unwrap().click_secondary();
    let entry = format!("{} Ignorer…", icons::EYE_SLASH);
    wait_until(&mut h, "entrée « Ignorer… »", |h| has(h, &entry));
    h.get_by_label(&entry).click();
    wait_until(&mut h, "motifs proposés", |h| has(h, "*.log"));
    h.get_by_label("*.log").click();
    // Le dialogue est centré : on attend l'aperçu du motif choisi avant de cliquer plus bas.
    wait_until(&mut h, "premier aperçu", |h| has(h, &preview_row("sub/y.log")) && has(h, &preview_row("x.log")));
    click_label(&mut h, "sub/.gitignore (propre à ce dossier)");
    // Limité au dossier : x.log (à la racine) n'est plus concerné.
    wait_until(&mut h, "aperçu du dossier", |h| has(h, &preview_row("sub/y.log")) && !has(h, &preview_row("x.log")));
    validate(&mut h);
    wait_git(&mut h, "motif dans sub/.gitignore", || {
        std::fs::read_to_string(repo.join("sub").join(".gitignore")).is_ok_and(|c| c == "*.log\n")
    });
    assert!(!repo.join(".gitignore").exists());
}
