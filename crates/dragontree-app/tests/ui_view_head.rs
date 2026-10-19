//! Affichage : Révéler HEAD (Ctrl+0) et panneau de détails (Ctrl+Maj+D).

mod common;

use common::*;
use dragontree_app::StartOptions;
use dragontree_app::settings::Settings;
use egui::{Key, Modifiers};
use egui_kittest::kittest::Queryable;

#[test]
fn reveal_head_reselects_head_and_details_panel_can_be_hidden() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("fabrikam");
    init_repo(&repo);
    commit_file(&repo, "a.txt", "a\n", "Premier commit");
    let first = git(&repo, &["rev-parse", "HEAD"]).trim().to_string();
    commit_file(&repo, "b.txt", "b\n", "Second commit");
    let head = git(&repo, &["rev-parse", "HEAD"]).trim().to_string();

    let mut h = harness(StartOptions { open: vec![repo], view: Some("history".into()), ..start(tmp.path()) });
    wait_until(&mut h, "historique", |h| h.query_by_label("Premier commit").is_some());

    // Un autre commit est sélectionné : le panneau affiche son SHA complet.
    h.get_by_label("Premier commit").click();
    wait_until(&mut h, "détails du premier commit", |h| h.query_by_label(&first).is_some());
    assert!(h.query_by_label(&head).is_none());

    h.key_press_modifiers(Modifiers::COMMAND, Key::Num0);
    wait_until(&mut h, "HEAD resélectionné", |h| h.query_by_label(&head).is_some());
    assert!(h.query_by_label(&first).is_none());

    // Le panneau de détails se masque (Ctrl+Maj+D) et le réglage est enregistré.
    h.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::D);
    wait_until(&mut h, "panneau masqué", |h| h.query_by_label(&head).is_none());
    let saved = Settings::load_from(&tmp.path().join("profil").join("settings.json"));
    assert!(!saved.details_panel_visible);

    h.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::D);
    wait_until(&mut h, "panneau réaffiché", |h| h.query_by_label(&head).is_some());
}
