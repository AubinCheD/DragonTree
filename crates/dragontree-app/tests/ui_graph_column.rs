//! Historique : la colonne du graphe se redimensionne à la poignée de l'en-tête, la largeur est mémorisée.

mod common;

use std::path::Path;

use common::*;
use dragontree_app::settings::Settings;
use dragontree_app::{DragonTreeApp, StartOptions};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

const GRIP: &str = "Redimensionner la colonne du graphe";

fn open(base: &Path, repo: &Path) -> Harness<'static, DragonTreeApp> {
    let mut h = harness(StartOptions { open: vec![repo.to_path_buf()], view: Some("history".into()), ..start(base) });
    wait_until(&mut h, "poignée de la colonne du graphe", |h| h.query_by_label(GRIP).is_some());
    h.run_steps(5);
    h
}

fn grip_center(h: &Harness<'static, DragonTreeApp>) -> egui::Pos2 {
    h.query_by_label(GRIP).expect("poignée absente").rect().center()
}

fn pointer_at(h: &mut Harness<'static, DragonTreeApp>, pos: egui::Pos2, pressed: Option<bool>) {
    let events = &mut h.input_mut().events;
    events.push(egui::Event::PointerMoved(pos));
    if let Some(pressed) = pressed {
        let button = egui::PointerButton::Primary;
        events.push(egui::Event::PointerButton { pos, button, pressed, modifiers: egui::Modifiers::NONE });
    }
    h.step();
}

fn saved_width(base: &Path) -> Option<f32> {
    Settings::load_from(&base.join("profil").join("settings.json")).graph_column_width
}

#[test]
fn dragging_the_graph_handle_resizes_the_column_and_the_width_survives_a_restart() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("contoso");
    init_repo(&repo);
    commit_file(&repo, "a.txt", "1\n", "Premier");
    commit_file(&repo, "a.txt", "2\n", "Second");

    let before = {
        let mut h = open(tmp.path(), &repo);
        let start = grip_center(&h);
        assert!(saved_width(tmp.path()).is_none(), "largeur calculée par défaut");
        let end = start + egui::vec2(120.0, 0.0);
        pointer_at(&mut h, start, Some(true));
        for i in 1..=4 {
            pointer_at(&mut h, start + (end - start) * (i as f32 / 4.0), None);
        }
        pointer_at(&mut h, end, Some(false));
        h.remove_cursor();
        h.run_steps(3);
        let moved = grip_center(&h);
        assert!((moved.x - end.x).abs() < 2.0, "la poignée suit le pointeur : {moved:?} / {end:?}");
        wait_until(&mut h, "largeur enregistrée", |_| saved_width(tmp.path()).is_some());
        moved
    };

    // Nouvelle session, même profil.
    let h = open(tmp.path(), &repo);
    assert!((grip_center(&h).x - before.x).abs() < 2.0, "largeur restaurée après redémarrage");
}

#[test]
fn double_clicking_the_graph_handle_returns_to_the_computed_width() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("contoso");
    init_repo(&repo);
    commit_file(&repo, "a.txt", "1\n", "Premier");

    let mut h = open(tmp.path(), &repo);
    let auto = grip_center(&h);
    let end = auto + egui::vec2(80.0, 0.0);
    pointer_at(&mut h, auto, Some(true));
    pointer_at(&mut h, end, None);
    pointer_at(&mut h, end, Some(false));
    h.remove_cursor();
    h.run_steps(3);
    assert!(grip_center(&h).x > auto.x + 50.0);
    wait_until(&mut h, "largeur enregistrée", |_| saved_width(tmp.path()).is_some());

    let grip = grip_center(&h);
    click_at(&mut h, grip, egui::PointerButton::Primary);
    click_at(&mut h, grip, egui::PointerButton::Primary);
    wait_until(&mut h, "largeur automatique rétablie", |_| saved_width(tmp.path()).is_none());
    h.run_steps(3);
    assert!((grip_center(&h).x - auto.x).abs() < 2.0);
}

/// Une largeur choisie en grande fenêtre ne pousse pas la poignée hors de la zone dans une fenêtre étroite : elle reste
/// visible, se déplace, et la préférence enregistrée n'est pas modifiée par le simple affichage.
#[test]
fn a_large_chosen_width_keeps_the_handle_reachable_in_a_narrow_window() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("contoso");
    init_repo(&repo);
    commit_file(&repo, "a.txt", "1\n", "Premier");
    std::fs::create_dir_all(tmp.path().join("profil")).unwrap();
    std::fs::write(tmp.path().join("profil").join("settings.json"), r#"{"graph_column_width": 600.0}"#).unwrap();

    let size = egui::vec2(820.0, 600.0);
    let opts = StartOptions { open: vec![repo.clone()], view: Some("history".into()), ..start(tmp.path()) };
    let mut h = Harness::builder().with_size(size).build_eframe(move |cc| DragonTreeApp::new(cc, opts));
    wait_until(&mut h, "poignée", |h| h.query_by_label(GRIP).is_some());
    h.run_steps(5);
    let grip = h.query_by_label(GRIP).unwrap().rect();
    assert!(grip.right() <= size.x && grip.left() > 0.0, "poignée dans la fenêtre : {grip:?}");
    assert!(grip.center().x < size.x - 338.0 - 100.0, "auteur, date et SHA gardent leur place : {grip:?}");
    assert_eq!(saved_width(tmp.path()), Some(600.0), "préférence intacte");

    // Glisser loin à droite : la poignée reste dans la zone.
    let start = grip.center();
    pointer_at(&mut h, start, Some(true));
    pointer_at(&mut h, egui::pos2(size.x - 5.0, start.y), None);
    pointer_at(&mut h, egui::pos2(size.x - 5.0, start.y), Some(false));
    h.remove_cursor();
    h.run_steps(3);
    let after = h.query_by_label(GRIP).unwrap().rect();
    assert!(after.center().x < size.x - 338.0 - 100.0, "toujours bornée : {after:?}");
}
