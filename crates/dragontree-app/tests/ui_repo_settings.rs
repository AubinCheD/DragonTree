//! Réglages du dépôt : identité locale utilisée par les commits, règles de liens vers les tickets (enregistrées
//! par dépôt, liens cliquables dans les détails du commit) et modèle de message de commit.

mod common;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use common::*;
use dragontree_app::settings::{Settings, TicketRule, path_key};
use dragontree_app::{DragonTreeApp, StartOptions};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

type H = Harness<'static, DragonTreeApp>;

/// Identité globale connue des tests : un fichier temporaire, jamais le `~/.gitconfig` de l'utilisateur. Chaque test
/// tient le résultat jusqu'à sa fin.
fn isolate_global_config() -> Arc<GlobalGitConfig> {
    isolate_global_git_config("[user]\n\tname = Globale\n\temail = globale@fabrikam.test\n")
}

fn settings_path(base: &Path) -> PathBuf {
    base.join("profil").join("settings.json")
}

fn open(base: &Path, repo: &Path, view: &str) -> H {
    let mut h = harness(StartOptions { open: vec![repo.to_path_buf()], view: Some(view.into()), ..start(base) });
    wait_until(&mut h, "dépôt chargé", |h| h.query_by_label("main").is_some());
    h
}

fn open_dialog(h: &mut H) {
    open_menu(h, "Dépôt");
    click_menu_item(h, "Réglages du dépôt…");
    wait_until(h, "dialogue des réglages", |h| h.query_by_label("Enregistrer").is_some());
}

/// Champ de saisie dont le texte indicatif est `hint` : on y tape `text`.
fn type_in(h: &mut H, hint: &str, text: &str) {
    let field = h.get_by(|n| n.placeholder() == Some(hint));
    field.focus();
    field.type_text(text);
    h.run_steps(2);
}

#[test]
fn the_local_identity_saved_in_the_dialog_is_used_by_commits() {
    let _global = isolate_global_config();
    let tmp = tempfile::tempdir().unwrap();
    let repo = demo_repo(tmp.path());
    // Pas d'identité propre au dépôt : l'identité globale s'applique, affichée en indication.
    git(&repo, &["config", "--unset", "user.name"]);
    git(&repo, &["config", "--unset", "user.email"]);
    let mut h = open(tmp.path(), &repo, "changes");
    open_dialog(&mut h);
    type_in(&mut h, "Globale", "Alice Fabrikam");
    type_in(&mut h, "globale@fabrikam.test", "alice@fabrikam.test");
    h.get_by_label("Enregistrer").click();
    wait_git(&mut h, "identité locale écrite", || {
        try_git(&repo, &["config", "--local", "user.name"]).is_some_and(|n| n.trim() == "Alice Fabrikam")
    });

    wait_until(&mut h, "bouton « Tout indexer »", |h| h.query_by_label_contains("Tout indexer").is_some());
    h.get_by_label_contains("Tout indexer").click();
    wait_until(&mut h, "fichiers indexés", |h| h.query_by_label_contains("Commit (2 fichiers)").is_some());
    let subject = h.get_by(|n| n.placeholder() == Some("Résumé du commit (obligatoire)"));
    subject.focus();
    subject.type_text("Commit avec l'identité locale");
    h.run_steps(3);
    h.get_by_label_contains("Commit (2 fichiers)").click();
    wait_git(&mut h, "commit créé", || {
        try_git(&repo, &["log", "-1", "--format=%an <%ae>"])
            .is_some_and(|s| s.trim() == "Alice Fabrikam <alice@fabrikam.test>")
    });
}

#[test]
fn emptying_the_identity_fields_falls_back_to_the_global_identity() {
    let _global = isolate_global_config();
    let tmp = tempfile::tempdir().unwrap();
    let repo = demo_repo(tmp.path());
    git(&repo, &["config", "user.name", "Locale"]);
    let mut h = open(tmp.path(), &repo, "changes");
    open_dialog(&mut h);
    wait_until(&mut h, "nom local prérempli", |h| has_text_input(h, "Locale"));
    let field = h.get_by(|n| n.role() == egui::accesskit::Role::TextInput && n.value().as_deref() == Some("Locale"));
    field.focus();
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    h.key_press(egui::Key::Backspace);
    h.run_steps(2);
    h.get_by_label("Enregistrer").click();
    wait_git(&mut h, "clé locale retirée", || try_git(&repo, &["config", "--local", "--get", "user.name"]).is_none());
}

