//! Filtres de l'historique : branche courante seulement (Affichage, Ctrl+Maj+A) et filtre sur des branches choisies
//! (entonnoir de la barre latérale, bandeau « Filtré par »).

mod common;

use common::*;
use dragontree_app::StartOptions;
use dragontree_app::settings::Settings;
use egui::{Key, Modifiers};
use egui_kittest::kittest::Queryable;

#[test]
fn current_branch_shortcut_hides_and_restores_the_other_branch_commits() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("divergent");
    init_repo(&repo);
    commit_file(&repo, "base.txt", "base\n", "Commit commun");
    git(&repo, &["switch", "-q", "-c", "autre"]);
    commit_file(&repo, "autre.txt", "autre\n", "Commit de l'autre branche");
    git(&repo, &["switch", "-q", "main"]);
    commit_file(&repo, "main.txt", "main\n", "Commit de la branche courante");

    let mut h = harness(StartOptions { open: vec![repo], view: Some("history".into()), ..start(tmp.path()) });
    wait_until(&mut h, "historique complet", |h| {
        h.query_by_label("Commit de l'autre branche").is_some()
            && h.query_by_label("Commit de la branche courante").is_some()
    });

    let entry = "Branche courante seulement Ctrl+Maj+A";
    open_menu(&mut h, "Affichage");
    wait_until(&mut h, "entrée du filtre", |h| menu_item(h, entry).is_some());
    assert_eq!(menu_item_checked(&h, entry), Some(false));
    h.key_press(Key::Escape);

    h.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::A);
    wait_until(&mut h, "commit de l'autre branche masqué", |h| {
        h.query_by_label("Commit de l'autre branche").is_none()
    });
    assert!(h.query_by_label("Commit de la branche courante").is_some());
    open_menu(&mut h, "Affichage");
    wait_until(&mut h, "entrée du filtre", |h| menu_item(h, entry).is_some());
    assert_eq!(menu_item_checked(&h, entry), Some(true));
    h.key_press(Key::Escape);

    h.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::A);
    wait_until(&mut h, "commit de l'autre branche réaffiché", |h| {
        h.query_by_label("Commit de l'autre branche").is_some()
    });
    open_menu(&mut h, "Affichage");
    wait_until(&mut h, "entrée du filtre", |h| menu_item(h, entry).is_some());
    assert_eq!(menu_item_checked(&h, entry), Some(false));
}

// --- Filtre sur des branches choisies (entonnoir et bandeau) -----------------------------------

/// `main`, `a`, `b`, `c`, `feature/x` et `feature/y` ont chacune un commit propre.
fn filter_repo(base: &std::path::Path) -> std::path::PathBuf {
    let repo = base.join("contoso");
    init_repo(&repo);
    commit_file(&repo, "base.txt", "base\n", "Commit commun");
    for branch in ["a", "b", "c", "feature/x", "feature/y"] {
        git(&repo, &["switch", "-q", "-c", branch, "main"]);
        commit_file(&repo, &format!("{}.txt", branch.replace('/', "-")), "x\n", &format!("Commit de {branch}"));
    }
    git(&repo, &["switch", "-q", "main"]);
    commit_file(&repo, "main.txt", "main\n", "Commit principal");
    repo
}

const OWN_COMMITS: [&str; 5] =
    ["Commit de a", "Commit de b", "Commit de c", "Commit de feature/x", "Commit de feature/y"];

fn open_filter_repo(tmp: &std::path::Path) -> egui_kittest::Harness<'static, dragontree_app::DragonTreeApp> {
    let mut h = harness(StartOptions { open: vec![tmp.join("contoso")], view: Some("history".into()), ..start(tmp) });
    wait_until(&mut h, "historique complet", |h| OWN_COMMITS.iter().all(|s| h.query_by_label(s).is_some()));
    h
}

