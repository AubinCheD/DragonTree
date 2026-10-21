//! Menu contextuel d'un onglet : ses actions visent l'onglet sur lequel il a été ouvert, même si un autre onglet
//! se ferme pendant que le menu est ouvert (les onglets sont désignés par identifiant, pas par position).

mod common;

use std::path::{Path, PathBuf};

use common::*;
use dragontree_app::settings::Settings;
use dragontree_app::{DragonTreeApp, StartOptions};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

fn repo(base: &Path, name: &str) -> PathBuf {
    let dir = base.join(name);
    init_repo(&dir);
    commit_file(&dir, "a.txt", "a\n", "Initial");
    dir
}

fn saved(base: &Path) -> Settings {
    Settings::load_from(&base.join("profil").join("settings.json"))
}

fn open_names(base: &Path) -> Vec<String> {
    saved(base).open_tabs.iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect()
}

/// Centre de l'onglet `name` : le libellé le plus haut (la barre d'outils porte aussi le nom du dépôt).
fn tab_center(h: &Harness<'static, DragonTreeApp>, name: &str) -> egui::Pos2 {
    h.query_all_by_label(name)
        .map(|n| n.rect().center())
        .min_by(|a, b| a.y.total_cmp(&b.y))
        .unwrap_or_else(|| panic!("onglet {name}"))
}

#[test]
fn the_tab_menu_closes_the_tab_it_was_opened_on_even_if_an_earlier_tab_closes_meanwhile() {
    let tmp = tempfile::tempdir().unwrap();
    let repos: Vec<PathBuf> = ["alpha", "bravo", "charlie"].iter().map(|n| repo(tmp.path(), n)).collect();
    let mut h = harness(StartOptions { open: repos, ..start(tmp.path()) });
    wait_until(&mut h, "trois onglets", |_| open_names(tmp.path()) == ["alpha", "bravo", "charlie"]);

    // « alpha » devient l'onglet actif.
    let at = tab_center(&h, "alpha");
    click_at(&mut h, at, egui::PointerButton::Primary);
    h.run_steps(3);

    // Menu ouvert sur « bravo » (deuxième position).
    let at = tab_center(&h, "bravo");
    click_at(&mut h, at, egui::PointerButton::Secondary);
    wait_until(&mut h, "menu de l'onglet", |h| h.query_by_label("Fermer l'onglet").is_some());

    // « alpha » (actif, première position) se ferme pendant que le menu est ouvert : « bravo » passe en première
    // position, « charlie » en deuxième.
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::W);
    wait_until(&mut h, "alpha fermé", |_| open_names(tmp.path()) == ["bravo", "charlie"]);
    wait_until(&mut h, "menu toujours ouvert", |h| h.query_by_label("Fermer l'onglet").is_some());

    // « Fermer l'onglet » ferme « bravo », pas son voisin « charlie » qui occupe maintenant la position du menu.
    click_label(&mut h, "Fermer l'onglet");
    wait_until(&mut h, "bravo fermé", |_| open_names(tmp.path()) == ["charlie"]);
}
