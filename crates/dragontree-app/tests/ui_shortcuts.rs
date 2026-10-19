//! Raccourcis Fetch / Pull / Push / Nouvelle branche / Stash : chacun ouvre le bon dialogue ou lance le fetch,
//! seulement quand la commande est utilisable (mêmes conditions que la barre d'outils).

mod common;

use std::path::{Path, PathBuf};
use std::time::Duration;

use common::*;
use dragontree_app::{DragonTreeApp, StartOptions};
use egui::{Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};

const CTRL_SHIFT: Modifiers = Modifiers { command: true, ctrl: true, shift: true, ..Modifiers::NONE };

fn press(h: &mut Harness<'static, DragonTreeApp>, key: Key) {
    h.key_press_modifiers(CTRL_SHIFT, key);
    h.run_steps(3);
}

fn has(h: &Harness<'static, DragonTreeApp>, label: &str) -> bool {
    h.query_by_label(label).is_some()
}

/// Dépôt de travail cloné d'un remote local (bare), un commit à pousser, un fichier modifié ; un « collègue »
/// a poussé un commit de son côté (à récupérer par fetch).
struct Fixture {
    work: PathBuf,
    other: PathBuf,
}

fn fixture(base: &Path) -> Fixture {
    let bare = base.join("remote.git");
    std::fs::create_dir_all(&bare).unwrap();
    git(&bare, &["init", "-q", "--bare", "-b", "main"]);
    let url = format!("file:///{}", bare.display().to_string().replace('\\', "/"));
    let (work, other) = (base.join("travail"), base.join("collegue"));
    git(base, &["clone", "-q", &url, work.to_str().unwrap()]);
    for (k, v) in [("user.name", "Testeur"), ("user.email", "t@example.com"), ("commit.gpgsign", "false")] {
        git(&work, &["config", k, v]);
    }
    commit_file(&work, "a.txt", "a\n", "premier");
    git(&work, &["push", "-q", "-u", "origin", "main"]);
    git(base, &["clone", "-q", &url, other.to_str().unwrap()]);
    for (k, v) in [("user.name", "Collègue"), ("user.email", "c@example.com"), ("commit.gpgsign", "false")] {
        git(&other, &["config", k, v]);
    }
    commit_file(&other, "c.txt", "c\n", "commit du collègue");
    git(&other, &["push", "-q", "origin", "main"]);
    commit_file(&work, "b.txt", "b\n", "local à pousser");
    std::fs::write(work.join("a.txt"), "modifié\n").unwrap();
    Fixture { work, other }
}

fn open(base: &Path, repo: &Path) -> Harness<'static, DragonTreeApp> {
    let mut start: StartOptions = start(base);
    start.open = vec![repo.to_path_buf()];
    start.view = Some("history".into());
    start.screenshot_after = Duration::ZERO;
    let mut h = harness(start);
    wait_until(&mut h, "dépôt chargé", |h| has(h, "local à pousser"));
    // Remotes et branche connus : Fetch, Pull et Push sont actifs dans la barre d'outils.
    wait_until(&mut h, "remote connu", |h| {
        ["Fetch", "Pull", "Push"].iter().all(|l| h.query_by_label(l).is_some_and(|n| !n.accesskit_node().is_disabled()))
    });
    h
}

#[test]
fn ctrl_shift_f_fetches() {
    let tmp = tempfile::tempdir().unwrap();
    let f = fixture(tmp.path());
    let mut h = open(tmp.path(), &f.work);
    press(&mut h, Key::F);
    let expected = git(&f.other, &["rev-parse", "HEAD"]).trim().to_string();
    wait_git(&mut h, "fetch terminé", || {
        try_git(&f.work, &["rev-parse", "origin/main"]).is_some_and(|s| s.trim() == expected)
    });
}

#[test]
fn ctrl_shift_u_opens_the_pull_dialog() {
    let tmp = tempfile::tempdir().unwrap();
    let f = fixture(tmp.path());
    let mut h = open(tmp.path(), &f.work);
    assert!(!has(&h, "Rebaser au lieu de fusionner (--rebase)"));
    press(&mut h, Key::U);
    wait_until(&mut h, "dialogue de pull", |h| has(h, "Rebaser au lieu de fusionner (--rebase)"));
}

#[test]
fn ctrl_shift_p_opens_the_push_dialog() {
    let tmp = tempfile::tempdir().unwrap();
    let f = fixture(tmp.path());
    let mut h = open(tmp.path(), &f.work);
    press(&mut h, Key::P);
    wait_until(&mut h, "dialogue de push", |h| has(h, "Pousser aussi tous les tags"));
}

#[test]
fn ctrl_shift_b_opens_the_new_branch_dialog() {
    let tmp = tempfile::tempdir().unwrap();
    let f = fixture(tmp.path());
    let mut h = open(tmp.path(), &f.work);
    assert!(!has(&h, "Nouvelle branche"));
    press(&mut h, Key::B);
    wait_until(&mut h, "dialogue de nouvelle branche", |h| has(h, "Nouvelle branche"));
}

#[test]
fn ctrl_shift_h_opens_the_stash_dialog() {
    let tmp = tempfile::tempdir().unwrap();
    let f = fixture(tmp.path());
    let mut h = open(tmp.path(), &f.work);
    press(&mut h, Key::H);
    wait_until(&mut h, "dialogue de stash", |h| has(h, "Inclure les fichiers non suivis"));
}

