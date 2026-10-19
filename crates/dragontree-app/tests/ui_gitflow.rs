//! Git Flow : Dépôt › Git Flow (initialiser, démarrer une feature, terminer une branche), entrées grisées tant que
//! le dépôt n'est pas initialisé, et arrêt sur conflit avec l'état de fusion de git.

mod common;

use std::path::{Path, PathBuf};

use common::*;
use dragontree_app::{DragonTreeApp, StartOptions};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};

type H = Harness<'static, DragonTreeApp>;

fn repo(base: &Path) -> PathBuf {
    let dir = base.join("fabrikam");
    init_repo(&dir);
    commit_file(&dir, "a.txt", "1\n", "Premier commit");
    dir
}

fn open(base: &Path, repo: &Path) -> H {
    let mut h = harness(StartOptions { open: vec![repo.to_path_buf()], view: Some("history".into()), ..start(base) });
    wait_until(&mut h, "dépôt chargé", |h| h.query_by_label("Premier commit").is_some());
    h
}

/// Ouvre Dépôt › Git Flow et attend que l'entrée soit utilisable (la configuration est lue en tâche de fond) ; le
/// menu reste ouvert.
fn open_enabled_flow_entry(h: &mut H, entry: &str) {
    open_menu(h, "Dépôt");
    open_menu(h, "Git Flow");
    wait_until(h, &format!("entrée « {entry} » utilisable"), |h| {
        menu_item(h, entry).is_some_and(|n| !n.accesskit_node().is_disabled())
    });
}

/// Choisit une entrée de Dépôt › Git Flow une fois qu'elle est utilisable : cliquer plus tôt, avant que les
/// branches et la configuration soient lues, serait un clic sur une entrée grisée.
fn flow_menu(h: &mut H, entry: &str) {
    open_enabled_flow_entry(h, entry);
    click_menu_item(h, entry);
}

/// Écrit dans le champ qui a le focus (le premier du dialogue à l'ouverture).
fn type_in_focused(h: &mut H, text: &str) {
    wait_until(h, "champ avec le focus", |h| {
        h.query_by(|n| n.role() == egui::accesskit::Role::TextInput && n.is_focused()).is_some()
    });
    h.get_by(|n| n.role() == egui::accesskit::Role::TextInput && n.is_focused()).type_text(text);
    h.run_steps(2);
}

/// Clique le bouton dès qu'il est utilisable, en une image : le dialogue centré bouge tant que son contenu change.
fn click_button(h: &mut H, label: &str) {
    click_label(h, label);
    h.run_steps(3);
}

fn current(repo: &Path) -> Option<String> {
    try_git(repo, &["branch", "--show-current"]).map(|s| s.trim().to_string())
}

fn initialize(h: &mut H, repo: &Path) {
    flow_menu(h, "Initialiser Git Flow…");
    click_button(h, "Initialiser");
    wait_git(h, "develop créée", || try_git(repo, &["rev-parse", "--verify", "--quiet", "develop"]).is_some());
    // La configuration relue active les entrées de démarrage.
    open_enabled_flow_entry(h, "Démarrer une feature…");
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
}

#[test]
fn entries_are_greyed_until_git_flow_is_initialized() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo(tmp.path());
    let mut h = open(tmp.path(), &repo);
    // Les branches sont lues quand « Initialiser » devient utilisable : seulement ensuite l'état grisé est fiable.
    open_enabled_flow_entry(&mut h, "Initialiser Git Flow…");
    for entry in
        ["Démarrer une feature…", "Démarrer une release…", "Démarrer un hotfix…", "Terminer la branche Git Flow…"]
    {
        assert!(menu_item_disabled(&h, entry), "{entry} doit être grisée");
    }
}

