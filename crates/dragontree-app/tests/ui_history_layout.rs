//! Affichage › Disposition : panneau de détails sous la liste (verticale) ou à sa droite (horizontale).

mod common;

use common::*;
use dragontree_app::StartOptions;
use dragontree_app::settings::{HistoryLayout, Settings};
use egui_kittest::kittest::Queryable;

#[test]
fn details_panel_is_below_the_list_or_on_its_right_by_layout() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("fabrikam");
    init_repo(&repo);
    commit_file(&repo, "a.txt", "a\n", "Premier commit");
    let first = git(&repo, &["rev-parse", "HEAD"]).trim().to_string();
    commit_file(&repo, "b.txt", "b\n", "Second commit");

    let mut h = harness(StartOptions { open: vec![repo], view: Some("history".into()), ..start(tmp.path()) });
    wait_until(&mut h, "historique", |h| h.query_by_label("Premier commit").is_some());
    h.get_by_label("Premier commit").click();
    wait_until(&mut h, "détails du premier commit", |h| h.query_by_label(&first).is_some());

    // Disposition d'origine : détails sous la liste (le SHA complet n'est que dans le panneau).
    let list = h.get_by_label("Second commit").rect();
    let details = h.get_by_label(&first).rect();
    assert!(details.min.y >= list.max.y, "détails {details:?} pas sous la liste {list:?}");

    // Horizontale : détails à droite de la liste.
    open_menu(&mut h, "Affichage");
    open_menu(&mut h, "Disposition");
    click_menu_item(&mut h, "Horizontale (détails à droite)");
    wait_until(&mut h, "détails à droite", |h| {
        h.query_by_label(&first).is_some_and(|d| d.rect().min.x >= h.get_by_label("Second commit").rect().max.x)
    });
    let saved = Settings::load_from(&tmp.path().join("profil").join("settings.json"));
    assert_eq!(saved.history_layout, HistoryLayout::Horizontal);

    // De nouveau verticale.
    open_menu(&mut h, "Affichage");
    open_menu(&mut h, "Disposition");
    click_menu_item(&mut h, "Verticale (détails sous la liste)");
    wait_until(&mut h, "détails sous la liste", |h| {
        h.query_by_label(&first).is_some_and(|d| d.rect().min.y >= h.get_by_label("Second commit").rect().max.y)
    });
}
