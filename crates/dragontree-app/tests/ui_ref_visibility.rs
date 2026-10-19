//! Visibilité des références dans le graphe : tags et branches distantes (Affichage).

mod common;

use common::*;
use dragontree_app::StartOptions;
use dragontree_app::settings::Settings;
use egui_kittest::kittest::Queryable;

/// Dépôt avec un commit portant le tag `v1.0` et un commit pointé seulement par la branche distante
/// `origin/dev` (créée à la main : pas de réseau).
fn tagged_repo(base: &std::path::Path) -> std::path::PathBuf {
    let repo = base.join("fabrikam");
    init_repo(&repo);
    commit_file(&repo, "base.txt", "base\n", "Commit commun");
    git(&repo, &["switch", "-q", "-c", "temporaire"]);
    commit_file(&repo, "tag.txt", "tag\n", "Commit du tag");
    git(&repo, &["tag", "v1.0"]);
    git(&repo, &["switch", "-q", "main"]);
    git(&repo, &["switch", "-q", "-c", "temporaire-distant"]);
    commit_file(&repo, "dev.txt", "dev\n", "Commit distant");
    let tip = git(&repo, &["rev-parse", "HEAD"]);
    git(&repo, &["update-ref", "refs/remotes/origin/dev", tip.trim()]);
    git(&repo, &["switch", "-q", "main"]);
    git(&repo, &["branch", "-q", "-D", "temporaire", "temporaire-distant"]);
    commit_file(&repo, "main.txt", "main\n", "Commit principal");
    repo
}

fn settings(base: &std::path::Path) -> Settings {
    Settings::load_from(&base.join("profil").join("settings.json"))
}

#[test]
fn hiding_tags_removes_the_tag_commit_and_label_and_is_saved() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tagged_repo(tmp.path());
    let mut h = harness(StartOptions { open: vec![repo], view: Some("history".into()), ..start(tmp.path()) });
    wait_until(&mut h, "historique complet", |h| {
        h.query_by_label("Commit du tag").is_some() && h.query_by_label("Commit principal").is_some()
    });

    let entry = "Afficher les tags";
    open_menu(&mut h, "Affichage");
    wait_until(&mut h, "entrée", |h| menu_item(h, entry).is_some());
    assert_eq!(menu_item_checked(&h, entry), Some(true));
    click_menu_item(&mut h, entry);

    wait_until(&mut h, "commit du tag masqué", |h| h.query_by_label("Commit du tag").is_none());
    assert!(h.query_by_label("Commit principal").is_some());
    assert!(!settings(tmp.path()).show_tags);
    assert!(settings(tmp.path()).show_remote_branches, "l'autre réglage ne change pas");

    open_menu(&mut h, "Affichage");
    wait_until(&mut h, "entrée", |h| menu_item(h, entry).is_some());
    assert_eq!(menu_item_checked(&h, entry), Some(false));
    click_menu_item(&mut h, entry);
    wait_until(&mut h, "commit du tag réaffiché", |h| h.query_by_label("Commit du tag").is_some());
    assert!(settings(tmp.path()).show_tags);
}

#[test]
fn hiding_remote_branches_removes_their_commits() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tagged_repo(tmp.path());
    let mut h = harness(StartOptions { open: vec![repo], view: Some("history".into()), ..start(tmp.path()) });
    wait_until(&mut h, "historique complet", |h| h.query_by_label("Commit distant").is_some());

    let entry = "Afficher les branches distantes";
    open_menu(&mut h, "Affichage");
    wait_until(&mut h, "entrée", |h| menu_item(h, entry).is_some());
    assert_eq!(menu_item_checked(&h, entry), Some(true));
    click_menu_item(&mut h, entry);

    wait_until(&mut h, "commit distant masqué", |h| h.query_by_label("Commit distant").is_none());
    assert!(h.query_by_label("Commit du tag").is_some(), "les tags restent affichés");
    assert!(!settings(tmp.path()).show_remote_branches);
}

#[test]
fn hiding_tags_also_removes_the_tag_label_of_a_visible_commit() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tagged_repo(tmp.path());
    git(&repo, &["tag", "v2.0"]);
    let mut h = harness(StartOptions { open: vec![repo], view: Some("history".into()), ..start(tmp.path()) });
    wait_until(&mut h, "historique", |h| h.query_all_by_label("Commit principal").next().is_some());
    h.query_all_by_label("Commit principal").next().unwrap().click();
    // Le panneau de détails liste les références du commit sélectionné.
    wait_until(&mut h, "étiquette du tag", |h| h.query_by_label("v2.0").is_some());

    open_menu(&mut h, "Affichage");
    click_menu_item(&mut h, "Afficher les tags");
    wait_until(&mut h, "étiquette retirée", |h| h.query_by_label("v2.0").is_none());
    assert!(h.query_all_by_label("Commit principal").next().is_some());
}

// --- Masquer une branche, un dossier ou un remote (œil de la barre latérale) --------------------