#[test]
fn initializing_then_starting_a_feature_creates_and_checks_out_it() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo(tmp.path());
    let mut h = open(tmp.path(), &repo);
    initialize(&mut h, &repo);
    assert_eq!(git(&repo, &["config", "--local", "--get", "gitflow.branch.develop"]).trim(), "develop");
    assert_eq!(git(&repo, &["config", "--local", "--get", "gitflow.prefix.feature"]).trim(), "feature/");

    flow_menu(&mut h, "Démarrer une feature…");
    type_in_focused(&mut h, "x");
    click_button(&mut h, "Démarrer la feature");
    wait_git(&mut h, "feature/x sortie", || current(&repo).as_deref() == Some("feature/x"));
    assert_eq!(
        git(&repo, &["rev-parse", "feature/x"]).trim(),
        git(&repo, &["rev-parse", "develop"]).trim(),
        "la feature part de develop"
    );
    // Sur une feature, « Terminer » devient utilisable.
    open_enabled_flow_entry(&mut h, "Terminer la branche Git Flow…");
}

#[test]
fn finishing_a_feature_merges_it_and_deletes_the_branch_when_asked() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo(tmp.path());
    let mut h = open(tmp.path(), &repo);
    initialize(&mut h, &repo);
    flow_menu(&mut h, "Démarrer une feature…");
    type_in_focused(&mut h, "x");
    click_button(&mut h, "Démarrer la feature");
    wait_git(&mut h, "feature/x sortie", || current(&repo).as_deref() == Some("feature/x"));
    commit_file(&repo, "f.txt", "f\n", "Travail de la feature");
    let tip = git(&repo, &["rev-parse", "HEAD"]).trim().to_string();

    open_enabled_flow_entry(&mut h, "Terminer la branche Git Flow…");
    click_menu_item(&mut h, "Terminer la branche Git Flow…");
    // La suppression n'est faite que si on la demande.
    click_button(&mut h, "Supprimer la branche « feature/x » ensuite");
    click_button(&mut h, "Terminer la branche");
    wait_git(&mut h, "branche supprimée", || {
        try_git(&repo, &["rev-parse", "--verify", "--quiet", "feature/x"]).is_none()
    });
    assert_eq!(current(&repo).as_deref(), Some("develop"));
    assert!(try_git(&repo, &["merge-base", "--is-ancestor", &tip, "develop"]).is_some());
    assert_eq!(git(&repo, &["rev-list", "--parents", "-n", "1", "develop"]).split_whitespace().count(), 3);
}

#[test]
fn a_conflict_while_finishing_a_release_leaves_git_in_its_merge_state_and_says_what_remains() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo(tmp.path());
    // Initialisation faite hors de l'interface : le test porte sur la fin de la release.
    git(&repo, &["branch", "develop"]);
    for (key, value) in [("gitflow.branch.master", "main"), ("gitflow.branch.develop", "develop")] {
        git(&repo, &["config", "--local", key, value]);
    }
    git(&repo, &["checkout", "-q", "-b", "release/1.0", "develop"]);
    commit_file(&repo, "a.txt", "release\n", "Release");
    git(&repo, &["checkout", "-q", "main"]);
    commit_file(&repo, "a.txt", "production\n", "Correctif direct");
    git(&repo, &["checkout", "-q", "release/1.0"]);

    let mut h = open(tmp.path(), &repo);
    open_enabled_flow_entry(&mut h, "Terminer la branche Git Flow…");
    click_menu_item(&mut h, "Terminer la branche Git Flow…");
    click_button(&mut h, "Terminer la branche");
    wait_git(&mut h, "fusion en conflit", || repo.join(".git").join("MERGE_HEAD").exists());
    // Le message d'erreur nomme ce qui reste : tag, fusion dans develop.
    wait_until(&mut h, "reste à faire signalé", |h| {
        h.query_by_label_contains("créer le tag annoté « 1.0 »").is_some()
    });
    assert!(git(&repo, &["tag", "-l"]).trim().is_empty(), "rien n'est tagué avant la résolution");
    assert_eq!(current(&repo).as_deref(), Some("main"));
}
