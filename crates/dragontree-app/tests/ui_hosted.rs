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

#[test]
fn hosted_the_settings_command_opens_the_host_settings() {
    let tmp = tempfile::tempdir().unwrap();
    let mut h = harness(start(tmp.path()));
    let calls = Arc::new(AtomicUsize::new(0));
    let c = calls.clone();
    h.state_mut().set_hosted(true);
    h.state_mut().set_settings_link(Box::new(move || {
        c.fetch_add(1, Ordering::SeqCst);
    }));
    h.run_steps(2);
    // Configuration › Réglages… (Ctrl+,) : son libellé porte le raccourci, on passe par celui-ci.
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Comma);
    h.run_steps(3);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(h.query_by_label("Enregistrer").is_none(), "la fenêtre Réglages de DragonTree reste fermée");
}

/// Ses réglages dessinés dans un `Ui` de l'hôte (la page DragonTree des réglages d'Eyrie).
#[test]
fn the_settings_panel_draws_in_a_host_ui() {
    /// Un hôte minimal : il dessine les réglages de DragonTree dans son propre `Ui`.
    struct Host(dragontree_app::DragonTreeApp);
    impl eframe::App for Host {
        fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
            self.0.settings_ui(ui);
        }
    }
    let tmp = tempfile::tempdir().unwrap();
    let start = start(tmp.path());
    let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(900.0, 700.0)).build_eframe(move |cc| {
        let mut app = dragontree_app::DragonTreeApp::new(cc, start);
        app.set_hosted(true);
        Host(app)
    });
    h.run_steps(3);
    assert!(h.query_by_label("Enregistrer").is_some(), "Enregistrer et Annuler, comme dans sa fenêtre");
    assert!(h.query_by_label("Thème").is_none(), "le thème vient de l'hôte");
    assert!(h.query_by_label("Taille de l'interface").is_none(), "le zoom aussi : celui de l'hôte vaut pour tous");
}

/// Un hôte qui dessine la page DragonTree de ses réglages tant qu'elle est montrée, sans jamais dessiner sa case
/// (onglet caché).
struct PageHost {
    app: dragontree_app::DragonTreeApp,
    shown: bool,
}

impl eframe::App for PageHost {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if self.shown {
            self.app.settings_ui(ui);
        }
    }
}

fn page_host(dir: &std::path::Path) -> egui_kittest::Harness<'static, PageHost> {
    let start = start(dir);
    let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(900.0, 900.0)).build_eframe(move |cc| {
        let mut app = dragontree_app::DragonTreeApp::new(cc, start);
        app.set_hosted(true);
        PageHost { app, shown: true }
    });
    h.run_steps(3);
    h
}

/// La page quittée chez l'hôte (fenêtre fermée, autre page) : le brouillon est oublié, comme à la fermeture de sa
/// fenêtre Réglages ; rouverte, elle montre les réglages en vigueur.
#[test]
fn leaving_the_host_page_forgets_the_draft() {
    let tmp = tempfile::tempdir().unwrap();
    let mut h = page_host(tmp.path());
    let field = || egui::accesskit::Role::TextInput;
    let before = h.get_by_role_and_label(field(), "Exécutable git").value();
    h.get_by_role_and_label(field(), "Exécutable git").focus();
    h.run_steps(2);
    h.get_by_role_and_label(field(), "Exécutable git").type_text("-brouillon");
    h.run_steps(2);
    assert_ne!(h.get_by_role_and_label(field(), "Exécutable git").value(), before, "saisi dans le brouillon");
    h.state_mut().shown = false;
    h.state_mut().app.settings_left();
    h.run_steps(2);
    h.state_mut().shown = true;
    h.run_steps(3);
    assert_eq!(h.get_by_role_and_label(field(), "Exécutable git").value(), before, "brouillon oublié");
}

/// « Gérer les comptes… » de sa page chez l'hôte mène tout de suite aux comptes de l'hôte, même si sa case n'est pas
/// dessinée.
#[test]
fn the_host_page_opens_the_host_accounts_without_its_tab() {
    let tmp = tempfile::tempdir().unwrap();
    let mut h = page_host(tmp.path());
    let calls = Arc::new(AtomicUsize::new(0));
    let c = calls.clone();
    h.state_mut().app.set_accounts_link(Box::new(move || {
        c.fetch_add(1, Ordering::SeqCst);
    }));
    h.get_by_label_contains("Gérer les comptes").click_accesskit();
    h.run_steps(3);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}
