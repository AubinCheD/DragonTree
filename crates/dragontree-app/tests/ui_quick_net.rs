//! Pull et Push rapides : Ctrl+clic sur les boutons de la barre d'outils (ou Dépôt › Pull rapide / Push rapide)
//! lance l'opération sans dialogue, avec un remote local (bare).

mod common;

use std::path::{Path, PathBuf};

use common::*;
use dragontree_app::{DragonTreeApp, StartOptions};
use egui::Modifiers;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};

/// Textes des dialogues de push et de pull : ils ne doivent jamais s'afficher.
const PUSH_DIALOG: &str = "Pousser aussi tous les tags";
const PULL_DIALOG: &str = "Rebaser au lieu de fusionner (--rebase)";

struct Fixture {
    bare: PathBuf,
    work: PathBuf,
    other: PathBuf,
}

/// Remote local, dépôt de travail cloné (main suivie, déjà poussée) et un « collègue » cloné lui aussi.
fn fixture(base: &Path) -> Fixture {
    let bare = base.join("remote.git");
    std::fs::create_dir_all(&bare).unwrap();
    git(&bare, &["init", "-q", "--bare", "-b", "main"]);
    let url = format!("file:///{}", bare.display().to_string().replace('\\', "/"));
    let (work, other) = (base.join("travail"), base.join("collegue"));
    git(base, &["clone", "-q", &url, work.to_str().unwrap()]);
    configure(&work, "Testeur");
    commit_file(&work, "a.txt", "a\n", "premier");
    git(&work, &["push", "-q", "-u", "origin", "main"]);
    git(base, &["clone", "-q", &url, other.to_str().unwrap()]);
    configure(&other, "Collègue");
    Fixture { bare, work, other }
}

fn configure(dir: &Path, name: &str) {
    for (k, v) in [("user.name", name), ("user.email", "t@example.com"), ("commit.gpgsign", "false")] {
        git(dir, &["config", k, v]);
    }
}

/// Le collègue pousse un commit qui ajoute `file`.
fn colleague_pushes(f: &Fixture, file: &str) {
    git(&f.other, &["pull", "-q", "--no-rebase", "origin", "main"]);
    commit_file(&f.other, file, "c\n", &format!("ajout de {file}"));
    git(&f.other, &["push", "-q", "origin", "main"]);
}

fn enabled(h: &Harness<'static, DragonTreeApp>, label: &str) -> bool {
    h.query_by_label(label).is_some_and(|n| !n.accesskit_node().is_disabled())
}

/// Ouvre le dépôt de travail et attend que `shown` soit affiché et que Pull et Push soient actifs.
fn open(base: &Path, f: &Fixture, shown: &str) -> Harness<'static, DragonTreeApp> {
    let mut h = harness(StartOptions { open: vec![f.work.clone()], view: Some("history".into()), ..start(base) });
    wait_until(&mut h, "dépôt chargé", |h| h.query_by_label(shown).is_some());
    wait_until(&mut h, "Pull et Push actifs", |h| enabled(h, "Pull") && enabled(h, "Push"));
    h
}

/// Fait tourner l'interface jusqu'à `cond`, en vérifiant à chaque image qu'aucun dialogue de push ou de pull
/// ne s'est ouvert.
fn wait_without_dialog(h: &mut Harness<'static, DragonTreeApp>, what: &str, mut cond: impl FnMut() -> bool) {
    wait_until(h, what, |h| {
        assert!(h.query_by_label(PUSH_DIALOG).is_none(), "dialogue de push ouvert");
        assert!(h.query_by_label(PULL_DIALOG).is_none(), "dialogue de pull ouvert");
        cond()
    });
}

#[test]
fn ctrl_click_on_push_pushes_without_a_dialog() {
    let tmp = tempfile::tempdir().unwrap();
    let f = fixture(tmp.path());
    commit_file(&f.work, "b.txt", "b\n", "local à pousser");
    let mut h = open(tmp.path(), &f, "local à pousser");

    h.get_by_label("Push").hover();
    wait_until(&mut h, "infobulle de Push", |h| h.query_by_label_contains("Ctrl+clic").is_some());

    h.get_by_label("Push").click_modifiers(Modifiers::COMMAND);
    wait_without_dialog(&mut h, "commit poussé", || {
        try_git(&f.bare, &["log", "-1", "--format=%s", "main"]).is_some_and(|s| s.trim() == "local à pousser")
    });
}

#[test]
fn ctrl_click_on_pull_integrates_a_commit_pushed_by_another_clone() {
    let tmp = tempfile::tempdir().unwrap();
    let f = fixture(tmp.path());
    let mut h = open(tmp.path(), &f, "premier");
    colleague_pushes(&f, "c.txt");

    h.get_by_label("Pull").hover();
    wait_until(&mut h, "infobulle de Pull", |h| h.query_by_label_contains("Ctrl+clic").is_some());

    h.get_by_label("Pull").click_modifiers(Modifiers::COMMAND);
    wait_without_dialog(&mut h, "pull terminé", || f.work.join("c.txt").exists());
    assert_eq!(git(&f.work, &["log", "-1", "--format=%s"]).trim(), "ajout de c.txt");
}