fn shown(h: &egui_kittest::Harness<'static, dragontree_app::DragonTreeApp>) -> Vec<&'static str> {
    OWN_COMMITS.into_iter().filter(|s| h.query_by_label(s).is_some()).collect()
}

#[test]
fn filtering_on_two_branches_hides_the_third_and_the_banner_removes_them() {
    let tmp = tempfile::tempdir().unwrap();
    filter_repo(tmp.path());
    let mut h = open_filter_repo(tmp.path());
    assert!(h.query_by_label("Filtré par :").is_none(), "pas de bandeau sans filtre");

    click_label(&mut h, "Filtrer l'historique sur : a");
    wait_until(&mut h, "filtre sur a", |h| shown(h) == ["Commit de a"]);
    assert!(h.query_by_label("Retirer du filtre : a").is_some(), "le bandeau apparaît");
    click_label(&mut h, "Filtrer l'historique sur : b");
    wait_until(&mut h, "filtre sur a et b", |h| shown(h) == ["Commit de a", "Commit de b"]);
    assert!(h.query_by_label("Commit de c").is_none(), "le commit propre à la troisième branche disparaît");
    assert!(h.query_by_label("Commit principal").is_none(), "HEAD n'est plus parcourue");
    assert!(h.query_by_label("Commit commun").is_some(), "le tronc commun reste");
    assert!(h.query_by_label("Retirer du filtre : a").is_some());
    assert!(h.query_by_label("Retirer du filtre : b").is_some());

    // ✕ sur un élément du bandeau : il sort du filtre.
    click_label(&mut h, "Retirer du filtre : b");
    wait_until(&mut h, "b retirée du filtre", |h| shown(h) == ["Commit de a"]);
    assert!(h.query_by_label("Retirer du filtre : b").is_none());

    // « Tout afficher » rétablit tout et fait disparaître le bandeau.
    click_label(&mut h, "Filtrer l'historique sur : c");
    wait_until(&mut h, "filtre sur a et c", |h| shown(h) == ["Commit de a", "Commit de c"]);
    click_label(&mut h, "Tout afficher");
    wait_until(&mut h, "tout réaffiché", |h| shown(h) == OWN_COMMITS);
    assert!(h.query_by_label("Retirer du filtre : a").is_none());
    assert!(h.query_by_label("Commit principal").is_some());
}

#[test]
fn filtering_on_a_folder_keeps_the_commits_of_its_branches() {
    let tmp = tempfile::tempdir().unwrap();
    filter_repo(tmp.path());
    let mut h = open_filter_repo(tmp.path());
    click_label(&mut h, "Filtrer l'historique sur : feature");
    wait_until(&mut h, "filtre sur le dossier", |h| shown(h) == ["Commit de feature/x", "Commit de feature/y"]);
    assert!(h.query_by_label("Retirer du filtre : feature/*").is_some(), "le bandeau nomme le motif du dossier");
}

#[test]
fn current_branch_only_and_the_branch_filter_exclude_each_other() {
    let tmp = tempfile::tempdir().unwrap();
    filter_repo(tmp.path());
    let mut h = open_filter_repo(tmp.path());
    let entry = "Branche courante seulement Ctrl+Maj+A";

    click_label(&mut h, "Filtrer l'historique sur : a");
    wait_until(&mut h, "filtre sur a", |h| shown(h) == ["Commit de a"]);

    // Activer « branche courante seulement » retire le filtre et son bandeau.
    h.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::A);
    wait_until(&mut h, "branche courante seulement", |h| {
        shown(h).is_empty() && h.query_by_label("Retirer du filtre : a").is_none()
    });
    assert!(h.query_all_by_label("Commit principal").next().is_some());
    open_menu(&mut h, "Affichage");
    wait_until(&mut h, "entrée du filtre", |h| menu_item(h, entry).is_some());
    assert_eq!(menu_item_checked(&h, entry), Some(true));
    h.key_press(Key::Escape);

    // Filtrer sur une branche désactive « branche courante seulement ».
    click_label(&mut h, "Filtrer l'historique sur : b");
    wait_until(&mut h, "filtre sur b", |h| shown(h) == ["Commit de b"]);
    open_menu(&mut h, "Affichage");
    wait_until(&mut h, "entrée du filtre", |h| menu_item(h, entry).is_some());
    assert_eq!(menu_item_checked(&h, entry), Some(false));
}

