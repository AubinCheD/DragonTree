//! Sous-modules : ajout par Dépôt › Ajouter un sous-module…, puis URL, déplacement et suppression depuis le menu
//! d'un sous-module de la barre latérale. Les sous-modules viennent d'un dépôt local (`file://`).

mod common;

use std::path::{Path, PathBuf};

use common::*;
use dragontree_app::icons;
use dragontree_app::{DragonTreeApp, StartOptions};
use egui::Key;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};

type H = Harness<'static, DragonTreeApp>;

/// git refuse `file://` pour un sous-module (git ≥ 2.38.1) : autorisé pour ce processus de test seulement, par
/// l'environnement, que l'application et les commandes des tests héritent.
fn allow_file_protocol() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        // SAFETY : appelé avant tout lancement de git par ce fichier de tests, toujours avec les mêmes valeurs.
        unsafe {
            std::env::set_var("GIT_CONFIG_COUNT", "1");
            std::env::set_var("GIT_CONFIG_KEY_0", "protocol.file.allow");
            std::env::set_var("GIT_CONFIG_VALUE_0", "always");
        }
    });
}

fn file_url(path: &Path) -> String {
    format!("file:///{}", path.display().to_string().replace('\\', "/"))
}

/// Dépôt `contoso-lib` (la bibliothèque à ajouter) et dépôt hôte `fabrikam`.
fn repos(base: &Path) -> (PathBuf, PathBuf) {
    allow_file_protocol();
    let lib = base.join("contoso-lib");
    init_repo(&lib);
    commit_file(&lib, "lib.txt", "1\n", "Commit de la bibliothèque");
    let host = base.join("fabrikam");
    init_repo(&host);
    commit_file(&host, "a.txt", "1\n", "Premier commit");
    (lib, host)
}

fn open(base: &Path, repo: &Path) -> H {
    let mut h = harness(StartOptions { open: vec![repo.to_path_buf()], view: Some("history".into()), ..start(base) });
    wait_until(&mut h, "dépôt chargé", |h| h.query_by_label("Premier commit").is_some());
    h
}

/// Dépôt hôte dont `contoso-lib` est déjà un sous-module (chemin `lib`), commité.
fn host_with_submodule(base: &Path) -> (PathBuf, PathBuf) {
    let (lib, host) = repos(base);
    git(&host, &["submodule", "add", "-q", &file_url(&lib), "lib"]);
    git(&host, &["commit", "-q", "-m", "Ajoute lib"]);
    (lib, host)
}

fn row_menu(h: &mut H, path: &str, entry: &str) {
    wait_until(h, "sous-module dans la barre latérale", |h| h.query_by_label(path).is_some());
    h.get_by_label(path).click_secondary();
    wait_until(h, &format!("entrée « {entry} »"), |h| h.query_by_label(entry).is_some());
    h.get_by_label(entry).click();
    h.run_steps(3);
}

/// Remplace le contenu du champ de saisie qui contient `old` par `new`.
fn replace_field(h: &mut H, old: &str, new: &str) {
    wait_until(h, &format!("champ « {old} »"), |h| has_text_input(h, old));
    let field = h.get_by(|n| n.role() == egui::accesskit::Role::TextInput && n.value().as_deref() == Some(old));
    field.focus();
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    h.run_steps(1);
    h.get_by(|n| n.role() == egui::accesskit::Role::TextInput && n.is_focused()).type_text(new);
    h.run_steps(2);
}

/// Écrit dans le champ qui a le focus (le premier du dialogue à l'ouverture).
fn type_in_focused(h: &mut H, text: &str) {
    wait_until(h, "champ avec le focus", |h| {
        h.query_by(|n| n.role() == egui::accesskit::Role::TextInput && n.is_focused()).is_some()
    });
    h.get_by(|n| n.role() == egui::accesskit::Role::TextInput && n.is_focused()).type_text(text);
    h.run_steps(2);
}

fn add_dialog(h: &mut H) {
    open_menu(h, "Dépôt");
    // La commande reste grisée tant que les références du dépôt ne sont pas lues.
    wait_until(h, "commande utilisable", |h| {
        menu_item(h, "Ajouter un sous-module…").is_some_and(|n| !n.accesskit_node().is_disabled())
    });
    click_menu_item(h, "Ajouter un sous-module…");
}

fn submodule_paths(repo: &Path) -> Option<Vec<String>> {
    let out = try_git(repo, &["config", "-f", ".gitmodules", "--get-regexp", r"^submodule\..*\.path$"])
        .or_else(|| Some(String::new()))?;
    Some(out.lines().filter_map(|l| l.split_once(' ').map(|(_, p)| p.to_string())).collect())
}

#[test]
fn adding_from_the_dialog_clones_the_submodule_and_lists_it() {
    let tmp = tempfile::tempdir().unwrap();
    let (lib, host) = repos(tmp.path());
    let mut h = open(tmp.path(), &host);
    add_dialog(&mut h);
    // Le champ URL a le focus ; le chemin vide reprend le nom du dépôt (`contoso-lib`).
    type_in_focused(&mut h, &file_url(&lib));
    h.get_by_label("Ajouter").click();
    wait_git(&mut h, "sous-module enregistré", || submodule_paths(&host).is_some_and(|p| p == ["contoso-lib"]));
    assert!(host.join("contoso-lib").join("lib.txt").exists());
    wait_until(&mut h, "sous-module listé", |h| h.query_by_label("contoso-lib").is_some());
}