#[test]
fn quick_push_from_the_menu_creates_the_upstream_on_the_default_remote() {
    let tmp = tempfile::tempdir().unwrap();
    let f = fixture(tmp.path());
    git(&f.work, &["checkout", "-q", "-b", "fonction"]);
    commit_file(&f.work, "f.txt", "f\n", "sur la branche fonction");
    let mut h = open(tmp.path(), &f, "sur la branche fonction");

    open_menu(&mut h, "Dépôt");
    click_menu_item(&mut h, "Push rapide");
    wait_without_dialog(&mut h, "branche créée sur le remote, suivie", || {
        try_git(&f.bare, &["log", "-1", "--format=%s", "fonction"])
            .is_some_and(|s| s.trim() == "sur la branche fonction")
            && try_git(&f.work, &["config", "branch.fonction.remote"]).is_some_and(|s| s.trim() == "origin")
    });
    assert_eq!(git(&f.work, &["config", "branch.fonction.merge"]).trim(), "refs/heads/fonction");
}

#[test]
fn quick_pull_reuses_the_mode_last_chosen_in_the_pull_dialog() {
    let tmp = tempfile::tempdir().unwrap();
    let f = fixture(tmp.path());
    // Branches divergentes : un commit local, un commit du collègue.
    commit_file(&f.work, "l1.txt", "l1\n", "local 1");
    colleague_pushes(&f, "c1.txt");
    let mut h = open(tmp.path(), &f, "local 1");

    // Pull par le dialogue, en cochant « rebaser » : main passe de 2 commits à 3 (rebase) ou 4 (fusion).
    h.get_by_label("Pull").click();
    wait_until(&mut h, "dialogue de pull", |h| h.query_by_label(PULL_DIALOG).is_some());
    h.get_by_label(PULL_DIALOG).click();
    h.run_steps(3);
    h.query_all_by_label("Pull").last().unwrap().click();
    wait_git(&mut h, "premier pull terminé", || commits_on_main(&f.work) >= 3);
    assert_eq!(git(&f.work, &["rev-list", "--merges", "main"]).trim(), "", "rebase : aucun commit de fusion");

    // Nouvelle divergence : le pull rapide rebase lui aussi. main passe de 4 commits à 5 (rebase) ou 6 (fusion).
    colleague_pushes(&f, "c2.txt");
    commit_file(&f.work, "l2.txt", "l2\n", "local 2");
    wait_until(&mut h, "commit local affiché, Pull actif", |h| {
        h.query_by_label("local 2").is_some() && enabled(h, "Pull")
    });
    h.get_by_label("Pull").click_modifiers(Modifiers::COMMAND);
    wait_without_dialog(&mut h, "pull rapide terminé", || commits_on_main(&f.work) >= 5);
    assert_eq!(git(&f.work, &["rev-list", "--merges", "main"]).trim(), "", "rebase : aucun commit de fusion");
    assert!(f.work.join("c2.txt").exists());
}

/// Nombre de commits de main (0 si git échoue pendant que l'application modifie le dépôt).
fn commits_on_main(repo: &Path) -> usize {
    try_git(repo, &["rev-list", "--count", "main"]).and_then(|s| s.trim().parse().ok()).unwrap_or(0)
}

#[test]
fn quick_push_asks_first_when_the_upstream_has_another_name() {
    // `feature` suit `origin/main` (« Nouvelle branche à partir d'ici… » sur origin/main) : un push rapide
    // avancerait main sur le remote. Comme `push.default=simple`, il ouvre le dialogue au lieu de pousser.
    let tmp = tempfile::tempdir().unwrap();
    let f = fixture(tmp.path());
    git(&f.work, &["checkout", "-q", "-b", "feature", "--track", "origin/main"]);
    commit_file(&f.work, "f.txt", "f\n", "sur feature");
    let before = git(&f.bare, &["rev-parse", "main"]).trim().to_string();
    let moved = || try_git(&f.bare, &["rev-parse", "main"]).is_some_and(|s| s.trim() != before);
    let mut h = open(tmp.path(), &f, "sur feature");

    h.get_by_label("Push").click_modifiers(Modifiers::COMMAND);
    wait_until(&mut h, "dialogue de push (ou push)", |h| h.query_by_label(PUSH_DIALOG).is_some() || moved());
    assert!(!moved(), "origin/main n'a pas bougé");
    assert!(has_text_input(&h, "main"), "dialogue prérempli avec la branche suivie");
    h.get_by_label("Annuler").click();
    wait_until(&mut h, "dialogue fermé", |h| h.query_by_label(PUSH_DIALOG).is_none());

    open_menu(&mut h, "Dépôt");
    click_menu_item(&mut h, "Push rapide");
    wait_until(&mut h, "dialogue de push (ou push)", |h| h.query_by_label(PUSH_DIALOG).is_some() || moved());
    assert!(!moved(), "origin/main n'a pas bougé");
    let quiet = becomes_true(&mut h, std::time::Duration::from_millis(500), moved);
    assert!(!quiet, "aucun push en arrière-plan");
}
