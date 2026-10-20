//! Diff d'images : modes côte à côte, balayage et superposition.

mod common;

use common::*;
use dragontree_app::StartOptions;
use egui_kittest::kittest::Queryable;

fn png(path: &std::path::Path, width: u32, height: u32, rgba: [u8; 4]) {
    image::RgbaImage::from_pixel(width, height, image::Rgba(rgba)).save(path).unwrap();
}

#[test]
fn a_modified_png_offers_the_three_modes_and_the_swipe_handle() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("fabrikam");
    init_repo(&repo);
    png(&repo.join("logo.png"), 4, 4, [255, 0, 0, 255]);
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-q", "-m", "Ajoute le logo"]);
    // Taille différente : l'aperçu les aligne en haut à gauche, à la même échelle.
    png(&repo.join("logo.png"), 6, 3, [0, 255, 0, 255]);
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-q", "-m", "Redessine le logo"]);

    let mut h = harness(StartOptions { open: vec![repo], view: Some("history".into()), ..start(tmp.path()) });
    click_label(&mut h, "Redessine le logo");
    click_label(&mut h, "logo.png");

    wait_until(&mut h, "sélecteur de mode", |h| h.query_by_label("Balayage").is_some());
    assert!(h.query_by_label("Côte à côte").is_some() && h.query_by_label("Superposition").is_some());
    assert!(h.query_by_label("Curseur de balayage").is_none(), "côte à côte par défaut");

    h.get_by_label("Balayage").click();
    wait_until(&mut h, "curseur de balayage", |h| h.query_by_label("Curseur de balayage").is_some());

    h.get_by_label("Superposition").click();
    wait_until(&mut h, "curseur d'opacité", |h| {
        h.query_by_label("Opacité de la nouvelle image").is_some() && h.query_by_label("Curseur de balayage").is_none()
    });

    // Le mode est un réglage : il est enregistré.
    let settings = tmp.path().join("profil").join("settings.json");
    wait_git(&mut h, "réglage enregistré", || {
        std::fs::read_to_string(&settings).is_ok_and(|s| s.contains("\"image_diff_mode\": \"Overlay\""))
    });
}
