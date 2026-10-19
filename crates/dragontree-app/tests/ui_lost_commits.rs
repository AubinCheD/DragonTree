//! Commits perdus (reflog) dans le graphe : Affichage > Commits perdus (Ctrl+Maj+.), propre à l'onglet.

mod common;

use common::*;
use dragontree_app::StartOptions;
use egui::{Key, Modifiers};
use egui_kittest::kittest::Queryable;

const ENTRY: &str = "Commits perdus Ctrl+Maj+.";
const CTRL_SHIFT: Modifiers = Modifiers { command: true, ctrl: true, shift: true, ..Modifiers::NONE };

/// `main` porte « Commit commun » ; « Commit abandonné » n'est plus atteint que par le reflog (reset --hard).
fn reset_repo(base: &std::path::Path) -> std::path::PathBuf {
    let repo = base.join("contoso");
    init_repo(&repo);
    commit_file(&repo, "base.txt", "base\n", "Commit commun");
    commit_file(&repo, "perdu.txt", "perdu\n", "Commit abandonné");
    git(&repo, &["reset", "-q", "--hard", "HEAD~1"]);
    repo
}

/// Ligne du graphe d'un commit : le panneau de détails d'un commit sélectionné répète son message, mais vient avant.
fn row<'h>(
    h: &'h egui_kittest::Harness<'static, dragontree_app::DragonTreeApp>,
    summary: &'h str,
) -> egui_kittest::Node<'h> {
    h.query_all_by_label(summary).last().unwrap_or_else(|| panic!("ligne « {summary} » absente"))
}

fn open(base: &std::path::Path) -> egui_kittest::Harness<'static, dragontree_app::DragonTreeApp> {
    let repo = reset_repo(base);
    let mut h = harness(StartOptions { open: vec![repo], view: Some("history".into()), ..start(base) });
    wait_until(&mut h, "historique", |h| h.query_by_label("Commit commun").is_some());
    h
}

#[test]
fn the_menu_entry_shows_the_abandoned_commit_labelled_lost_and_hides_it_again() {
    let tmp = tempfile::tempdir().unwrap();
    let mut h = open(tmp.path());
    assert!(h.query_by_label("Commit abandonné").is_none(), "perdu : absent du graphe par défaut");

    open_menu(&mut h, "Affichage");
    wait_until(&mut h, "entrée", |h| menu_item(h, ENTRY).is_some());
    assert_eq!(menu_item_checked(&h, ENTRY), Some(false));
    click_menu_item(&mut h, ENTRY);

    wait_until(&mut h, "commit abandonné affiché", |h| h.query_by_label("Commit abandonné").is_some());
    assert!(h.query_by_label("Commit commun").is_some());
    // Sélectionné, le commit est marqué « perdu » dans ses références ; un commit ordinaire ne l'est pas.
    row(&h, "Commit commun").click();
    h.run_steps(5);
    assert!(h.query_by_label("perdu").is_none());
    row(&h, "Commit abandonné").click();
    wait_until(&mut h, "étiquette « perdu »", |h| h.query_by_label("perdu").is_some());

    // La sélection d'un autre commit libère le panneau de détails, qui répétait le message du commit perdu.
    row(&h, "Commit commun").click();
    wait_until(&mut h, "détails du commit commun", |h| h.query_all_by_label("Commit commun").count() == 2);
    open_menu(&mut h, "Affichage");
    wait_until(&mut h, "entrée", |h| menu_item(h, ENTRY).is_some());
    assert_eq!(menu_item_checked(&h, ENTRY), Some(true));
    click_menu_item(&mut h, ENTRY);
    wait_until(&mut h, "commit abandonné masqué", |h| h.query_by_label("Commit abandonné").is_none());
}

#[test]
fn ctrl_shift_period_toggles_the_lost_commits() {
    let tmp = tempfile::tempdir().unwrap();
    let mut h = open(tmp.path());
    h.key_press_modifiers(CTRL_SHIFT, Key::Period);
    wait_until(&mut h, "commit abandonné affiché", |h| h.query_by_label("Commit abandonné").is_some());
    h.key_press_modifiers(CTRL_SHIFT, Key::Period);
    wait_until(&mut h, "commit abandonné masqué", |h| h.query_by_label("Commit abandonné").is_none());
}

#[test]
fn a_new_branch_recovers_the_lost_commit_from_its_context_menu() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("contoso");
    let mut h = open(tmp.path());
    h.key_press_modifiers(CTRL_SHIFT, Key::Period);
    wait_until(&mut h, "commit abandonné affiché", |h| h.query_by_label("Commit abandonné").is_some());
    row(&h, "Commit abandonné").click_secondary();
    wait_until(&mut h, "menu du commit", |h| h.query_by_label_contains("Nouvelle branche ici").is_some());
    h.get_by_label_contains("Nouvelle branche ici").click();
    wait_until(&mut h, "dialogue de branche", |h| h.query_by_label("Créer").is_some());
    let name = h.get_by(|n| n.placeholder() == Some("nom-de-la-branche (ex. feature/ma-fonction)"));
    name.focus();
    name.type_text("secours");
    h.run_steps(3);
    h.get_by_label("Créer").click();
    wait_git(&mut h, "branche créée sur le commit abandonné", || {
        try_git(&repo, &["log", "-1", "--format=%s", "secours"]).is_some_and(|s| s.trim() == "Commit abandonné")
    });
    // Rattrapé par une branche, le commit n'est plus perdu : plus d'étiquette, mais il reste affiché.
    h.run_steps(5);
    wait_until(&mut h, "plus d'étiquette « perdu »", |h| h.query_by_label("perdu").is_none());
    assert!(h.query_all_by_label("Commit abandonné").next().is_some());
}

#[test]
fn a_failure_reading_the_lost_commits_is_reported_and_the_graph_stays_usable() {
    let tmp = tempfile::tempdir().unwrap();
    let mut h = open(tmp.path());
    // Une référence vers un objet absent fait échouer `rev-list --all`, pas le journal des branches.
    let refs = tmp.path().join("contoso").join(".git").join("refs").join("notes");
    std::fs::create_dir_all(&refs).unwrap();
    std::fs::write(refs.join("cassee"), "1111111111111111111111111111111111111111\n").unwrap();
    h.key_press_modifiers(CTRL_SHIFT, Key::Period);
    wait_until(&mut h, "message d'erreur", |h| h.query_by_label("Lecture des commits perdus impossible").is_some());
    assert!(h.query_by_label("Commit commun").is_some());
}
