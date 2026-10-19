//! Filtres de l'historique : branche courante seulement (Affichage, Ctrl+Maj+A).

mod common;

use common::*;
use dragontree_app::StartOptions;
use egui::{Key, Modifiers};
use egui_kittest::kittest::Queryable;

#[test]
fn current_branch_shortcut_hides_and_restores_the_other_branch_commits() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("divergent");
    init_repo(&repo);
    commit_file(&repo, "base.txt", "base\n", "Commit commun");
    git(&repo, &["switch", "-q", "-c", "autre"]);
    commit_file(&repo, "autre.txt", "autre\n", "Commit de l'autre branche");
    git(&repo, &["switch", "-q", "main"]);
    commit_file(&repo, "main.txt", "main\n", "Commit de la branche courante");

    let mut h = harness(StartOptions { open: vec![repo], view: Some("history".into()), ..start(tmp.path()) });
    wait_until(&mut h, "historique complet", |h| {
        h.query_by_label("Commit de l'autre branche").is_some()
            && h.query_by_label("Commit de la branche courante").is_some()
    });

    let entry = "Branche courante seulement Ctrl+Maj+A";
    open_menu(&mut h, "Affichage");
    wait_until(&mut h, "entrée du filtre", |h| menu_item(h, entry).is_some());
    assert_eq!(menu_item_checked(&h, entry), Some(false));
    h.key_press(Key::Escape);

    h.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::A);
    wait_until(&mut h, "commit de l'autre branche masqué", |h| {
        h.query_by_label("Commit de l'autre branche").is_none()
    });
    assert!(h.query_by_label("Commit de la branche courante").is_some());
    open_menu(&mut h, "Affichage");
    wait_until(&mut h, "entrée du filtre", |h| menu_item(h, entry).is_some());
    assert_eq!(menu_item_checked(&h, entry), Some(true));
    h.key_press(Key::Escape);

    h.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::A);
    wait_until(&mut h, "commit de l'autre branche réaffiché", |h| {
        h.query_by_label("Commit de l'autre branche").is_some()
    });
    open_menu(&mut h, "Affichage");
    wait_until(&mut h, "entrée du filtre", |h| menu_item(h, entry).is_some());
    assert_eq!(menu_item_checked(&h, entry), Some(false));
}