#[test]
fn the_path_and_branch_fields_are_used() {
    let tmp = tempfile::tempdir().unwrap();
    let (lib, host) = repos(tmp.path());
    git(&lib, &["branch", "stable"]);
    let mut h = open(tmp.path(), &host);
    add_dialog(&mut h);
    type_in_focused(&mut h, &file_url(&lib));
    h.key_press(Key::Tab);
    type_in_focused(&mut h, "vendor/lib");
    h.key_press(Key::Tab);
    type_in_focused(&mut h, "stable");
    h.get_by_label("Ajouter").click();
    // `.gitmodules` n'est écrit qu'après le clone : on attend l'enregistrement, pas le dossier.
    wait_git(&mut h, "sous-module enregistré", || submodule_paths(&host).is_some_and(|p| p == ["vendor/lib"]));
    let modules = std::fs::read_to_string(host.join(".gitmodules")).unwrap();
    assert!(modules.contains("branch = stable"), "{modules}");
    assert_eq!(submodule_paths(&host).unwrap(), ["vendor/lib"]);
}

#[test]
fn a_failing_add_shows_git_s_message_and_changes_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let (_lib, host) = repos(tmp.path());
    let mut h = open(tmp.path(), &host);
    add_dialog(&mut h);
    type_in_focused(&mut h, &file_url(&tmp.path().join("n-existe-pas")));
    h.get_by_label("Ajouter").click();
    wait_until(&mut h, "échec signalé", |h| h.query_by_label_contains("n-existe-pas").is_some());
    assert_eq!(submodule_paths(&host).unwrap(), Vec::<String>::new());
}

#[test]
fn the_add_button_stays_disabled_without_an_url() {
    let tmp = tempfile::tempdir().unwrap();
    let (_lib, host) = repos(tmp.path());
    let mut h = open(tmp.path(), &host);
    add_dialog(&mut h);
    wait_until(&mut h, "dialogue", |h| h.query_by_label("Annuler").is_some());
    assert!(h.get_by_label("Ajouter").accesskit_node().is_disabled());
}

#[test]
fn editing_the_url_from_the_submodule_menu_updates_gitmodules() {
    let tmp = tempfile::tempdir().unwrap();
    let (lib, host) = host_with_submodule(tmp.path());
    let mut h = open(tmp.path(), &host);
    row_menu(&mut h, "lib", &format!("{} Modifier l'URL…", icons::PENCIL_SIMPLE));
    replace_field(&mut h, &file_url(&lib), "https://git.fabrikam.test/lib.git");
    h.get_by_label("Enregistrer").click();
    wait_git(&mut h, "URL modifiée", || {
        try_git(&host, &["config", "-f", ".gitmodules", "submodule.lib.url"])
            .is_some_and(|u| u.trim() == "https://git.fabrikam.test/lib.git")
    });
}

#[test]
fn moving_from_the_submodule_menu_renames_the_folder() {
    let tmp = tempfile::tempdir().unwrap();
    let (_lib, host) = host_with_submodule(tmp.path());
    let mut h = open(tmp.path(), &host);
    row_menu(&mut h, "lib", &format!("{} Déplacer…", icons::ARROWS_LEFT_RIGHT));
    replace_field(&mut h, "lib", "deps/lib");
    h.get_by_label("Déplacer").click();
    wait_git(&mut h, "sous-module déplacé", || host.join("deps").join("lib").join("lib.txt").exists());
    assert_eq!(submodule_paths(&host).unwrap(), ["deps/lib"]);
    wait_until(&mut h, "nouveau chemin listé", |h| h.query_by_label("deps/lib").is_some());
}

#[test]
fn removing_needs_a_click_not_enter_and_leaves_no_trace() {
    let tmp = tempfile::tempdir().unwrap();
    let (_lib, host) = host_with_submodule(tmp.path());
    let mut h = open(tmp.path(), &host);
    row_menu(&mut h, "lib", &format!("{} Supprimer…", icons::TRASH));
    wait_until(&mut h, "dialogue de confirmation", |h| h.query_by_label("Supprimer").is_some());
    // Un dialogue destructeur ne se valide pas avec Entrée.
    h.key_press(Key::Enter);
    h.run_steps(5);
    assert!(host.join("lib").join("lib.txt").exists());
    h.get_by_label("Supprimer").click();
    // Dernière étape de la suppression : le clone gardé dans .git/modules.
    wait_git(&mut h, "sous-module supprimé", || !host.join(".git").join("modules").join("lib").exists());
    assert!(!host.join("lib").exists());
    assert!(!git(&host, &["ls-files", "--stage"]).contains("lib"), "toujours dans l'index");
    assert!(!host.join(".git").join("modules").join("lib").exists());
    assert_eq!(submodule_paths(&host).unwrap(), Vec::<String>::new());
}
