//! Worktrees : section de la barre latérale (ouvrir, réactiver, supprimer), création par Dépôt › Créer un
//! worktree…, et checkout d'une branche sortie dans un autre worktree.

mod common;

use std::path::{Path, PathBuf};

use common::*;
use dragontree_app::settings::Settings;
use dragontree_app::{DragonTreeApp, StartOptions};
use egui::{Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

/// Dépôt `fabrikam` avec deux commits et une branche `fonction` (sur le premier).
fn repo(base: &Path) -> PathBuf {
    let dir = base.join("fabrikam");
    init_repo(&dir);
    commit_file(&dir, "a.txt", "1\n", "Premier commit");
    git(&dir, &["branch", "fonction"]);
    commit_file(&dir, "a.txt", "2\n", "Second commit");
    dir
}

fn open(base: &Path, repo: &Path) -> Harness<'static, DragonTreeApp> {
    let mut h = harness(StartOptions { open: vec![repo.to_path_buf()], view: Some("history".into()), ..start(base) });
    wait_until(&mut h, "dépôt chargé", |h| h.query_by_label("Second commit").is_some());
    h
}

/// Noms des dossiers des onglets de dépôt enregistrés, dans l'ordre.
fn saved_tabs(base: &Path) -> Vec<String> {
    Settings::load_from(&base.join("profil").join("settings.json"))
        .open_tabs
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect()
}

/// Ligne de la section Worktrees pour la branche `branch` (libellé « branche — chemin »).
fn has_row(h: &Harness<'static, DragonTreeApp>, branch: &str) -> bool {
    let prefix = format!("{branch} — ");
    h.query_by(|n| n.label().is_some_and(|l| l.starts_with(&prefix))).is_some()
}

fn click_row(h: &mut Harness<'static, DragonTreeApp>, branch: &str, secondary: bool) {
    wait_until(h, &format!("ligne du worktree {branch}"), |h| has_row(h, branch));
    let prefix = format!("{branch} — ");
    let row = h.get_by(|n| n.label().is_some_and(|l| l.starts_with(&prefix)));
    if secondary {
        row.click_secondary()
    } else {
        row.click()
    }
    h.run_steps(3);
}

/// L'onglet actif est celui du dossier `name` : son nom figure dans la barre d'outils en plus de la barre d'onglets.
fn active_is(h: &Harness<'static, DragonTreeApp>, name: &str) -> bool {
    h.query_all_by_label(name).count() == 2
}

#[test]
fn a_click_on_a_worktree_opens_its_tab_and_a_second_click_reactivates_it() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo(tmp.path());
    let wt = tmp.path().join("fabrikam-fonction");
    git(&repo, &["worktree", "add", "-q", wt.to_str().unwrap(), "fonction"]);
    let mut h = open(tmp.path(), &repo);
    click_row(&mut h, "fonction", false);
    wait_until(&mut h, "onglet du worktree", |h| {
        saved_tabs(tmp.path()) == ["fabrikam", "fabrikam-fonction"] && active_is(h, "fabrikam-fonction")
    });

    // Retour au dépôt principal, puis second clic : l'onglet existant est réactivé, sans doublon.
    h.key_press_modifiers(Modifiers::COMMAND, Key::Num1);
    wait_until(&mut h, "onglet principal actif", |h| active_is(h, "fabrikam"));
    click_row(&mut h, "fonction", false);
    wait_until(&mut h, "onglet du worktree réactivé", |h| active_is(h, "fabrikam-fonction"));
    h.run_steps(5);
    assert_eq!(saved_tabs(tmp.path()), ["fabrikam", "fabrikam-fonction"], "aucun onglet en double");
}