/// T4 : « Branche courante seulement » est mémorisé par dépôt et retrouvé à la réouverture.
#[test]
fn current_branch_only_is_remembered_per_repository_across_sessions() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("divergent");
    init_repo(&repo);
    commit_file(&repo, "base.txt", "base\n", "Commit commun");
    git(&repo, &["switch", "-q", "-c", "autre"]);
    commit_file(&repo, "autre.txt", "autre\n", "Commit de l'autre branche");
    git(&repo, &["switch", "-q", "main"]);
    commit_file(&repo, "main.txt", "main\n", "Commit de la branche courante");
    let other_repo = tmp.path().join("autre-depot");
    init_repo(&other_repo);
    commit_file(&other_repo, "x.txt", "x\n", "Commit de l'autre dépôt");

    let open = |repo: &std::path::Path| {
        harness(StartOptions { open: vec![repo.to_path_buf()], view: Some("history".into()), ..start(tmp.path()) })
    };
    let entry = "Branche courante seulement Ctrl+Maj+A";
    {
        let mut h = open(&repo);
        wait_until(&mut h, "historique complet", |h| h.query_by_label("Commit de l'autre branche").is_some());
        h.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::A);
        wait_until(&mut h, "filtre actif", |h| h.query_by_label("Commit de l'autre branche").is_none());
        wait_until(&mut h, "réglage enregistré", |_| {
            Settings::load_from(&tmp.path().join("profil").join("settings.json")).current_branch_only_repos.len() == 1
        });
    }

    let mut h = open(&repo);
    wait_until(&mut h, "historique filtré", |h| h.query_by_label("Commit de la branche courante").is_some());
    h.run_steps(10);
    assert!(h.query_by_label("Commit de l'autre branche").is_none(), "filtre restauré à la réouverture");
    open_menu(&mut h, "Affichage");
    wait_until(&mut h, "entrée du filtre", |h| menu_item(h, entry).is_some());
    assert_eq!(menu_item_checked(&h, entry), Some(true));
    h.key_press(Key::Escape);
    drop(h);

    // Un autre dépôt n'est pas touché.
    let mut h = open(&other_repo);
    wait_until(&mut h, "autre dépôt", |h| h.query_by_label("Commit de l'autre dépôt").is_some());
    open_menu(&mut h, "Affichage");
    wait_until(&mut h, "entrée du filtre", |h| menu_item(h, entry).is_some());
    assert_eq!(menu_item_checked(&h, entry), Some(false));
    h.key_press(Key::Escape);
    drop(h);

    // Le décocher l'oublie.
    let mut h = open(&repo);
    wait_until(&mut h, "historique filtré", |h| h.query_by_label("Commit de la branche courante").is_some());
    h.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::A);
    wait_until(&mut h, "filtre retiré", |h| h.query_by_label("Commit de l'autre branche").is_some());
    wait_until(&mut h, "réglage oublié", |_| {
        Settings::load_from(&tmp.path().join("profil").join("settings.json")).current_branch_only_repos.is_empty()
    });
}

/// Un filtre sur des branches désactive « branche courante seulement » pour la session ; le retirer (dernière
/// pastille ou « Tout afficher ») revient au choix mémorisé.
#[test]
fn clearing_the_branch_filter_returns_to_the_remembered_current_branch_only_choice() {
    let tmp = tempfile::tempdir().unwrap();
    filter_repo(tmp.path());
    let mut h = open_filter_repo(tmp.path());
    let entry = "Branche courante seulement Ctrl+Maj+A";

    h.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::A);
    wait_until(&mut h, "branche courante seulement", |h| shown(h).is_empty());

    for clear in ["Retirer du filtre : a", "Tout afficher"] {
        click_label(&mut h, "Filtrer l'historique sur : a");
        wait_until(&mut h, "filtre sur a", |h| shown(h) == ["Commit de a"]);
        open_menu(&mut h, "Affichage");
        wait_until(&mut h, "entrée du filtre", |h| menu_item(h, entry).is_some());
        assert_eq!(menu_item_checked(&h, entry), Some(false), "le filtre prend le pas pour la session");
        h.key_press(Key::Escape);

        click_label(&mut h, clear);
        wait_until(&mut h, "retour à la branche courante seulement", |h| shown(h).is_empty());
        assert!(h.query_by_label("Commit principal").is_some());
        open_menu(&mut h, "Affichage");
        wait_until(&mut h, "entrée du filtre", |h| menu_item(h, entry).is_some());
        assert_eq!(menu_item_checked(&h, entry), Some(true), "après {clear}");
        h.key_press(Key::Escape);
    }
}
