//! Instance unique (RV4b) : un dépôt transmis par une instance lancée ensuite sur le même profil s'ouvre dans un
//! onglet de la première, qui passe au premier plan. Les deux instances sont simulées dans ce processus, sur la clé du
//! profil temporaire du test.
#![cfg(windows)]

mod common;

use std::path::Path;
use std::time::Duration;

use common::*;
use dragontree_app::DragonTreeApp;
use dragontree_app::settings::Settings;
use dragontree_app::single_instance::{self, Forwarded, Startup};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

fn open_tabs(base: &Path) -> Vec<std::path::PathBuf> {
    Settings::load_from(&base.join("profil").join("settings.json")).open_tabs
}

#[test]
fn a_repository_sent_by_a_second_instance_opens_and_the_window_comes_forward() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("contoso-depot");
    init_repo(&repo);
    commit_file(&repo, "a.txt", "a\n", "Initial");
    let start = start(tmp.path());
    let key = single_instance::profile_key(start.profile_dir.as_ref().unwrap());
    let Startup::Primary(instance) = single_instance::start(&key, &Forwarded::default(), Duration::from_secs(10))
    else {
        panic!("première instance du profil attendue");
    };
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |cc| {
        let mut app = DragonTreeApp::new(cc, start);
        app.set_instance(instance);
        app
    });
    h.run_steps(3);
    assert!(open_tabs(tmp.path()).is_empty());

    let second = single_instance::start(&key, &Forwarded { open: vec![repo.clone()] }, Duration::from_secs(10));
    assert!(matches!(second, Startup::Forwarded), "la seconde instance transmet et quitte");
    wait_until(&mut h, "fenêtre au premier plan", |h| viewport_command_sent(h, &egui::ViewportCommand::Focus));
    wait_until(&mut h, "dépôt ouvert dans un onglet", |_| open_tabs(tmp.path()) == [repo.clone()]);
}

/// Une capture d'écran (`--screenshot`) est une instance à part : sans `--profile`, elle lit le profil d'une instance
/// peut-être ouverte. Elle n'écrit donc jamais les réglages.
#[test]
fn a_screenshot_run_never_writes_the_settings() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("depot");
    init_repo(&repo);
    commit_file(&repo, "a.txt", "a\n", "Initial");
    let start = dragontree_app::StartOptions {
        open: vec![repo],
        screenshot: Some(tmp.path().join("capture.png")),
        // Jamais atteint : la capture n'est pas demandée pendant le test.
        screenshot_after: Duration::from_secs(3600),
        ..start(tmp.path())
    };
    let mut h = harness(start);
    // L'ouverture de l'onglet et le changement de largeur des panneaux marquent les réglages à enregistrer.
    h.run_steps(10);
    assert!(!tmp.path().join("profil").join("settings.json").exists(), "réglages écrits par une capture d'écran");
}

/// RV4a (revue finale, A2) : un dépôt transmis pendant une fermeture suspendue (« Attendre ») l'annule : la fenêtre
/// reste ouverte à la fin des opérations, et le dépôt transmis s'y ouvre au lieu d'être perdu.
#[test]
fn a_repository_sent_while_waiting_to_close_cancels_the_closing() {
    let tmp = tempfile::tempdir().unwrap();
    let server = SilentServer::start();
    let busy = repo_with_silent_remote(tmp.path(), &server);
    let other = tmp.path().join("contoso-depot");
    init_repo(&other);
    commit_file(&other, "a.txt", "a\n", "Initial");
    let start = dragontree_app::StartOptions { open: vec![busy.clone()], ..start(tmp.path()) };
    let key = single_instance::profile_key(start.profile_dir.as_ref().unwrap());
    let Startup::Primary(instance) = single_instance::start(&key, &Forwarded::default(), Duration::from_secs(10))
    else {
        panic!("première instance du profil attendue");
    };
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |cc| {
        let mut app = DragonTreeApp::new(cc, start);
        app.set_instance(instance);
        app
    });
    click_label(&mut h, "Fetch");
    wait_until(&mut h, "fetch bloqué sur le serveur muet", |_| server.connections() > 0);
    assert!(request_close(&mut h), "fermeture suspendue");
    click_label(&mut h, "Attendre");

    let second = single_instance::start(&key, &Forwarded { open: vec![other.clone()] }, Duration::from_secs(10));
    assert!(matches!(second, Startup::Forwarded));
    wait_until(&mut h, "dépôt transmis ouvert", |_| open_tabs(tmp.path()).contains(&other));
    wait_until(&mut h, "plus de question de fermeture", |h| {
        h.query_by_label_contains("Des opérations sont en cours").is_none()
    });
    server.release();
    wait_until(&mut h, "fetch terminé", |h| h.query_by_label_contains("Fetch…").is_none());
    for _ in 0..10 {
        h.step();
        assert!(!close_requested(&h), "la fenêtre reste ouverte");
    }
}
