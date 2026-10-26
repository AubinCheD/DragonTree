//! Lancé seul, DragonTree met ses menus dans sa barre de titre, comme Fawkes et Eyrie (`suite_titlebar`) ; embarqué,
//! ils restent dans sa case, sous la barre de l'hôte.

mod common;

use common::*;
use egui_kittest::kittest::Queryable;
use suite_titlebar::hit;

/// La zone du bouton de ce nom dans la barre de titre (le titre de la fenêtre et l'icône ne sont pas des boutons).
fn in_title_bar(h: &egui_kittest::Harness<'static, dragontree_app::DragonTreeApp>, label: &str) -> Option<egui::Rect> {
    h.query_all_by(|n| n.label().as_deref() == Some(label) && n.role() == egui::accesskit::Role::Button)
        .map(|n| n.rect())
        .find(|r| r.bottom() <= suite_titlebar::HEIGHT + 0.5)
}

#[test]
fn alone_the_menus_are_in_the_title_bar() {
    let tmp = tempfile::tempdir().unwrap();
    let mut h = harness(start(tmp.path()));
    h.run_steps(3);
    let caption = suite_titlebar::caption().expect("barre confiée à Windows");
    let at = |p: egui::Pos2| caption.hit(p.x as i32, p.y as i32, 1400, 900, false);
    for label in ["Fichier", "Affichage", "Dépôt", "Configuration", "Réduire", "Agrandir", "Fermer"] {
        let r = in_title_bar(&h, label).unwrap_or_else(|| panic!("{label} dans la barre de titre"));
        assert_eq!(at(r.center()), hit::CLIENT, "{label} reste à DragonTree");
    }
    let after_menus = in_title_bar(&h, "Configuration").unwrap().right() + 150.0;
    assert_eq!(at(egui::pos2(after_menus, 16.0)), hit::CAPTION, "le reste de la barre déplace la fenêtre");
}

#[test]
fn hosted_the_menus_stay_in_the_pane() {
    let tmp = tempfile::tempdir().unwrap();
    let start = start(tmp.path());
    let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1200.0, 800.0)).build_eframe(move |cc| {
        let mut app = dragontree_app::DragonTreeApp::new(cc, start);
        app.set_hosted(true);
        app
    });
    h.run_steps(3);
    assert!(h.query_by_label("Réduire").is_none(), "pas de boutons de fenêtre : la barre est celle de l'hôte");
    assert!(h.query_by_label("Fichier").is_some(), "les menus restent dans la case");
    assert_eq!(suite_titlebar::caption(), None, "rien n'est confié à Windows");
}
