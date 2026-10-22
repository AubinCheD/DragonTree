//! `--theme` et `--lang` valent pour cette exécution seulement (RV5a) : en vigueur (cochés dans les menus, montrés
//! par la fenêtre Réglages) mais jamais enregistrés, même quand d'autres réglages le sont ; un thème choisi dans
//! l'application (menu, Réglages) est enregistré et remplace celui de la ligne de commande ; de même pour la langue.
//! La langue reste le français dans tout ce binaire (`start` passe `--lang fr`) : elle est globale au processus.
//! Le harnais impose son propre thème (sombre) après la création de l'application : le thème appliqué au démarrage
//! ne s'observe pas ici, seulement le choix en vigueur et les changements faits ensuite.

mod common;

use std::path::{Path, PathBuf};

use common::*;
use dragontree_app::settings::{Settings, ThemeChoice};
use dragontree_app::{DragonTreeApp, StartOptions};
use dragontree_i18n::Lang;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};

fn settings_file(base: &Path) -> PathBuf {
    base.join("profil").join("settings.json")
}

fn saved(base: &Path) -> Settings {
    Settings::load_from(&settings_file(base))
}

/// Réglages enregistrés avant le lancement, avec ce thème (langue du système).
fn save_theme(base: &Path, theme: ThemeChoice) {
    Settings { theme, ..Settings::default() }.save_to(&settings_file(base)).unwrap();
}

/// Choix sélectionné dans la fenêtre Réglages (bouton du thème ou de la langue).
fn selected_in_window(h: &Harness<'static, DragonTreeApp>, label: &str) -> bool {
    h.get_by_label_contains(label).accesskit_node().toggled() == Some(egui::accesskit::Toggled::True)
}

/// Thème coché dans Affichage › Thème (le menu est refermé ensuite).
fn checked_theme(h: &mut Harness<'static, DragonTreeApp>) -> Vec<&'static str> {
    open_menu(h, "Affichage");
    open_menu(h, "Thème");
    wait_until(h, "sous-menu Thème", |h| menu_item(h, "Sombre").is_some());
    let checked =
        ["Système", "Clair", "Sombre"].into_iter().filter(|t| menu_item_checked(h, t) == Some(true)).collect();
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    checked
}

#[test]
fn the_command_line_theme_and_language_are_in_effect_but_never_saved() {
    let tmp = tempfile::tempdir().unwrap();
    save_theme(tmp.path(), ThemeChoice::Light);
    let mut h =
        harness(StartOptions { theme: Some(ThemeChoice::Dark), window: Some("settings".into()), ..start(tmp.path()) });
    wait_until(&mut h, "fenêtre Réglages", |h| h.query_by_label("Enregistrer").is_some());
    // La fenêtre montre le thème et la langue en vigueur (la langue enregistrée est celle du système).
    assert!(selected_in_window(&h, "Sombre"), "thème en vigueur montré");
    assert!(selected_in_window(&h, "Français"), "langue en vigueur montrée");

    // « Enregistrer » dans la fenêtre Réglages, sans toucher au thème ni à la langue : l'autre réglage est
    // enregistré, sans le thème ni la langue de la ligne de commande, qui restent en vigueur.
    h.get_by_label("Afficher l'état de chaque dépôt (modifié, en avance, en retard)").click();
    h.run_steps(2);
    h.get_by_label("Enregistrer").click();
    wait_until(&mut h, "réglage de la fenêtre enregistré", |_| !saved(tmp.path()).status_sweep);
    let s = saved(tmp.path());
    assert_eq!((s.theme, s.language), (ThemeChoice::Light, None), "--theme et --lang non enregistrés");
    assert_eq!(checked_theme(&mut h), ["Sombre"], "le menu coche le thème en vigueur");

    // De même pour un réglage changé par un menu.
    open_menu(&mut h, "Affichage");
    click_menu_item(&mut h, "Afficher les tags");
    wait_until(&mut h, "réglage du menu enregistré", |_| !saved(tmp.path()).show_tags);
    let s = saved(tmp.path());
    assert_eq!((s.theme, s.language), (ThemeChoice::Light, None), "--theme et --lang non enregistrés");
    drop(h);

    // Lancement suivant, sans --theme : le thème enregistré.
    let mut h = harness(start(tmp.path()));
    assert_eq!(checked_theme(&mut h), ["Clair"], "thème enregistré au lancement suivant");
}

#[test]
fn a_theme_chosen_in_the_menu_is_saved_even_if_it_is_the_command_line_one() {
    let tmp = tempfile::tempdir().unwrap();
    save_theme(tmp.path(), ThemeChoice::Light);
    let mut h = harness(StartOptions { theme: Some(ThemeChoice::Dark), ..start(tmp.path()) });

    open_menu(&mut h, "Affichage");
    open_menu(&mut h, "Thème");
    click_menu_item(&mut h, "Sombre");
    wait_until(&mut h, "thème du menu enregistré", |_| saved(tmp.path()).theme == ThemeChoice::Dark);
    assert_eq!(checked_theme(&mut h), ["Sombre"]);
}

/// Langue : comme le thème, l'entrée du menu est enregistrée même si c'est celle de `--lang` (`start` passe
/// `--lang fr`). Le choix d'une autre langue, qui changerait celle du processus, se vérifie dans des binaires à part :
/// `ui_menus_lang` (menu) et `ui_lang` (fenêtre Réglages) partent d'un `--lang` et passent à l'autre langue, ce qui
/// n'arrive que si le choix met fin au remplacement.
#[test]
fn a_language_chosen_in_the_menu_is_saved_even_if_it_is_the_command_line_one() {
    let tmp = tempfile::tempdir().unwrap();
    let mut h = harness(start(tmp.path()));

    open_menu(&mut h, "Configuration");
    open_menu(&mut h, "Langue");
    click_menu_item(&mut h, "Français");
    wait_until(&mut h, "langue du menu enregistrée", |_| saved(tmp.path()).language == Some(Lang::Fr));
    open_menu(&mut h, "Configuration");
    open_menu(&mut h, "Langue");
    wait_until(&mut h, "sous-menu Langue", |h| menu_item(h, "English").is_some());
    assert_eq!(menu_item_checked(&h, "Français"), Some(true));
    assert_eq!(menu_item_checked(&h, "Système"), Some(false));
}

#[test]
fn a_theme_chosen_in_the_settings_window_replaces_the_command_line_one() {
    let tmp = tempfile::tempdir().unwrap();
    let mut h =
        harness(StartOptions { theme: Some(ThemeChoice::Dark), window: Some("settings".into()), ..start(tmp.path()) });
    wait_until(&mut h, "fenêtre Réglages", |h| h.query_by_label("Enregistrer").is_some());

    h.get_by_label_contains("Clair").click();
    h.run_steps(2);
    h.get_by_label("Enregistrer").click();
    // Le thème sombre (celui de --theme, et celui du harnais) laisse la place au thème choisi.
    wait_until(&mut h, "thème de la fenêtre appliqué", |h| !h.ctx.global_style().visuals.dark_mode);
    assert_eq!(saved(tmp.path()).theme, ThemeChoice::Light, "thème de la fenêtre enregistré");
    assert_eq!(checked_theme(&mut h), ["Clair"]);
}
