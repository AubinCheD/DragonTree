//! Diff de la vue Changements : le menu contextuel d'une ligne copie la sélection (un clic droit ne la réduit pas
//! à la ligne cliquée) et le texte copié est celui du fichier, sans marqueur `+`/`-`.

mod common;

type H = egui_kittest::Harness<'static, dragontree_app::DragonTreeApp>;

use common::*;
use dragontree_app::StartOptions;
use egui_kittest::kittest::Queryable;

#[test]
fn right_click_keeps_the_selected_lines_and_copy_gives_the_file_text() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = demo_repo(tmp.path());
    let mut h = harness(StartOptions { open: vec![repo], view: Some("changes".into()), ..start(tmp.path()) });
    wait_until(&mut h, "diff du fichier modifié", |h| h.query_by_label_contains("Indexer le hunk").is_some());
    h.run_steps(5);
    let header = h.get_by_label_contains("Indexer le hunk").rect();
    // Les lignes suivent l'en-tête du hunk : contexte, supprimée, ajoutée (hauteur de ligne de la police du diff).
    let pitch = ROW_PITCH;
    let x = header.left() - 40.0;
    // Le bouton est posé 2 px sous le haut de la ligne d'en-tête (rang 0) ; contexte, supprimée, ajoutée suivent.
    let row_center = |rank: f32| egui::pos2(x, header.top() - 2.0 + pitch * (rank + 0.5));
    let (removed, added) = (row_center(2.0), row_center(3.0));
    let press =
        |pos, button, pressed| egui::Event::PointerButton { pos, button, pressed, modifiers: egui::Modifiers::NONE };
    // Clic sur la ligne supprimée, puis Ctrl+clic sur la ligne ajoutée : les deux sont sélectionnées.
    let click = |h: &mut H, pos, ctrl: bool| {
        let modifiers = if ctrl { egui::Modifiers::CTRL } else { egui::Modifiers::NONE };
        h.event(egui::Event::PointerMoved(pos));
        h.step();
        h.event(egui::Event::ModifiersChanged(modifiers));
        h.event(press(pos, egui::PointerButton::Primary, true));
        h.step();
        h.event(press(pos, egui::PointerButton::Primary, false));
        h.step();
        h.event(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
        h.step();
    };
    click(&mut h, removed, false);
    click(&mut h, added, true);
    wait_until(&mut h, "boutons de la sélection", |h| h.query_by_label_contains("Indexer 2 lignes").is_some());

    // Clic droit sur une des lignes sélectionnées : la sélection reste, le menu propose de copier les deux.
    h.event(egui::Event::PointerMoved(added));
    h.step();
    h.event(press(added, egui::PointerButton::Secondary, true));
    h.step();
    h.event(press(added, egui::PointerButton::Secondary, false));
    wait_until(&mut h, "menu de copie", |h| h.query_by_label("Copier 2 lignes").is_some());
    h.get_by_label("Copier 2 lignes").click();
    let copied = copied_text(&mut h).expect("texte copié");
    assert_eq!(copied, "    a + b\n    a.checked_add(b).unwrap()\n");
}

const ROW_PITCH: f32 = 19.0;
