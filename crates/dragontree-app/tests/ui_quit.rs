//! Fichier › Quitter et fermeture de la fenêtre : demande la fermeture (ce qui passe par `save`) ; pendant des
//! opérations visibles, une confirmation la suspend (RV4a). Remote fictif servi par un serveur HTTP local muet.

mod common;

use common::*;
use dragontree_app::{DragonTreeApp, StartOptions};
use egui_kittest::kittest::Queryable;

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

/// Lance un fetch visible (bouton de la barre d'outils) qui reste bloqué sur le serveur muet.
fn blocked_fetch(tmp: &std::path::Path, server: &SilentServer) -> egui_kittest::Harness<'static, DragonTreeApp> {
    let repo = repo_with_silent_remote(tmp, server);
    let mut h = harness(StartOptions { open: vec![repo], ..start(tmp) });
    click_label(&mut h, "Fetch");
    wait_until(&mut h, "fetch bloqué sur le serveur muet", |_| server.connections() > 0);
    h
}

fn confirmation_shown(h: &egui_kittest::Harness<'static, DragonTreeApp>) -> bool {
    h.query_by_label_contains("Des opérations sont en cours").is_some()
}

/// RV4a : fermer la fenêtre pendant une opération visible demande confirmation ; « Quitter quand même » ferme la
/// fenêtre et arrête l'opération (git est arrêté : sa connexion se ferme).
#[test]
fn closing_during_an_operation_asks_and_quitting_anyway_stops_it() {
    let tmp = tempfile::tempdir().unwrap();
    let server = SilentServer::start();
    let mut h = blocked_fetch(tmp.path(), &server);

    assert!(request_close(&mut h), "fermeture suspendue (CancelClose)");
    wait_until(&mut h, "confirmation affichée", confirmation_shown);
    assert!(h.query_by_label("• Fetch").is_some(), "l'opération en cours est nommée");
    assert_eq!(server.closed(), 0, "rien n'est arrêté avant la réponse");

    // Le clic est traité dans l'image que joue `click_label`, qui émet aussitôt la fermeture.
    click_label(&mut h, "Quitter quand même");
    assert!(close_requested(&h), "fermeture demandée");
    wait_until(&mut h, "fetch arrêté (connexion fermée par git)", |_| server.closed() > 0);
    // La fermeture qui suit (celle que `ViewportCommand::Close` déclenche) n'est plus suspendue.
    assert!(!request_close(&mut h), "fermeture acceptée");
    server.release();
}

/// « Attendre » : la fenêtre se ferme d'elle-même à la fin des opérations.
#[test]
fn waiting_closes_the_window_when_the_operations_end() {
    let tmp = tempfile::tempdir().unwrap();
    let server = SilentServer::start();
    let mut h = blocked_fetch(tmp.path(), &server);

    assert!(request_close(&mut h));
    click_label(&mut h, "Attendre");
    h.run_steps(5);
    assert!(!close_requested(&h), "pas de fermeture tant que le fetch tourne");
    assert_eq!(server.closed(), 0, "le fetch n'est pas arrêté");
    server.release();
    wait_until(&mut h, "fermeture à la fin du fetch", close_requested);
    assert!(!request_close(&mut h), "fermeture acceptée");
}

/// Seules des opérations silencieuses (fetch automatique) tournent : pas de question, elles sont arrêtées.
#[test]
fn closing_while_only_the_automatic_fetch_runs_does_not_ask() {
    let tmp = tempfile::tempdir().unwrap();
    let server = SilentServer::start();
    let repo = repo_with_silent_remote(tmp.path(), &server);
    let mut h = harness(StartOptions {
        open: vec![repo],
        auto_fetch_every: Some(std::time::Duration::ZERO),
        ..start(tmp.path())
    });
    wait_until(&mut h, "fetch automatique bloqué", |_| server.connections() > 0);

    assert!(!request_close(&mut h), "fermeture acceptée sans question");
    assert!(!confirmation_shown(&h));
    wait_until(&mut h, "fetch automatique arrêté", |_| server.closed() > 0);
    server.release();
}
