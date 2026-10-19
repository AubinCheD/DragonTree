//! Barre de menus Fichier / Affichage / Dépôt / Configuration, raccourcis et palette du registre de commandes.

mod common;

use common::*;
use dragontree_app::StartOptions;
use egui::{Key, Modifiers};
use egui_kittest::kittest::{NodeT, Queryable};

#[test]
fn every_menu_opens_and_shows_its_entries_with_their_shortcuts() {
    let tmp = tempfile::tempdir().unwrap();
    let mut h = harness(start(tmp.path()));

    for (menu, entry) in [
        ("Fichier", "Cloner un dépôt… Ctrl+Maj+C"),
        ("Affichage", "Journal git Ctrl+Maj+L"),
        ("Dépôt", "Actualiser F5"),
        ("Configuration", "Réglages… Ctrl+,"),
    ] {
        open_menu(&mut h, menu);
        wait_until(&mut h, entry, |h| menu_item(h, entry).is_some());
        h.key_press(Key::Escape);
        wait_until(&mut h, &format!("menu « {menu} » fermé"), |h| menu_item(h, entry).is_none());
    }

    // Une entrée de Fichier agit : un second onglet d'accueil s'ouvre.
    open_menu(&mut h, "Fichier");
    click_menu_item(&mut h, "Nouvel onglet Ctrl+T");
    wait_until(&mut h, "second onglet", |h| h.query_all_by_label("Nouvel onglet").count() == 2);
}

#[test]
fn repo_entries_are_disabled_on_the_welcome_tab() {
    let tmp = tempfile::tempdir().unwrap();
    let mut h = harness(start(tmp.path()));

    open_menu(&mut h, "Dépôt");
    wait_until(&mut h, "entrée Fetch", |h| menu_item(h, "Fetch Ctrl+Maj+F").is_some());
    assert!(menu_item_disabled(&h, "Fetch Ctrl+Maj+F"), "Fetch est grisée sans dépôt ouvert");
    assert!(menu_item_disabled(&h, "Nouvelle branche… Ctrl+Maj+B"));
}

#[test]
fn view_menu_switches_an_open_repo_to_local_changes() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = demo_repo(tmp.path());
    let mut h = harness(StartOptions { open: vec![repo], view: Some("history".into()), ..start(tmp.path()) });

    wait_until(&mut h, "historique chargé", |h| h.query_by_label("Ajout de la multiplication").is_some());
    open_menu(&mut h, "Affichage");
    wait_until(&mut h, "vue cochée", |h| menu_item(h, "Tous les commits").is_some());
    assert_eq!(menu_item_checked(&h, "Tous les commits"), Some(true));
    assert_eq!(menu_item_checked(&h, "Modifications locales"), Some(false));
    click_menu_item(&mut h, "Modifications locales");
    wait_until(&mut h, "vue des modifications locales", |h| h.query_by_label_contains("Tout indexer").is_some());

    open_menu(&mut h, "Affichage");
    wait_until(&mut h, "entrée cochée", |h| menu_item(h, "Modifications locales").is_some());
    assert_eq!(menu_item_checked(&h, "Modifications locales"), Some(true));
    assert_eq!(menu_item_checked(&h, "Tous les commits"), Some(false));
}

#[test]
fn repo_menu_opens_the_new_branch_dialog() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = demo_repo(tmp.path());
    let mut h = harness(StartOptions { open: vec![repo], view: Some("history".into()), ..start(tmp.path()) });

    // Les références (branche courante) sont chargées : le bouton Branche de la barre d'outils est actif.
    wait_until(&mut h, "références chargées", |h| {
        h.query_by_label("Branche").is_some_and(|n| !n.accesskit_node().is_disabled())
    });
    open_menu(&mut h, "Dépôt");
    click_menu_item(&mut h, "Nouvelle branche… Ctrl+Maj+B");
    wait_until(&mut h, "dialogue de branche", |h| {
        h.query_by(|n| n.placeholder() == Some("nom-de-la-branche (ex. feature/ma-fonction)")).is_some()
    });
}

#[test]
fn ctrl_comma_opens_the_settings() {
    let tmp = tempfile::tempdir().unwrap();
    let mut h = harness(start(tmp.path()));
    h.run_steps(3);

    h.key_press_modifiers(Modifiers::COMMAND, Key::Comma);
    wait_until(&mut h, "fenêtre des réglages", |h| h.query_all_by_label("Taille de l'interface").next().is_some());
}

#[test]
fn language_submenu_checks_the_current_language() {
    let tmp = tempfile::tempdir().unwrap();
    let mut h = harness(start(tmp.path()));

    open_menu(&mut h, "Configuration");
    open_menu(&mut h, "Langue");
    wait_until(&mut h, "sous-menu Langue", |h| menu_item(h, "English").is_some());
    assert_eq!(menu_item_checked(&h, "Français"), Some(true));
    assert_eq!(menu_item_checked(&h, "English"), Some(false));
    assert_eq!(menu_item_checked(&h, "Système"), Some(false));
}

#[test]
fn palette_runs_a_command_of_the_registry() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = demo_repo(tmp.path());
    let mut h = harness(StartOptions { open: vec![repo], view: Some("history".into()), ..start(tmp.path()) });

    wait_until(&mut h, "références chargées", |h| {
        h.query_by_label("Branche").is_some_and(|n| !n.accesskit_node().is_disabled())
    });
    h.key_press_modifiers(Modifiers::COMMAND, Key::P);
    let hint = "Dépôt, branche ou commande…";
    wait_until(&mut h, "palette", |h| h.query_by(|n| n.placeholder() == Some(hint)).is_some());
    h.get_by(|n| n.placeholder() == Some(hint)).type_text("Nouvelle branche");
    h.run_steps(3);
    h.key_press(Key::Enter);
    wait_until(&mut h, "dialogue de branche", |h| {
        h.query_by(|n| n.placeholder() == Some("nom-de-la-branche (ex. feature/ma-fonction)")).is_some()
    });
}