#[test]
fn checking_out_a_branch_checked_out_elsewhere_offers_to_open_its_worktree() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo(tmp.path());
    let wt = tmp.path().join("fabrikam-fonction");
    git(&repo, &["worktree", "add", "-q", wt.to_str().unwrap(), "fonction"]);
    let mut h = open(tmp.path(), &repo);
    wait_until(&mut h, "worktree listé", |h| has_row(h, "fonction"));

    h.get_by_label("fonction").click_secondary();
    h.run_steps(3);
    let checkout = "Checkout « fonction »";
    wait_until(&mut h, "menu de la branche", |h| h.query_by_label_contains(checkout).is_some());
    h.get_by_label_contains(checkout).click();
    wait_until(&mut h, "proposition d'ouvrir le worktree", |h| h.query_by_label("Ouvrir le worktree").is_some());
    assert_eq!(git(&repo, &["branch", "--show-current"]).trim(), "main", "le checkout a été refusé");

    h.get_by_label("Ouvrir le worktree").click();
    wait_until(&mut h, "onglet du worktree", |h| {
        saved_tabs(tmp.path()) == ["fabrikam", "fabrikam-fonction"] && active_is(h, "fabrikam-fonction")
    });
}

#[test]
fn create_worktree_from_the_repository_menu_proposes_a_folder_and_opens_it() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo(tmp.path());
    let mut h = open(tmp.path(), &repo);
    wait_until(&mut h, "branches lues", |h| h.query_by_label("fonction").is_some());

    open_menu(&mut h, "Dépôt");
    click_menu_item(&mut h, "Créer un worktree…");
    // Dossier proposé : <parent>/<dépôt>-<branche>, la seule branche libre étant `fonction`.
    wait_until(&mut h, "dossier proposé", |h| {
        h.query_by(|n| {
            n.role() == egui::accesskit::Role::TextInput
                && n.value().is_some_and(|v| v.ends_with(r"\fabrikam-fonction") || v.ends_with("/fabrikam-fonction"))
        })
        .is_some()
    });
    h.get_by_label("Créer").click();

    let wt = tmp.path().join("fabrikam-fonction");
    wait_git(&mut h, "worktree créé", || wt.join("a.txt").exists());
    assert_eq!(git(&wt, &["branch", "--show-current"]).trim(), "fonction");
    wait_until(&mut h, "onglet du worktree", |h| {
        saved_tabs(tmp.path()) == ["fabrikam", "fabrikam-fonction"] && active_is(h, "fabrikam-fonction")
    });
}

#[test]
fn removing_a_dirty_worktree_needs_an_explicit_force() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo(tmp.path());
    let wt = tmp.path().join("fabrikam-fonction");
    git(&repo, &["worktree", "add", "-q", wt.to_str().unwrap(), "fonction"]);
    std::fs::write(wt.join("a.txt"), "modifié\n").unwrap();
    let mut h = open(tmp.path(), &repo);

    let delete = "Supprimer le worktree…";
    let force = "Forcer : supprimer aussi les modifications non commitées et les fichiers non suivis (--force)";

    // Sans forcer : Entrée ne valide pas (dialogue destructeur) ; le bouton lance la suppression, que git refuse.
    click_row(&mut h, "fonction", true);
    wait_until(&mut h, "menu du worktree", |h| h.query_by_label_contains(delete).is_some());
    h.get_by_label_contains(delete).click();
    wait_until(&mut h, "confirmation", |h| h.query_by_label(force).is_some());
    h.key_press(Key::Enter);
    h.run_steps(5);
    assert!(h.query_by_label(force).is_some(), "Entrée ne valide pas une suppression");
    h.get_by_label("Supprimer").click();
    wait_until(&mut h, "refus de git signalé", |h| h.query_by_label_contains("a échoué").is_some());
    assert!(wt.join("a.txt").exists(), "rien n'est supprimé sans --force");

    // En cochant « Forcer » : le worktree et ses modifications sont supprimés, la ligne disparaît.
    click_row(&mut h, "fonction", true);
    wait_until(&mut h, "menu du worktree", |h| h.query_by_label_contains(delete).is_some());
    h.get_by_label_contains(delete).click();
    wait_until(&mut h, "confirmation", |h| h.query_by_label(force).is_some());
    h.get_by_label(force).click();
    h.run_steps(3);
    h.get_by_label("Supprimer").click();
    wait_git(&mut h, "worktree supprimé", || !wt.exists());
    wait_until(&mut h, "ligne retirée", |h| !has_row(h, "fonction"));
}

