//! Fenêtre Réglages : « Enregistrer » ne reprend que les réglages modifiés dans la fenêtre ; ceux changés ailleurs
//! pendant qu'elle était ouverte (menu Affichage, zoom au clavier…) sont conservés.

mod common;

use common::*;
use dragontree_app::StartOptions;
use dragontree_app::settings::{Settings, ThemeChoice};
use egui::{Key, Modifiers};
use egui_kittest::kittest::Queryable;

#[test]
fn saving_the_settings_window_keeps_what_changed_elsewhere_while_it_was_open() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = demo_repo(tmp.path());
    let settings = || Settings::load_from(&tmp.path().join("profil").join("settings.json"));
    let mut h = harness(StartOptions {
        open: vec![repo],
        view: Some("history".into()),
        window: Some("settings".into()),
        ..start(tmp.path())
    });
    wait_until(&mut h, "fenêtre Réglages et dépôt chargé", |h| {
        h.query_by_label("Enregistrer").is_some() && h.query_by_label("main").is_some()
    });

    // Pendant que la fenêtre est ouverte : Affichage › Afficher les tags (que la fenêtre ne montre pas), et un zoom au
    // clavier (la taille de l'interface, que la fenêtre montre mais qu'on n'y touche pas).
    open_menu(&mut h, "Affichage");
    click_menu_item(&mut h, "Afficher les tags");
    h.key_press_modifiers(Modifiers::COMMAND, Key::Equals);
    wait_until(&mut h, "réglages changés ailleurs enregistrés", |_| {
        let s = settings();
        !s.show_tags && s.ui_scale == 1.05
    });

    // Un réglage changé dans la fenêtre, puis « Enregistrer ».
    h.get_by_label_contains("Sombre").click();
    h.run_steps(2);
    h.get_by_label("Enregistrer").click();
    wait_until(&mut h, "fenêtre fermée", |h| h.query_by_label("Enregistrer").is_none());
    h.run_steps(2);

    let saved = settings();
    assert_eq!(saved.theme, ThemeChoice::Dark, "le réglage de la fenêtre est enregistré");
    assert!(!saved.show_tags, "Afficher les tags, décoché pendant que la fenêtre était ouverte, le reste");
    assert_eq!(saved.ui_scale, 1.05, "le zoom fait pendant que la fenêtre était ouverte est conservé");
}