#[test]
fn greyed_commands_do_nothing_from_the_keyboard() {
    // Dépôt avec remote, arbre propre, HEAD détachée : Pull, Push et Stash sont grisés (Fetch reste actif).
    let tmp = tempfile::tempdir().unwrap();
    let f = fixture(tmp.path());
    git(&f.work, &["checkout", "-q", "--", "a.txt"]);
    git(&f.work, &["checkout", "-q", "--detach"]);
    let mut h = open_loaded(tmp.path(), &f.work, |h| {
        // refs (HEAD détachée affichée), status (arbre propre affiché) et remotes (Fetch actif) sont chargés.
        h.query_by_label_contains("HEAD détachée").is_some() && clean_tree_shown(h) && enabled(h, "Fetch")
    });
    assert!(!enabled(&h, "Pull") && !enabled(&h, "Push") && !enabled(&h, "Stash"), "boutons grisés");
    for key in [Key::U, Key::P, Key::H] {
        press(&mut h, key);
    }
    h.run_steps(10);
    assert!(!has(&h, "Rebaser au lieu de fusionner (--rebase)"), "pas de dialogue de pull");
    assert!(!has(&h, "Pousser aussi tous les tags"), "pas de dialogue de push");
    assert!(!has(&h, "Inclure les fichiers non suivis"), "pas de dialogue de stash");

    // Dépôt sans remote : Fetch est grisé, aucun fetch ne démarre ; Nouvelle branche reste utilisable.
    let repo = tmp.path().join("seul");
    init_repo(&repo);
    commit_file(&repo, "a.txt", "a\n", "unique commit");
    // Un stash : signal positif que les extras (stashes, remotes) sont chargés ; l'arbre reste propre.
    std::fs::write(repo.join("a.txt"), "modifié\n").unwrap();
    git(&repo, &["stash", "-q"]);
    let mut h = open_loaded(&tmp.path().join("p2"), &repo, |h| {
        // refs (Branche actif), status (arbre propre affiché) et extras, remotes compris (Pop actif : le stash est lu).
        enabled(h, "Branche") && clean_tree_shown(h) && enabled(h, "Pop")
    });
    assert!(!enabled(&h, "Fetch"), "Fetch grisé sans remote");
    press(&mut h, Key::F);
    let end = std::time::Instant::now() + Duration::from_secs(1);
    while std::time::Instant::now() < end {
        h.step();
        assert!(!has(&h, "Fetch terminé"), "aucun fetch ne démarre");
        std::thread::sleep(Duration::from_millis(25));
    }
    press(&mut h, Key::B);
    wait_until(&mut h, "dialogue de nouvelle branche", |h| has(h, "Nouvelle branche"));
}

#[test]
fn repo_shortcuts_do_nothing_while_a_dialog_is_open() {
    let tmp = tempfile::tempdir().unwrap();
    let f = fixture(tmp.path());
    let mut h = open(tmp.path(), &f.work);
    press(&mut h, Key::B);
    wait_until(&mut h, "dialogue de nouvelle branche", |h| has(h, "Nouvelle branche"));
    let hint = "nom-de-la-branche (ex. feature/ma-fonction)";
    h.get_by(|n| n.placeholder() == Some(hint)).type_text("ma-branche");
    h.run_steps(3);

    for key in [Key::H, Key::U, Key::P, Key::B] {
        press(&mut h, key);
    }
    h.run_steps(5);
    assert!(has(&h, "Nouvelle branche"), "le dialogue reste ouvert");
    assert!(has_text_input(&h, "ma-branche"), "sa saisie est conservée");
    assert!(!has(&h, "Inclure les fichiers non suivis"), "pas de dialogue de stash");
    assert!(!has(&h, "Pousser aussi tous les tags"), "pas de dialogue de push");
    assert!(!has(&h, "Rebaser au lieu de fusionner (--rebase)"), "pas de dialogue de pull");
}

#[test]
fn toolbar_tooltips_mention_the_shortcuts() {
    let tmp = tempfile::tempdir().unwrap();
    let f = fixture(tmp.path());
    let mut h = open(tmp.path(), &f.work);
    for (button, shortcut) in [
        ("Fetch", "Ctrl+Maj+F"),
        ("Pull", "Ctrl+Maj+U"),
        ("Push", "Ctrl+Maj+P"),
        ("Stash", "Ctrl+Maj+H"),
        ("Branche", "Ctrl+Maj+B"),
    ] {
        h.get_by_label(button).hover();
        wait_until(&mut h, &format!("infobulle de {button}"), |h| h.query_by_label_contains(shortcut).is_some());
        h.get_by_label(button).hover();
    }
}

fn enabled(h: &Harness<'static, DragonTreeApp>, label: &str) -> bool {
    h.query_by_label(label).is_some_and(|n| !n.accesskit_node().is_disabled())
}

/// Ouvre `repo` (profil dans `profile`) et attend `loaded` : l'état du dépôt est chargé avant d'appuyer sur les touches.
fn open_loaded(
    profile: &Path,
    repo: &Path,
    loaded: impl Fn(&Harness<'static, DragonTreeApp>) -> bool,
) -> Harness<'static, DragonTreeApp> {
    let mut start = start(profile);
    start.open = vec![repo.to_path_buf()];
    start.view = Some("changes".into());
    let mut h = harness(start);
    wait_until(&mut h, "dépôt chargé", |h| loaded(h));
    h
}

/// L'état du dépôt est lu (vue Modifications) et l'arbre est propre.
fn clean_tree_shown(h: &Harness<'static, DragonTreeApp>) -> bool {
    has(h, "Aucune modification locale — tout est commité.")
}
