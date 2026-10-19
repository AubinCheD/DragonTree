//! Vue Changements : fichiers ignorés (Affichage › Afficher les fichiers ignorés) et non suivis masqués.

mod common;

use common::*;
use dragontree_app::DragonTreeApp;
use dragontree_app::StartOptions;
use dragontree_app::settings::Settings;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

#[test]
fn ignored_files_show_on_demand_and_untracked_files_can_be_hidden() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("contoso");
    init_repo(&repo);
    commit_file(&repo, ".gitignore", "*.log\n", "Base");
    std::fs::write(repo.join("build.log"), "l\n").unwrap();
    std::fs::write(repo.join("notes.txt"), "n\n").unwrap();

    let mut h = harness(StartOptions { open: vec![repo.clone()], view: Some("changes".into()), ..start(tmp.path()) });
    wait_until(&mut h, "fichier non suivi", |h| shown(h, "notes.txt"));
    // Par défaut, le fichier ignoré n'apparaît pas.
    assert!(!shown(&h, "build.log"));

    // Option « Afficher les fichiers ignorés » : il apparaît, sans changer le nombre de non indexés.
    open_menu(&mut h, "Affichage");
    click_menu_item(&mut h, "Afficher les fichiers ignorés");
    wait_until(&mut h, "fichier ignoré affiché", |h| shown(h, "build.log"));
    let settings = tmp.path().join("profil").join("settings.json");
    wait_until(&mut h, "réglage enregistré", |_| Settings::load_from(&settings).show_ignored_files);
    assert!(shown(&h, "notes.txt"));

    // « Masquer les fichiers non suivis » : notes.txt disparaît, le fichier ignoré reste.
    open_menu(&mut h, "Affichage");
    click_menu_item(&mut h, "Masquer les fichiers non suivis");
    wait_until(&mut h, "non suivi masqué", |h| !shown(h, "notes.txt"));
    assert!(shown(&h, "build.log"));
    assert!(Settings::load_from(&settings).hide_untracked_files);

    // Réaffiché, puis « Tout indexer » n'indexe pas le fichier ignoré.
    open_menu(&mut h, "Affichage");
    click_menu_item(&mut h, "Masquer les fichiers non suivis");
    wait_until(&mut h, "non suivi réaffiché", |h| shown(h, "notes.txt"));
    h.get_by_label_contains("Tout indexer").click();
    wait_git(&mut h, "notes.txt indexé", || git(&repo, &["diff", "--cached", "--name-only"]).trim() == "notes.txt");
    assert_eq!(git(&repo, &["diff", "--cached", "--name-only"]).trim(), "notes.txt");

    // « Indexer quand même » (clic droit) : git add -f.
    h.get_by_label("build.log").click_secondary();
    h.run_steps(3);
    h.get_by_label_contains("Indexer quand même").click();
    wait_git(&mut h, "fichier ignoré forcé", || {
        git(&repo, &["diff", "--cached", "--name-only"]).contains("build.log")
    });
}

/// Le texte est affiché (liste, et aussi titre du diff si le fichier est sélectionné).
fn shown(h: &Harness<'static, DragonTreeApp>, text: &str) -> bool {
    h.query_all_by_label(text).next().is_some()
}
