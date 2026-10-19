//! Stashs dans le graphe : Affichage > Afficher les stashs, ligne de stash, détails et menu contextuel.

mod common;

use common::*;
use dragontree_app::StartOptions;
use dragontree_app::settings::Settings;
use egui_kittest::kittest::Queryable;

const ENTRY: &str = "Afficher les stashs";
const SUMMARY: &str = "On main: mon stash";

/// Un commit puis un stash (fichier suivi modifié + fichier non suivi, donc avec ses commits « index » et
/// « untracked files »).
fn stash_repo(base: &std::path::Path) -> std::path::PathBuf {
    let repo = base.join("fabrikam");
    init_repo(&repo);
    commit_file(&repo, "f.txt", "base\n", "Commit de base");
    std::fs::write(repo.join("f.txt"), "modifié\n").unwrap();
    std::fs::write(repo.join("neuf.txt"), "non suivi\n").unwrap();
    git(&repo, &["stash", "push", "-q", "-u", "-m", "mon stash"]);
    repo
}

fn open(
    repo: std::path::PathBuf,
    base: &std::path::Path,
) -> egui_kittest::Harness<'static, dragontree_app::DragonTreeApp> {
    harness(StartOptions { open: vec![repo], view: Some("history".into()), ..start(base) })
}

/// Lignes du graphe de libellé exactement `summary` (la barre latérale nomme le stash « stash@{0}  … »).
fn rows(h: &egui_kittest::Harness<'static, dragontree_app::DragonTreeApp>, summary: &str) -> usize {
    h.query_all_by_label(summary).count()
}

#[test]
fn a_stash_is_one_graph_row_without_its_technical_commits_and_the_option_hides_it() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = stash_repo(tmp.path());
    let mut h = open(repo, tmp.path());
    wait_until(&mut h, "ligne de stash", |h| rows(h, SUMMARY) == 1);
    wait_until(&mut h, "commit de base", |h| rows(h, "Commit de base") == 1);
    h.run_steps(10);
    assert_eq!(rows(&h, SUMMARY), 1, "une seule fois");
    assert!(h.query_by_label_contains("index on main").is_none());
    assert!(h.query_by_label_contains("untracked files on main").is_none());

    open_menu(&mut h, "Affichage");
    wait_until(&mut h, "entrée", |h| menu_item(h, ENTRY).is_some());
    assert_eq!(menu_item_checked(&h, ENTRY), Some(true), "coché par défaut");
    click_menu_item(&mut h, ENTRY);
    wait_until(&mut h, "ligne de stash retirée", |h| rows(h, SUMMARY) == 0);
    assert!(h.query_by_label("Commit de base").is_some());
    assert!(!Settings::load_from(&tmp.path().join("profil").join("settings.json")).show_stashes);

    open_menu(&mut h, "Affichage");
    wait_until(&mut h, "entrée", |h| menu_item(h, ENTRY).is_some());
    assert_eq!(menu_item_checked(&h, ENTRY), Some(false));
    click_menu_item(&mut h, ENTRY);
    wait_until(&mut h, "ligne de stash réaffichée", |h| rows(h, SUMMARY) == 1);
}

#[test]
fn clicking_a_stash_row_shows_its_changes_against_its_base() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = stash_repo(tmp.path());
    let mut h = open(repo, tmp.path());
    wait_until(&mut h, "ligne de stash", |h| rows(h, SUMMARY) == 1);
    h.get_by_label(SUMMARY).click();
    // Le fichier suivi modifié est listé ; le fichier non suivi (3e parent) n'est pas une modification du stash.
    wait_until(&mut h, "fichier du stash", |h| h.query_by_label("f.txt").is_some());
    assert!(h.query_by_label("neuf.txt").is_none());
}

#[test]
fn the_context_menu_of_a_stash_row_pops_the_stash() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = stash_repo(tmp.path());
    let mut h = open(repo.clone(), tmp.path());
    wait_until(&mut h, "ligne de stash", |h| rows(h, SUMMARY) == 1);
    h.get_by_label(SUMMARY).click_secondary();
    wait_until(&mut h, "menu du stash", |h| h.query_by_label_contains("Appliquer et supprimer (pop)").is_some());
    // Menu de stash, pas de commit ordinaire.
    assert!(h.query_by_label_contains("Cherry-pick").is_none());
    h.get_by_label_contains("Appliquer et supprimer (pop)").click();
    wait_git(&mut h, "stash retiré de la liste", || {
        try_git(&repo, &["stash", "list"]).is_some_and(|l| l.trim().is_empty())
    });
    assert_eq!(std::fs::read_to_string(repo.join("f.txt")).unwrap().trim(), "modifié");
    wait_until(&mut h, "ligne de stash disparue", |h| rows(h, SUMMARY) == 0);
}

#[test]
fn a_stash_created_outside_appears_in_the_graph() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("northwind");
    init_repo(&repo);
    commit_file(&repo, "f.txt", "base\n", "Commit de base");
    let mut h = open(repo.clone(), tmp.path());
    wait_until(&mut h, "historique", |h| rows(h, "Commit de base") == 1);
    std::fs::write(repo.join("f.txt"), "autre\n").unwrap();
    git(&repo, &["stash", "push", "-q", "-m", "mon stash"]);
    wait_until(&mut h, "ligne de stash", |h| rows(h, SUMMARY) == 1);
}

/// `main` (« Commit principal ») et la branche `autre` (« Commit de autre ») ; un stash fait sur `autre`.
fn stash_on_branch_repo(base: &std::path::Path) -> std::path::PathBuf {
    let repo = base.join("contoso");
    init_repo(&repo);
    commit_file(&repo, "f.txt", "base\n", "Commit commun");
    git(&repo, &["switch", "-q", "-c", "autre"]);
    commit_file(&repo, "autre.txt", "x\n", "Commit de autre");
    std::fs::write(repo.join("f.txt"), "modifié\n").unwrap();
    git(&repo, &["stash", "push", "-q", "-m", "mon stash"]);
    git(&repo, &["switch", "-q", "main"]);
    commit_file(&repo, "main.txt", "m\n", "Commit principal");
    repo
}

#[test]
fn a_stash_on_a_hidden_branch_does_not_bring_that_branch_back() {
    const SUMMARY: &str = "On autre: mon stash";
    let tmp = tempfile::tempdir().unwrap();
    let repo = stash_on_branch_repo(tmp.path());
    let mut h = open(repo, tmp.path());
    wait_until(&mut h, "stash et sa base", |h| rows(h, SUMMARY) == 1 && rows(h, "Commit de autre") == 1);

    click_label(&mut h, "Masquer du graphe : autre");
    wait_until(&mut h, "branche masquée", |h| rows(h, "Commit de autre") == 0);
    // Le stash reste dans la barre latérale, mais pas dans le graphe : sa base n'y est plus.
    h.run_steps(10);
    assert_eq!(rows(&h, SUMMARY), 0);
    assert_eq!(rows(&h, "Commit de autre"), 0, "l'historique masqué ne revient pas par le stash");
    assert!(h.query_by_label_contains("stash@{0}").is_some(), "toujours listé dans la barre latérale");
    assert_eq!(rows(&h, "Commit principal"), 1);

    click_label(&mut h, "Afficher dans le graphe : autre");
    wait_until(&mut h, "stash réaffiché avec sa base", |h| rows(h, SUMMARY) == 1 && rows(h, "Commit de autre") == 1);
}