#[test]
fn a_ticket_rule_added_in_the_dialog_is_saved_for_this_repository_only() {
    let _global = isolate_global_config();
    let tmp = tempfile::tempdir().unwrap();
    let repo = demo_repo(tmp.path());
    let mut h = open(tmp.path(), &repo, "changes");
    open_dialog(&mut h);
    h.get_by_label_contains("Ajouter une règle").click();
    h.run_steps(2);
    type_in(&mut h, r"#(\d+)", r"#(\d+)");
    type_in(&mut h, "https://tickets.contoso.test/$1", "https://tickets.contoso.test/$1");
    h.get_by_label("Enregistrer").click();
    let path = settings_path(tmp.path());
    wait_until(&mut h, "règle enregistrée", |_| !Settings::load_from(&path).ticket_links.is_empty());
    let saved = Settings::load_from(&path);
    let rules = saved.ticket_rules_of(&repo);
    assert_eq!(rules, [TicketRule { pattern: r"#(\d+)".into(), url: "https://tickets.contoso.test/$1".into() }]);
    assert_eq!(saved.ticket_links.len(), 1, "une entrée, propre à ce dépôt");
}

#[test]
fn a_ticket_reference_in_the_message_is_a_link_that_opens_the_expanded_url() {
    let _global = isolate_global_config();
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("fabrikam");
    init_repo(&repo);
    commit_file(&repo, "a.txt", "1\n", "Corrige le calcul (#42)");
    let mut seeded = Settings::default();
    seeded.ticket_links.insert(
        path_key(&repo),
        vec![TicketRule { pattern: r"#(\d+)".into(), url: "https://tickets.contoso.test/item/$1".into() }],
    );
    seeded.save_to(&settings_path(tmp.path())).unwrap();
    let mut h = open(tmp.path(), &repo, "history");
    wait_until(&mut h, "commit affiché", |h| h.query_by_label("Corrige le calcul (#42)").is_some());
    h.get_by_label("Corrige le calcul (#42)").click();
    // Le lien « #42 » n'existe que dans les détails (la liste affiche le sujet entier).
    wait_until(&mut h, "lien du ticket", |h| h.query_by_label("#42").is_some());
    h.get_by_label("#42").click();
    let mut opened = None;
    wait_until(&mut h, "URL ouverte", |h| {
        opened = h.output().platform_output.commands.iter().find_map(|c| match c {
            egui::OutputCommand::OpenUrl(u) => Some(u.url.clone()),
            _ => None,
        });
        opened.is_some()
    });
    assert_eq!(opened.as_deref(), Some("https://tickets.contoso.test/item/42"));
}

#[test]
fn the_commit_template_prefills_an_empty_commit_box() {
    let _global = isolate_global_config();
    let tmp = tempfile::tempdir().unwrap();
    let repo = demo_repo(tmp.path());
    std::fs::write(repo.join("modele.txt"), "chore: résumé\n\nDétails du changement\n").unwrap();
    git(&repo, &["config", "commit.template", "modele.txt"]);
    let mut h = open(tmp.path(), &repo, "changes");
    wait_until(&mut h, "sujet prérempli", |h| has_text_input(h, "chore: résumé"));
    wait_until(&mut h, "corps prérempli", |h| {
        h.query_by(|n| {
            n.role() == egui::accesskit::Role::MultilineTextInput
                && n.value().as_deref() == Some("Détails du changement")
        })
        .is_some()
    });
}

/// Remplace le contenu du champ de saisie qui contient `old` par `new`.
fn replace_field(h: &mut H, old: &str, new: &str) {
    wait_until(h, &format!("champ « {old} »"), |h| has_text_input(h, old));
    let field = h.get_by(|n| n.role() == egui::accesskit::Role::TextInput && n.value().as_deref() == Some(old));
    field.focus();
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    h.run_steps(1);
    h.get_by(|n| n.role() == egui::accesskit::Role::TextInput && n.is_focused()).type_text(new);
    h.run_steps(2);
}

