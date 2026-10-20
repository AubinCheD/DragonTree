//! « Ouvrir dans » : Dépôt › Ouvrir dans liste les éditeurs détectés (au moins l'éditeur par défaut de Windows) et
//! la barre d'outils propose la flèche de choix. Aucun éditeur n'est lancé : la détection lit la machine, le
//! lancement n'est vérifié que par les tests unitaires de `editors`.

mod common;

use std::path::{Path, PathBuf};

use common::*;
use dragontree_app::StartOptions;
use egui_kittest::kittest::Queryable;

fn repo(base: &Path) -> PathBuf {
    let dir = base.join("fabrikam");
    init_repo(&dir);
    commit_file(&dir, "a.txt", "1\n", "Premier commit");
    dir
}

#[test]
fn the_repository_menu_lists_the_default_editor_and_the_toolbar_has_a_chooser() {
    let base = tempfile::tempdir().unwrap();
    let repo = repo(base.path());
    let mut h = harness(StartOptions { open: vec![repo], view: Some("history".into()), ..start(base.path()) });
    wait_until(&mut h, "dépôt chargé", |h| h.query_by_label("Premier commit").is_some());
    wait_until(&mut h, "flèche de choix de l'éditeur", |h| h.query_by_label("Choisir l'éditeur").is_some());

    open_menu(&mut h, "Dépôt");
    open_menu(&mut h, "Ouvrir dans");
    wait_until(&mut h, "éditeur par défaut listé", |h| menu_item(h, "Éditeur par défaut").is_some());
    assert!(!menu_item_disabled(&h, "Éditeur par défaut"));
}
