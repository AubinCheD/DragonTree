//! Fichier › Clés SSH… : la fenêtre liste les clés d'un dossier .ssh de test (jamais le vrai) et écrit /
//! retire `core.sshCommand` dans une configuration globale temporaire (jamais le vrai ~/.gitconfig).

mod common;

use common::*;
use dragontree_app::StartOptions;
use dragontree_git::Git;
use dragontree_git::ssh::{self, KeyType};
use egui_kittest::kittest::Queryable;

#[test]
fn ssh_keys_window_lists_a_test_key_and_selects_it_for_git() {
    // Seul test de ce fichier : la configuration globale est isolée avant toute commande git.
    let global_config = isolate_global_git_config("");
    let global = global_config.file();
    let tmp = tempfile::tempdir().unwrap();
    let ssh_dir = tmp.path().join("ssh-test");
    let key = ssh::generate_key(
        &ssh_dir,
        &ssh::find_ssh_keygen(Git::discover().unwrap().exe()),
        "id_ed25519",
        KeyType::Ed25519,
        "jean@fabrikam.test",
        "",
    )
    .unwrap();
    let config = || std::fs::read_to_string(&global).unwrap();

    let mut h = harness(StartOptions { ssh_dir: Some(ssh_dir), ..start(tmp.path()) });
    open_menu(&mut h, "Fichier");
    click_menu_item(&mut h, "Clés SSH…");

    // La fenêtre liste la clé : nom, commentaire et empreinte lue par ssh-keygen.
    wait_until(&mut h, "clé listée", |h| h.query_by_label(&key.fingerprint).is_some());
    assert!(h.query_by_label("id_ed25519").is_some());
    assert!(h.query_by_label("jean@fabrikam.test").is_some());
    assert!(h.query_by_label_contains("utilisée pour git").is_none());

    // « Utiliser pour git » écrit core.sshCommand (global) et signale la clé.
    h.get_by_label("Utiliser pour git").click();
    wait_until(&mut h, "core.sshCommand écrit", |_| config().contains("sshCommand"));
    assert!(config().contains("IdentitiesOnly=yes") && config().contains("id_ed25519"), "{}", config());
    wait_until(&mut h, "clé signalée", |h| h.query_by_label_contains("utilisée pour git (global)").is_some());

    // Retour à la configuration par défaut.
    h.get_by_label("Revenir à la configuration par défaut").click();
    wait_until(&mut h, "core.sshCommand retiré", |_| !config().contains("sshCommand"));
    wait_until(&mut h, "signalement retiré", |h| h.query_by_label_contains("utilisée pour git").is_none());

    // Une commande écrite à la main n'est ni remplacée ni retirée sans confirmation explicite.
    let foreign = "ssh -o StrictHostKeyChecking=no";
    std::fs::write(&global, format!("[core]\n\tsshCommand = {foreign}\n")).unwrap();
    h.get_by_label_contains("Actualiser").click();
    h.run_steps(3);
    // Le rafraîchissement lit la config : on laisse la liste se recharger avant de cliquer.
    becomes_true(&mut h, std::time::Duration::from_millis(600), || false);
    h.get_by_label("Revenir à la configuration par défaut").click();
    wait_until(&mut h, "confirmation affichée", |h| h.query_by_label_contains("n'a pas été écrite").is_some());
    assert!(h.query_by_label_contains(foreign).is_some(), "la valeur actuelle est montrée");
    assert!(config().contains("StrictHostKeyChecking"), "rien n'est retiré avant confirmation");
    // Entrée ne confirme pas.
    h.key_press(egui::Key::Enter);
    h.run_steps(5);
    assert!(config().contains("StrictHostKeyChecking"), "Entrée ne valide pas un dialogue destructeur");
    assert!(h.query_by_label_contains("n'a pas été écrite").is_some());
    // Annuler laisse tout en place.
    h.get_by_label("Annuler").click();
    h.run_steps(5);
    assert!(h.query_by_label_contains("n'a pas été écrite").is_none());
    assert!(config().contains("StrictHostKeyChecking"));
    // « Utiliser pour git » demande aussi confirmation, puis remplace après le clic.
    h.get_by_label("Utiliser pour git").click();
    wait_until(&mut h, "confirmation affichée", |h| h.query_by_label_contains("n'a pas été écrite").is_some());
    assert!(config().contains("StrictHostKeyChecking"));
    h.get_by_label("Remplacer").click();
    wait_until(&mut h, "valeur remplacée", |_| config().contains("id_ed25519") && !config().contains("Strict"));
}
