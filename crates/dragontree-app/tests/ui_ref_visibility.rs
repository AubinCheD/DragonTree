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
