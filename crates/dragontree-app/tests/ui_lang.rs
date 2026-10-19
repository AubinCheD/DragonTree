//! Langue de l'interface : choisie au démarrage, puis changée dans les réglages sans redémarrer.
//! Binaire de test à part : la langue est globale au processus et les autres tests attendent le français.

use std::time::{Duration, Instant};

use dragontree_app::{DragonTreeApp, StartOptions};
use dragontree_i18n::Lang;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

fn wait_until(
    h: &mut Harness<'static, DragonTreeApp>,
    what: &str,
    mut cond: impl FnMut(&Harness<'static, DragonTreeApp>) -> bool,
) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        h.step();
        if cond(h) {
            return;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    panic!("délai dépassé : {what}");
}

#[test]
fn language_can_be_switched_from_the_settings() {
    let tmp = tempfile::tempdir().unwrap();
    let profile = tmp.path().join("profil");
    let start = StartOptions {
        profile_dir: Some(profile.clone()),
        screenshot_after: Duration::ZERO,
        lang: Some(Lang::En),
        window: Some("settings".into()),
        ..Default::default()
    };
    let mut h =
        Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |cc| DragonTreeApp::new(cc, start));

    wait_until(&mut h, "interface en anglais", |h| h.query_by_label_contains("Open a repository").is_some());
    assert!(h.query_by_label_contains("Ouvrir un dépôt").is_none(), "texte français resté affiché");

    h.get_by_label("Français").click();
    h.step();
    h.get_by_label("Save").click();
    wait_until(&mut h, "interface en français", |h| h.query_by_label_contains("Ouvrir un dépôt").is_some());
    assert!(h.query_by_label_contains("Open a repository").is_none(), "texte anglais resté affiché");

    let saved = std::fs::read_to_string(profile.join("settings.json")).unwrap();
    assert!(saved.contains("\"language\": \"fr\""), "langue non enregistrée : {saved}");
}
