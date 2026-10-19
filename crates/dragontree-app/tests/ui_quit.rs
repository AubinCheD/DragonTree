//! Fichier › Quitter : demande la fermeture de la fenêtre (ce qui passe par `save`).

mod common;

use common::*;

#[test]
fn quit_menu_entry_requests_closing_the_window() {
    let tmp = tempfile::tempdir().unwrap();
    let mut h = harness(start(tmp.path()));
    h.run_steps(3);
    assert!(!close_requested(&h), "pas de fermeture avant le clic");

    open_menu(&mut h, "Fichier");
    assert!(!menu_item_disabled(&h, "Quitter"));
    menu_item(&h, "Quitter").unwrap().click();
    // La commande est émise dans l'image qui traite le clic : on cherche sur les images suivantes.
    let mut closed = false;
    for _ in 0..5 {
        h.step();
        closed |= close_requested(&h);
    }
    assert!(closed, "Quitter doit émettre ViewportCommand::Close");
}
