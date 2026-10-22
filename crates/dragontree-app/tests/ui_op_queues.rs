//! Files d'opérations d'un dépôt (RV3) : le fetch automatique passe par une file à part, et ne fait plus attendre les
//! écritures ; une opération réseau visible attend la fin du fetch automatique en cours, sans l'arrêter. Remote fictif
//! servi par un serveur HTTP local qui accepte les connexions sans jamais répondre.

mod common;

use std::time::Duration;

use common::*;
use dragontree_app::StartOptions;
use egui_kittest::kittest::Queryable;

#[test]
fn staging_completes_while_the_automatic_fetch_is_blocked() {
    let tmp = tempfile::tempdir().unwrap();
    let server = SilentServer::start();
    let repo = repo_with_silent_remote(tmp.path(), &server);
    std::fs::write(repo.join("a.txt"), "2\n").unwrap();
    let mut h = harness(StartOptions {
        open: vec![repo.clone()],
        view: Some("changes".into()),
        auto_fetch_every: Some(Duration::ZERO),
        ..start(tmp.path())
    });

    // Le fetch automatique part dès l'ouverture et reste bloqué : le serveur ne répond jamais.
    wait_until(&mut h, "fetch automatique bloqué sur le serveur muet", |_| server.connections() > 0);
    wait_until(&mut h, "bouton « Tout indexer »", |h| h.query_by_label_contains("Tout indexer").is_some());
    h.get_by_label_contains("Tout indexer").click();
    // L'indexation ne fait pas la queue derrière le fetch : elle se termine pendant qu'il est bloqué.
    wait_git(&mut h, "a.txt indexé pendant le fetch", || staged(&repo).is_some_and(|s| s == ["a.txt"]));
    assert_eq!(server.connections(), 1, "un seul fetch automatique à la fois");

    server.release();
}

/// Un fetch visible lancé pendant le fetch automatique ne l'arrête pas (git tué pendant l'écriture des refs laisserait
/// des fichiers `*.lock`) : il attend sa fin, et la barre de progression le dit.
#[test]
fn a_visible_fetch_waits_for_the_running_automatic_fetch_and_says_so() {
    let tmp = tempfile::tempdir().unwrap();
    let server = SilentServer::start();
    let repo = repo_with_silent_remote(tmp.path(), &server);
    let mut h = harness(StartOptions { open: vec![repo], auto_fetch_every: Some(Duration::ZERO), ..start(tmp.path()) });
    wait_until(&mut h, "fetch automatique bloqué sur le serveur muet", |_| server.connections() > 0);

    click_label(&mut h, "Fetch");
    let waiting = "Fetch — en attente de la fin du fetch automatique…";
    wait_until(&mut h, "attente affichée", |h| h.query_by_label(waiting).is_some());
    assert_eq!(server.closed(), 0, "le fetch automatique n'est pas arrêté");
    assert_eq!(server.connections(), 1, "le fetch visible n'est pas parti");

    // Fin du fetch automatique (le serveur ferme sa connexion) : le fetch visible part à son tour.
    server.release();
    wait_until(&mut h, "fetch visible parti", |_| server.connections() == 2);
    wait_until(&mut h, "plus d'attente affichée", |h| h.query_by_label(waiting).is_none());
    server.release();
}
