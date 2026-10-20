//! Configuration › Raccourcis clavier (liste groupée, recherche) et À propos (version, git détecté, licence).

mod common;

use common::*;
use dragontree_app::StartOptions;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

type H = Harness<'static, dragontree_app::DragonTreeApp>;

fn open_app(base: &std::path::Path) -> H {
    let mut h = harness(StartOptions { open: Vec::new(), ..start(base) });
    wait_until(&mut h, "barre de menus", |h| menu_item(h, "Configuration").is_some());
    h
}

#[test]
fn the_shortcuts_window_lists_the_registry_shortcuts_and_the_search_filters_them() {
    let tmp = tempfile::tempdir().unwrap();
    let mut h = open_app(tmp.path());
    open_menu(&mut h, "Configuration");
    click_menu_item(&mut h, "Raccourcis clavier");

    // Un raccourci du registre, un raccourci fixe, et les titres de groupes (les menus).
    wait_until(&mut h, "raccourci du registre", |h| h.query_by_label("Ctrl+Maj+T").is_some());
    assert!(h.query_by_label("Ctrl+Maj+F").is_some());
    assert!(h.query_by_label("Ctrl+Tab / Ctrl+Maj+Tab").is_some());
    assert!(h.query_by_label("Navigation et listes").is_some());

    let search = h.get_by(|n| n.placeholder() == Some("Rechercher un raccourci ou une action"));
    search.focus();
    search.type_text("onglet");
    wait_until(&mut h, "liste filtrée", |h| h.query_by_label("Ctrl+Maj+F").is_none());
    assert!(h.query_by_label("Ctrl+Maj+T").is_some(), "« Rouvrir l'onglet fermé » reste affiché");
    assert!(h.query_by_label("Ctrl+Tab / Ctrl+Maj+Tab").is_some(), "la recherche porte aussi sur les raccourcis fixes");

    // Une recherche sans résultat le dit.
    let search = h.get_by(|n| n.placeholder() == Some("Rechercher un raccourci ou une action"));
    search.type_text(" zzz");
    wait_until(&mut h, "aucun résultat", |h| h.query_by_label("Aucun raccourci ne correspond.").is_some());
}

#[test]
fn the_about_window_shows_the_package_version_and_the_detected_git() {
    let tmp = tempfile::tempdir().unwrap();
    let mut h = open_app(tmp.path());
    open_menu(&mut h, "Configuration");
    click_menu_item(&mut h, "À propos de DragonTree");

    let version = format!("Version {}", env!("CARGO_PKG_VERSION"));
    wait_until(&mut h, "version du paquet", |h| h.query_by_label(&version).is_some());
    // La version de git est lue en arrière-plan : elle apparaît ensuite.
    wait_until(&mut h, "version de git", |h| h.query_by_label_contains("git détecté : ").is_some());
    assert!(h.query_by_label("DragonTree est distribué sous licence MIT.").is_some());
    assert!(h.query_by_label_contains("Phosphor Icons").is_some());
}
