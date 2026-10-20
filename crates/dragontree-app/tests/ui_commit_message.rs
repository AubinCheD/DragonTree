//! Zone de commit : compteur de longueur du résumé, « Messages récents » (mémoire du dépôt et sujets de
//! l'historique) et « Co-auteur » (auteurs de l'historique, ligne `Co-authored-by:` sans doublon).

mod common;

use std::path::{Path, PathBuf};

use common::*;
use dragontree_app::StartOptions;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

/// Un champ (d'une ligne ou multiligne) contient exactement `value`.
fn has_value(h: &H, value: &str) -> bool {
    h.query_all_by(|n| n.value().as_deref() == Some(value)).next().is_some()
}

const RECENT: &str = "Messages récents ";
const COAUTHOR: &str = "Co-auteur ";

type H = Harness<'static, dragontree_app::DragonTreeApp>;

fn open(base: &Path) -> (H, PathBuf) {
    let repo = demo_repo(base);
    let mut h = harness(StartOptions { open: vec![repo.clone()], view: Some("changes".into()), ..start(base) });
    wait_until(&mut h, "zone de commit", |h| h.query_by_label(RECENT).is_some());
    (h, repo)
}

/// Ouvre un menu de la zone de commit et attend que `item` y figure. Un clic perdu (interface chargée) est refait
/// après une dizaine d'images ; recliquer plus tôt refermerait le menu.
fn open_commit_menu(h: &mut H, menu: &str, item: &str) {
    let mut frames = 0;
    wait_until(h, &format!("menu {menu}"), |h| {
        if h.query_by_label(item).is_some() {
            return true;
        }
        if frames % 12 == 0
            && let Some(button) = h.query_by_label(menu)
        {
            button.click();
        }
        frames += 1;
        false
    });
}

#[test]
fn a_subject_from_the_history_fills_the_summary_and_the_counter_follows_its_length() {
    let tmp = tempfile::tempdir().unwrap();
    let (mut h, _repo) = open(tmp.path());
    // Les sujets de l'historique chargé sont proposés ; le choix remplit le résumé.
    open_commit_menu(&mut h, RECENT, "Ajout de la multiplication");
    h.get_by_label("Ajout de la multiplication").click();
    wait_until(&mut h, "résumé rempli", |h| has_text_input(h, "Ajout de la multiplication"));
    // 26 caractères : le compteur est rapporté à la longueur conseillée (50).
    wait_until(&mut h, "compteur du résumé", |h| h.query_by_label("26/50").is_some());

    // Au-delà de la longueur conseillée, il est rapporté à la longueur maximale (72).
    let subject = h.get_by(|n| {
        n.role() == egui::accesskit::Role::TextInput && n.value().as_deref() == Some("Ajout de la multiplication")
    });
    subject.focus();
    subject.type_text(&" x".repeat(20));
    wait_until(&mut h, "compteur d'avertissement", |h| h.query_by_label("66/72").is_some());
}

#[test]
fn a_coauthor_from_the_history_is_added_once_to_the_description() {
    let tmp = tempfile::tempdir().unwrap();
    let (mut h, _repo) = open(tmp.path());
    let author = "Testeur <t@example.com>";
    open_commit_menu(&mut h, COAUTHOR, author);
    h.get_by_label(author).click();
    let trailer = "Co-authored-by: Testeur <t@example.com>";
    wait_until(&mut h, "ligne Co-authored-by ajoutée", |h| has_value(h, trailer));

    // Le même co-auteur une seconde fois : la description ne change pas.
    open_commit_menu(&mut h, COAUTHOR, author);
    h.get_by_label(author).click();
    h.run_steps(10);
    assert!(has_value(&h, trailer), "la description a changé");

    // Choisir un sujet de l'historique ne remplace que le résumé : la description saisie reste.
    open_commit_menu(&mut h, RECENT, "Initial");
    h.get_by_label("Initial").click();
    wait_until(&mut h, "résumé rempli", |h| has_text_input(h, "Initial"));
    assert!(has_value(&h, trailer), "la description a été effacée");
}

#[test]
fn a_successful_commit_is_remembered_in_the_recent_messages_of_the_repository() {
    let tmp = tempfile::tempdir().unwrap();
    let (mut h, repo) = open(tmp.path());
    wait_until(&mut h, "bouton « Tout indexer »", |h| h.query_by_label_contains("Tout indexer").is_some());
    h.get_by_label_contains("Tout indexer").click();
    wait_until(&mut h, "fichiers indexés", |h| h.query_by_label_contains("Commit (2 fichiers)").is_some());
    let subject = h.get_by(|n| n.placeholder() == Some("Résumé du commit (obligatoire)"));
    subject.focus();
    subject.type_text("Message à retenir");
    h.run_steps(3);
    h.get_by_label_contains("Commit (2 fichiers)").click();
    wait_git(&mut h, "commit créé", || {
        try_git(&repo, &["log", "-1", "--format=%s"]).is_some_and(|s| s.trim() == "Message à retenir")
    });
    // Le résumé est vidé, et le message figure en tête des messages récents (et dans les réglages enregistrés).
    wait_until(&mut h, "résumé vidé", |h| !has_text_input(h, "Message à retenir"));
    open_commit_menu(&mut h, RECENT, "Message à retenir");
    wait_git(&mut h, "réglages enregistrés", || {
        std::fs::read_to_string(tmp.path().join("profil").join("settings.json"))
            .is_ok_and(|s| s.contains("recent_messages") && s.contains("Message à retenir"))
    });
}
