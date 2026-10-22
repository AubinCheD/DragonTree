//! Configuration › Identité git… : le dialogue lit puis écrit l'identité globale (fichier de test désigné par
//! `GIT_CONFIG_GLOBAL`, jamais le `~/.gitconfig` de l'utilisateur), et un commit sans identité propose d'ouvrir
//! ce dialogue.

mod common;

use common::*;
use dragontree_app::StartOptions;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};

type H = Harness<'static, dragontree_app::DragonTreeApp>;

fn replace_field(h: &mut H, current: &str, text: &str) {
    let field = h.get_by(|n| n.role() == egui::accesskit::Role::TextInput && n.value().as_deref() == Some(current));
    field.focus();
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    h.run_steps(1);
    h.get_by(|n| n.role() == egui::accesskit::Role::TextInput && n.value().as_deref() == Some(current)).type_text(text);
    h.run_steps(2);
}

/// Un seul test : `GIT_CONFIG_GLOBAL` est un état du processus et le fichier est réécrit entre les deux parties.
#[test]
fn the_dialog_writes_the_global_identity_and_a_commit_without_identity_offers_it() {
    let global_config = isolate_global_git_config("[user]\n\tname = Globale\n\temail = globale@fabrikam.test\n");
    let global = global_config.file();
    let tmp = tempfile::tempdir().unwrap();
    let repo = demo_repo(tmp.path());
    git(&repo, &["config", "--unset", "user.name"]);
    git(&repo, &["config", "--unset", "user.email"]);
    let mut h = harness(StartOptions { open: vec![repo.clone()], view: Some("changes".into()), ..start(tmp.path()) });
    wait_until(&mut h, "dépôt chargé", |h| h.query_by_label("main").is_some());

    // Les valeurs actuelles sont préremplies, puis remplacées et enregistrées.
    open_menu(&mut h, "Configuration");
    click_menu_item(&mut h, "Identité git…");
    wait_until(&mut h, "nom global prérempli", |h| has_text_input(h, "Globale"));
    assert!(has_text_input(&h, "globale@fabrikam.test"));
    replace_field(&mut h, "Globale", "Zoé Northwind");
    replace_field(&mut h, "globale@fabrikam.test", "zoe@northwind.test");
    h.get_by_label("Enregistrer").click();
    wait_git(&mut h, "identité globale écrite dans le fichier de test", || {
        std::fs::read_to_string(&global).is_ok_and(|c| c.contains("Zoé Northwind") && c.contains("zoe@northwind.test"))
    });
    wait_until(&mut h, "dialogue refermé", |h| h.query_by_label("Enregistrer").is_none());

    // Sans identité (git ne devine plus rien), le commit échoue et la notification ouvre le dialogue.
    std::fs::write(&global, "[user]\n\tuseConfigOnly = true\n").unwrap();
    wait_until(&mut h, "bouton « Tout indexer »", |h| h.query_by_label_contains("Tout indexer").is_some());
    h.get_by_label_contains("Tout indexer").click();
    wait_until(&mut h, "fichiers indexés", |h| h.query_by_label_contains("Commit (2 fichiers)").is_some());
    let subject = h.get_by(|n| n.placeholder() == Some("Résumé du commit (obligatoire)"));
    subject.focus();
    subject.type_text("Sans identité");
    wait_until(&mut h, "résumé saisi", |h| has_text_input(h, "Sans identité"));
    click_label_containing(&mut h, "Commit (2 fichiers)");
    wait_until(&mut h, "notification d'identité manquante", |h| {
        h.query_by_label("Commit impossible : aucune identité git").is_some()
    });
    // Une image de plus : la zone ancrée des notifications est placée d'après la taille de l'image précédente, et un
    // clic reçu pendant cette image tombe sur l'ancien emplacement du bouton.
    h.step();
    h.get_by_label_contains("Régler l'identité git").click();
    wait_until(&mut h, "dialogue d'identité ouvert", |h| h.query_by_label("Enregistrer").is_some());
    // Aucune identité : les champs sont lus (donc modifiables) et vides.
    wait_until(&mut h, "champs lus", |h| {
        h.query_all_by_role(egui::accesskit::Role::TextInput)
            .any(|n| n.value().as_deref() == Some("") && !n.accesskit_node().is_disabled())
    });
}
