//! Fichier de réglages illisible au démarrage (L3) : il n'est jamais écrasé tant qu'il n'a pas été relu ; un
//! indicateur dans la barre de menus et une notification (une seule par série) préviennent que les modifications ne
//! sont pas enregistrées ; relu, il reçoit les modifications de la session et garde ses autres valeurs, sans le
//! `--lang` de la ligne de commande (`start` passe `--lang fr`).
#![cfg(windows)]

mod common;

use std::path::{Path, PathBuf};

use common::*;
use dragontree_app::DragonTreeApp;
use dragontree_app::settings::{Settings, ThemeChoice};
use dragontree_i18n::Lang;
use egui::accesskit::Role;
use egui_kittest::kittest::AccessKitNode;
use egui_kittest::kittest::Queryable;
use egui_kittest::{Harness, Node};
use windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ;

const INDICATOR: &str = "Réglages non enregistrés";
const WARNING: &str = "Modifications des réglages non enregistrées";

/// L'indicateur de la barre de menus (bouton) ; la notification cite aussi son libellé.
fn is_indicator(n: &AccessKitNode<'_>) -> bool {
    n.role() == Role::Button && n.label().is_some_and(|l| l.contains(INDICATOR))
}

fn indicator<'h>(h: &'h Harness<'static, DragonTreeApp>) -> Option<Node<'h>> {
    h.query_by(is_indicator)
}

fn settings_file(base: &Path) -> PathBuf {
    base.join("profil").join("settings.json")
}

/// Réglages du fichier : thème clair, tags masqués, un favori, interface en anglais.
fn file_settings() -> Settings {
    Settings {
        theme: ThemeChoice::Light,
        show_tags: false,
        favorites: vec![PathBuf::from(r"C:\DEV\Fabrikam")],
        language: Some(Lang::En),
        ..Settings::default()
    }
}

/// Fichier tenu ouvert par un autre programme (antivirus, synchronisation…) ; `share` : ce qu'il laisse faire aux
/// autres (0 : rien, pas même lire ; `FILE_SHARE_READ` : lire, mais ni écrire ni renommer).
fn hold_open(file: &Path, share: u32) -> std::fs::File {
    use std::os::windows::fs::OpenOptionsExt;
    std::fs::OpenOptions::new().read(true).share_mode(share).open(file).unwrap()
}

/// Bascule une entrée du menu Affichage (un réglage enregistré).
fn toggle_view(h: &mut Harness<'static, DragonTreeApp>, label: &str) {
    open_menu(h, "Affichage");
    click_menu_item(h, label);
}

/// Deux modifications avec le fichier toujours illisible : indicateur et une seule notification.
fn modifications_are_not_saved(h: &mut Harness<'static, DragonTreeApp>) {
    assert!(indicator(h).is_none(), "rien de modifié, pas d'indicateur");
    toggle_view(h, "Afficher les branches distantes");
    wait_until(h, "indicateur et notification", |h| {
        indicator(h).is_some() && h.query_by_label_contains(WARNING).is_some()
    });
    toggle_view(h, "Afficher les stashs");
    h.run_steps(3);
    assert_eq!(h.query_all_by_label_contains(WARNING).count(), 1, "une seule notification par série");
    assert!(indicator(h).is_some(), "l'indicateur reste");
}

/// Fichier relu : modifications de la session et autres valeurs du fichier, sans la langue de `--lang` ; l'indicateur
/// disparaît et l'application reprend les réglages du fichier.
fn file_is_merged(h: &mut Harness<'static, DragonTreeApp>, file: &Path, expected_tags: bool) {
    wait_until(h, "fichier relu et enregistré", |_| {
        std::fs::read(file).is_ok_and(|b| serde_json::from_slice::<Settings>(&b).is_ok_and(|s| !s.show_stashes))
    });
    let s = Settings::load_from(file);
    assert!(!s.show_remote_branches && !s.show_stashes, "modifications de la session");
    assert_eq!(s.theme, ThemeChoice::Light, "valeur du fichier");
    assert_eq!(s.show_tags, expected_tags, "valeur du fichier");
    assert_eq!(s.favorites, [PathBuf::from(r"C:\DEV\Fabrikam")], "valeur du fichier");
    assert_eq!(s.language, Some(Lang::En), "langue du fichier, pas celle de --lang");
    wait_until(h, "indicateur effacé", |h| indicator(h).is_none());
    assert!(h.query_by_label_contains("Réglages relus").is_some(), "notification de la relecture");
    open_menu(h, "Affichage");
    assert_eq!(menu_item_checked(h, "Afficher les tags"), Some(expected_tags), "réglage du fichier repris");
    assert_eq!(menu_item_checked(h, "Afficher les stashs"), Some(false), "modification de la session gardée");
}

#[test]
fn a_locked_settings_file_is_never_overwritten_then_receives_the_session_changes() {
    let tmp = tempfile::tempdir().unwrap();
    let file = settings_file(tmp.path());
    file_settings().save_to(&file).unwrap();
    let bytes = std::fs::read(&file).unwrap();
    let lock = hold_open(&file, 0);
    let mut h = harness(start(tmp.path()));
    wait_until(&mut h, "notification du démarrage", |h| h.query_by_label_contains("Réglages illisibles").is_some());
    modifications_are_not_saved(&mut h);

    // Le verrou levé, le fichier est intact ; un clic sur l'indicateur relance l'enregistrement, qui le relit.
    drop(lock);
    assert_eq!(std::fs::read(&file).unwrap(), bytes, "fichier jamais écrasé");
    click_by(&mut h, "indicateur", egui::PointerButton::Primary, is_indicator);
    file_is_merged(&mut h, &file, false);
}

#[test]
fn an_invalid_settings_file_held_open_is_never_overwritten_then_merged_once_repaired() {
    let tmp = tempfile::tempdir().unwrap();
    let file = settings_file(tmp.path());
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, "{ \"theme\": \"Light\", contenu invalide").unwrap();
    let bytes = std::fs::read(&file).unwrap();
    // Lisible mais impossible à mettre de côté (renommer) tant qu'il est tenu ouvert.
    let held = hold_open(&file, FILE_SHARE_READ);
    let mut h = harness(start(tmp.path()));
    wait_until(&mut h, "notification du démarrage", |h| h.query_by_label_contains("Réglages illisibles").is_some());
    modifications_are_not_saved(&mut h);
    assert_eq!(std::fs::read(&file).unwrap(), bytes, "fichier jamais écrasé");

    // Réparé à la main (tags affichés cette fois) : la modification suivante le relit.
    drop(held);
    Settings { show_tags: true, ..file_settings() }.save_to(&file).unwrap();
    toggle_view(&mut h, "Afficher les fichiers ignorés");
    file_is_merged(&mut h, &file, true);
    assert!(Settings::load_from(&file).show_ignored_files, "modification qui a relu le fichier");
}
