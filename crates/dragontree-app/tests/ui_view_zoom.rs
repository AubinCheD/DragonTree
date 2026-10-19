//! Affichage › Zoom (Ctrl+= / Ctrl+-) et Thème : les réglages persistants sont les seuls à agir.

mod common;

use std::path::Path;

use common::*;
use dragontree_app::settings::Settings;
use egui::{Key, Modifiers};

fn saved_scale(base: &Path) -> f32 {
    Settings::load_from(&base.join("profil").join("settings.json")).ui_scale
}

#[test]
fn ctrl_equals_and_ctrl_minus_zoom_and_persist() {
    let tmp = tempfile::tempdir().unwrap();
    let mut h = harness(start(tmp.path()));
    h.run_steps(3);

    h.key_press_modifiers(Modifiers::COMMAND, Key::Equals);
    wait_until(&mut h, "zoom avant enregistré", |_| saved_scale(tmp.path()) == 1.05);
    assert_eq!(h.ctx.zoom_factor(), 1.05, "le zoom est appliqué");

    h.key_press_modifiers(Modifiers::COMMAND, Key::Minus);
    h.key_press_modifiers(Modifiers::COMMAND, Key::Minus);
    wait_until(&mut h, "zoom arrière enregistré", |_| saved_scale(tmp.path()) == 0.95);
    assert_eq!(h.ctx.zoom_factor(), 0.95);
}

#[test]
fn ctrl_plus_zooms_in_too() {
    let tmp = tempfile::tempdir().unwrap();
    let mut h = harness(start(tmp.path()));
    h.run_steps(3);

    h.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::Plus);
    wait_until(&mut h, "zoom avant enregistré", |_| saved_scale(tmp.path()) == 1.05);
}

#[test]
fn view_menu_zooms_and_resets_the_size() {
    let tmp = tempfile::tempdir().unwrap();
    let mut h = harness(start(tmp.path()));

    open_menu(&mut h, "Affichage");
    click_menu_item(&mut h, "Zoom avant Ctrl+=");
    wait_until(&mut h, "zoom avant enregistré", |_| saved_scale(tmp.path()) == 1.05);

    open_menu(&mut h, "Affichage");
    click_menu_item(&mut h, "Taille par défaut");
    wait_until(&mut h, "taille par défaut enregistrée", |_| saved_scale(tmp.path()) == 1.0);
}

#[test]
fn theme_submenu_applies_and_checks_the_theme() {
    let tmp = tempfile::tempdir().unwrap();
    let mut h = harness(start(tmp.path()));

    open_menu(&mut h, "Affichage");
    open_menu(&mut h, "Thème");
    wait_until(&mut h, "sous-menu Thème", |h| menu_item(h, "Sombre").is_some());
    assert_eq!(menu_item_checked(&h, "Système"), Some(true));
    assert_eq!(menu_item_checked(&h, "Sombre"), Some(false));
    click_menu_item(&mut h, "Sombre");
    wait_until(&mut h, "thème sombre", |h| h.ctx.global_style().visuals.dark_mode);

    open_menu(&mut h, "Affichage");
    open_menu(&mut h, "Thème");
    wait_until(&mut h, "sous-menu Thème", |h| menu_item(h, "Sombre").is_some());
    assert_eq!(menu_item_checked(&h, "Sombre"), Some(true));
    assert_eq!(menu_item_checked(&h, "Système"), Some(false));
    click_menu_item(&mut h, "Clair");
    wait_until(&mut h, "thème clair", |h| !h.ctx.global_style().visuals.dark_mode);
}

#[test]
fn egui_keyboard_zoom_is_disabled() {
    let tmp = tempfile::tempdir().unwrap();
    let h = harness(start(tmp.path()));
    assert!(!h.ctx.options(|o| o.zoom_with_keyboard));
}