#[test]
fn the_section_follows_worktrees_added_and_removed_outside_dragontree() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo(tmp.path());

    // Le worktree principal et un worktree lié ouverts, le lié actif : sa liste est dans `<common>/worktrees`.
    let wt = tmp.path().join("fabrikam-fonction");
    git(&repo, &["worktree", "add", "-q", wt.to_str().unwrap(), "fonction"]);
    let profile = tmp.path().join("p2");
    let mut h = harness(StartOptions { open: vec![repo.clone(), wt.clone()], ..start(&profile) });
    wait_until(&mut h, "worktrees listés", |h| has_row(h, "fonction") && has_row(h, "main"));

    let other = tmp.path().join("fabrikam-autre");
    git(&repo, &["worktree", "add", "-q", "-b", "autre", other.to_str().unwrap()]);
    wait_until(&mut h, "worktree ajouté, vu du worktree lié", |h| has_row(h, "autre"));

    h.key_press_modifiers(Modifiers::COMMAND, Key::Num1);
    wait_until(&mut h, "onglet principal actif", |h| active_is(h, "fabrikam") && has_row(h, "autre"));
    git(&repo, &["worktree", "remove", other.to_str().unwrap()]);
    wait_until(&mut h, "worktree retiré, vu du principal", |h| !has_row(h, "autre"));
}

#[test]
fn removing_a_worktree_open_in_a_tab_closes_the_tab_first() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo(tmp.path());
    let mut h = open(tmp.path(), &repo);
    wait_until(&mut h, "branches lues", |h| h.query_by_label("fonction").is_some());

    // Le worktree créé s'ouvre dans son onglet.
    open_menu(&mut h, "Dépôt");
    click_menu_item(&mut h, "Créer un worktree…");
    wait_until(&mut h, "dialogue de création", |h| h.query_by_label("Créer").is_some());
    h.get_by_label("Créer").click();
    let wt = tmp.path().join("fabrikam-fonction");
    wait_until(&mut h, "onglet du worktree", |h| {
        saved_tabs(tmp.path()) == ["fabrikam", "fabrikam-fonction"] && active_is(h, "fabrikam-fonction")
    });
    wait_until(&mut h, "worktree chargé", |h| h.query_by_label("fonction").is_some());

    // Suppression depuis l'onglet principal : l'onglet du worktree se ferme d'abord, puis le dossier est supprimé.
    h.key_press_modifiers(Modifiers::COMMAND, Key::Num1);
    wait_until(&mut h, "onglet principal actif", |h| active_is(h, "fabrikam"));
    let delete = "Supprimer le worktree…";
    let force = "Forcer : supprimer aussi les modifications non commitées et les fichiers non suivis (--force)";
    click_row(&mut h, "fonction", true);
    wait_until(&mut h, "menu du worktree", |h| h.query_by_label_contains(delete).is_some());
    h.get_by_label_contains(delete).click();
    wait_until(&mut h, "confirmation", |h| h.query_by_label(force).is_some());
    click_label(&mut h, "Supprimer");
    wait_until(&mut h, "onglet du worktree fermé", |h| {
        saved_tabs(tmp.path()) == ["fabrikam"] && active_is(h, "fabrikam")
    });
    wait_git(&mut h, "worktree supprimé", || !wt.exists());
    wait_until(&mut h, "ligne retirée", |h| !has_row(h, "fonction"));

    // L'onglet fermé n'est pas retenu : Ctrl+Maj+T ne rouvre pas un dossier supprimé.
    h.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::T);
    h.run_steps(5);
    assert_eq!(saved_tabs(tmp.path()), ["fabrikam"]);
}
