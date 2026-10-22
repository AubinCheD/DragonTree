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

/// Remote nu `remote.git` et dépôt `travail` qui le suit, avec un commit poussé ; `b.txt` attend d'être commité. Le
/// hook `pre-commit` du dépôt bloque chaque commit jusqu'à la création du fichier renvoyé (la « porte »).
struct GatedRepo {
    bare: std::path::PathBuf,
    work: std::path::PathBuf,
    gate: std::path::PathBuf,
    /// Le commit poussé.
    pushed: String,
}

fn gated_repo(base: &std::path::Path) -> GatedRepo {
    let bare = base.join("remote.git");
    std::fs::create_dir_all(&bare).unwrap();
    git(&bare, &["init", "-q", "--bare", "-b", "main"]);
    let work = base.join("travail");
    init_repo(&work);
    commit_file(&work, "a.txt", "a\n", "déjà poussé");
    let url = format!("file:///{}", bare.display().to_string().replace('\\', "/"));
    git(&work, &["remote", "add", "origin", &url]);
    git(&work, &["push", "-q", "-u", "origin", "main"]);
    let pushed = git(&bare, &["rev-parse", "main"]).trim().to_string();
    let gate = base.join("porte");
    let hook =
        format!("#!/bin/sh\nwhile [ ! -f '{}' ]; do sleep 0.05; done\n", gate.display().to_string().replace('\\', "/"));
    std::fs::write(work.join(".git/hooks/pre-commit"), hook).unwrap();
    std::fs::write(work.join("b.txt"), "b\n").unwrap();
    GatedRepo { bare, work, gate, pushed }
}

/// Indexe `b.txt` et coche « Pousser après le commit » dans la vue des changements.
fn stage_with_push_after(h: &mut egui_kittest::Harness<'static, DragonTreeApp>) {
    click_label_containing(h, "Tout indexer");
    wait_until(h, "fichier indexé", |h| h.query_by_label_contains("Commit (1 fichier)").is_some());
    click_label(h, "Pousser après le commit");
}

/// Fermeture demandée pendant le commit bloqué par son hook, puis « Attendre » ; ouvre ensuite la porte.
fn close_and_wait_during_the_commit(h: &mut egui_kittest::Harness<'static, DragonTreeApp>, r: &GatedRepo) {
    wait_until(h, "commit bloqué par le hook", |h| h.query_by_label_contains("Commit…").is_some());
    assert!(request_close(h), "fermeture suspendue pendant le commit");
    wait_until(h, "confirmation affichée", confirmation_shown);
    assert!(h.query_by_label("• Commit").is_some(), "le commit est nommé");
    click_label(h, "Attendre");
    std::fs::write(&r.gate, "").unwrap();
}

/// RV4a (revue finale, A1) : « Attendre » attend aussi l'opération qu'une autre enchaîne à sa fin. Le push qui suit le
/// commit (« Pousser après le commit ») part quand l'interface traite la fin du commit ; la fenêtre ne se ferme
/// qu'après lui, et le remote a le commit.
#[test]
fn waiting_also_waits_for_the_push_that_follows_a_commit() {
    let tmp = tempfile::tempdir().unwrap();
    let r = gated_repo(tmp.path());
    let mut h = harness(StartOptions { open: vec![r.work.clone()], view: Some("changes".into()), ..start(tmp.path()) });
    stage_with_push_after(&mut h);
    let subject = h.get_by(|n| n.placeholder() == Some("Résumé du commit (obligatoire)"));
    subject.focus();
    subject.type_text("Commit puis push");
    wait_until(&mut h, "message saisi", |h| has_text_input(h, "Commit puis push"));
    click_label_containing(&mut h, "Commit (1 fichier)");

    close_and_wait_during_the_commit(&mut h, &r);
    // À chaque image : quand la fenêtre se ferme, le push a déjà eu lieu. La branche du remote est lue dans son fichier,
    // aussitôt : un push parti dans la même image n'a pas encore pu l'écrire.
    let mut remote_at_close = None;
    wait_until(&mut h, "fermeture à la fin du push", |h| {
        if close_requested(h) {
            remote_at_close = Some(std::fs::read_to_string(r.bare.join("refs/heads/main")).unwrap_or_default());
        }
        remote_at_close.is_some()
    });
    let head = git(&r.work, &["rev-parse", "HEAD"]);
    assert_ne!(head.trim(), r.pushed, "commit créé");
    assert_eq!(remote_at_close.unwrap().trim(), head.trim(), "fenêtre fermée avant la fin du push");
    assert!(!request_close(&mut h), "fermeture acceptée");
}

