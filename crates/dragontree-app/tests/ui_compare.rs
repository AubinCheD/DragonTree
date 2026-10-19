//! Comparaison : deux commits (marquer puis comparer), un commit avec la copie de travail, et l'inversion A/B.
//! Les fichiers figurent aussi dans les détails du commit sélectionné : les recherches sont limitées à la fenêtre.

mod common;

use std::path::{Path, PathBuf};

use common::*;
use dragontree_app::icons;
use dragontree_app::{DragonTreeApp, StartOptions};
use egui_kittest::Harness;
use egui_kittest::kittest::{AccessKitNode, Queryable};

type H = Harness<'static, DragonTreeApp>;

/// Dépôt `fabrikam` : « Premier commit » (a.txt), « Second commit » (a.txt modifié, b.txt ajouté).
fn repo(base: &Path) -> PathBuf {
    let dir = base.join("fabrikam");
    init_repo(&dir);
    commit_file(&dir, "a.txt", "1\n", "Premier commit");
    std::fs::write(dir.join("b.txt"), "bravo\n").unwrap();
    commit_file(&dir, "a.txt", "2\n", "Second commit");
    dir
}

fn open(base: &Path, repo: &Path) -> H {
    let mut h = harness(StartOptions { open: vec![repo.to_path_buf()], view: Some("history".into()), ..start(base) });
    wait_until(&mut h, "dépôt chargé", |h| {
        h.query_by_label("Premier commit").is_some() && h.query_by_label("Second commit").is_some()
    });
    // Les références lues (la branche dans la barre latérale) : le menu d'un commit gagne alors ses entrées de
    // fusion et de rebase, avant celles de comparaison, qui se décalent. Lu plus tard, il bougerait sous le clic.
    wait_until(&mut h, "branche lue", |h| h.query_all_by_label("main").next().is_some());
    h
}

fn in_compare_window(node: &AccessKitNode<'_>) -> bool {
    std::iter::successors(Some(*node), |n| n.parent())
        .any(|n| n.role() == egui::accesskit::Role::Window && n.label().is_some_and(|l| l.contains("Comparaison")))
}

/// Élément de libellé exactement `label` dans la fenêtre de comparaison.
fn in_window(h: &H, label: &str) -> bool {
    h.query_by(|n| (n.label().as_deref() == Some(label) || n.value().as_deref() == Some(label)) && in_compare_window(n))
        .is_some()
}

fn menu_entry(icon: &str, label: &str) -> String {
    format!("{icon} {label}")
}

fn pick(h: &mut H, row: &str, entry: &str) {
    right_click_label(h, row);
    click_label(h, entry);
    h.run_steps(2);
}

#[test]
fn comparing_two_commits_lists_the_changed_files_and_shows_the_diff() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo(tmp.path());
    let mut h = open(tmp.path(), &repo);
    pick(&mut h, "Premier commit", &menu_entry(icons::GIT_DIFF, "Marquer pour comparaison"));
    let short = git(&repo, &["rev-parse", "--short=7", "HEAD~1"]);
    let compare = menu_entry(icons::GIT_DIFF, &format!("Comparer avec {} (marqué)", short.trim()));
    pick(&mut h, "Second commit", &compare);
    // A = « Second commit », B = « Premier commit » : a.txt revient à « 1 », b.txt disparaît.
    wait_until(&mut h, "fichiers comparés", |h| in_window(h, "a.txt") && in_window(h, "b.txt"));
    wait_until(&mut h, "diff de a.txt", |h| in_window(h, "+1  -1"));
}

#[test]
fn comparing_a_commit_with_local_changes_diffs_against_the_working_copy() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo(tmp.path());
    std::fs::write(repo.join("a.txt"), "1\n2\n3\n").unwrap();
    let mut h = open(tmp.path(), &repo);
    pick(&mut h, "Second commit", &menu_entry(icons::GIT_DIFF, "Comparer avec les modifications locales"));
    wait_until(&mut h, "fichier modifié localement", |h| in_window(h, "a.txt"));
    assert!(!in_window(&h, "b.txt"), "b.txt n'a pas changé depuis ce commit");
    wait_until(&mut h, "diff de a.txt", |h| in_window(h, "+2  -0"));
}

#[test]
fn the_swap_button_exchanges_a_and_b() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo(tmp.path());
    let mut h = open(tmp.path(), &repo);
    pick(&mut h, "Second commit", &menu_entry(icons::GIT_DIFF, "Comparer avec HEAD"));
    let head = git(&repo, &["rev-parse", "--short=7", "HEAD"]);
    let head = head.trim().to_string();
    wait_until(&mut h, "fenêtre ouverte", |h| in_window(h, &format!("A : {head}")) && in_window(h, "B : HEAD"));
    // La fenêtre est centrée : elle peut encore bouger pendant que la liste se remplit, d'où un clic en une image.
    click_by(&mut h, "bouton d'inversion", egui::PointerButton::Primary, |n| {
        n.label().as_deref() == Some(icons::SWAP) && in_compare_window(n)
    });
    wait_until(&mut h, "côtés échangés", |h| in_window(h, "A : HEAD") && in_window(h, &format!("B : {head}")));
}