/// Dépôt `fabrikam` avec un modèle `modele.txt` configuré, et une branche `feature` qui entre en conflit avec main.
fn conflicting_repo_with_template(base: &Path) -> PathBuf {
    let repo = base.join("fabrikam");
    init_repo(&repo);
    commit_file(&repo, "a.txt", "1\n", "Base");
    git(&repo, &["branch", "feature"]);
    commit_file(&repo, "a.txt", "main\n", "Côté main");
    git(&repo, &["checkout", "-q", "feature"]);
    commit_file(&repo, "a.txt", "feature\n", "Côté feature");
    git(&repo, &["checkout", "-q", "main"]);
    std::fs::write(repo.join("modele.txt"), "chore: résumé du modèle\n# commentaire d'aide\n").unwrap();
    git(&repo, &["config", "commit.template", "modele.txt"]);
    repo
}

#[test]
fn a_merge_stopped_on_conflicts_replaces_the_untouched_template_with_the_merge_message() {
    let _global = isolate_global_config();
    let tmp = tempfile::tempdir().unwrap();
    let repo = conflicting_repo_with_template(tmp.path());
    let mut h = open(tmp.path(), &repo, "changes");
    wait_until(&mut h, "modèle prérempli", |h| has_text_input(h, "chore: résumé du modèle"));
    // La fusion s'arrête sur un conflit (git sort en erreur).
    assert!(try_git(&repo, &["merge", "feature"]).is_none());
    wait_until(&mut h, "message de fusion", |h| has_text_input(h, "Merge branch 'feature'"));
}

#[test]
fn a_tab_opened_in_the_middle_of_a_merge_shows_the_merge_message_not_the_template() {
    let _global = isolate_global_config();
    let tmp = tempfile::tempdir().unwrap();
    let repo = conflicting_repo_with_template(tmp.path());
    assert!(try_git(&repo, &["merge", "feature"]).is_none());
    let mut h = open(tmp.path(), &repo, "changes");
    wait_until(&mut h, "message de fusion", |h| has_text_input(h, "Merge branch 'feature'"));
    assert!(!has_text_input(&h, "chore: résumé du modèle"));
}

#[test]
fn template_comment_lines_are_not_prefilled() {
    let _global = isolate_global_config();
    let tmp = tempfile::tempdir().unwrap();
    let repo = demo_repo(tmp.path());
    std::fs::write(repo.join("modele.txt"), "# Aide\nfeat: résumé\n# autre aide\n\nDétails").unwrap();
    git(&repo, &["config", "commit.template", "modele.txt"]);
    let mut h = open(tmp.path(), &repo, "changes");
    wait_until(&mut h, "sujet sans commentaire", |h| has_text_input(h, "feat: résumé"));
    assert!(h.query_by(|n| n.value().is_some_and(|v| v.contains("# "))).is_none(), "aucune ligne de commentaire");
}

#[test]
fn changing_the_template_updates_an_untouched_box_but_never_an_edited_one() {
    let _global = isolate_global_config();
    let tmp = tempfile::tempdir().unwrap();
    let repo = demo_repo(tmp.path());
    std::fs::write(repo.join("modele.txt"), "chore: premier\n").unwrap();
    std::fs::write(repo.join("autre.txt"), "chore: second\n").unwrap();
    git(&repo, &["config", "commit.template", "modele.txt"]);
    let mut h = open(tmp.path(), &repo, "changes");
    wait_until(&mut h, "premier modèle", |h| has_text_input(h, "chore: premier"));

    // Zone intacte : le nouveau modèle la remplace.
    open_dialog(&mut h);
    replace_field(&mut h, "modele.txt", "autre.txt");
    h.get_by_label("Enregistrer").click();
    wait_until(&mut h, "second modèle", |h| has_text_input(h, "chore: second"));

    // Zone éditée : elle reste telle quelle quand le modèle change encore.
    let subject =
        h.get_by(|n| n.role() == egui::accesskit::Role::TextInput && n.value().as_deref() == Some("chore: second"));
    subject.focus();
    subject.type_text(" (édité)");
    h.run_steps(3);
    open_dialog(&mut h);
    replace_field(&mut h, "autre.txt", "modele.txt");
    h.get_by_label("Enregistrer").click();
    wait_git(&mut h, "configuration écrite", || {
        try_git(&repo, &["config", "commit.template"]).is_some_and(|t| t.trim() == "modele.txt")
    });
    for _ in 0..24 {
        h.step();
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    assert!(!has_text_input(&h, "chore: premier"));
    assert!(has_text_input(&h, "chore: second (édité)"));
}
