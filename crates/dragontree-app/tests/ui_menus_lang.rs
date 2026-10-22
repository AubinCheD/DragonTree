//! Langue choisie depuis le menu Configuration › Langue.
//! Binaire de test à part : la langue est globale au processus et les autres tests attendent le français.

mod common;

use common::*;

/// `start` passe `--lang fr` : l'interface ne passe en anglais que si le choix du menu met fin au remplacement de la
/// langue par la ligne de commande (`Command::Language`, RV5a).
#[test]
fn language_menu_switches_the_interface_to_english() {
    let tmp = tempfile::tempdir().unwrap();
    let mut h = harness(start(tmp.path()));
    assert_eq!(start(tmp.path()).lang, Some(dragontree_i18n::Lang::Fr), "langue imposée par la ligne de commande");

    open_menu(&mut h, "Configuration");
    open_menu(&mut h, "Langue");
    click_menu_item(&mut h, "English");
    wait_until(&mut h, "barre de menus en anglais", |h| menu_item(h, "File").is_some());
    assert!(menu_item(&h, "Fichier").is_none(), "texte français resté affiché");

    open_menu(&mut h, "Configuration");
    open_menu(&mut h, "Language");
    wait_until(&mut h, "sous-menu Language", |h| menu_item(h, "English").is_some());
    assert_eq!(menu_item_checked(&h, "English"), Some(true));
    assert_eq!(menu_item_checked(&h, "Français"), Some(false));

    let saved = std::fs::read_to_string(tmp.path().join("profil").join("settings.json")).unwrap();
    assert!(saved.contains("\"language\": \"en\""), "langue non enregistrée : {saved}");
}
