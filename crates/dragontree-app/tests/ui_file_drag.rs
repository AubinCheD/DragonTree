//! Vue Changements : glisser des fichiers entre « Non indexés » et « Indexés », simulé par des événements pointeur.

mod common;

use std::path::{Path, PathBuf};

use common::*;
use dragontree_app::{DragonTreeApp, StartOptions};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

/// Dépôt `contoso` : trois fichiers suivis modifiés, non indexés.
fn repo(base: &Path) -> PathBuf {
    let dir = base.join("contoso");
    init_repo(&dir);
    for name in ["a.txt", "b.txt", "c.txt"] {
        commit_file(&dir, name, "1\n", &format!("Ajout de {name}"));
    }
    for name in ["a.txt", "b.txt", "c.txt"] {
        std::fs::write(dir.join(name), "2\n").unwrap();
    }
    dir
}

fn open(base: &Path, repo: &Path) -> Harness<'static, DragonTreeApp> {
    let mut h = harness(StartOptions { open: vec![repo.to_path_buf()], view: Some("changes".into()), ..start(base) });
    wait_until(&mut h, "fichiers non indexés", |h| {
        ["a.txt", "b.txt", "c.txt"].iter().all(|n| h.query_all_by_label(n).count() > 0)
    });
    h.run_steps(10);
    h
}

/// Centre de la ligne de liste d'un fichier (l'élément le plus à gauche de ce libellé : le titre du diff est à droite).
fn row_center(h: &Harness<'static, DragonTreeApp>, name: &str) -> egui::Pos2 {
    h.query_all_by_label(name)
        .map(|n| n.rect().center())
        .min_by(|a, b| a.x.total_cmp(&b.x))
        .unwrap_or_else(|| panic!("ligne de {name} absente"))
}

/// Centre du premier élément dont le libellé contient `part` (en-tête ou message de liste vide).
fn label_center(h: &Harness<'static, DragonTreeApp>, part: &str) -> egui::Pos2 {
    h.query_all_by_label_contains(part)
        .map(|n| n.rect().center())
        .min_by(|a, b| a.x.total_cmp(&b.x))
        .unwrap_or_else(|| panic!("libellé {part} absent"))
}

/// Amène le pointeur en `pos` et, avec `pressed`, y appuie ou relâche le bouton principal, en une image.
fn pointer_at(h: &mut Harness<'static, DragonTreeApp>, pos: egui::Pos2, pressed: Option<bool>) {
    let events = &mut h.input_mut().events;
    events.push(egui::Event::PointerMoved(pos));
    if let Some(pressed) = pressed {
        let button = egui::PointerButton::Primary;
        events.push(egui::Event::PointerButton { pos, button, pressed, modifiers: egui::Modifiers::NONE });
    }
    h.step();
}

/// Glisse depuis `start` jusqu'à `end` (par étapes) et relâche.
fn drag(h: &mut Harness<'static, DragonTreeApp>, start: egui::Pos2, end: egui::Pos2) {
    pointer_at(h, start, Some(true));
    for i in 1..=4 {
        pointer_at(h, start + (end - start) * (i as f32 / 4.0), None);
    }
    pointer_at(h, end, Some(false));
    h.remove_cursor();
    h.run_steps(3);
}

#[test]
fn dragging_a_file_to_the_staged_list_stages_it_and_back_unstages_it() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo(tmp.path());
    let mut h = open(tmp.path(), &repo);

    let (from, to) = (row_center(&h, "a.txt"), label_center(&h, "Aucun fichier indexé"));
    drag(&mut h, from, to);
    wait_git(&mut h, "a.txt indexé", || staged(&repo).is_some_and(|s| s == ["a.txt"]));

    // Retour : de « Indexés » vers « Non indexés ».
    wait_until(&mut h, "a.txt dans les indexés", |h| h.query_all_by_label("Aucun fichier indexé").count() == 0);
    h.run_steps(3);
    let (from, to) = (row_center(&h, "a.txt"), label_center(&h, "Non indexés"));
    assert!(from.y > to.y, "a.txt est dans la liste du bas");
    drag(&mut h, from, to);
    wait_git(&mut h, "a.txt désindexé", || staged(&repo).is_some_and(|s| s.is_empty()));
}

#[test]
fn dragging_a_selected_file_drags_the_whole_selection() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo(tmp.path());
    let mut h = open(tmp.path(), &repo);

    h.query_all_by_label("a.txt").min_by(|a, b| a.rect().min.x.total_cmp(&b.rect().min.x)).unwrap().click();
    h.run_steps(3);
    h.query_all_by_label("b.txt")
        .min_by(|a, b| a.rect().min.x.total_cmp(&b.rect().min.x))
        .unwrap()
        .click_modifiers(egui::Modifiers::CTRL);
    h.run_steps(3);

    let (from, to) = (row_center(&h, "b.txt"), label_center(&h, "Aucun fichier indexé"));
    drag(&mut h, from, to);
    wait_git(&mut h, "a.txt et b.txt indexés", || staged(&repo).is_some_and(|s| s == ["a.txt", "b.txt"]));
    assert_eq!(staged(&repo).unwrap(), ["a.txt", "b.txt"], "c.txt reste non indexé");
}

#[test]
fn dropping_a_file_back_on_its_own_list_changes_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo(tmp.path());
    let mut h = open(tmp.path(), &repo);

    let (from, to) = (row_center(&h, "a.txt"), row_center(&h, "c.txt"));
    // Le glisser a bien lieu (le fantôme suit le pointeur) mais le dépôt sur sa propre liste ne fait rien.
    pointer_at(&mut h, from, Some(true));
    for i in 1..=4 {
        pointer_at(&mut h, from + (to - from) * (i as f32 / 4.0), None);
    }
    assert!(h.query_by_label_contains("1 fichier").is_some(), "le fantôme du glisser est affiché");
    pointer_at(&mut h, to, Some(false));
    h.remove_cursor();
    assert!(
        !becomes_true(&mut h, std::time::Duration::from_secs(2), || staged(&repo).is_some_and(|s| !s.is_empty())),
        "rien ne doit être indexé"
    );
}