/// `main` (commit principal), `autre`, `feature/a` et `feature/b` ont chacune un commit propre ; `origin/dev`
/// pointe sur un commit propre (remote déclaré sans réseau).
fn branches_repo(base: &std::path::Path) -> std::path::PathBuf {
    let repo = base.join("northwind");
    init_repo(&repo);
    commit_file(&repo, "base.txt", "base\n", "Commit commun");
    for (branch, summary) in [
        ("autre", "Commit de autre"),
        ("feature/a", "Commit de feature a"),
        ("feature/b", "Commit de feature b"),
        ("dev-distant", "Commit de dev distant"),
    ] {
        git(&repo, &["switch", "-q", "-c", branch, "main"]);
        commit_file(&repo, &format!("{}.txt", branch.replace('/', "-")), "x\n", summary);
    }
    let tip = git(&repo, &["rev-parse", "HEAD"]);
    git(&repo, &["update-ref", "refs/remotes/origin/dev", tip.trim()]);
    git(&repo, &["remote", "add", "origin", "file:///inexistant"]);
    git(&repo, &["switch", "-q", "main"]);
    git(&repo, &["branch", "-q", "-D", "dev-distant"]);
    commit_file(&repo, "main.txt", "main\n", "Commit principal");
    repo
}

fn open_branches(tmp: &std::path::Path) -> egui_kittest::Harness<'static, dragontree_app::DragonTreeApp> {
    let repo = tmp.join("northwind");
    let mut h = harness(StartOptions { open: vec![repo], view: Some("history".into()), ..start(tmp) });
    wait_until(&mut h, "historique complet", |h| {
        ["Commit de autre", "Commit de feature a", "Commit de feature b", "Commit de dev distant"]
            .iter()
            .all(|s| h.query_by_label(s).is_some())
    });
    h
}

#[test]
fn the_eye_hides_a_branch_and_the_choice_is_kept_when_the_repo_is_reopened() {
    let tmp = tempfile::tempdir().unwrap();
    branches_repo(tmp.path());
    let mut h = open_branches(tmp.path());

    h.get_by_label("Masquer du graphe : autre").click();
    wait_until(&mut h, "commit de autre masqué", |h| h.query_by_label("Commit de autre").is_none());
    assert!(h.query_by_label("Commit de feature a").is_some(), "les autres branches restent");
    // L'œil devient barré et permanent : il propose maintenant de réafficher.
    wait_until(&mut h, "œil barré", |h| h.query_by_label("Afficher dans le graphe : autre").is_some());
    wait_git(&mut h, "réglage enregistré", || {
        let saved = Settings::load_from(&tmp.path().join("profil").join("settings.json"));
        saved.hidden_refs.values().any(|v| v == &["refs/heads/autre".to_string()])
    });
    drop(h);

    // Même profil, nouvelle fenêtre : la branche reste masquée.
    let repo = tmp.path().join("northwind");
    let mut h = harness(StartOptions { open: vec![repo], view: Some("history".into()), ..start(tmp.path()) });
    wait_until(&mut h, "historique rouvert", |h| h.query_by_label("Commit de feature a").is_some());
    wait_until(&mut h, "branche encore masquée", |h| h.query_by_label("Afficher dans le graphe : autre").is_some());
    assert!(h.query_by_label("Commit de autre").is_none());

    h.get_by_label("Afficher dans le graphe : autre").click();
    wait_until(&mut h, "commit de autre réaffiché", |h| h.query_by_label("Commit de autre").is_some());
}

#[test]
fn hiding_a_folder_then_showing_all_hidden_branches() {
    let tmp = tempfile::tempdir().unwrap();
    branches_repo(tmp.path());
    let mut h = open_branches(tmp.path());

    let entry = "Réafficher toutes les branches masquées";
    open_menu(&mut h, "Affichage");
    wait_until(&mut h, "entrée", |h| menu_item(h, entry).is_some());
    assert!(menu_item_disabled(&h, entry), "rien n'est masqué : entrée grisée");
    h.key_press(egui::Key::Escape);

    h.get_by_label("Masquer du graphe : feature").click();
    wait_until(&mut h, "commits du dossier masqués", |h| {
        h.query_by_label("Commit de feature a").is_none() && h.query_by_label("Commit de feature b").is_none()
    });
    assert!(h.query_by_label("Commit de autre").is_some());

    open_menu(&mut h, "Affichage");
    wait_until(&mut h, "entrée", |h| menu_item(h, entry).is_some());
    assert!(!menu_item_disabled(&h, entry));
    click_menu_item(&mut h, entry);
    wait_until(&mut h, "commits du dossier réaffichés", |h| {
        h.query_by_label("Commit de feature a").is_some() && h.query_by_label("Commit de feature b").is_some()
    });
}

#[test]
fn hiding_a_remote_removes_its_branches_commits() {
    let tmp = tempfile::tempdir().unwrap();
    branches_repo(tmp.path());
    let mut h = open_branches(tmp.path());
    h.get_by_label("Masquer du graphe : origin").click();
    wait_until(&mut h, "commit distant masqué", |h| h.query_by_label("Commit de dev distant").is_none());
    assert!(h.query_by_label("Commit de autre").is_some());
}

#[test]
fn hiding_the_current_branch_keeps_head_in_the_graph() {
    let tmp = tempfile::tempdir().unwrap();
    branches_repo(tmp.path());
    let mut h = open_branches(tmp.path());
    h.get_by_label("Masquer du graphe : main").click();
    wait_until(&mut h, "œil barré sur main", |h| h.query_by_label("Afficher dans le graphe : main").is_some());
    // Masquer aussi `autre` : quand son commit disparaît, le journal a été relu avec `main` déjà masquée.
    h.get_by_label("Masquer du graphe : autre").click();
    wait_until(&mut h, "commit de autre masqué", |h| h.query_by_label("Commit de autre").is_none());
    // HEAD (donc le commit de main) reste parcourue malgré le masquage de sa branche.
    assert!(h.query_all_by_label("Commit principal").next().is_some());
}
