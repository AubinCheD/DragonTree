//! Fichier › Espaces de travail (créer, changer, renommer, supprimer) et couleurs d'onglets : les onglets de chaque
//! espace sont rangés puis restaurés, et tout est persisté dans `settings.json`.

mod common;

use std::path::{Path, PathBuf};

use common::*;
use dragontree_app::settings::Settings;
use dragontree_app::workspaces::TabColor;
use dragontree_app::{DragonTreeApp, StartOptions};
use egui_kittest::Harness;
use egui_kittest::kittest::{AccessKitNode, Queryable};

fn repo(base: &Path, name: &str) -> PathBuf {
    let dir = base.join(name);
    init_repo(&dir);
    commit_file(&dir, "a.txt", "a\n", "Initial");
    dir
}

fn saved(base: &Path) -> Settings {
    Settings::load_from(&base.join("profil").join("settings.json"))
}

fn names(paths: &[PathBuf]) -> Vec<String> {
    paths.iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect()
}

/// Deux dépôts ouverts dans l'espace « Par défaut ».
fn open_two(base: &Path) -> Harness<'static, DragonTreeApp> {
    let (a, b) = (repo(base, "alpha"), repo(base, "bravo"));
    let mut h = harness(StartOptions { open: vec![a, b], ..start(base) });
    wait_until(&mut h, "deux onglets", |_| names(&saved(base).open_tabs) == ["alpha", "bravo"]);
    h
}

fn workspace_menu(h: &mut Harness<'static, DragonTreeApp>, entry: &str) {
    open_menu(h, "Fichier");
    open_menu(h, "Espaces de travail");
    click_menu_item(h, entry);
}

/// Le champ de nom de la fenêtre : celui qui a le focus (le filtre des branches est un autre champ de texte).
fn is_dialog_field(node: &AccessKitNode<'_>) -> bool {
    node.role() == egui::accesskit::Role::TextInput && node.is_focused()
}

/// Saisit `name` dans la fenêtre ouverte et clique `ok`.
fn answer_dialog(h: &mut Harness<'static, DragonTreeApp>, name: &str, ok: &str) {
    wait_until(h, "fenêtre des espaces", |h| h.query_by(is_dialog_field).is_some());
    let field = h.get_by(is_dialog_field);
    field.focus();
    field.type_text(name);
    h.run_steps(2);
    h.get_by_label(ok).click();
    h.run_steps(3);
}

#[test]
fn a_new_workspace_starts_empty_and_each_workspace_gets_its_tabs_back() {
    let tmp = tempfile::tempdir().unwrap();
    let mut h = open_two(tmp.path());

    workspace_menu(&mut h, "Nouvel espace de travail…");
    answer_dialog(&mut h, "Client Contoso", "Créer");
    wait_until(&mut h, "espace créé et actif", |_| {
        let s = saved(tmp.path());
        s.current_workspace == "Client Contoso" && s.open_tabs.is_empty()
    });
    // Les onglets de l'espace quitté sont rangés.
    assert_eq!(names(&saved(tmp.path()).workspaces[""].tabs), ["alpha", "bravo"]);

    // Ouvre un dépôt dans le nouvel espace (dépôts récents de l'accueil).
    wait_until(&mut h, "dépôts récents", |h| h.query_by_label_contains("bravo").is_some());
    h.get_by_label_contains("bravo").click();
    wait_until(&mut h, "bravo ouvert", |_| names(&saved(tmp.path()).open_tabs) == ["bravo"]);

    // Retour à « Par défaut » : alpha et bravo sont rouverts, l'autre espace garde bravo.
    workspace_menu(&mut h, "Par défaut");
    wait_until(&mut h, "onglets restaurés", |_| {
        let s = saved(tmp.path());
        s.current_workspace.is_empty() && names(&s.open_tabs) == ["alpha", "bravo"]
    });
    assert_eq!(names(&saved(tmp.path()).workspaces["Client Contoso"].tabs), ["bravo"]);

    // Et inversement.
    workspace_menu(&mut h, "Client Contoso");
    wait_until(&mut h, "onglets de l'autre espace", |_| names(&saved(tmp.path()).open_tabs) == ["bravo"]);
}

#[test]
fn a_workspace_can_be_renamed_and_deleted_but_not_the_default_one() {
    let tmp = tempfile::tempdir().unwrap();
    let mut h = open_two(tmp.path());

    // « Par défaut » : renommer et supprimer sont grisés.
    open_menu(&mut h, "Fichier");
    open_menu(&mut h, "Espaces de travail");
    assert!(menu_item_disabled(&h, "Renommer l'espace de travail…"));
    assert!(menu_item_disabled(&h, "Supprimer l'espace de travail…"));
    h.key_press(egui::Key::Escape);
    h.run_steps(3);

    workspace_menu(&mut h, "Nouvel espace de travail…");
    answer_dialog(&mut h, "Northwind", "Créer");
    wait_until(&mut h, "Northwind actif", |_| saved(tmp.path()).current_workspace == "Northwind");

    workspace_menu(&mut h, "Renommer l'espace de travail…");
    wait_until(&mut h, "fenêtre", |h| h.query_by(is_dialog_field).is_some());
    let field = h.get_by(is_dialog_field);
    field.focus();
    field.type_text("-2");
    h.run_steps(2);
    h.get_by_label("Renommer").click();
    wait_until(&mut h, "espace renommé", |_| saved(tmp.path()).current_workspace == "Northwind-2");

    // Supprimer : Entrée ne valide pas (destructif), le bouton oui ; on revient à « Par défaut » et à ses onglets.
    workspace_menu(&mut h, "Supprimer l'espace de travail…");
    wait_until(&mut h, "confirmation", |h| h.query_by_label("Supprimer").is_some());
    h.key_press(egui::Key::Enter);
    h.run_steps(5);
    assert_eq!(saved(tmp.path()).current_workspace, "Northwind-2", "Entrée ne confirme pas une suppression");
    h.get_by_label("Supprimer").click();
    wait_until(&mut h, "espace supprimé", |_| {
        let s = saved(tmp.path());
        s.current_workspace.is_empty() && s.workspaces.is_empty() && names(&s.open_tabs) == ["alpha", "bravo"]
    });
}

