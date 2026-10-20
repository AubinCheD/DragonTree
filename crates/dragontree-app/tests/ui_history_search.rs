//! Recherche dans l'historique par contenu des modifications (`git log -G`) et par chemin.

mod common;

use common::*;
use dragontree_app::{DragonTreeApp, StartOptions};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

type H = Harness<'static, DragonTreeApp>;

/// Trois commits dont aucun message ne dit ce qu'ils changent.
fn open_repo(base: &std::path::Path) -> H {
    let repo = base.join("fabrikam");
    init_repo(&repo);
    commit_file(&repo, "src/lib.rs", "pub fn a() {}\n", "Premier commit");
    commit_file(&repo, "src/lib.rs", "pub fn a() {}\npub fn zebulon() {}\n", "Deuxième commit");
    commit_file(&repo, "docs/guide.md", "guide\n", "Troisième commit");
    let mut h = harness(StartOptions { open: vec![repo], view: Some("history".into()), ..start(base) });
    wait_until(&mut h, "historique complet", |h| h.query_by_label("Troisième commit").is_some());
    h
}

fn search_field(h: &H, hint: &str) -> String {
    h.get_by(|n| n.placeholder().is_some_and(|p| p.ends_with(hint))).value().unwrap_or_default()
}

/// Sélecteur de mode : la liste déroulante du champ de recherche.
fn choose_mode(h: &mut H, wanted: &str) {
    wait_until(h, "sélecteur de mode", |h| h.query_by_role(egui::accesskit::Role::ComboBox).is_some());
    h.get_by_role(egui::accesskit::Role::ComboBox).click();
    h.run_steps(3);
    wait_until(h, "liste des modes", |h| h.query_by_label(wanted).is_some());
    h.get_by_label(wanted).click();
    h.run_steps(3);
}

fn type_in(h: &mut H, hint: &str, text: &str) {
    let field = h.get_by(|n| n.placeholder().is_some_and(|p| p.ends_with(hint)));
    field.focus();
    field.type_text(text);
    h.run_steps(2);
}

fn shown(h: &H) -> Vec<&'static str> {
    ["Premier commit", "Deuxième commit", "Troisième commit"]
        .into_iter()
        .filter(|s| h.query_by_label(s).is_some())
        .collect()
}

#[test]
fn content_mode_finds_the_commit_that_introduced_a_string_absent_from_the_messages() {
    let tmp = tempfile::tempdir().unwrap();
    let mut h = open_repo(tmp.path());
    assert_eq!(search_field(&h, "Rechercher : message, auteur, SHA"), "");
    choose_mode(&mut h, "Contenu des modifications");
    type_in(&mut h, "Rechercher : texte ou expression régulière dans les modifications", "zebulon");
    wait_until(&mut h, "seul le commit trouvé", |h| shown(h) == ["Deuxième commit"]);
}

#[test]
fn invalid_regex_shows_the_error_of_git() {
    let tmp = tempfile::tempdir().unwrap();
    let mut h = open_repo(tmp.path());
    choose_mode(&mut h, "Contenu des modifications");
    type_in(&mut h, "Rechercher : texte ou expression régulière dans les modifications", "zeb(");
    // Le message de git (« invalid regex: Unmatched ( or ( ») est affiché à la place de la liste, sans panique.
    wait_until(&mut h, "erreur de git", |h| h.query_by_label_contains("Unmatched").is_some() && shown(h).is_empty());
}

#[test]
fn path_mode_accepts_globs() {
    let tmp = tempfile::tempdir().unwrap();
    let mut h = open_repo(tmp.path());
    choose_mode(&mut h, "Chemin");
    type_in(&mut h, "Rechercher : chemins ou motifs, séparés par des espaces", "*.md");
    wait_until(&mut h, "commit du guide", |h| shown(h) == ["Troisième commit"]);
}
