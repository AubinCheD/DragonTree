//! DragonTree embarqué par un hôte (Eyrie) : les comptes se règlent chez lui, pour toute la suite ; le thème et la
//! langue viennent de lui, et disparaissent des réglages et des menus de DragonTree.

mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use common::*;
use dragontree_app::StartOptions;
use egui_kittest::kittest::Queryable;

#[test]
fn hosted_the_accounts_command_opens_the_host_accounts() {
    let tmp = tempfile::tempdir().unwrap();
    let mut h = harness(start(tmp.path()));
    let calls = Arc::new(AtomicUsize::new(0));
    let c = calls.clone();
    h.state_mut().set_hosted(true);
    h.state_mut().set_accounts_link(Box::new(move || {
        c.fetch_add(1, Ordering::SeqCst);
    }));
    h.run_steps(2);
    open_menu(&mut h, "Configuration");
    click_menu_item(&mut h, "Comptes…");
    h.run_steps(3);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(
        h.query_by_label("Aucun compte pour l'instant.").is_none(),
        "la fenêtre Comptes de DragonTree reste fermée"
    );
}

#[test]
fn hosted_settings_hide_theme_and_language() {
    let tmp = tempfile::tempdir().unwrap();
    let mut h = harness(StartOptions { window: Some("settings".into()), ..start(tmp.path()) });
    wait_until(&mut h, "fenêtre Réglages", |h| h.query_by_label("Enregistrer").is_some());
    assert!(h.query_by_label("Thème").is_some());
    h.state_mut().set_hosted(true);
    h.run_steps(3);
    assert!(h.query_by_label("Thème").is_none(), "le thème vient de l'hôte");
    assert!(h.query_by_label("Langue").is_none(), "la langue vient de l'hôte");
}

#[test]
fn hosted_menus_drop_theme_and_language() {
    let tmp = tempfile::tempdir().unwrap();
    let mut h = harness(start(tmp.path()));
    h.state_mut().set_hosted(true);
    h.run_steps(2);
    open_menu(&mut h, "Affichage");
    assert!(h.query_by_label_contains("Thème").is_none());
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    open_menu(&mut h, "Configuration");
    assert!(h.query_by_label_contains("Langue").is_none());
    assert!(h.query_by_label_contains("Comptes…").is_some(), "les comptes restent, ouverts chez l'hôte");
}
