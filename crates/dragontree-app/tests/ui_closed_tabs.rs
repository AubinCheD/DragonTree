//! Fichier › Rouvrir l'onglet fermé (Ctrl+Maj+T) : la pile des onglets fermés, et Ctrl+T qui reste « Nouvel onglet ».

mod common;

use std::path::{Path, PathBuf};

use common::*;
use dragontree_app::StartOptions;
use dragontree_app::settings::Settings;
use egui::{Key, Modifiers};

fn repo(base: &Path, name: &str) -> PathBuf {
    let dir = base.join(name);
    init_repo(&dir);
    commit_file(&dir, "a.txt", "a\n", "Initial");
    dir
}

fn saved(base: &Path) -> Settings {
    Settings::load_from(&base.join("profil").join("settings.json"))
}

/// Noms des dossiers des onglets de dépôt enregistrés, dans l'ordre.
fn saved_tabs(base: &Path) -> Vec<String> {
    saved(base).open_tabs.iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect()
}

fn welcome_tabs(h: &egui_kittest::Harness<'static, dragontree_app::DragonTreeApp>) -> usize {
    use egui_kittest::kittest::Queryable;
    // Le titre d'un onglet d'accueil est un bouton « Nouvel onglet » (menu fermé : pas d'entrée homonyme).
    h.query_all_by_label("Nouvel onglet").count()
}

#[test]
fn ctrl_shift_t_reopens_the_closed_tab_where_it_was_and_ctrl_t_still_opens_a_new_tab() {
    let tmp = tempfile::tempdir().unwrap();
    let (a, b) = (repo(tmp.path(), "alpha"), repo(tmp.path(), "bravo"));
    let mut h = harness(StartOptions { open: vec![a, b], ..start(tmp.path()) });
    wait_until(&mut h, "deux onglets", |_| saved_tabs(tmp.path()) == ["alpha", "bravo"]);

    // Ferme le premier onglet.
    h.key_press_modifiers(Modifiers::COMMAND, Key::Num1);
    h.key_press_modifiers(Modifiers::COMMAND, Key::W);
    wait_until(&mut h, "alpha fermé", |_| saved_tabs(tmp.path()) == ["bravo"]);

    // Ctrl+T ouvre un nouvel onglet (il ne rouvre pas alpha).
    assert_eq!(welcome_tabs(&h), 0);
    h.key_press_modifiers(Modifiers::COMMAND, Key::T);
    h.run_steps(5);
    assert_eq!(welcome_tabs(&h), 1, "Ctrl+T ouvre un onglet d'accueil");
    assert_eq!(saved_tabs(tmp.path()), ["bravo"], "Ctrl+T ne rouvre pas l'onglet fermé");

    // Ctrl+Maj+T rouvre alpha à sa place d'origine, et l'active.
    h.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::T);
    wait_until(&mut h, "alpha rouvert", |_| saved_tabs(tmp.path()) == ["alpha", "bravo"]);
    assert_eq!(saved(tmp.path()).active_tab, 0, "l'onglet rouvert est actif");
    assert_eq!(welcome_tabs(&h), 1, "Ctrl+Maj+T n'ouvre pas d'onglet d'accueil de plus");
}

#[test]
fn reopen_entry_is_greyed_until_a_tab_is_closed_then_empties_the_stack() {
    let tmp = tempfile::tempdir().unwrap();
    let (a, b) = (repo(tmp.path(), "alpha"), repo(tmp.path(), "bravo"));
    let mut h = harness(StartOptions { open: vec![a, b], ..start(tmp.path()) });
    wait_until(&mut h, "deux onglets", |_| saved_tabs(tmp.path()) == ["alpha", "bravo"]);

    open_menu(&mut h, "Fichier");
    assert!(menu_item_disabled(&h, "Rouvrir l'onglet fermé Ctrl+Maj+T"));
    h.key_press(Key::Escape);
    h.run_steps(3);

    h.key_press_modifiers(Modifiers::COMMAND, Key::W);
    wait_until(&mut h, "bravo fermé", |_| saved_tabs(tmp.path()) == ["alpha"]);
    open_menu(&mut h, "Fichier");
    assert!(!menu_item_disabled(&h, "Rouvrir l'onglet fermé Ctrl+Maj+T"));
    click_menu_item(&mut h, "Rouvrir l'onglet fermé Ctrl+Maj+T");
    wait_until(&mut h, "bravo rouvert", |_| saved_tabs(tmp.path()) == ["alpha", "bravo"]);

    open_menu(&mut h, "Fichier");
    assert!(menu_item_disabled(&h, "Rouvrir l'onglet fermé Ctrl+Maj+T"), "la pile est vide");
}

#[test]
fn successive_reopens_walk_back_through_the_closed_tabs() {
    let tmp = tempfile::tempdir().unwrap();
    let paths: Vec<_> = ["alpha", "bravo", "charlie"].iter().map(|n| repo(tmp.path(), n)).collect();
    let mut h = harness(StartOptions { open: paths, ..start(tmp.path()) });
    wait_until(&mut h, "trois onglets", |_| saved_tabs(tmp.path()).len() == 3);

    // Ferme les trois onglets avec Ctrl+W (le dernier ouvert est actif).
    for expected in [vec!["alpha", "bravo"], vec!["alpha"], vec![]] {
        h.key_press_modifiers(Modifiers::COMMAND, Key::W);
        wait_until(&mut h, "onglet fermé", |_| saved_tabs(tmp.path()) == expected);
    }
    // Du plus récent au plus ancien (alpha, bravo, charlie), chacun à sa position d'origine.
    for expected in [vec!["alpha"], vec!["alpha", "bravo"], vec!["alpha", "bravo", "charlie"]] {
        h.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::T);
        wait_until(&mut h, "onglet rouvert", |_| saved_tabs(tmp.path()) == expected);
    }
}