/// RV4a (revue finale, A1) : l'amend attendu d'un commit déjà poussé finit sur la proposition du push forcé (L5) ; la
/// fermeture y renonce pour la montrer, au lieu de fermer la fenêtre sans rien pousser.
#[test]
fn the_force_push_offer_that_follows_an_awaited_amend_cancels_the_closing() {
    let tmp = tempfile::tempdir().unwrap();
    let r = gated_repo(tmp.path());
    let mut h = harness(StartOptions { open: vec![r.work.clone()], view: Some("changes".into()), ..start(tmp.path()) });
    stage_with_push_after(&mut h);
    click_label(&mut h, "Amend");
    wait_until(&mut h, "message du dernier commit repris", |h| has_text_input(h, "déjà poussé"));
    click_label_containing(&mut h, "Modifier le commit");

    close_and_wait_during_the_commit(&mut h, &r);
    wait_until(&mut h, "proposition du push forcé", |h| {
        assert!(!close_requested(h), "fenêtre fermée sans proposer le push forcé");
        h.query_by_label("Pousser le commit modifié ?").is_some()
    });
    for _ in 0..10 {
        h.step();
        assert!(!close_requested(&h), "la fermeture a renoncé");
    }
    assert!(!confirmation_shown(&h), "plus de question de fermeture");
    assert!(h.query_by_label("Pousser en forçant").is_some(), "la proposition reste affichée");
    assert_eq!(git(&r.bare, &["rev-parse", "main"]).trim(), r.pushed, "rien n'est poussé sans confirmation");
}

/// RV4a (revue finale, A3) : « Quitter quand même » pendant un clonage annule git ; la fin de l'application attend
/// (2 s au plus) que le clonage supprime son dossier partiel, sans quoi son thread mourrait avec le processus.
#[test]
fn quitting_anyway_during_a_clone_removes_its_partial_folder_before_the_end() {
    let tmp = tempfile::tempdir().unwrap();
    let clones = tmp.path().join("clones");
    let profile = tmp.path().join("profil");
    std::fs::create_dir_all(&profile).unwrap();
    std::fs::write(profile.join("settings.json"), serde_json::json!({ "default_clone_dir": clones }).to_string())
        .unwrap();
    let server = SilentServer::start();
    let mut h = harness(StartOptions { window: Some("clone".into()), ..start(tmp.path()) });
    let hint = "https://github.com/organisation/depot.git";
    wait_until(&mut h, "fenêtre de clonage", |h| h.query_by(|n| n.placeholder() == Some(hint)).is_some());
    let url = h.get_by(|n| n.placeholder() == Some(hint));
    url.focus();
    url.type_text(&format!("http://127.0.0.1:{}/depot.git", server.port));
    let dest = clones.join("depot");
    wait_until(&mut h, "dossier proposé", |h| has_text_input(h, "depot"));
    click_label(&mut h, &format!("{} Cloner", dragontree_app::icons::DOWNLOAD_SIMPLE));
    wait_until(&mut h, "clonage bloqué sur le serveur muet", |_| server.connections() > 0 && dest.exists());

    assert!(request_close(&mut h), "fermeture suspendue pendant le clonage");
    wait_until(&mut h, "confirmation affichée", confirmation_shown);
    assert!(h.query_by_label("• Clonage").is_some());
    click_label(&mut h, "Quitter quand même");
    assert!(close_requested(&h));
    // Fin de l'application, comme eframe l'appelle après la fermeture.
    eframe::App::on_exit(h.state_mut(), None);
    assert!(!dest.exists(), "dossier partiel du clonage supprimé avant la fin du processus");
    server.release();
}