#[test]
fn a_tab_color_is_chosen_from_the_tab_menu_and_persisted_per_repository() {
    let tmp = tempfile::tempdir().unwrap();
    let mut h = open_two(tmp.path());

    // Le libellé de l'onglet « bravo » se trouve dans la barre d'onglets (la plus haute) et dans la barre d'outils.
    let tab_y = |h: &Harness<'static, DragonTreeApp>| {
        h.query_all_by_label("bravo")
            .map(|n| n.rect().center())
            .min_by(|a, b| a.y.total_cmp(&b.y))
            .expect("onglet bravo")
    };
    let at = tab_y(&h);
    h.get_all_by_label("bravo").find(|n| n.rect().center() == at).unwrap().click_secondary();
    h.run_steps(3);
    wait_until(&mut h, "menu de l'onglet", |h| h.query_by_label_contains("Couleur").is_some());
    h.get_by_label_contains("Couleur").click();
    wait_until(&mut h, "couleurs", |h| h.query_by_label("Rouge").is_some());
    h.get_by_label("Rouge").click();
    wait_until(&mut h, "couleur enregistrée", |_| {
        let s = saved(tmp.path());
        s.tab_color_of(&tmp.path().join("bravo")) == Some(TabColor::Red)
    });
    let json = std::fs::read_to_string(tmp.path().join("profil").join("settings.json")).unwrap();
    assert!(json.contains("\"red\""), "{json}");
    assert_eq!(saved(tmp.path()).tab_color_of(&tmp.path().join("alpha")), None, "par dépôt, pas pour tous");

    // La couleur retirée disparaît des réglages.
    let at = tab_y(&h);
    h.get_all_by_label("bravo").find(|n| n.rect().center() == at).unwrap().click_secondary();
    h.run_steps(3);
    h.get_by_label_contains("Couleur").click();
    wait_until(&mut h, "couleurs", |h| h.query_by_label("Aucune").is_some());
    h.get_by_label("Aucune").click();
    wait_until(&mut h, "couleur retirée", |_| saved(tmp.path()).tab_colors.is_empty());
}

#[test]
fn the_active_tab_chosen_by_a_click_is_the_one_restored_after_a_round_trip() {
    let tmp = tempfile::tempdir().unwrap();
    let (a, b, c) = (repo(tmp.path(), "alpha"), repo(tmp.path(), "bravo"), repo(tmp.path(), "charlie"));
    let mut h = harness(StartOptions { open: vec![a, b, c], ..start(tmp.path()) });
    wait_until(&mut h, "trois onglets", |_| names(&saved(tmp.path()).open_tabs) == ["alpha", "bravo", "charlie"]);

    // Active le premier onglet par un clic (le dernier ouvert était actif), puis fait l'aller-retour.
    let top = h
        .query_all_by_label("alpha")
        .min_by(|x, y| x.rect().center().y.total_cmp(&y.rect().center().y))
        .map(|n| n.rect().center())
        .unwrap();
    h.get_all_by_label("alpha").find(|n| n.rect().center() == top).unwrap().click();
    h.run_steps(3);
    workspace_menu(&mut h, "Nouvel espace de travail…");
    answer_dialog(&mut h, "Client Contoso", "Créer");
    wait_until(&mut h, "espace créé", |_| saved(tmp.path()).current_workspace == "Client Contoso");
    assert_eq!(saved(tmp.path()).workspaces[""].active, 0, "l'onglet actif est rangé avec l'espace quitté");
    workspace_menu(&mut h, "Par défaut");
    wait_until(&mut h, "retour", |_| names(&saved(tmp.path()).open_tabs) == ["alpha", "bravo", "charlie"]);
    assert_eq!(saved(tmp.path()).active_tab, 0, "alpha est de nouveau actif");
}

#[test]
fn the_palette_lists_the_other_workspaces_and_switches_to_the_chosen_one() {
    let tmp = tempfile::tempdir().unwrap();
    let mut h = open_two(tmp.path());
    workspace_menu(&mut h, "Nouvel espace de travail…");
    answer_dialog(&mut h, "Northwind", "Créer");
    wait_until(&mut h, "Northwind actif", |_| saved(tmp.path()).current_workspace == "Northwind");

    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::P);
    let hint = "Dépôt, branche ou commande…";
    wait_until(&mut h, "palette", |h| h.query_by(|n| n.placeholder() == Some(hint)).is_some());
    h.get_by(|n| n.placeholder() == Some(hint)).type_text("Espace de travail : Par");
    h.run_steps(3);
    h.key_press(egui::Key::Enter);
    wait_until(&mut h, "retour à Par défaut", |_| {
        let s = saved(tmp.path());
        s.current_workspace.is_empty() && names(&s.open_tabs) == ["alpha", "bravo"]
    });
}
